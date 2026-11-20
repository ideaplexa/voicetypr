//! HTTP integration tests for remote transcription
//!
//! These tests verify the actual HTTP server and client communication
//! without requiring whisper models (uses mock transcription).
//!
//! Test coverage includes:
//! - Route creation and configuration
//! - Status endpoint responses
//! - Transcription endpoint handling
//! - Authentication middleware
//! - Request validation
//! - Error response formatting

use crate::remote::http::{create_routes, ClientActivityMap, ServerContext};
use crate::remote::server::{ErrorResponse, StatusResponse, TranscribeResponse};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::{RwLock, Semaphore};

fn test_transcription_guard() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(1))
}

fn test_client_activity() -> ClientActivityMap {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Test context for mock transcription
struct TestContext {
    model_name: String,
    server_name: String,
    password: Option<String>,
    /// Mock transcription result
    mock_result: String,
}

impl ServerContext for TestContext {
    fn license_cache(
        &self,
    ) -> std::sync::Arc<tokio::sync::RwLock<crate::commands::license::RuntimeLicenseCache>> {
        crate::remote::http::licensed_host_cache()
    }

    fn get_model_name(&self) -> String {
        self.model_name.clone()
    }

    fn get_server_name(&self) -> String {
        self.server_name.clone()
    }

    fn get_password(&self) -> Option<String> {
        self.password.clone()
    }

    fn transcribe(
        &self,
        _audio_data: &[u8],
        _spoken_language: Option<&str>,
        _transcription_task: Option<&str>,
    ) -> Result<crate::transcription::TranscriptionResult, String> {
        let job = crate::transcription::TranscriptionJob::from_legacy_settings(
            crate::transcription::TranscriptionSource::RemoteServer,
            "remote",
            self.model_name.clone(),
            None,
            false,
        );
        Ok(
            crate::transcription::TranscriptionResult::new(&job, self.mock_result.clone())
                .with_processing_duration_ms(Some(1000)),
        )
    }
}

fn create_test_context() -> Arc<RwLock<TestContext>> {
    Arc::new(RwLock::new(TestContext {
        model_name: "test-model".to_string(),
        server_name: "test-server".to_string(),
        password: None,
        mock_result: "Hello, this is a test transcription.".to_string(),
    }))
}

fn create_test_context_with_password(password: &str) -> Arc<RwLock<TestContext>> {
    Arc::new(RwLock::new(TestContext {
        model_name: "test-model".to_string(),
        server_name: "test-server".to_string(),
        password: Some(password.to_string()),
        mock_result: "Hello, this is a test transcription.".to_string(),
    }))
}

/// Test status endpoint returns correct JSON
#[tokio::test]
async fn test_status_endpoint_returns_ok() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);

    let body: StatusResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body.status, "ok");
    assert_eq!(body.model, "test-model");
    assert_eq!(body.name, "test-server");
}

/// Test status endpoint with password protection (no password provided)
#[tokio::test]
async fn test_status_endpoint_requires_password() {
    let ctx = create_test_context_with_password("secret123");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 401);
}

/// Test status endpoint with correct password
#[tokio::test]
async fn test_status_endpoint_accepts_correct_password() {
    let ctx = create_test_context_with_password("secret123");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .header("X-Voicetypr-Key", "secret123")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
}

/// Test status endpoint with wrong password
#[tokio::test]
async fn test_status_endpoint_rejects_wrong_password() {
    let ctx = create_test_context_with_password("secret123");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .header("X-Voicetypr-Key", "wrongpassword")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 401);
}

/// Test transcribe endpoint accepts audio and returns transcription
#[tokio::test]
async fn test_transcribe_endpoint_returns_transcription() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    // Create fake audio data (WAV header + some samples)
    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);

    let body: TranscribeResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body.text, "Hello, this is a test transcription.");
    assert_eq!(body.model, "test-model");
}

/// Test transcribe endpoint requires authentication when password set
#[tokio::test]
async fn test_transcribe_endpoint_requires_password() {
    let ctx = create_test_context_with_password("secret123");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 401);
}

/// Test transcribe endpoint with correct password
#[tokio::test]
async fn test_transcribe_endpoint_accepts_correct_password() {
    let ctx = create_test_context_with_password("secret123");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .header("X-Voicetypr-Key", "secret123")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
}

/// Test transcribe endpoint rejects non-audio content type
#[tokio::test]
async fn test_transcribe_endpoint_rejects_wrong_content_type() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "application/json")
        .body("{\"foo\": \"bar\"}")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 415); // Unsupported Media Type
}

/// Helper to create minimal WAV data for testing
fn create_test_wav_data() -> Vec<u8> {
    // Minimal WAV header (44 bytes) + some samples
    let sample_rate = 16000u32;
    let num_channels = 1u16;
    let bits_per_sample = 16u16;
    let num_samples = 1600u32; // 100ms at 16kHz

    let data_size = num_samples * (bits_per_sample as u32 / 8);
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&num_channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(
        &(sample_rate * num_channels as u32 * bits_per_sample as u32 / 8).to_le_bytes(),
    ); // byte rate
    wav.extend_from_slice(&(num_channels * bits_per_sample / 8).to_le_bytes()); // block align
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    // Samples (silence)
    wav.extend(std::iter::repeat_n(0u8, data_size as usize));

    wav
}

// ============================================================================
// Additional Test Contexts
// ============================================================================

/// Test context that returns an error on transcription
struct FailingContext {
    model_name: String,
    server_name: String,
    password: Option<String>,
    error_message: String,
}

impl ServerContext for FailingContext {
    fn license_cache(
        &self,
    ) -> std::sync::Arc<tokio::sync::RwLock<crate::commands::license::RuntimeLicenseCache>> {
        crate::remote::http::licensed_host_cache()
    }

    fn get_model_name(&self) -> String {
        self.model_name.clone()
    }

    fn get_server_name(&self) -> String {
        self.server_name.clone()
    }

    fn get_password(&self) -> Option<String> {
        self.password.clone()
    }

    fn transcribe(
        &self,
        _audio_data: &[u8],
        _spoken_language: Option<&str>,
        _transcription_task: Option<&str>,
    ) -> Result<crate::transcription::TranscriptionResult, String> {
        Err(self.error_message.clone())
    }
}

fn create_failing_context(error_message: &str) -> Arc<RwLock<FailingContext>> {
    Arc::new(RwLock::new(FailingContext {
        model_name: "test-model".to_string(),
        server_name: "test-server".to_string(),
        password: None,
        error_message: error_message.to_string(),
    }))
}

/// Test context with custom model and server names
fn create_custom_context(model: &str, server: &str) -> Arc<RwLock<TestContext>> {
    Arc::new(RwLock::new(TestContext {
        model_name: model.to_string(),
        server_name: server.to_string(),
        password: None,
        mock_result: "Custom transcription result".to_string(),
    }))
}

// ============================================================================
// Route Creation and Configuration Tests
// ============================================================================

/// Test that routes are created successfully
#[tokio::test]
async fn test_routes_are_created() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    // Status route should work
    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
}

/// Test that unknown routes return 404
#[tokio::test]
async fn test_unknown_route_returns_404() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/unknown")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 404);
}

/// Test that wrong HTTP method returns appropriate error
#[tokio::test]
async fn test_wrong_method_on_status_endpoint() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    // POST to status should fail (it's GET only)
    assert_eq!(response.status(), 405); // Method Not Allowed
}

/// Test that wrong HTTP method on transcribe endpoint returns error
#[tokio::test]
async fn test_wrong_method_on_transcribe_endpoint() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/transcribe")
        .reply(&routes)
        .await;

    // GET to transcribe should fail (it's POST only)
    assert_eq!(response.status(), 405); // Method Not Allowed
}

// ============================================================================
// Status Endpoint Response Tests
// ============================================================================

/// Test status response includes all required fields
#[tokio::test]
async fn test_status_response_has_all_fields() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);

    let body: StatusResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body.status, "ok");
    assert!(!body.version.is_empty());
    assert!(!body.model.is_empty());
    assert!(!body.name.is_empty());
    assert!(!body.machine_id.is_empty());
}

/// Test status response reflects custom model name
#[tokio::test]
async fn test_status_response_custom_model_name() {
    let ctx = create_custom_context("large-v3-turbo", "My Desktop");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    let body: StatusResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body.model, "large-v3-turbo");
    assert_eq!(body.name, "My Desktop");
}

/// Test status response version matches package version
#[tokio::test]
async fn test_status_response_version_is_valid() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    let body: StatusResponse = serde_json::from_slice(response.body()).unwrap();
    // Version should be in semver format (e.g., "1.11.2")
    assert!(body.version.contains('.'), "Version should contain a dot");
}

// ============================================================================
// Transcription Endpoint Handling Tests
// ============================================================================

/// Test transcribe endpoint accepts audio/mpeg content type
#[tokio::test]
async fn test_transcribe_accepts_audio_mpeg() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let audio_data = vec![0u8; 100]; // Minimal audio data

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/mpeg")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
}

/// Test transcribe endpoint accepts audio/webm content type
#[tokio::test]
async fn test_transcribe_accepts_audio_webm() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let audio_data = vec![0u8; 100];

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/webm")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
}

/// Test transcribe response includes model name
#[tokio::test]
async fn test_transcribe_response_includes_model() {
    let ctx = create_custom_context("base.en", "Test Server");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .body(audio_data)
        .reply(&routes)
        .await;

    let body: TranscribeResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body.model, "base.en");
}

/// Test transcribe response includes duration
#[tokio::test]
async fn test_transcribe_response_includes_duration() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .body(audio_data)
        .reply(&routes)
        .await;

    let body: TranscribeResponse = serde_json::from_slice(response.body()).unwrap();
    assert!(body.duration_ms > 0);
}

// ============================================================================
// Authentication Middleware Tests
// ============================================================================

/// Test authentication with empty password string (should still require auth)
#[tokio::test]
async fn test_auth_with_empty_password_string() {
    let ctx = Arc::new(RwLock::new(TestContext {
        model_name: "test-model".to_string(),
        server_name: "test-server".to_string(),
        password: Some("".to_string()), // Empty string password
        mock_result: "Test".to_string(),
    }));
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    // Without header - should fail because empty string is still "some" password
    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 401);

    // With empty string header - still rejected: an empty configured password is
    // treated as fail-closed (no usable secret), so no key can authenticate.
    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .header("X-Voicetypr-Key", "")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 401);
}

/// Test that password is case-sensitive
#[tokio::test]
async fn test_password_is_case_sensitive() {
    let ctx = create_test_context_with_password("Secret123");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    // Lowercase should fail
    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .header("X-Voicetypr-Key", "secret123")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 401);
}

/// Test authentication on transcribe endpoint with wrong password
#[tokio::test]
async fn test_transcribe_auth_wrong_password() {
    let ctx = create_test_context_with_password("correct");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .header("X-Voicetypr-Key", "wrong")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 401);
}

/// Test no password required when password is None
#[tokio::test]
async fn test_no_auth_when_password_none() {
    let ctx = create_test_context(); // password is None
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    // Should work without any auth header
    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
}

// ============================================================================
// Request Validation Tests
// ============================================================================

/// Test transcribe rejects text/plain content type
#[tokio::test]
async fn test_transcribe_rejects_text_plain() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "text/plain")
        .body("not audio data")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 415);
}

/// Test transcribe rejects application/octet-stream content type
#[tokio::test]
async fn test_transcribe_rejects_octet_stream() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "application/octet-stream")
        .body(vec![0u8; 100])
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 415);
}

/// Test transcribe rejects multipart/form-data content type
#[tokio::test]
async fn test_transcribe_rejects_multipart() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "multipart/form-data")
        .body(vec![0u8; 100])
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 415);
}

// ============================================================================
// Error Response Formatting Tests
// ============================================================================

/// Test unauthorized error response format
#[tokio::test]
async fn test_unauthorized_error_format() {
    let ctx = create_test_context_with_password("secret");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 401);

    let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body.error, "unauthorized");
}

/// Test unsupported media type error response format
#[tokio::test]
async fn test_unsupported_media_type_error_format() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "application/json")
        .body("{}")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 415);

    let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body.error, "unsupported_media_type");
}

/// Test transcription failure returns a stable sanitized error response.
#[tokio::test]
async fn test_transcription_error_is_sanitized() {
    let ctx = create_failing_context("Model failed to load");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 500);

    let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body.error, "transcription_failed");
}

/// Test transcription failure does not leak internal paths.
#[tokio::test]
async fn test_transcription_error_does_not_preserve_internal_message() {
    let ctx =
        create_failing_context("/Users/alice/Library/Application Support/models/model.bin failed");
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .body(audio_data)
        .reply(&routes)
        .await;

    let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body.error, "transcription_failed");
}

// ============================================================================
// Content-Type Variations Tests
// ============================================================================

/// Test audio/wav with charset parameter is accepted
#[tokio::test]
async fn test_audio_wav_with_charset_accepted() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav; charset=binary")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
}

/// Test audio/x-wav is accepted (alternative MIME type)
#[tokio::test]
async fn test_audio_x_wav_accepted() {
    let ctx = create_test_context();
    let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/x-wav")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
}

// ============================================================================
// ServerContext Trait Tests
// ============================================================================

/// Test that TestContext correctly implements ServerContext
#[test]
fn test_context_implements_trait() {
    let ctx = TestContext {
        model_name: "test".to_string(),
        server_name: "server".to_string(),
        password: Some("pass".to_string()),
        mock_result: "result".to_string(),
    };

    assert_eq!(ctx.get_model_name(), "test");
    assert_eq!(ctx.get_server_name(), "server");
    assert_eq!(ctx.get_password(), Some("pass".to_string()));

    let result = ctx.transcribe(&[1, 2, 3], None, None).unwrap();
    assert_eq!(result.raw_text, "result");
}

/// Test that FailingContext correctly returns errors
#[test]
fn test_failing_context_returns_error() {
    let ctx = FailingContext {
        model_name: "test".to_string(),
        server_name: "server".to_string(),
        password: None,
        error_message: "Test error".to_string(),
    };

    let result = ctx.transcribe(&[1, 2, 3], None, None);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "Test error");
}
