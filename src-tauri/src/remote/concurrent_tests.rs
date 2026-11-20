//! Concurrent Transcription Tests (Issue #3)
//!
//! These tests verify correct behavior when multiple transcription requests
//! arrive simultaneously, simulating the scenario where:
//! - Host is doing local transcription
//! - Remote client sends transcription request at the same time
//!
//! The expected behavior is:
//! - Requests are serialized at the async mutex level
//! - Both requests complete successfully
//! - No deadlocks or race conditions occur

#[cfg(test)]
mod tests {
    use crate::remote::http::{create_routes, ClientActivityMap, ServerContext};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
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

    /// Mock context with configurable delay for simulating transcription time
    struct DelayedMockContext {
        delay_ms: u64,
        server_name: String,
        model_name: String,
        /// Counter to track how many concurrent transcriptions are running
        active_transcriptions: Arc<AtomicU32>,
        /// Maximum concurrent transcriptions observed
        max_concurrent: Arc<AtomicU32>,
        /// Total transcriptions completed
        completed_count: Arc<AtomicU32>,
    }

    impl DelayedMockContext {
        fn new(delay_ms: u64) -> Self {
            Self {
                delay_ms,
                server_name: "test-server".to_string(),
                model_name: "test-model".to_string(),
                active_transcriptions: Arc::new(AtomicU32::new(0)),
                max_concurrent: Arc::new(AtomicU32::new(0)),
                completed_count: Arc::new(AtomicU32::new(0)),
            }
        }

        fn get_max_concurrent(&self) -> u32 {
            self.max_concurrent.load(Ordering::SeqCst)
        }

        fn get_completed_count(&self) -> u32 {
            self.completed_count.load(Ordering::SeqCst)
        }
    }

    impl ServerContext for DelayedMockContext {
        fn license_cache(
            &self,
        ) -> std::sync::Arc<tokio::sync::RwLock<crate::commands::license::RuntimeLicenseCache>>
        {
            crate::remote::http::licensed_host_cache()
        }

        fn get_model_name(&self) -> String {
            self.model_name.clone()
        }

        fn get_server_name(&self) -> String {
            self.server_name.clone()
        }

        fn get_password(&self) -> Option<String> {
            None
        }

        fn transcribe(
            &self,
            audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<crate::transcription::TranscriptionResult, String> {
            // Track concurrent transcriptions
            let current = self.active_transcriptions.fetch_add(1, Ordering::SeqCst) + 1;

            // Update max if this is higher
            let mut max = self.max_concurrent.load(Ordering::SeqCst);
            while current > max {
                match self.max_concurrent.compare_exchange(
                    max,
                    current,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(actual) => max = actual,
                }
            }

            // Simulate transcription time with blocking sleep
            // (real transcription is CPU-bound and blocking)
            std::thread::sleep(Duration::from_millis(self.delay_ms));

            // Decrement active count
            self.active_transcriptions.fetch_sub(1, Ordering::SeqCst);
            self.completed_count.fetch_add(1, Ordering::SeqCst);

            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                self.model_name.clone(),
                None,
                false,
            );
            Ok(crate::transcription::TranscriptionResult::new(
                &job,
                format!("Transcribed {} bytes", audio_data.len()),
            )
            .with_processing_duration_ms(Some(self.delay_ms)))
        }
    }

    /// Helper to start a test server and return its address
    async fn start_test_server<T: ServerContext + 'static>(
        ctx: Arc<RwLock<T>>,
    ) -> (
        std::net::SocketAddr,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await; // Give server time to be ready

        (addr, shutdown_tx, handle)
    }

    /// Test: Two concurrent transcription requests serialize correctly
    ///
    /// This simulates the scenario where:
    /// - Host starts local transcription
    /// - Remote client sends request at same time
    /// - Both should complete, with one waiting for the other
    #[tokio::test]
    async fn test_concurrent_transcription_requests_serialize() {
        // Use 100ms delay to make timing measurable
        let ctx = Arc::new(RwLock::new(DelayedMockContext::new(100)));
        let (addr, shutdown_tx, handle) = start_test_server(ctx.clone()).await;

        let client = reqwest::Client::new();
        let url = format!("http://{}/api/v1/transcribe", addr);

        // Create test audio data
        let audio_data = vec![0u8; 1000];

        // Start two concurrent requests
        let start = Instant::now();

        let client1 = client.clone();
        let url1 = url.clone();
        let audio1 = audio_data.clone();
        let req1 = tokio::spawn(async move {
            client1
                .post(&url1)
                .header("Content-Type", "audio/wav")
                .body(audio1)
                .timeout(Duration::from_secs(10))
                .send()
                .await
        });

        let client2 = client.clone();
        let url2 = url.clone();
        let audio2 = audio_data.clone();
        let req2 = tokio::spawn(async move {
            client2
                .post(&url2)
                .header("Content-Type", "audio/wav")
                .body(audio2)
                .timeout(Duration::from_secs(10))
                .send()
                .await
        });

        // Wait for both to complete
        let (result1, result2) = tokio::join!(req1, req2);

        let elapsed = start.elapsed();

        // Both should succeed
        let response1 = result1.expect("Task 1 panicked").expect("Request 1 failed");
        let response2 = result2.expect("Task 2 panicked").expect("Request 2 failed");

        assert!(response1.status().is_success(), "Request 1 should succeed");
        assert!(response2.status().is_success(), "Request 2 should succeed");

        // Elapsed time should be at least 200ms (2 x 100ms) due to serialization
        // Allow some margin for network/scheduling overhead
        assert!(
            elapsed >= Duration::from_millis(180),
            "Requests should serialize: elapsed {:?} should be >= 180ms",
            elapsed
        );

        // Verify both completed
        let ctx_guard = ctx.read().await;
        assert_eq!(
            ctx_guard.get_completed_count(),
            2,
            "Both requests should complete"
        );

        // With proper serialization, max concurrent should be 1
        // (the async mutex ensures only one transcription runs at a time)
        assert_eq!(
            ctx_guard.get_max_concurrent(),
            1,
            "Max concurrent should be 1 due to mutex serialization"
        );

        // Cleanup
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    }

    /// Test: Multiple clients can queue requests without deadlock
    ///
    /// Simulates multiple remote clients sending requests while host is busy
    #[tokio::test]
    async fn test_multiple_clients_queue_without_deadlock() {
        let ctx = Arc::new(RwLock::new(DelayedMockContext::new(50)));
        let (addr, shutdown_tx, handle) = start_test_server(ctx.clone()).await;

        let client = reqwest::Client::new();
        let url = format!("http://{}/api/v1/transcribe", addr);
        let audio_data = vec![0u8; 500];

        // Spawn 5 concurrent requests (simulating host + 4 remote clients)
        let mut handles = Vec::new();
        for i in 0..5 {
            let client = client.clone();
            let url = url.clone();
            let audio = audio_data.clone();
            handles.push(tokio::spawn(async move {
                let start = Instant::now();
                let result = client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio)
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await;
                (i, result, start.elapsed())
            }));
        }

        // Wait for all to complete
        let mut successes = 0;
        for handle in handles {
            let (id, result, _duration) = handle.await.expect("Task panicked");
            if let Ok(response) = result {
                if response.status().is_success() {
                    successes += 1;
                } else {
                    println!("Request {} failed with status: {}", id, response.status());
                }
            } else {
                println!("Request {} failed: {:?}", id, result);
            }
        }

        // All 5 should succeed
        assert_eq!(successes, 5, "All 5 requests should succeed");

        // Verify completion count
        let ctx_guard = ctx.read().await;
        assert_eq!(ctx_guard.get_completed_count(), 5);
        assert_eq!(ctx_guard.get_max_concurrent(), 1);

        // Cleanup
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    }

    /// Test: Requests complete in order (FIFO) due to fair mutex
    ///
    /// Verifies that Tokio's mutex provides fair ordering
    #[tokio::test]
    async fn test_request_completion_order_is_fifo() {
        let ctx = Arc::new(RwLock::new(DelayedMockContext::new(30)));
        let (addr, shutdown_tx, handle) = start_test_server(ctx.clone()).await;

        let client = reqwest::Client::new();
        let url = format!("http://{}/api/v1/transcribe", addr);

        // Track completion order
        let completion_order = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        // Send 3 requests with slight delays between them
        let mut handles = Vec::new();
        for i in 0..3 {
            let client = client.clone();
            let url = url.clone();
            let order = completion_order.clone();

            // Stagger request starts slightly to establish arrival order
            sleep(Duration::from_millis(5)).await;

            handles.push(tokio::spawn(async move {
                let audio = vec![0u8; 100];
                let result = client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await;

                if result.is_ok() {
                    order.lock().await.push(i);
                }
                (i, result)
            }));
        }

        // Wait for all to complete
        for handle in handles {
            let _ = handle.await;
        }

        // Check completion order is FIFO
        let order = completion_order.lock().await;
        assert_eq!(order.len(), 3, "All 3 requests should complete");
        assert_eq!(order[0], 0, "First request should complete first");
        assert_eq!(order[1], 1, "Second request should complete second");
        assert_eq!(order[2], 2, "Third request should complete third");

        // Cleanup
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    }

    /// Test: Status endpoint remains responsive during transcription
    ///
    /// Host should be able to check server status even while transcription is running
    #[tokio::test]
    async fn test_status_endpoint_responsive_during_transcription() {
        // Use longer delay to ensure transcription is still running when we check status
        let ctx = Arc::new(RwLock::new(DelayedMockContext::new(200)));
        let (addr, shutdown_tx, handle) = start_test_server(ctx.clone()).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let status_url = format!("http://{}/api/v1/status", addr);

        // Start a transcription request (will take 200ms)
        let client1 = client.clone();
        let transcribe_handle = tokio::spawn(async move {
            client1
                .post(&transcribe_url)
                .header("Content-Type", "audio/wav")
                .body(vec![0u8; 100])
                .timeout(Duration::from_secs(10))
                .send()
                .await
        });

        // Give transcription request time to start
        sleep(Duration::from_millis(50)).await;

        // Check status while transcription is running
        // Note: This will wait for the mutex since both endpoints use the same context
        // But it should still complete within reasonable time
        let status_start = Instant::now();
        let status_result = client
            .get(&status_url)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        let status_elapsed = status_start.elapsed();

        // Status should complete (after waiting for transcription)
        let status_response = status_result.expect("Status request failed");
        assert!(
            status_response.status().is_success(),
            "Status should succeed"
        );

        // Verify status response is valid JSON
        let status_json: serde_json::Value = status_response
            .json()
            .await
            .expect("Failed to parse status JSON");
        assert!(status_json["status"].is_string());
        assert!(status_json["model"].is_string());

        // Transcription should also complete
        let transcribe_result = transcribe_handle.await.expect("Task panicked");
        assert!(transcribe_result
            .expect("Transcription request failed")
            .status()
            .is_success());

        println!(
            "Status request took {:?} (waited for transcription mutex)",
            status_elapsed
        );

        // Cleanup
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    }

    /// Test: Error in one request doesn't affect others
    ///
    /// Simulates one client sending bad data while another sends valid data
    #[tokio::test]
    async fn test_error_isolation_between_requests() {
        let ctx = Arc::new(RwLock::new(DelayedMockContext::new(50)));
        let (addr, shutdown_tx, handle) = start_test_server(ctx.clone()).await;

        let client = reqwest::Client::new();
        let url = format!("http://{}/api/v1/transcribe", addr);

        // Request 1: Invalid content type (should fail)
        let client1 = client.clone();
        let url1 = url.clone();
        let bad_req = tokio::spawn(async move {
            client1
                .post(&url1)
                .header("Content-Type", "text/plain") // Invalid!
                .body(vec![0u8; 100])
                .timeout(Duration::from_secs(10))
                .send()
                .await
        });

        // Request 2: Valid request (should succeed)
        let client2 = client.clone();
        let url2 = url.clone();
        let good_req = tokio::spawn(async move {
            client2
                .post(&url2)
                .header("Content-Type", "audio/wav")
                .body(vec![0u8; 100])
                .timeout(Duration::from_secs(10))
                .send()
                .await
        });

        let (bad_result, good_result) = tokio::join!(bad_req, good_req);

        // Bad request should fail with 415 Unsupported Media Type
        let bad_response = bad_result.expect("Task panicked").expect("Request failed");
        assert_eq!(
            bad_response.status(),
            reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE
        );

        // Good request should succeed
        let good_response = good_result.expect("Task panicked").expect("Request failed");
        assert!(good_response.status().is_success());

        // Cleanup
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    }

    /// Test: Simulated host local + remote client scenario
    ///
    /// This is the exact scenario from Issue #3:
    /// - PC (Windows) running server with GPU
    /// - MacBook connected as remote client
    /// - Both try to transcribe at the same time
    #[tokio::test]
    async fn test_host_local_plus_remote_client_concurrent() {
        // Simulate real transcription time (~500ms for short audio)
        let ctx = Arc::new(RwLock::new(DelayedMockContext::new(100)));
        let (addr, shutdown_tx, handle) = start_test_server(ctx.clone()).await;

        let client = reqwest::Client::new();
        let url = format!("http://{}/api/v1/transcribe", addr);

        // Simulate host local transcription (larger audio)
        let host_audio = vec![0u8; 2000];
        let client_host = client.clone();
        let url_host = url.clone();
        let host_start = Instant::now();
        let host_handle = tokio::spawn(async move {
            client_host
                .post(&url_host)
                .header("Content-Type", "audio/wav")
                .body(host_audio)
                .timeout(Duration::from_secs(30))
                .send()
                .await
        });

        // Small delay to ensure host request arrives first
        sleep(Duration::from_millis(10)).await;

        // Simulate remote client transcription (smaller audio)
        let remote_audio = vec![0u8; 1000];
        let client_remote = client.clone();
        let url_remote = url.clone();
        let remote_start = Instant::now();
        let remote_handle = tokio::spawn(async move {
            client_remote
                .post(&url_remote)
                .header("Content-Type", "audio/wav")
                .body(remote_audio)
                .timeout(Duration::from_secs(30))
                .send()
                .await
        });

        // Wait for both to complete
        let host_result = host_handle.await.expect("Host task panicked");
        let host_elapsed = host_start.elapsed();

        let remote_result = remote_handle.await.expect("Remote task panicked");
        let remote_elapsed = remote_start.elapsed();

        // Both should succeed
        let host_response = host_result.expect("Host request failed");
        let remote_response = remote_result.expect("Remote request failed");

        assert!(
            host_response.status().is_success(),
            "Host transcription should succeed"
        );
        assert!(
            remote_response.status().is_success(),
            "Remote transcription should succeed"
        );

        // Parse responses
        let host_json: serde_json::Value = host_response.json().await.unwrap();
        let remote_json: serde_json::Value = remote_response.json().await.unwrap();

        assert!(host_json["text"].is_string());
        assert!(remote_json["text"].is_string());

        println!("Host transcription completed in {:?}", host_elapsed);
        println!("Remote transcription completed in {:?}", remote_elapsed);

        // Verify serialization occurred
        let ctx_guard = ctx.read().await;
        assert_eq!(ctx_guard.get_completed_count(), 2);
        assert_eq!(ctx_guard.get_max_concurrent(), 1);

        // Cleanup
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;

        println!("✓ Host local + remote client concurrent test passed!");
    }
}
