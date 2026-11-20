//! Remote transcription client
//!
//! HTTP client for connecting to other Voicetypr instances
//! to use their transcription capabilities.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::StatusCode;
use sha2::{Digest, Sha256};

use crate::remote::server::{host_license_refusal_message, ErrorResponse};

/// Upper bound (in bytes) on how much of a `/api/v1/transcribe` response the
/// client buffers into memory before parsing. A real reply is JSON and is many
/// orders of magnitude smaller; this is purely a guard against a hostile or
/// buggy LAN server memory-bombing the client.
const MAX_TRANSCRIBE_RESPONSE_BYTES: u64 = 50 * 1024 * 1024;

/// Source of audio for transcription (affects timeout calculation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionSource {
    /// Live recording from microphone
    LiveRecording,
    /// Uploaded audio/video file
    Upload,
}

/// Connection configuration for a remote Voicetypr server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteServerConnection {
    /// Hostname or IP address
    pub host: String,
    /// Port number
    pub port: u16,
    /// Optional password for authentication
    pub password: Option<String>,
}

impl RemoteServerConnection {
    /// Create a new remote server connection
    pub fn new(host: String, port: u16, password: Option<String>) -> Self {
        Self {
            host,
            port,
            password,
        }
    }

    /// Get the URL for the status endpoint
    pub fn status_url(&self) -> String {
        format!("{}/api/v1/status", format_base_url(&self.host, self.port))
    }

    /// Get the URL for the transcribe endpoint
    pub fn transcribe_url(&self) -> String {
        format!(
            "{}/api/v1/transcribe",
            format_base_url(&self.host, self.port)
        )
    }

    /// Get the URL for the remote model-control endpoint
    pub fn model_control_url(&self) -> String {
        format!(
            "{}/api/v1/control/models",
            format_base_url(&self.host, self.port)
        )
    }

    #[cfg(test)]
    pub fn display_name(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Request to transcribe audio
#[derive(Debug, Clone)]
pub struct TranscriptionRequest {
    /// Raw audio data (WAV format)
    pub audio_data: Vec<u8>,
    #[cfg(test)]
    /// Source of the audio (affects timeout)
    pub source: TranscriptionSource,
    /// Optional spoken language hint for the remote engine.
    pub spoken_language: Option<String>,
    /// Optional transcription task (`transcribe` or `translate_to_english`).
    pub transcription_task: Option<String>,
    /// Optional privacy-preserving request context for engines that advertise support.
    pub context: Option<String>,
}

impl TranscriptionRequest {
    /// Create a new transcription request
    pub fn new(audio_data: Vec<u8>, source: TranscriptionSource) -> Self {
        #[cfg(not(test))]
        let _ = source;
        Self {
            audio_data,
            #[cfg(test)]
            source,
            spoken_language: None,
            transcription_task: None,
            context: None,
        }
    }

    pub fn with_language_and_task(
        mut self,
        spoken_language: Option<String>,
        transcription_task: Option<String>,
    ) -> Self {
        self.spoken_language = spoken_language;
        self.transcription_task = transcription_task;
        self
    }

    pub fn with_context(mut self, context: Option<String>) -> Self {
        self.context = context;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteEndpoint {
    Status,
    Transcribe,
    ModelControl,
}

impl fmt::Display for RemoteEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteEndpoint::Status => f.write_str("status"),
            RemoteEndpoint::Transcribe => f.write_str("transcription"),
            RemoteEndpoint::ModelControl => f.write_str("remote model control"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteClientError {
    AuthFailed {
        endpoint: RemoteEndpoint,
        body: Option<String>,
    },
    /// The host has no active license or trial; `message` is fixed copy.
    HostLicense {
        endpoint: RemoteEndpoint,
        message: &'static str,
    },
    Timeout {
        endpoint: RemoteEndpoint,
        timeout_ms: u64,
        detail: String,
    },
    ConnectFailed {
        endpoint: RemoteEndpoint,
        detail: String,
    },
    HttpStatus {
        endpoint: RemoteEndpoint,
        status: StatusCode,
        body: Option<String>,
    },
    ResponseDecode {
        endpoint: RemoteEndpoint,
        detail: String,
        body: Option<String>,
    },
    ResponseSchema {
        endpoint: RemoteEndpoint,
        detail: String,
        body: Option<String>,
    },
    RequestBuild {
        endpoint: RemoteEndpoint,
        detail: String,
    },
    JoinFailed {
        endpoint: RemoteEndpoint,
        detail: String,
    },
}

impl fmt::Display for RemoteClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthFailed { .. } => f.write_str("Authentication failed"),
            Self::HostLicense { message, .. } => f.write_str(message),
            Self::Timeout {
                timeout_ms, detail, ..
            } => {
                if detail.trim().is_empty() {
                    write!(f, "Request timed out after {}ms", timeout_ms)
                } else {
                    write!(f, "Request timed out after {}ms: {}", timeout_ms, detail)
                }
            }
            Self::ConnectFailed { detail, .. } => write!(f, "Failed to connect: {}", detail),
            Self::HttpStatus { status, .. } => write!(f, "Server error: {}", status),
            Self::ResponseDecode { detail, .. } => {
                write!(f, "Failed to parse response: {}", detail)
            }
            Self::ResponseSchema { detail, .. } => write!(f, "Invalid response: {}", detail),
            Self::RequestBuild { detail, .. } => {
                write!(f, "Failed to create HTTP client: {}", detail)
            }
            Self::JoinFailed { detail, .. } => write!(f, "Task join error: {}", detail),
        }
    }
}

impl std::error::Error for RemoteClientError {}

impl RemoteClientError {
    #[cfg(test)]
    pub fn endpoint(&self) -> RemoteEndpoint {
        match self {
            Self::AuthFailed { endpoint, .. }
            | Self::HostLicense { endpoint, .. }
            | Self::Timeout { endpoint, .. }
            | Self::ConnectFailed { endpoint, .. }
            | Self::HttpStatus { endpoint, .. }
            | Self::ResponseDecode { endpoint, .. }
            | Self::ResponseSchema { endpoint, .. }
            | Self::RequestBuild { endpoint, .. }
            | Self::JoinFailed { endpoint, .. } => *endpoint,
        }
    }

    pub fn is_auth_failure(&self) -> bool {
        matches!(self, Self::AuthFailed { .. })
    }

    #[cfg(test)]
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }

    #[cfg(test)]
    pub fn status_code(&self) -> Option<StatusCode> {
        match self {
            Self::HttpStatus { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn server_error_body(&self) -> Option<&str> {
        match self {
            Self::AuthFailed { body, .. }
            | Self::HttpStatus { body, .. }
            | Self::ResponseDecode { body, .. }
            | Self::ResponseSchema { body, .. } => body.as_deref(),
            _ => None,
        }
    }
}

fn format_base_url(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("http://[{}]:{}", host, port)
    } else {
        format!("http://{}:{}", host, port)
    }
}

fn lossy_body(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Produce a **non-sensitive** diagnostic marker for a server response body.
///
/// `ResponseSchema`/`ResponseDecode` failures can carry the full transcription
/// reply (dictated text) in `body`; surfacing that via `server_error_body()`
/// would leak it into logs and failed-transcription history. Instead we keep
/// only the byte length (a size signal) and a truncated SHA-256 digest (a
/// stable correlation fingerprint) so two identical bad responses still match
/// while the raw content never leaves this function.
fn redact_body(bytes: &[u8]) -> String {
    // Full 64-char digest, truncated to 16 hex chars (8 bytes / 64 bits) below —
    // enough to correlate identical bad responses without revealing content.
    let fingerprint = hex::encode(Sha256::digest(bytes));
    format!(
        "<redacted {} bytes; sha256/{}>",
        bytes.len(),
        &fingerprint[..16]
    )
}

fn classify_reqwest_error(
    endpoint: RemoteEndpoint,
    error: reqwest::Error,
    timeout_ms: u64,
) -> RemoteClientError {
    if error.is_timeout() {
        RemoteClientError::Timeout {
            endpoint,
            timeout_ms,
            detail: error.to_string(),
        }
    } else {
        RemoteClientError::ConnectFailed {
            endpoint,
            detail: error.to_string(),
        }
    }
}

/// Buffer a remote response body, rejecting anything larger than `max_bytes`
/// before it can exhaust client memory.
///
/// A `Content-Length` header advertising more than the cap is rejected up front,
/// before any bytes are read. A body that streams past the cap anyway (chunked
/// transfer with no `Content-Length`, or a lying header) is rejected as soon as
/// the limit is crossed. This guards a hostile or buggy LAN server from
/// memory-bombing the client via an oversized `/api/v1/transcribe` reply.
async fn read_capped_bytes(
    response: reqwest::Response,
    endpoint: RemoteEndpoint,
    max_bytes: u64,
    timeout_ms: u64,
) -> Result<Vec<u8>, RemoteClientError> {
    if let Some(content_length) = response.content_length() {
        if content_length > max_bytes {
            return Err(RemoteClientError::HttpStatus {
                endpoint,
                status: StatusCode::PAYLOAD_TOO_LARGE,
                body: Some(format!(
                    "remote response exceeds {max_bytes} bytes (Content-Length: {content_length})"
                )),
            });
        }
    }

    let mut stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| classify_reqwest_error(endpoint, e, timeout_ms))?;
        buf.extend_from_slice(&chunk);
        if buf.len() as u64 > max_bytes {
            return Err(RemoteClientError::HttpStatus {
                endpoint,
                status: StatusCode::PAYLOAD_TOO_LARGE,
                body: Some(format!(
                    "remote response exceeded {max_bytes} bytes while streaming"
                )),
            });
        }
    }

    Ok(buf)
}

fn parse_json_value<T>(endpoint: RemoteEndpoint, body: &[u8]) -> Result<T, RemoteClientError>
where
    T: serde::de::DeserializeOwned,
{
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| RemoteClientError::ResponseDecode {
            endpoint,
            detail: e.to_string(),
            body: Some(redact_body(body)),
        })?;

    serde_json::from_value(value).map_err(|e| RemoteClientError::ResponseSchema {
        endpoint,
        detail: e.to_string(),
        body: Some(redact_body(body)),
    })
}

/// Calculate timeout in milliseconds based on audio duration and source
///
/// For live recordings:
/// - Base: 30 seconds
/// - Plus: 2x the audio duration
/// - Maximum: 2 minutes (120 seconds)
///
/// For uploads:
/// - Base: 60 seconds
/// - Plus: 3x the audio duration
/// - No maximum (long files need long timeouts)
pub fn calculate_timeout_ms(audio_duration_ms: u64, source: TranscriptionSource) -> u64 {
    match source {
        TranscriptionSource::LiveRecording => {
            // Base 30s + 2x duration, capped at 2 minutes
            let timeout = 30_000u64.saturating_add(audio_duration_ms.saturating_mul(2));
            timeout.min(120_000)
        }
        TranscriptionSource::Upload => {
            // Base 60s + 3x duration, no cap
            60_000u64.saturating_add(audio_duration_ms.saturating_mul(3))
        }
    }
}

fn context_header_value(request: &TranscriptionRequest) -> Option<String> {
    request
        .context
        .as_deref()
        .filter(|context| !context.is_empty())
        .map(|context| BASE64.encode(context.as_bytes()))
}

pub fn timeout_ms_for_wav_file(audio_path: &str, source: TranscriptionSource) -> u64 {
    let base_timeout_ms = calculate_timeout_ms(0, source);

    let reader = match hound::WavReader::open(audio_path) {
        Ok(reader) => reader,
        Err(e) => {
            log::warn!(
                "[Remote Client] Could not inspect audio duration for '{}': {}",
                audio_path,
                e
            );
            return base_timeout_ms;
        }
    };

    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        log::warn!(
            "[Remote Client] Invalid WAV metadata for '{}'; using base timeout",
            audio_path
        );
        return base_timeout_ms;
    }

    let frames = reader.duration() as u64 / spec.channels as u64;
    let duration_ms = frames.saturating_mul(1000) / spec.sample_rate as u64;
    calculate_timeout_ms(duration_ms, source)
}

/// Test connection to a remote server using the Intel-Mac-safe blocking probe path.
pub async fn test_connection(
    connection: &RemoteServerConnection,
) -> Result<crate::remote::server::StatusResponse, RemoteClientError> {
    const STATUS_TIMEOUT_MS: u64 = 10_000;

    let connection = connection.clone();
    let endpoint = RemoteEndpoint::Status;
    let url = connection.status_url();
    let password = connection.password.clone();

    let status = tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(STATUS_TIMEOUT_MS))
            .build()
            .map_err(|e| RemoteClientError::RequestBuild {
                endpoint,
                detail: e.to_string(),
            })?;

        let mut request = client.get(&url);
        if let Some(pwd) = password.as_ref() {
            request = request.header("X-Voicetypr-Key", pwd);
        }

        let response = request
            .send()
            .map_err(|e| classify_reqwest_error(endpoint, e, STATUS_TIMEOUT_MS))?;
        let status = response.status();
        let body = response
            .bytes()
            .map_err(|e| classify_reqwest_error(endpoint, e, STATUS_TIMEOUT_MS))?;

        if status == StatusCode::UNAUTHORIZED {
            return Err(RemoteClientError::AuthFailed {
                endpoint,
                body: Some(lossy_body(body.as_ref())),
            });
        }

        if !status.is_success() {
            return Err(RemoteClientError::HttpStatus {
                endpoint,
                status,
                body: Some(lossy_body(body.as_ref())),
            });
        }

        parse_json_value(endpoint, body.as_ref())
    })
    .await
    .map_err(|e| RemoteClientError::JoinFailed {
        endpoint,
        detail: e.to_string(),
    })??;

    Ok(status)
}

pub async fn get_model_control(
    connection: &RemoteServerConnection,
) -> Result<crate::remote::server::RemoteModelControlSnapshot, RemoteClientError> {
    const CONTROL_TIMEOUT_MS: u64 = 10_000;
    let endpoint = RemoteEndpoint::ModelControl;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(CONTROL_TIMEOUT_MS))
        .build()
        .map_err(|e| RemoteClientError::RequestBuild {
            endpoint,
            detail: e.to_string(),
        })?;

    let mut request = client.get(connection.model_control_url());
    if let Some(password) = connection.password.as_ref() {
        request = request.header("X-Voicetypr-Key", password);
    }

    let response = request
        .send()
        .await
        .map_err(|e| classify_reqwest_error(endpoint, e, CONTROL_TIMEOUT_MS))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|e| classify_reqwest_error(endpoint, e, CONTROL_TIMEOUT_MS))?;

    if status == StatusCode::UNAUTHORIZED {
        return Err(RemoteClientError::AuthFailed {
            endpoint,
            body: Some(lossy_body(body.as_ref())),
        });
    }
    if !status.is_success() {
        return Err(RemoteClientError::HttpStatus {
            endpoint,
            status,
            body: Some(lossy_body(body.as_ref())),
        });
    }

    parse_json_value(endpoint, body.as_ref())
}

pub async fn update_model_control(
    connection: &RemoteServerConnection,
    update: crate::remote::server::RemoteModelControlUpdate,
) -> Result<crate::remote::server::RemoteModelControlSnapshot, RemoteClientError> {
    const CONTROL_TIMEOUT_MS: u64 = 10_000;
    let endpoint = RemoteEndpoint::ModelControl;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(CONTROL_TIMEOUT_MS))
        .build()
        .map_err(|e| RemoteClientError::RequestBuild {
            endpoint,
            detail: e.to_string(),
        })?;

    let mut request = client.patch(connection.model_control_url()).json(&update);
    if let Some(password) = connection.password.as_ref() {
        request = request.header("X-Voicetypr-Key", password);
    }

    let response = request
        .send()
        .await
        .map_err(|e| classify_reqwest_error(endpoint, e, CONTROL_TIMEOUT_MS))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|e| classify_reqwest_error(endpoint, e, CONTROL_TIMEOUT_MS))?;

    if status == StatusCode::UNAUTHORIZED {
        return Err(RemoteClientError::AuthFailed {
            endpoint,
            body: Some(lossy_body(body.as_ref())),
        });
    }
    if !status.is_success() {
        return Err(RemoteClientError::HttpStatus {
            endpoint,
            status,
            body: Some(lossy_body(body.as_ref())),
        });
    }

    parse_json_value(endpoint, body.as_ref())
}

/// Submit a remote transcription request with a caller-provided timeout.
pub async fn transcribe_audio(
    connection: &RemoteServerConnection,
    request: TranscriptionRequest,
    timeout_ms: u64,
) -> Result<crate::remote::server::TranscribeResponse, RemoteClientError> {
    let endpoint = RemoteEndpoint::Transcribe;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| RemoteClientError::RequestBuild {
            endpoint,
            detail: e.to_string(),
        })?;

    let mut request_builder = client
        .post(connection.transcribe_url())
        .header("Content-Type", "audio/wav");

    if let Some(pwd) = connection.password.as_ref() {
        request_builder = request_builder.header("X-Voicetypr-Key", pwd);
    }
    if let Some(spoken_language) = request.spoken_language.as_deref() {
        request_builder = request_builder.header("X-Voicetypr-Speech-Language", spoken_language);
    }
    if let Some(transcription_task) = request.transcription_task.as_deref() {
        request_builder =
            request_builder.header("X-Voicetypr-Transcription-Task", transcription_task);
    }
    if let Some(context) = context_header_value(&request) {
        request_builder = request_builder.header("X-Voicetypr-Context", context);
    }

    // Move the audio body in last so the header borrows above stay valid.
    let request_builder = request_builder.body(request.audio_data);

    let response = request_builder
        .send()
        .await
        .map_err(|e| classify_reqwest_error(endpoint, e, timeout_ms))?;
    let status = response.status();
    let body = read_capped_bytes(
        response,
        endpoint,
        MAX_TRANSCRIBE_RESPONSE_BYTES,
        timeout_ms,
    )
    .await?;

    if status == StatusCode::UNAUTHORIZED {
        return Err(RemoteClientError::AuthFailed {
            endpoint,
            body: Some("Remote authentication failed".to_string()),
        });
    }

    if status == StatusCode::FORBIDDEN {
        if let Some(message) = serde_json::from_slice::<ErrorResponse>(body.as_ref())
            .ok()
            .and_then(|error| host_license_refusal_message(&error.error))
        {
            return Err(RemoteClientError::HostLicense { endpoint, message });
        }
    }

    if !status.is_success() {
        return Err(RemoteClientError::HttpStatus {
            endpoint,
            status,
            body: Some("Remote transcription request failed".to_string()),
        });
    }

    parse_json_value(endpoint, body.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_urls() {
        let conn = RemoteServerConnection::new("localhost".to_string(), 47842, None);
        assert!(conn.status_url().contains("/api/v1/status"));
        assert!(conn.transcribe_url().contains("/api/v1/transcribe"));
    }

    #[test]
    fn transcription_request_defaults_to_audio_only() {
        let request = TranscriptionRequest::new(vec![1, 2, 3], TranscriptionSource::Upload);

        assert_eq!(request.audio_data, vec![1, 2, 3]);
        assert_eq!(request.source, TranscriptionSource::Upload);
        assert!(request.spoken_language.is_none());
        assert!(request.transcription_task.is_none());
        assert!(request.context.is_none());
    }

    #[test]
    fn transcription_request_context_header_is_optional() {
        let request = TranscriptionRequest::new(vec![1], TranscriptionSource::LiveRecording)
            .with_context(Some("project terms".to_string()));

        assert_eq!(request.context.as_deref(), Some("project terms"));
        assert_eq!(
            context_header_value(&request),
            Some(BASE64.encode("project terms".as_bytes()))
        );

        let empty_context = TranscriptionRequest::new(vec![1], TranscriptionSource::LiveRecording)
            .with_context(Some(String::new()));
        assert_eq!(context_header_value(&empty_context), None);

        let no_context = TranscriptionRequest::new(vec![1], TranscriptionSource::LiveRecording)
            .with_context(None);
        assert_eq!(context_header_value(&no_context), None);
    }

    #[test]
    fn transcription_request_carries_language_and_task_headers() {
        let request = TranscriptionRequest::new(vec![1], TranscriptionSource::LiveRecording)
            .with_language_and_task(
                Some("en".to_string()),
                Some("translate_to_english".to_string()),
            );

        assert_eq!(request.spoken_language.as_deref(), Some("en"));
        assert_eq!(
            request.transcription_task.as_deref(),
            Some("translate_to_english")
        );
    }

    #[test]
    fn redact_body_never_exposes_raw_content() {
        let secret = "super secret dictated transcript";
        let marker = redact_body(secret.as_bytes());

        assert!(
            !marker.contains(secret),
            "raw content leaked into marker: {marker}"
        );
        assert!(
            marker.contains(&secret.len().to_string()),
            "byte length missing: {marker}"
        );
        assert!(
            marker.contains("sha256/"),
            "hash fingerprint missing: {marker}"
        );
        // Deterministic for identical input.
        assert_eq!(redact_body(secret.as_bytes()), marker);
        // Distinct content yields a distinct fingerprint.
        assert_ne!(redact_body(b"totally different content"), marker);
    }

    #[test]
    fn parse_json_value_redacts_response_schema_body() {
        // Valid JSON that does NOT match the expected schema: the server returned
        // a transcription-shaped body we cannot deserialize. Previously the whole
        // body (which may hold dictated text) was captured verbatim into the
        // error and leaked via server_error_body(); it must now be redacted.
        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        struct TranscribeShape {
            text: String,
        }

        let secret = "the quick brown fox";
        let body = format!(r#"{{"not_text":"{secret}"}}"#); // valid JSON, missing `text`
        let err = parse_json_value::<TranscribeShape>(RemoteEndpoint::Transcribe, body.as_bytes())
            .expect_err("schema mismatch should error");

        let leaked = err.server_error_body().unwrap_or_default();
        assert!(!leaked.contains(secret), "raw transcript leaked: {leaked}");
        assert!(
            leaked.starts_with("<redacted"),
            "expected redacted marker: {leaked}"
        );
    }

    #[test]
    fn parse_json_value_redacts_response_decode_body() {
        let secret = "not-valid-json-body-with-credentials";
        let err =
            parse_json_value::<serde_json::Value>(RemoteEndpoint::Transcribe, secret.as_bytes())
                .expect_err("invalid JSON should error");

        let leaked = err.server_error_body().unwrap_or_default();
        assert!(!leaked.contains(secret), "raw body leaked: {leaked}");
        assert!(
            leaked.starts_with("<redacted"),
            "expected redacted marker: {leaked}"
        );
    }

    #[tokio::test]
    async fn read_capped_bytes_rejects_response_above_limit() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/transcribe"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 64]))
            .mount(&server)
            .await;

        let response = reqwest::Client::new()
            .post(format!("{}/api/v1/transcribe", server.uri()))
            .send()
            .await
            .expect("mock request should succeed");

        let err = read_capped_bytes(response, RemoteEndpoint::Transcribe, 8, 1_000)
            .await
            .expect_err("oversized response must be rejected before buffering");
        assert_eq!(err.status_code(), Some(StatusCode::PAYLOAD_TOO_LARGE));
    }

    #[tokio::test]
    async fn read_capped_bytes_returns_body_within_limit() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let payload = br#"{"text":"hello"}"#.to_vec();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/transcribe"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(payload.clone()))
            .mount(&server)
            .await;

        let response = reqwest::Client::new()
            .post(format!("{}/api/v1/transcribe", server.uri()))
            .send()
            .await
            .expect("mock request should succeed");

        let body = read_capped_bytes(response, RemoteEndpoint::Transcribe, 1024, 1_000)
            .await
            .expect("body within cap should be returned");
        assert_eq!(body, payload);
    }
}
