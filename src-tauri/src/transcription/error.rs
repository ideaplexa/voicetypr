use crate::cloud_stt::common::{is_transient, SttError};
use crate::remote::client::RemoteClientError;
use crate::transcription::TranscriptionSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionErrorCode {
    ModelUnavailable,
    EngineUnavailable,
    EngineFailed,
    AudioInvalid,
    Cancelled,
    Timeout,
    TransportFailed,
    StorageLimitExceeded,
    Unauthorized,
    UnsupportedMediaType,
    ResponseInvalid,
    Internal,
}

#[derive(Debug, Clone)]
pub struct TranscriptionError {
    pub code: TranscriptionErrorCode,
    pub retryable: bool,
    pub user_message: String,
    /// Log/history-only. NEVER serialize to remote clients.
    pub detail: Option<String>,
    pub source: TranscriptionSource,
}

#[derive(Serialize)]
pub struct PublicTranscriptionError<'a> {
    pub code: TranscriptionErrorCode,
    pub retryable: bool,
    pub user_message: &'a str,
}

impl TranscriptionError {
    pub fn new(
        code: TranscriptionErrorCode,
        source: TranscriptionSource,
        user_message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            retryable: default_retryable(code),
            user_message: user_message.into(),
            detail: None,
            source,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn public(&self) -> PublicTranscriptionError<'_> {
        PublicTranscriptionError {
            code: self.code,
            retryable: self.retryable,
            user_message: &self.user_message,
        }
    }
}

pub fn from_remote_client_error(
    err: &RemoteClientError,
    source: TranscriptionSource,
) -> TranscriptionError {
    use RemoteClientError as E;

    let code = match err {
        E::AuthFailed { .. } => TranscriptionErrorCode::Unauthorized,
        E::Timeout { .. } => TranscriptionErrorCode::Timeout,
        // Per the shared error contract, every remaining remote-client failure
        // (connect, HTTP status, response decode/schema, request build, task
        // join) is a transport failure (design doc line 171).
        E::ConnectFailed { .. }
        | E::HttpStatus { .. }
        | E::ResponseDecode { .. }
        | E::ResponseSchema { .. }
        | E::RequestBuild { .. }
        | E::JoinFailed { .. } => TranscriptionErrorCode::TransportFailed,
    };

    TranscriptionError::new(code, source, user_message_for_code(code))
        .with_detail(remote_detail(err))
}

pub(crate) fn from_stt_error(err: &SttError, source: TranscriptionSource) -> TranscriptionError {
    use SttError as E;

    let code = match err {
        E::Auth => TranscriptionErrorCode::Unauthorized,
        E::ModelUnavailable => TranscriptionErrorCode::ModelUnavailable,
        E::RateLimited | E::Network => TranscriptionErrorCode::TransportFailed,
        E::LimitExceeded { .. } => TranscriptionErrorCode::StorageLimitExceeded,
        E::Timeout => TranscriptionErrorCode::Timeout,
        E::Server => TranscriptionErrorCode::EngineFailed,
        E::BadResponse => TranscriptionErrorCode::ResponseInvalid,
    };

    let mut mapped = TranscriptionError::new(code, source, user_message_for_code(code))
        .with_detail(err.message("Cloud STT"));
    mapped.retryable = is_transient(err);
    mapped
}

pub fn from_local_engine_string(raw: &str, source: TranscriptionSource) -> TranscriptionError {
    let lower = raw.to_ascii_lowercase();
    let code = if lower.contains("cancelled") {
        TranscriptionErrorCode::Cancelled
    } else if lower.contains("too short") {
        TranscriptionErrorCode::AudioInvalid
    } else if lower.contains("timed out") {
        TranscriptionErrorCode::Timeout
    } else if lower.contains("no speech recognition models") || lower.contains("downloaded") {
        // "downloaded" catches the genuine model-unavailable runtime strings
        // ("... is not downloaded", "no model downloaded", "not found or not downloaded")
        // WITHOUT re-catching transient failures like "failed to load model tensors".
        TranscriptionErrorCode::ModelUnavailable
    } else {
        TranscriptionErrorCode::EngineFailed
    };

    TranscriptionError::new(code, source, user_message_for_code(code)).with_detail(raw)
}

fn default_retryable(code: TranscriptionErrorCode) -> bool {
    matches!(
        code,
        TranscriptionErrorCode::EngineFailed
            | TranscriptionErrorCode::Timeout
            | TranscriptionErrorCode::TransportFailed
            | TranscriptionErrorCode::ResponseInvalid
    )
}

fn user_message_for_code(code: TranscriptionErrorCode) -> &'static str {
    match code {
        TranscriptionErrorCode::ModelUnavailable => {
            "The selected transcription model is unavailable."
        }
        TranscriptionErrorCode::EngineUnavailable => {
            "The selected transcription engine is unavailable."
        }
        TranscriptionErrorCode::EngineFailed => "Transcription failed. Please try again.",
        TranscriptionErrorCode::AudioInvalid => "The audio could not be transcribed.",
        TranscriptionErrorCode::Cancelled => "Transcription was cancelled.",
        TranscriptionErrorCode::Timeout => "Transcription timed out. Please try again.",
        TranscriptionErrorCode::TransportFailed => {
            "Could not reach the transcription service. Please try again."
        }
        TranscriptionErrorCode::Unauthorized => {
            "Authentication failed for the transcription service."
        }
        TranscriptionErrorCode::StorageLimitExceeded => {
            "Cloud storage limit reached. Delete stored files (Sources → Cloud → Clean up stored files, or your provider's console) and try again."
        }
        TranscriptionErrorCode::UnsupportedMediaType => "This audio format is not supported.",
        TranscriptionErrorCode::ResponseInvalid => {
            "The transcription service returned an invalid response."
        }
        TranscriptionErrorCode::Internal => "An internal transcription error occurred.",
    }
}

fn remote_detail(err: &RemoteClientError) -> String {
    match err.server_error_body() {
        Some(body) => format!("{}\nserver body: {}", err, body),
        None => err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::client::RemoteEndpoint;
    use reqwest::StatusCode;

    const SOURCE: TranscriptionSource = TranscriptionSource::DesktopRecording;

    fn assert_error(error: TranscriptionError, code: TranscriptionErrorCode, retryable: bool) {
        assert_eq!(error.code, code);
        assert_eq!(error.retryable, retryable);
    }

    #[test]
    fn remote_client_errors_map_to_contract_codes() {
        let endpoint = RemoteEndpoint::Transcribe;
        let cases = [
            (
                RemoteClientError::AuthFailed {
                    endpoint,
                    body: Some("secret auth body".to_string()),
                },
                TranscriptionErrorCode::Unauthorized,
                false,
            ),
            (
                RemoteClientError::Timeout {
                    endpoint,
                    timeout_ms: 10,
                    detail: "deadline".to_string(),
                },
                TranscriptionErrorCode::Timeout,
                true,
            ),
            (
                RemoteClientError::ConnectFailed {
                    endpoint,
                    detail: "refused".to_string(),
                },
                TranscriptionErrorCode::TransportFailed,
                true,
            ),
            (
                RemoteClientError::HttpStatus {
                    endpoint,
                    status: StatusCode::BAD_GATEWAY,
                    body: Some("upstream".to_string()),
                },
                TranscriptionErrorCode::TransportFailed,
                true,
            ),
            (
                RemoteClientError::ResponseDecode {
                    endpoint,
                    detail: "json".to_string(),
                    body: Some("not json".to_string()),
                },
                TranscriptionErrorCode::TransportFailed,
                true,
            ),
            (
                RemoteClientError::ResponseSchema {
                    endpoint,
                    detail: "missing text".to_string(),
                    body: None,
                },
                TranscriptionErrorCode::TransportFailed,
                true,
            ),
            (
                RemoteClientError::RequestBuild {
                    endpoint,
                    detail: "client".to_string(),
                },
                TranscriptionErrorCode::TransportFailed,
                true,
            ),
            (
                RemoteClientError::JoinFailed {
                    endpoint,
                    detail: "panic".to_string(),
                },
                TranscriptionErrorCode::TransportFailed,
                true,
            ),
        ];

        for (err, code, retryable) in cases {
            assert_error(from_remote_client_error(&err, SOURCE), code, retryable);
        }
    }

    #[test]
    fn stt_errors_map_to_contract_codes() {
        let cases = [
            (SttError::Auth, TranscriptionErrorCode::Unauthorized, false),
            (
                SttError::ModelUnavailable,
                TranscriptionErrorCode::ModelUnavailable,
                false,
            ),
            (
                SttError::RateLimited,
                TranscriptionErrorCode::TransportFailed,
                true,
            ),
            (
                SttError::LimitExceeded { file_storage: true },
                TranscriptionErrorCode::StorageLimitExceeded,
                false,
            ),
            (
                SttError::Network,
                TranscriptionErrorCode::TransportFailed,
                true,
            ),
            (SttError::Server, TranscriptionErrorCode::EngineFailed, true),
            (
                SttError::BadResponse,
                TranscriptionErrorCode::ResponseInvalid,
                false,
            ),
        ];

        for (err, code, retryable) in cases {
            assert_error(from_stt_error(&err, SOURCE), code, retryable);
        }
    }

    #[test]
    fn local_engine_strings_map_marker_table() {
        let cases = [
            (
                "Transcription cancelled by user",
                TranscriptionErrorCode::Cancelled,
                false,
            ),
            (
                "Audio is too short to transcribe",
                TranscriptionErrorCode::AudioInvalid,
                false,
            ),
            ("Whisper timed out", TranscriptionErrorCode::Timeout, true),
            (
                "No speech recognition models are installed",
                TranscriptionErrorCode::ModelUnavailable,
                false,
            ),
            (
                "model download required",
                TranscriptionErrorCode::EngineFailed,
                true,
            ),
            ("decoder failed", TranscriptionErrorCode::EngineFailed, true),
        ];

        for (raw, code, retryable) in cases {
            assert_error(from_local_engine_string(raw, SOURCE), code, retryable);
        }
    }

    #[test]
    fn not_downloaded_model_is_non_retryable_model_unavailable() {
        let error = from_local_engine_string("Parakeet model is not downloaded", SOURCE);
        assert_eq!(error.code, TranscriptionErrorCode::ModelUnavailable);
        assert!(!error.retryable);
    }

    #[test]
    fn transient_local_model_word_is_retryable_engine_failure() {
        let error = from_local_engine_string("failed to load model tensors", SOURCE);

        assert_eq!(error.code, TranscriptionErrorCode::EngineFailed);
        assert!(error.retryable);
        assert_eq!(
            error.detail.as_deref(),
            Some("failed to load model tensors")
        );
    }

    #[test]
    fn public_error_serialization_drops_detail_and_body() {
        let secret = "secret body token";
        let err = RemoteClientError::AuthFailed {
            endpoint: RemoteEndpoint::Transcribe,
            body: Some(secret.to_string()),
        };
        let mapped = from_remote_client_error(&err, SOURCE);

        assert_eq!(mapped.code, TranscriptionErrorCode::Unauthorized);
        assert_ne!(mapped.user_message, secret);
        assert!(!mapped.user_message.contains(secret));
        assert!(mapped
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains(secret)));

        let json = serde_json::to_string(&mapped.public()).expect("serialize public error");
        assert!(!json.contains("detail"));
        assert!(!json.contains(secret));
        assert!(json.contains("unauthorized"));
    }
}
