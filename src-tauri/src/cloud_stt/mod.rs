//! Cloud speech-to-text provider seam.
//!
//! Single source of truth for curated cloud STT providers. Each provider owns
//! its HTTP transcription flow and key-validation in its own submodule; this
//! module exposes a data-driven `CloudProvider` enum used by the model catalog,
//! engine resolution, settings normalization, recognition availability, the
//! tray, and the STT key commands.
//!
//! API keys live in the encrypted secure store under `stt_api_key_<id>`.

mod cohere;
pub(crate) mod common;
mod deepgram;
mod groq;
mod openai;
mod soniox;
// Realtime (WebSocket) streaming path — reachable from the recorder tap factory
// in commands::audio (plan 043).
pub(crate) mod soniox_rt;
pub(crate) mod soniox_ws;
pub(crate) mod deepgram_rt;
pub(crate) mod deepgram_ws;

use crate::transcription::TranscriptionWord;
use std::path::Path;
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudProvider {
    Soniox,
    Openai,
    Groq,
    Deepgram,
    Cohere,
}

/// Transcript returned by a cloud provider, optionally with per-word speaker data.
#[derive(Debug, Clone)]
pub struct CloudTranscript {
    pub text: String,
    pub words: Vec<TranscriptionWord>,
}

impl CloudProvider {
    /// Catalog order: all curated providers.
    pub const ALL: &'static [CloudProvider] = &[
        Self::Soniox,
        Self::Openai,
        Self::Groq,
        Self::Deepgram,
        Self::Cohere,
    ];

    /// Canonical engine/model id used across settings, catalog, and the wire.
    pub fn id(self) -> &'static str {
        match self {
            Self::Soniox => "soniox",
            Self::Openai => "openai",
            Self::Groq => "groq",
            Self::Deepgram => "deepgram",
            Self::Cohere => "cohere",
        }
    }

    /// Resolve a provider from an engine/model id (trimmed, case-insensitive).
    pub fn from_id(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "soniox" => Some(Self::Soniox),
            "openai" => Some(Self::Openai),
            "groq" => Some(Self::Groq),
            "deepgram" => Some(Self::Deepgram),
            "cohere" => Some(Self::Cohere),
            _ => None,
        }
    }

    /// Human-readable provider name (no suffix).
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Soniox => "Soniox",
            Self::Openai => "OpenAI",
            Self::Groq => "Groq",
            Self::Deepgram => "Deepgram",
            Self::Cohere => "Cohere",
        }
    }
    /// Underlying transcription model id used by this provider (single source of truth).
    pub fn model_name(self) -> &'static str {
        match self {
            Self::Soniox => soniox::MODEL,
            Self::Openai => openai::MODEL,
            Self::Groq => groq::MODEL,
            Self::Deepgram => deepgram::MODEL,
            Self::Cohere => cohere::MODEL,
        }
    }

    /// Display label for menus/history, e.g. `Soniox (Cloud)`.
    pub fn cloud_label(self) -> String {
        format!("{} (Cloud)", self.display_name())
    }

    /// Secure-store key under which this provider's API key is persisted.
    pub fn key_name(self) -> &'static str {
        match self {
            Self::Soniox => "stt_api_key_soniox",
            Self::Openai => "stt_api_key_openai",
            Self::Groq => "stt_api_key_groq",
            Self::Deepgram => "stt_api_key_deepgram",
            Self::Cohere => "stt_api_key_cohere",
        }
    }

    /// HTTPS origin whose connection transcription will reuse.
    pub fn base_origin(self) -> &'static str {
        match self {
            Self::Soniox => "https://api.soniox.com",
            Self::Openai => "https://api.openai.com",
            Self::Groq => "https://api.groq.com",
            Self::Deepgram => "https://api.deepgram.com",
            Self::Cohere => "https://api.cohere.com",
        }
    }

    /// Pre-warm the connection so the next transcription reuses a hot pool.
    pub async fn warm_up(self) {
        common::warm_origin(self.base_origin()).await;
    }

    /// Catalog speed hint (0-9, higher = faster).
    pub fn speed_score(self) -> u8 {
        match self {
            Self::Soniox => 8,
            Self::Openai => 7,
            Self::Groq => 9,
            Self::Deepgram => 9,
            Self::Cohere => 6,
        }
    }

    /// Catalog accuracy hint (0-9, higher = better).
    pub fn accuracy_score(self) -> u8 {
        match self {
            Self::Soniox => 9,
            Self::Openai => 9,
            Self::Groq => 8,
            Self::Deepgram => 8,
            Self::Cohere => 8,
        }
    }

    /// Validate an API key against the provider (no persistence).
    pub async fn validate_key(self, api_key: &str) -> Result<(), String> {
        let key = api_key.trim();
        if key.is_empty() {
            return Err("API key cannot be empty".to_string());
        }
        match self {
            Self::Soniox => soniox::validate_key(key).await,
            Self::Openai => openai::validate_key(key).await,
            Self::Groq => groq::validate_key(key).await,
            Self::Deepgram => deepgram::validate_key(key).await,
            Self::Cohere => cohere::validate_key(key).await,
        }
    }

    pub(crate) async fn transcribe_typed(
        self,
        app: &AppHandle,
        api_key: &str,
        audio_path: &Path,
        language: Option<&str>,
    ) -> Result<String, common::SttError> {
        match self {
            Self::Soniox => soniox::transcribe_typed(app, api_key, audio_path, language).await,
            Self::Openai => openai::transcribe_typed(app, api_key, audio_path, language).await,
            Self::Groq => groq::transcribe_typed(app, api_key, audio_path, language).await,
            Self::Deepgram => deepgram::transcribe_typed(app, api_key, audio_path, language).await,
            Self::Cohere => cohere::transcribe_typed(app, api_key, audio_path, language).await,
        }
    }

    /// Transcribe `audio_path` with diarization using the stored API key.
    ///
    /// Providers that support diarization (Deepgram, Soniox) fill `words`; others
    /// return an empty `words` vec and the plain transcript text.
    pub async fn transcribe_diarized(
        self,
        app: &AppHandle,
        audio_path: &Path,
        language: Option<&str>,
    ) -> Result<CloudTranscript, String> {
        let key = crate::secure_store::secure_get(app, self.key_name())?
            .ok_or_else(|| format!("{} API key not set", self.display_name()))?;
        self.transcribe_typed_diarized(app, &key, audio_path, language)
            .await
            .map_err(|e| e.message(self.display_name()))
    }

    pub(crate) async fn transcribe_typed_diarized(
        self,
        app: &AppHandle,
        api_key: &str,
        audio_path: &Path,
        language: Option<&str>,
    ) -> Result<CloudTranscript, common::SttError> {
        match self {
            Self::Deepgram => {
                deepgram::transcribe_typed_diarized(app, api_key, audio_path, language).await
            }
            Self::Soniox => {
                soniox::transcribe_typed_diarized(app, api_key, audio_path, language).await
            }
            _ => {
                let text = self
                    .transcribe_typed(app, api_key, audio_path, language)
                    .await?;
                Ok(CloudTranscript {
                    text,
                    words: vec![],
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_id_round_trips_all_providers() {
        for provider in CloudProvider::ALL {
            assert_eq!(CloudProvider::from_id(provider.id()), Some(*provider));
        }
    }

    #[test]
    fn from_id_is_case_insensitive_and_trims() {
        assert_eq!(
            CloudProvider::from_id("  Deepgram "),
            Some(CloudProvider::Deepgram)
        );
        assert_eq!(
            CloudProvider::from_id("COHERE"),
            Some(CloudProvider::Cohere)
        );
        assert_eq!(CloudProvider::from_id("whisper"), None);
        assert_eq!(CloudProvider::from_id(""), None);
    }

    #[test]
    fn key_names_are_namespaced_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for provider in CloudProvider::ALL {
            assert_eq!(
                provider.key_name(),
                format!("stt_api_key_{}", provider.id())
            );
            assert!(seen.insert(provider.key_name()), "duplicate key name");
        }
    }

    #[test]
    fn base_origin_covers_each_provider() {
        assert_eq!(CloudProvider::Soniox.base_origin(), "https://api.soniox.com");
        assert_eq!(CloudProvider::Openai.base_origin(), "https://api.openai.com");
        assert_eq!(CloudProvider::Groq.base_origin(), "https://api.groq.com");
        assert_eq!(CloudProvider::Deepgram.base_origin(), "https://api.deepgram.com");
        assert_eq!(CloudProvider::Cohere.base_origin(), "https://api.cohere.com");
    }
}
