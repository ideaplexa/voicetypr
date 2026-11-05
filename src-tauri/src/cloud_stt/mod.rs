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
mod soniox_tokens;
// Realtime (WebSocket) streaming path — reachable from the recorder tap factory
// in commands::audio (plan 043).
pub(crate) mod deepgram_rt;
pub(crate) mod deepgram_ws;
pub(crate) mod soniox_rt;
pub(crate) mod soniox_ws;
pub(crate) use soniox::{cleanup_stored, storage_counts, SonioxCleanupResult, SonioxStorageCounts};

use crate::transcription::TranscriptionWord;
use std::collections::HashMap;
use std::path::Path;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

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
    /// True only when the adapter actually sent the style hint in its prompt.
    pub(crate) whisper_style_hint_sent: bool,
}

impl From<String> for CloudTranscript {
    fn from(text: String) -> Self {
        Self {
            text,
            words: vec![],
            whisper_style_hint_sent: false,
        }
    }
}

/// Settings-store key for the per-provider selected API model map.
pub const CLOUD_STT_MODELS_BY_PROVIDER_KEY: &str = "cloud_stt_models_by_provider";

/// Curated cloud STT model exposed to the UI and used at request time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct CloudSttModel {
    pub id: &'static str,
    pub display_name: &'static str,
}

const SONIOX_MODELS: &[CloudSttModel] = &[CloudSttModel {
    id: "stt-async-v6",
    display_name: "Soniox v6",
}];

const OPENAI_MODELS: &[CloudSttModel] = &[
    CloudSttModel {
        id: "gpt-transcribe",
        display_name: "GPT Transcribe",
    },
    CloudSttModel {
        id: "gpt-4o-mini-transcribe",
        display_name: "GPT-4o mini Transcribe",
    },
];

const GROQ_MODELS: &[CloudSttModel] = &[
    CloudSttModel {
        id: "whisper-large-v3-turbo",
        display_name: "Whisper Large v3 Turbo",
    },
    CloudSttModel {
        id: "whisper-large-v3",
        display_name: "Whisper Large v3",
    },
];

const DEEPGRAM_MODELS: &[CloudSttModel] = &[
    CloudSttModel {
        id: "nova-3",
        display_name: "Nova 3",
    },
    CloudSttModel {
        id: "nova-2",
        display_name: "Nova 2",
    },
];

const COHERE_MODELS: &[CloudSttModel] = &[CloudSttModel {
    id: "cohere-transcribe-03-2026",
    display_name: "Cohere Transcribe",
}];

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

    /// Curated API models for this provider. First entry is the default.
    pub fn available_models(self) -> &'static [CloudSttModel] {
        match self {
            Self::Soniox => SONIOX_MODELS,
            Self::Openai => OPENAI_MODELS,
            Self::Groq => GROQ_MODELS,
            Self::Deepgram => DEEPGRAM_MODELS,
            Self::Cohere => COHERE_MODELS,
        }
    }

    /// First catalog entry; used when nothing is stored or the stored id is invalid.
    pub fn default_model(self) -> &'static CloudSttModel {
        &self.available_models()[0]
    }

    /// Catalog lookup, accepting the retired Soniox v5 id as a v6 alias.
    /// Returning the canonical entry also makes stale settings saves write v6.
    pub fn model_by_id(self, model_id: &str) -> Option<&'static CloudSttModel> {
        let model_id = if self == Self::Soniox && model_id == "stt-async-v5" {
            "stt-async-v6"
        } else {
            model_id
        };
        self.available_models()
            .iter()
            .find(|model| model.id == model_id)
    }

    /// Accept only catalog ids; missing or invalid legacy values fall back to the default.
    pub fn resolve_model_id(self, stored: Option<&str>) -> &'static str {
        stored
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .and_then(|id| self.model_by_id(id))
            .map(|model| model.id)
            .unwrap_or_else(|| self.default_model().id)
    }

    /// A concrete API model is pinned. Provider aliases still follow Settings.
    pub(crate) fn execution_model_id(
        self,
        requested: &str,
        selected: impl FnOnce() -> &'static str,
    ) -> &'static str {
        self.model_by_id(requested).map_or_else(selected, |m| m.id)
    }

    /// Soniox has one API model. Report its executed id, not the caller's
    /// provider shorthand or retired alias; other providers retain their ids.
    pub fn canonical_model_id(self, requested: &str) -> &str {
        if self == Self::Soniox {
            self.resolve_model_id(Some(requested))
        } else {
            requested
        }
    }

    /// Selected API model for this provider from `cloud_stt_models_by_provider`.
    pub fn selected_model(self, app: &AppHandle) -> &'static CloudSttModel {
        let stored = read_stored_cloud_stt_model_id(app, self.id());
        self.model_by_id(self.resolve_model_id(stored.as_deref()))
            .unwrap_or_else(|| self.default_model())
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
        model: &str,
        audio_path: &Path,
        language: Option<&str>,
    ) -> Result<CloudTranscript, common::SttError> {
        match self {
            Self::Soniox => {
                let model = self.resolve_model_id(Some(model));
                let result =
                    soniox::transcribe_typed(app, api_key, model, audio_path, language).await;
                if result.is_ok() {
                    log::debug!("provider=Soniox transport=rest model={model} finished=true");
                }
                result.map(CloudTranscript::from)
            }
            Self::Openai => {
                openai::transcribe_typed(app, api_key, model, audio_path, language).await
            }
            Self::Groq => groq::transcribe_typed(app, api_key, model, audio_path, language).await,
            Self::Deepgram => deepgram::transcribe_typed(app, api_key, model, audio_path, language)
                .await
                .map(CloudTranscript::from),
            Self::Cohere => cohere::transcribe_typed(app, api_key, model, audio_path, language)
                .await
                .map(CloudTranscript::from),
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
        let model = self.selected_model(app).id;
        match self {
            Self::Deepgram => {
                deepgram::transcribe_typed_diarized(app, api_key, model, audio_path, language).await
            }
            Self::Soniox => {
                soniox::transcribe_typed_diarized(app, api_key, model, audio_path, language).await
            }
            _ => {
                self.transcribe_typed(app, api_key, model, audio_path, language)
                    .await
            }
        }
    }
}

pub(crate) fn stored_models_by_provider<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
) -> HashMap<String, String> {
    store
        .get(CLOUD_STT_MODELS_BY_PROVIDER_KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn read_stored_cloud_stt_model_id(app: &AppHandle, provider_id: &str) -> Option<String> {
    let store = app.store("settings").ok()?;
    stored_models_by_provider(&store).get(provider_id).cloned()
}

/// Migrate only the selected API model, never historical transcription records.
/// Provider selections (`current_model` / engine) remain the stable id `soniox`.
pub(crate) fn migrate_soniox_model<R: tauri::Runtime>(store: &tauri_plugin_store::Store<R>) {
    let Some(mut models) = store.get(CLOUD_STT_MODELS_BY_PROVIDER_KEY) else {
        return;
    };
    if models
        .get("soniox")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        == Some("stt-async-v5")
    {
        models["soniox"] = serde_json::json!("stt-async-v6");
        store.set(CLOUD_STT_MODELS_BY_PROVIDER_KEY, models);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soniox_v5_selection_resolves_to_v6() {
        let soniox = CloudProvider::Soniox;
        assert_eq!(
            soniox.resolve_model_id(Some("stt-async-v5")),
            "stt-async-v6"
        );
        assert_eq!(
            soniox.resolve_model_id(Some(" stt-async-v5 ")),
            "stt-async-v6"
        );
        assert_eq!(
            soniox.model_by_id("stt-async-v5"),
            soniox.model_by_id("stt-async-v6")
        );
        assert!(CloudProvider::Openai.model_by_id("stt-async-v5").is_none());
    }

    #[tokio::test]
    async fn settings_save_migrates_soniox_even_when_another_provider_is_active() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let store = tauri_plugin_store::StoreBuilder::new(app.handle(), &path)
            .build()
            .unwrap();
        store.set("current_model", serde_json::json!("openai"));
        store.set("current_model_engine", serde_json::json!("openai"));
        store.set(
            CLOUD_STT_MODELS_BY_PROVIDER_KEY,
            serde_json::json!({
                "soniox": "stt-async-v5", "openai": "gpt-4o-mini-transcribe", "future": "custom"
            }),
        );
        let history_path = dir.path().join("history.json");
        let history = br#"[{"model":"stt-async-v5","text":"historical"}]"#;
        std::fs::write(&history_path, history).unwrap();
        assert_eq!(
            CloudProvider::Soniox.resolve_model_id(
                stored_models_by_provider(&store)
                    .get("soniox")
                    .map(String::as_str)
            ),
            "stt-async-v6"
        );
        // Reading the old selection does not mutate the store.
        assert_eq!(
            store.get(CLOUD_STT_MODELS_BY_PROVIDER_KEY).unwrap()["soniox"],
            "stt-async-v5"
        );
        crate::commands::settings::persist_generic_settings(&store)
            .await
            .unwrap();
        crate::commands::settings::persist_generic_settings(&store)
            .await
            .unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            saved[CLOUD_STT_MODELS_BY_PROVIDER_KEY],
            serde_json::json!({
                "soniox": "stt-async-v6", "openai": "gpt-4o-mini-transcribe", "future": "custom"
            })
        );
        assert_eq!(saved["current_model"], "openai");
        assert_eq!(saved["current_model_engine"], "openai");
        assert_eq!(std::fs::read(history_path).unwrap(), history);
    }

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
        assert_eq!(
            CloudProvider::Soniox.base_origin(),
            "https://api.soniox.com"
        );
        assert_eq!(
            CloudProvider::Openai.base_origin(),
            "https://api.openai.com"
        );
        assert_eq!(CloudProvider::Groq.base_origin(), "https://api.groq.com");
        assert_eq!(
            CloudProvider::Deepgram.base_origin(),
            "https://api.deepgram.com"
        );
        assert_eq!(
            CloudProvider::Cohere.base_origin(),
            "https://api.cohere.com"
        );
    }

    #[test]
    fn catalog_first_entry_is_default_for_every_provider() {
        for provider in CloudProvider::ALL {
            let models = provider.available_models();
            assert!(!models.is_empty(), "{} catalog is empty", provider.id());
            assert_eq!(provider.default_model().id, models[0].id);
        }
    }

    #[test]
    fn catalog_contains_only_the_curated_models() {
        assert_eq!(
            CloudProvider::Soniox
                .available_models()
                .iter()
                .map(|m| (m.id, m.display_name))
                .collect::<Vec<_>>(),
            vec![("stt-async-v6", "Soniox v6")]
        );
        assert_eq!(
            CloudProvider::Openai
                .available_models()
                .iter()
                .map(|m| (m.id, m.display_name))
                .collect::<Vec<_>>(),
            vec![
                ("gpt-transcribe", "GPT Transcribe"),
                ("gpt-4o-mini-transcribe", "GPT-4o mini Transcribe"),
            ]
        );
        assert_eq!(
            CloudProvider::Groq
                .available_models()
                .iter()
                .map(|m| (m.id, m.display_name))
                .collect::<Vec<_>>(),
            vec![
                ("whisper-large-v3-turbo", "Whisper Large v3 Turbo"),
                ("whisper-large-v3", "Whisper Large v3"),
            ]
        );
        assert_eq!(
            CloudProvider::Deepgram
                .available_models()
                .iter()
                .map(|m| (m.id, m.display_name))
                .collect::<Vec<_>>(),
            vec![("nova-3", "Nova 3"), ("nova-2", "Nova 2")]
        );
        assert_eq!(
            CloudProvider::Cohere
                .available_models()
                .iter()
                .map(|m| (m.id, m.display_name))
                .collect::<Vec<_>>(),
            vec![("cohere-transcribe-03-2026", "Cohere Transcribe")]
        );
    }

    #[test]
    fn lookup_accepts_only_exact_catalog_ids() {
        assert_eq!(
            CloudProvider::Openai
                .model_by_id("gpt-transcribe")
                .map(|m| m.id),
            Some("gpt-transcribe")
        );
        assert_eq!(
            CloudProvider::Openai
                .model_by_id("gpt-4o-mini-transcribe")
                .map(|m| m.id),
            Some("gpt-4o-mini-transcribe")
        );
        assert_eq!(CloudProvider::Openai.model_by_id("GPT-transcribe"), None);
        assert_eq!(CloudProvider::Openai.model_by_id("gpt-4o-transcribe"), None);
        assert_eq!(
            CloudProvider::Groq.model_by_id("whisper-large-v3"),
            Some(&GROQ_MODELS[1])
        );
        assert_eq!(
            CloudProvider::Deepgram.model_by_id("nova-2").map(|m| m.id),
            Some("nova-2")
        );
        assert_eq!(CloudProvider::Deepgram.model_by_id("nova-3-general"), None);
    }

    #[test]
    fn resolve_falls_back_to_first_entry_for_missing_or_invalid() {
        let openai = CloudProvider::Openai;
        assert_eq!(openai.resolve_model_id(None), "gpt-transcribe");
        assert_eq!(openai.resolve_model_id(Some("")), "gpt-transcribe");
        assert_eq!(openai.resolve_model_id(Some("   ")), "gpt-transcribe");
        assert_eq!(
            openai.resolve_model_id(Some("gpt-4o-transcribe")),
            "gpt-transcribe"
        );
        assert_eq!(
            openai.resolve_model_id(Some("gpt-4o-mini-transcribe")),
            "gpt-4o-mini-transcribe"
        );
        assert_eq!(
            openai.resolve_model_id(Some("  gpt-4o-mini-transcribe  ")),
            "gpt-4o-mini-transcribe"
        );
        assert_eq!(
            CloudProvider::Groq.resolve_model_id(Some("whisper-large-v3")),
            "whisper-large-v3"
        );
        assert_eq!(
            CloudProvider::Deepgram.resolve_model_id(Some("nova-1")),
            "nova-3"
        );
    }
}
