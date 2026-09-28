//! Level 3 Integration Tests for Remote Transcription
//!
//! These tests require external resources (Whisper models) and are ignored by default.
//! They work on both macOS and Windows.
//!
//! The tests will **automatically download** the tiny.en model (~75MB) if not present.
//!
//! To run these tests:
//!
//! **macOS:**
//! ```bash
//! cargo test --package voicetypr_app integration_tests -- --ignored --nocapture
//! ```
//!
//! **Windows** (requires manifest embedding):
//! ```powershell
//! cd src-tauri
//! ./run-tests.ps1 -IgnoredOnly
//! ```

#[cfg(test)]
mod tests {
    use crate::remote::http::{create_routes, ClientActivityMap, ServerContext};
    use crate::remote::transcription::{RealTranscriptionContext, TranscriptionServerConfig};
    use futures_util::future::join_all;
    use serial_test::serial;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;
    use tokio::sync::{RwLock, Semaphore};
    use tokio::time::sleep;

    fn test_transcription_guard() -> Arc<Semaphore> {
        Arc::new(Semaphore::new(1))
    }

    fn test_client_activity() -> ClientActivityMap {
        use std::collections::HashMap;
        use std::sync::Mutex;
        Arc::new(Mutex::new(HashMap::new()))
    }

    /// URL for the tiny.en Whisper model (~75MB)
    const TINY_MODEL_URL: &str =
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin";
    const TINY_MODEL_FILENAME: &str = "ggml-tiny.en.bin";

    /// Expected phrases in the test audio file
    /// The test audio says: "testing testing, one two three"
    const EXPECTED_TRANSCRIPTION_PHRASES: &[&str] = &["testing", "one", "two", "three"];

    /// Get the path to the test audio file
    /// Returns the path relative to the project root
    fn get_test_audio_path() -> PathBuf {
        // Try to find the test audio file
        // When running from src-tauri/, we need to go up one level
        let possible_paths = [
            PathBuf::from("../tests/fixtures/audio-files/test-audio.wav"),
            PathBuf::from("tests/fixtures/audio-files/test-audio.wav"),
        ];

        for path in &possible_paths {
            if path.exists() {
                return path.clone();
            }
        }

        // Return the expected path even if not found (will fail with clear error)
        possible_paths[0].clone()
    }

    /// Get the model directory path for this platform
    fn get_model_dir() -> PathBuf {
        // Primary: User data directory (where Voicetypr downloads models)
        // - macOS: ~/Library/Application Support/com.ideaplexa.voicetypr/models/
        // - Windows: C:\Users\<user>\AppData\Roaming\com.ideaplexa.voicetypr\models\
        if let Some(data_dir) = dirs::data_dir() {
            return data_dir.join("com.ideaplexa.voicetypr").join("models");
        }

        // Fallback: data_local_dir
        if let Some(local_dir) = dirs::data_local_dir() {
            return local_dir.join("com.ideaplexa.voicetypr").join("models");
        }

        // Last resort: local models directory
        PathBuf::from("models")
    }

    /// Ensure the tiny.en model is available, downloading if necessary
    /// Returns the path to the model file
    /// Note: This uses blocking I/O and should be called via spawn_blocking in async contexts
    fn ensure_tiny_model_available_sync() -> Result<PathBuf, String> {
        let model_dir = get_model_dir();
        let model_path = model_dir.join(TINY_MODEL_FILENAME);

        // Check if model already exists
        if model_path.exists() {
            println!("Model already exists: {:?}", model_path);
            return Ok(model_path);
        }

        // Create model directory if needed
        fs::create_dir_all(&model_dir)
            .map_err(|e| format!("Failed to create model directory {:?}: {}", model_dir, e))?;

        println!("Downloading tiny.en model (~75MB) to {:?}...", model_path);
        println!("URL: {}", TINY_MODEL_URL);

        // Download the model using reqwest blocking client
        let response = reqwest::blocking::get(TINY_MODEL_URL)
            .map_err(|e| format!("Failed to download model: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to download model: HTTP {}",
                response.status()
            ));
        }

        let bytes = response
            .bytes()
            .map_err(|e| format!("Failed to read model bytes: {}", e))?;

        println!("Downloaded {} bytes, writing to disk...", bytes.len());

        // Write to file
        let mut file = fs::File::create(&model_path)
            .map_err(|e| format!("Failed to create model file: {}", e))?;

        file.write_all(&bytes)
            .map_err(|e| format!("Failed to write model file: {}", e))?;

        println!("Model downloaded successfully: {:?}", model_path);
        Ok(model_path)
    }

    /// Async wrapper for ensure_tiny_model_available_sync
    async fn ensure_tiny_model_available() -> Result<PathBuf, String> {
        tokio::task::spawn_blocking(ensure_tiny_model_available_sync)
            .await
            .map_err(|e| format!("Task join error: {}", e))?
    }

    /// Create a valid WAV file with silent audio samples (for stress tests)
    fn create_silent_wav(path: &std::path::Path, samples: usize) -> Result<(), String> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|e| format!("Failed to create WAV writer: {}", e))?;

        for _ in 0..samples {
            writer
                .write_sample(0i16)
                .map_err(|e| format!("Failed to write sample: {}", e))?;
        }

        writer
            .finalize()
            .map_err(|e| format!("Failed to finalize WAV: {}", e))?;

        Ok(())
    }

    /// Verify that transcription contains expected phrases
    fn verify_transcription(text: &str) -> bool {
        let text_lower = text.to_lowercase();
        let found_count = EXPECTED_TRANSCRIPTION_PHRASES
            .iter()
            .filter(|phrase| text_lower.contains(*phrase))
            .count();
        // Require at least 2 of the expected phrases (to allow for some transcription variance)
        found_count >= 2
    }

    /// Level 3 Integration Test: Full transcription pipeline
    ///
    /// This test:
    /// 1. Downloads the tiny.en model if not present
    /// 2. Creates a real transcription server with actual Whisper model
    /// 3. Sends real audio ("testing testing, one two three") via HTTP
    /// 4. Verifies transcription matches expected text
    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_full_transcription_pipeline() {
        // Ensure model is available (downloads if needed)
        let model_path = match ensure_tiny_model_available().await {
            Ok(p) => p,
            Err(e) => {
                panic!("Failed to ensure model is available: {}", e);
            }
        };

        println!("Using model: {:?}", model_path);

        // Use the real test audio file
        let audio_path = get_test_audio_path();
        assert!(
            audio_path.exists(),
            "Test audio file not found at {:?}",
            audio_path
        );
        println!("Using test audio: {:?}", audio_path);

        // Create server config
        let config = TranscriptionServerConfig {
            server_name: "Test Server".to_string(),
            password: None,
            model_name: "tiny.en".to_string(),
            model_path,
        };

        // Create real transcription context wrapped in Arc<Mutex>
        let context = Arc::new(RwLock::new(RealTranscriptionContext::new(config)));

        // Start server
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(context, test_transcription_guard(), test_client_activity());

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        // Wait for server to start
        let addr = addr_rx.await.expect("Server failed to start");
        println!("Server started at: http://{}", addr);

        // Give server time to fully initialize
        sleep(Duration::from_millis(100)).await;

        // Test status endpoint
        let client = reqwest::Client::new();
        let status_url = format!("http://{}/api/v1/status", addr);
        let status_response = client
            .get(&status_url)
            .send()
            .await
            .expect("Failed to get status");

        assert!(
            status_response.status().is_success(),
            "Status endpoint failed"
        );
        let status_json: serde_json::Value = status_response
            .json()
            .await
            .expect("Failed to parse status JSON");
        println!("Status response: {:?}", status_json);
        assert_eq!(status_json["status"], "ok");

        // Test transcription endpoint
        let audio_data = std::fs::read(&audio_path).expect("Failed to read audio file");
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let transcribe_response = client
            .post(&transcribe_url)
            .header("Content-Type", "audio/wav")
            .body(audio_data)
            .timeout(Duration::from_secs(60)) // Allow 60s for model loading + transcription
            .send()
            .await
            .expect("Failed to send transcription request");

        assert!(
            transcribe_response.status().is_success(),
            "Transcription endpoint failed with status: {}",
            transcribe_response.status()
        );

        let transcribe_json: serde_json::Value = transcribe_response
            .json()
            .await
            .expect("Failed to parse transcription JSON");
        println!("Transcription response: {:?}", transcribe_json);

        // Verify response structure
        assert!(
            transcribe_json["text"].is_string(),
            "Missing 'text' in response"
        );
        assert!(
            transcribe_json["model"].is_string(),
            "Missing 'model' in response"
        );

        // Verify transcription produced some text
        let transcribed_text = transcribe_json["text"].as_str().unwrap_or("");
        println!("Transcribed text: '{}'", transcribed_text);
        // Note: Content verification is relaxed because the tiny model may produce
        // varying results. The key test is that transcription completes successfully.
        assert!(
            !transcribed_text.is_empty(),
            "Transcription should not be empty"
        );
        assert!(
            verify_transcription(transcribed_text),
            "Transcription should contain expected phrases, got: '{}'",
            transcribed_text
        );

        // Shutdown server
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Full transcription pipeline test passed!");
        println!("  Transcribed: '{}'", transcribed_text);
    }

    /// Level 3 Integration Test: Server authentication
    ///
    /// Tests that password protection works correctly
    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_server_authentication() {
        let model_path = ensure_tiny_model_available()
            .await
            .expect("Failed to ensure model is available");

        // Create server with password
        let config = TranscriptionServerConfig {
            server_name: "Protected Server".to_string(),
            password: Some("test-password".to_string()),
            model_name: "tiny.en".to_string(),
            model_path,
        };

        let context = Arc::new(RwLock::new(RealTranscriptionContext::new(config)));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(context, test_transcription_guard(), test_client_activity());

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let status_url = format!("http://{}/api/v1/status", addr);

        // Request without password should fail
        let response = client
            .get(&status_url)
            .send()
            .await
            .expect("Request failed");
        assert_eq!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "Expected 401 without password"
        );

        // Request with wrong password should fail
        let response = client
            .get(&status_url)
            .header("X-Voicetypr-Key", "wrong-password")
            .send()
            .await
            .expect("Request failed");
        assert_eq!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "Expected 401 with wrong password"
        );

        // Request with correct password should succeed
        let response = client
            .get(&status_url)
            .header("X-Voicetypr-Key", "test-password")
            .send()
            .await
            .expect("Request failed");
        assert!(
            response.status().is_success(),
            "Expected success with correct password"
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Server authentication test passed!");
    }

    /// Level 3 Integration Test: Rapid sequential requests (Issue #2)
    ///
    /// Tests that multiple rapid requests from a single client all complete
    /// successfully with real transcription.
    ///
    /// This verifies:
    /// 1. All requests complete (queued, not rejected)
    /// 2. No crashes or data corruption
    /// 3. Server handles concurrent load
    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_rapid_sequential_requests_real_transcription() {
        let model_path = ensure_tiny_model_available()
            .await
            .expect("Failed to ensure model is available");

        println!("Using model: {:?}", model_path);

        // Use the real test audio file for all requests
        let test_audio_path = get_test_audio_path();
        assert!(test_audio_path.exists(), "Test audio not found");

        let num_requests = 3;
        let audio_files: Vec<PathBuf> =
            (0..num_requests).map(|_| test_audio_path.clone()).collect();
        println!("Using test audio for {} concurrent requests", num_requests);

        // Create server
        let config = TranscriptionServerConfig {
            server_name: "Rapid Test Server".to_string(),
            password: None,
            model_name: "tiny.en".to_string(),
            model_path,
        };

        let context = Arc::new(RwLock::new(RealTranscriptionContext::new(config)));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(context, test_transcription_guard(), test_client_activity());

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        println!("Server started at: http://{}", addr);
        sleep(Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);

        // Send rapid sequential requests concurrently
        println!("Sending {} rapid concurrent requests...", num_requests);
        let start_time = Instant::now();

        let mut handles = Vec::new();
        for (i, audio_path) in audio_files.iter().enumerate() {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = std::fs::read(audio_path).expect("Failed to read audio");
            let request_id = i;

            handles.push(tokio::spawn(async move {
                let req_start = Instant::now();
                println!("Request {} starting", request_id);

                let result = client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(120)) // 2 min timeout for model loading
                    .send()
                    .await;

                let req_duration = req_start.elapsed();
                println!("Request {} completed in {:?}", request_id, req_duration);

                (request_id, result, req_duration)
            }));
        }

        // Wait for all requests
        let mut success_count = 0;
        let mut total_chars = 0;
        for handle in handles {
            let (request_id, result, duration) = handle.await.expect("Task panicked");
            match result {
                Ok(response) if response.status().is_success() => {
                    let json: serde_json::Value =
                        response.json().await.expect("Failed to parse JSON");
                    let text = json["text"].as_str().unwrap_or("");
                    total_chars += text.len();
                    println!(
                        "Request {}: SUCCESS ({} chars, {:?})",
                        request_id,
                        text.len(),
                        duration
                    );
                    success_count += 1;
                }
                Ok(response) => {
                    println!(
                        "Request {}: FAILED with status {}",
                        request_id,
                        response.status()
                    );
                }
                Err(e) => {
                    println!("Request {}: ERROR - {}", request_id, e);
                }
            }
        }

        let total_time = start_time.elapsed();

        println!("\n--- Results ---");
        println!("Total requests: {}", num_requests);
        println!("Successful: {}", success_count);
        println!("Total time: {:?}", total_time);
        println!("Total chars transcribed: {}", total_chars);

        // Assertions
        assert_eq!(
            success_count, num_requests,
            "All {} requests should complete successfully",
            num_requests
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Rapid sequential requests test passed!");
    }

    /// Level 3 Integration Test: Sequential requests with varying audio sizes
    ///
    /// Tests that requests with different audio sizes all process correctly
    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_sequential_requests_varying_sizes() {
        let model_path = ensure_tiny_model_available()
            .await
            .expect("Failed to ensure model is available");

        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create audio files of varying sizes (0.5s, 1s, 1.5s of audio)
        let sample_counts = [8000, 16000, 24000]; // samples at 16kHz
        let mut audio_files = Vec::new();

        for (i, &samples) in sample_counts.iter().enumerate() {
            let audio_path = temp_dir.path().join(format!("test_size_{}.wav", i));
            create_silent_wav(&audio_path, samples).expect("Failed to create WAV");
            audio_files.push((audio_path, samples));
        }

        let config = TranscriptionServerConfig {
            server_name: "Size Test Server".to_string(),
            password: None,
            model_name: "tiny.en".to_string(),
            model_path,
        };

        let context = Arc::new(RwLock::new(RealTranscriptionContext::new(config)));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(context, test_transcription_guard(), test_client_activity());
            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });
            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);

        // Send requests sequentially
        for (audio_path, samples) in &audio_files {
            let audio_data = std::fs::read(audio_path).expect("Failed to read audio");
            let response = client
                .post(&transcribe_url)
                .header("Content-Type", "audio/wav")
                .body(audio_data)
                .timeout(Duration::from_secs(120))
                .send()
                .await
                .expect("Request failed");

            assert!(
                response.status().is_success(),
                "Request for {} samples should succeed",
                samples
            );

            let json: serde_json::Value = response.json().await.expect("Failed to parse JSON");
            assert!(json["text"].is_string(), "Response should have text field");
            assert!(
                json["model"].is_string(),
                "Response should have model field"
            );
            println!(
                "Processed {} samples: {:?}",
                samples,
                json["text"].as_str().unwrap_or("")
            );
        }

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Sequential requests with varying sizes test passed!");
    }

    /// Level 3 Integration Test: Host local + remote client concurrent transcription (Issue #3)
    ///
    /// This test simulates the scenario from Issue #3:
    /// 1. PC starts sharing (server mode)
    /// 2. MacBook connects as client
    /// 3. Both machines initiate transcription simultaneously
    /// 4. Verify both complete successfully without crashes
    /// 5. Verify BOTH transcriptions produce correct text
    ///
    /// In this test, we simulate "local" and "remote" by:
    /// - Local: Direct call to the transcription context (as if host is transcribing)
    /// - Remote: HTTP request to the server (as if client is requesting)
    ///
    /// The key verification is that both complete without error when running concurrently
    /// AND both produce correct transcription results.
    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_concurrent_local_and_remote_transcription() {
        let model_path = ensure_tiny_model_available()
            .await
            .expect("Failed to ensure model is available");

        println!("Using model: {:?}", model_path);

        // Use the real test audio file for both local and remote
        let test_audio_path = get_test_audio_path();
        assert!(test_audio_path.exists(), "Test audio not found");
        println!("Using test audio: {:?}", test_audio_path);

        // Create server config
        let config = TranscriptionServerConfig {
            server_name: "Concurrent Test Server".to_string(),
            password: None,
            model_name: "tiny.en".to_string(),
            model_path,
        };

        // Create separate transcription contexts to avoid serializing local and remote work
        let remote_context = Arc::new(RwLock::new(RealTranscriptionContext::new(config.clone())));
        let local_context = RealTranscriptionContext::new(config);

        // Start HTTP server for remote requests
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                remote_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        println!("Server started at: http://{}", addr);
        sleep(Duration::from_millis(100)).await;

        // Use the same test audio for both local and remote (they'll run concurrently)
        let audio_data_remote = std::fs::read(&test_audio_path).expect("Failed to read audio file");
        let audio_data_local = audio_data_remote.clone();

        // Spawn "remote client" request (HTTP)
        let addr_clone = addr;
        let remote_handle = tokio::spawn(async move {
            println!("[Remote] Starting HTTP transcription request...");
            let client = reqwest::Client::new();
            let transcribe_url = format!("http://{}/api/v1/transcribe", addr_clone);
            let response = client
                .post(&transcribe_url)
                .header("Content-Type", "audio/wav")
                .body(audio_data_remote)
                .timeout(Duration::from_secs(120))
                .send()
                .await
                .expect("[Remote] Failed to send request");

            let status = response.status();
            println!("[Remote] Response status: {}", status);
            assert!(
                status.is_success(),
                "[Remote] Transcription failed with status: {}",
                status
            );

            let json: serde_json::Value = response
                .json()
                .await
                .expect("[Remote] Failed to parse JSON");
            println!("[Remote] Transcription completed: {:?}", json);
            json
        });

        let local_handle = tokio::task::spawn_blocking(move || {
            println!("[Local] Starting direct transcription...");

            let result = local_context.transcribe(&audio_data_local, None, None);

            match result {
                Ok(response) => {
                    println!(
                        "[Local] Transcription completed: text='{}', duration={}ms, model='{}'",
                        response.raw_text,
                        response.timings.processing_duration_ms.unwrap_or_default(),
                        response.model
                    );
                    Ok(response)
                }
                Err(e) => {
                    println!("[Local] Transcription failed: {}", e);
                    Err(e)
                }
            }
        });

        // Wait for both to complete
        println!("Waiting for both transcriptions to complete...");
        let (remote_result, local_result) = tokio::join!(remote_handle, local_handle);

        // Verify remote request succeeded
        let remote_json = remote_result.expect("[Remote] Task panicked");
        assert!(
            remote_json["text"].is_string(),
            "[Remote] Missing 'text' in response"
        );
        assert!(
            remote_json["model"].is_string(),
            "[Remote] Missing 'model' in response"
        );
        let remote_text = remote_json["text"].as_str().unwrap_or("");
        println!("[Remote] Transcribed: '{}'", remote_text);
        // Note: Content verification is relaxed - key test is that transcription completes
        assert!(
            !remote_text.is_empty(),
            "[Remote] Transcription should not be empty"
        );
        assert!(
            verify_transcription(remote_text),
            "[Remote] Transcription should contain expected phrases, got: '{}'",
            remote_text
        );

        // Verify local transcription succeeded
        let local_response = local_result.expect("[Local] Task panicked");
        assert!(
            local_response.is_ok(),
            "[Local] Transcription failed: {:?}",
            local_response.err()
        );
        let local_response = local_response.unwrap();
        assert!(!local_response.model.is_empty(), "[Local] Empty model name");
        println!("[Local] Transcribed: '{}'", local_response.raw_text);
        // Note: Content verification is relaxed - key test is that transcription completes
        assert!(
            !local_response.raw_text.is_empty(),
            "[Local] Transcription should not be empty"
        );
        assert!(
            verify_transcription(&local_response.raw_text),
            "[Local] Transcription should contain expected phrases, got: '{}'",
            local_response.raw_text
        );

        // Shutdown server
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Concurrent local + remote transcription test passed!");
        println!("  Both transcriptions completed with correct results.");
    }

    /// Level 3 Integration Test: Multiple concurrent remote requests
    ///
    /// This test simulates multiple clients sending requests simultaneously,
    /// verifying the server can queue and handle them correctly.
    /// All clients use the real test audio and verify correct transcription.
    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_multiple_concurrent_remote_requests() {
        let model_path = ensure_tiny_model_available()
            .await
            .expect("Failed to ensure model is available");

        println!("Using model: {:?}", model_path);

        // Use the real test audio
        let test_audio_path = get_test_audio_path();
        assert!(test_audio_path.exists(), "Test audio not found");
        let audio_data = std::fs::read(&test_audio_path).expect("Failed to read audio file");
        println!("Using test audio: {:?}", test_audio_path);

        // Create server
        let config = TranscriptionServerConfig {
            server_name: "Multi-Client Test Server".to_string(),
            password: None,
            model_name: "tiny.en".to_string(),
            model_path,
        };

        let context = Arc::new(RwLock::new(RealTranscriptionContext::new(config)));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(context, test_transcription_guard(), test_client_activity());

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        println!("Server started at: http://{}", addr);
        sleep(Duration::from_millis(100)).await;

        // Number of concurrent requests to send
        const NUM_REQUESTS: usize = 3;

        // Spawn multiple concurrent HTTP requests
        let mut handles = Vec::new();
        for i in 0..NUM_REQUESTS {
            let audio_data_clone = audio_data.clone();
            let addr_clone = addr;

            handles.push(tokio::spawn(async move {
                println!("[Client {}] Starting transcription request...", i);
                let start = Instant::now();

                let client = reqwest::Client::new();
                let transcribe_url = format!("http://{}/api/v1/transcribe", addr_clone);
                let response = client
                    .post(&transcribe_url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data_clone)
                    .timeout(Duration::from_secs(180)) // Allow time for queued requests
                    .send()
                    .await
                    .unwrap_or_else(|_| panic!("[Client {}] Failed to send request", i));

                let status = response.status();
                let elapsed = start.elapsed();
                println!(
                    "[Client {}] Response status: {} (took {:?})",
                    i, status, elapsed
                );

                assert!(
                    status.is_success(),
                    "[Client {}] Failed with status: {}",
                    i,
                    status
                );

                let json: serde_json::Value = response
                    .json()
                    .await
                    .unwrap_or_else(|_| panic!("[Client {}] Failed to parse JSON", i));

                println!("[Client {}] Completed successfully", i);
                (i, json, elapsed)
            }));
        }

        // Wait for all requests to complete
        println!(
            "Waiting for {} concurrent requests to complete...",
            NUM_REQUESTS
        );
        let results = join_all(handles).await;

        // Verify all succeeded with correct transcription
        let mut total_duration = Duration::ZERO;
        let mut verified_count = 0;
        for result in results {
            let (client_id, json, elapsed) = result.expect("Task panicked");
            assert!(
                json["text"].is_string(),
                "[Client {}] Missing 'text' in response",
                client_id
            );
            assert!(
                json["model"].is_string(),
                "[Client {}] Missing 'model' in response",
                client_id
            );

            let text = json["text"].as_str().unwrap_or("");
            println!("[Client {}] Transcribed: '{}'", client_id, text);
            // Note: Content verification is relaxed - key test is that transcription completes
            assert!(
                !text.is_empty(),
                "[Client {}] Transcription should not be empty",
                client_id
            );
            assert!(
                verify_transcription(text),
                "[Client {}] Transcription should contain expected phrases, got: '{}'",
                client_id,
                text
            );
            verified_count += 1;
            total_duration += elapsed;
        }

        // Shutdown server
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Multiple concurrent remote requests test passed!");
        println!(
            "  All {} requests completed with correct transcription.",
            verified_count
        );
        println!(
            "  Average response time: {:?}",
            total_duration / NUM_REQUESTS as u32
        );
    }
}
