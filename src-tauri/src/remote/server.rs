//! Remote transcription server
//!
//! HTTP server that allows other Voicetypr instances to use this machine's
//! transcription capabilities.

use serde::{ser::SerializeStruct, Deserialize, Serialize};

pub const REMOTE_PROTOCOL_VERSION: u16 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RemoteCapabilities {
    #[serde(default)]
    pub supports_initial_prompt: bool,
    #[serde(default)]
    pub supports_structured_terms: bool,
    #[serde(default)]
    pub supports_vocabulary_terms: bool,
    #[serde(default)]
    pub accepts_request_context: bool,
    #[serde(default)]
    pub max_context_bytes: u32,
    #[serde(default)]
    pub acceleration: Vec<String>,
}

/// Response from the /api/v1/status endpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub model: String,
    pub name: String,
    /// Unique machine identifier to prevent self-connection
    pub machine_id: String,
    #[serde(default)]
    pub protocol_version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<RemoteCapabilities>,
}

/// Response from the /api/v1/transcribe endpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscribeResponse {
    pub text: String,
    pub duration_ms: u64,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_language: Option<String>,
}

/// Error response for API endpoints
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorResponse {
    pub error: String,
}

/// A host without an active license or trial refuses transcription (403).
pub const HOST_LICENSE_REQUIRED: &str = "host_license_required";
/// The host's license check failed, is pending or needs online verification.
pub const HOST_LICENSE_UNVERIFIED: &str = "host_license_unverified";

/// The host serves transcription on the hotkey's license decision.
pub(crate) fn host_license_refusal(
    state: crate::commands::audio::RecordingLicenseState,
) -> Option<&'static str> {
    use crate::commands::audio::RecordingLicenseState as State;
    match state {
        State::Ready => None,
        State::Blocked => Some(HOST_LICENSE_REQUIRED),
        State::Loading | State::CheckFailed | State::VerificationRequired => {
            Some(HOST_LICENSE_UNVERIFIED)
        }
    }
}

/// What a client shows when the host refuses on its license.
pub fn host_license_refusal_message(code: &str) -> Option<&'static str> {
    match code {
        HOST_LICENSE_REQUIRED => {
            Some("The other computer's Voicetypr has no active license or trial.")
        }
        HOST_LICENSE_UNVERIFIED => Some(
            "The other computer's Voicetypr could not check its license. Open Voicetypr on that computer, check Settings → License, then try again.",
        ),
        _ => None,
    }
}

/// Safe metadata for a shareable local transcription model exposed over remote control.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShareableRemoteModelInfo {
    /// Stable model identifier used for selection.
    pub id: String,
    /// Human-friendly label for UI display.
    pub display_name: String,
    /// Transcription engine backing this model (`whisper` or `parakeet`).
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_score: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accuracy_score: Option<u8>,
}

/// Snapshot of the server's shared model and locally available shareable models.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteModelControlSnapshot {
    pub current: ShareableRemoteModelInfo,
    pub available: Vec<ShareableRemoteModelInfo>,
}

/// Request body for switching the server's shared model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteModelControlUpdate {
    pub model: String,
    pub engine: String,
}

/// Configuration for the remote transcription server
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RemoteServerConfig {
    /// Port to listen on (default: 47842)
    pub port: u16,
    /// Optional password for authentication
    #[serde(default)]
    pub password: Option<String>,
    /// Whether sharing is enabled
    pub enabled: bool,
    /// Whether trusted remote clients may switch the shared model
    #[serde(default)]
    pub allow_model_control: bool,
}

impl Default for RemoteServerConfig {
    fn default() -> Self {
        Self {
            port: 47842,
            password: None,
            enabled: false,
            allow_model_control: false,
        }
    }
}

impl Serialize for RemoteServerConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("RemoteServerConfig", 4)?;
        state.serialize_field("port", &self.port)?;
        state.serialize_field(
            "has_password",
            &self.password.as_ref().is_some_and(|p| !p.is_empty()),
        )?;
        state.serialize_field("enabled", &self.enabled)?;
        state.serialize_field("allow_model_control", &self.allow_model_control)?;
        state.end()
    }
}

#[cfg(test)]
impl RemoteServerConfig {
    /// Validate a password against the configured password
    ///
    /// Returns true if:
    /// - No password is required (self.password is None)
    /// - The provided password matches the configured password
    pub fn validate_password(&self, provided: Option<&str>) -> bool {
        match &self.password {
            None => true, // No password required
            Some(required) => {
                // Password required - check if provided matches
                provided.map(|p| p == required).unwrap_or(false)
            }
        }
    }
}

/// Current status of the remote server
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub enum ServerStatus {
    /// Server is not running
    Idle,
    /// Server is running and accepting connections
    Running { port: u16, connections: usize },
}

#[cfg(test)]
impl ServerStatus {
    /// Check if the server is currently running
    pub fn is_running(&self) -> bool {
        matches!(self, ServerStatus::Running { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RemoteServerConfig::default();
        assert_eq!(config.port, 47842);
        assert!(config.password.is_none());
        assert!(!config.enabled);
        assert!(!config.allow_model_control);
    }

    #[test]
    fn status_response_serializes_current_contract() {
        let response = StatusResponse {
            status: "ok".to_string(),
            version: "1.2.3".to_string(),
            model: "base.en".to_string(),
            name: "desk".to_string(),
            machine_id: "machine".to_string(),
            protocol_version: REMOTE_PROTOCOL_VERSION,
            engine: Some("whisper".to_string()),
            capabilities: Some(RemoteCapabilities {
                supports_initial_prompt: true,
                supports_structured_terms: false,
                supports_vocabulary_terms: false,
                accepts_request_context: true,
                max_context_bytes: 900,
                acceleration: vec!["cpu".to_string()],
            }),
        };

        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["status"], "ok");
        assert_eq!(value["version"], "1.2.3");
        assert_eq!(value["model"], "base.en");
        assert_eq!(value["name"], "desk");
        assert_eq!(value["machine_id"], "machine");
        assert_eq!(value["protocol_version"], REMOTE_PROTOCOL_VERSION);
        assert_eq!(value["engine"], "whisper");
        assert_eq!(value["capabilities"]["supports_initial_prompt"], true);
        assert_eq!(value["capabilities"]["supports_structured_terms"], false);
        assert_eq!(value["capabilities"]["supports_vocabulary_terms"], false);
        assert_eq!(value["capabilities"]["accepts_request_context"], true);
        assert_eq!(value["capabilities"]["max_context_bytes"], 900);
        assert_eq!(value["capabilities"]["acceleration"][0], "cpu");
        assert_eq!(value.as_object().unwrap().len(), 8);
    }

    #[test]
    fn transcribe_response_omits_absent_transcript_language() {
        let value = serde_json::to_value(TranscribeResponse {
            text: "hello".to_string(),
            duration_ms: 42,
            model: "base.en".to_string(),
            transcript_language: None,
        })
        .unwrap();

        assert_eq!(value["text"], "hello");
        assert_eq!(value["duration_ms"], 42);
        assert_eq!(value["model"], "base.en");
        assert!(value.get("transcript_language").is_none());
    }

    #[test]
    fn remote_model_control_snapshot_serializes_expected_shape() {
        let snapshot = RemoteModelControlSnapshot {
            current: ShareableRemoteModelInfo {
                id: "large-v3-turbo".to_string(),
                display_name: "Large v3 Turbo".to_string(),
                engine: "whisper".to_string(),
                recommended: Some(true),
                speed_score: Some(7),
                accuracy_score: Some(9),
            },
            available: vec![ShareableRemoteModelInfo {
                id: "base.en".to_string(),
                display_name: "Base (English)".to_string(),
                engine: "whisper".to_string(),
                recommended: Some(false),
                speed_score: Some(8),
                accuracy_score: Some(5),
            }],
        };

        let value = serde_json::to_value(snapshot).unwrap();
        assert_eq!(value["current"]["id"], "large-v3-turbo");
        assert_eq!(value["current"]["display_name"], "Large v3 Turbo");
        assert_eq!(value["current"]["engine"], "whisper");
        assert_eq!(value["available"][0]["id"], "base.en");
        assert!(value["current"].get("path").is_none());
    }

    #[test]
    fn remote_model_control_update_deserializes() {
        let update: RemoteModelControlUpdate =
            serde_json::from_str(r#"{"model":"base.en","engine":"whisper"}"#).unwrap();
        assert_eq!(update.model, "base.en");
        assert_eq!(update.engine, "whisper");
    }
}
