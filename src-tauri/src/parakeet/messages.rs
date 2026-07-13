#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct ParakeetVocabularyTerm {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParakeetCommand {
    LoadModel {
        model_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        force_download: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        local_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_dir: Option<String>,
        #[serde(default = "default_precision")]
        precision: String,
        #[serde(default = "default_attention")]
        attention: String,
        #[serde(default = "default_local_context")]
        local_attention_context: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        chunk_duration: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        overlap_duration: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        eager_unload: Option<bool>,
    },
    UnloadModel {},
    Transcribe {
        audio_path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        #[serde(default)]
        translate_to_english: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        use_word_timestamps: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        chunk_duration: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        overlap_duration: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        attention: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        local_attention_context: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        custom_vocabulary: Option<Vec<ParakeetVocabularyTerm>>,
    },
    Diarize {
        audio_path: String,
    },
    Status {},
    DownloadCtcModels {},
    Warmup {},
    EouModelStatus {
        chunk_ms: u16,
    },
    DownloadEouModel {
        chunk_ms: u16,
    },
    WarmupEou {
        chunk_ms: u16,
    },
    StartStream {
        model_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_version: Option<String>,
        sample_rate: u32,
        channels: u16,
        #[serde(default = "default_stream_engine")]
        engine: ParakeetStreamEngine,
        #[serde(skip_serializing_if = "Option::is_none")]
        chunk_ms: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        config: Option<ParakeetStreamConfig>,
    },
    AudioChunk {
        pcm_b64: String,
    },
    FinalizeStream {},
    CancelStream {},
    DeleteModel {
        #[serde(skip_serializing_if = "Option::is_none")]
        model_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_version: Option<String>,
    },
    Shutdown {},
}

pub const SHORT_REQUEST_TIMEOUT_SECS: u64 = 30;
pub const WARMUP_TIMEOUT_SECS: u64 = 60;
pub const LOAD_MODEL_TIMEOUT_SECS: u64 = 300;
pub const DOWNLOAD_MODEL_TIMEOUT_SECS: u64 = 60 * 60;
pub const TRANSCRIBE_TIMEOUT_SECS: u64 = 180;
pub const MAX_TRANSCRIBE_TIMEOUT_SECS: u64 = 30 * 60;

impl ParakeetCommand {
    pub fn operation_name(&self) -> &'static str {
        match self {
            Self::LoadModel { .. } => "load_model",
            Self::UnloadModel { .. } => "unload_model",
            Self::Transcribe { .. } => "transcribe",
            Self::Diarize { .. } => "diarize",
            Self::Status { .. } => "status",
            Self::DownloadCtcModels { .. } => "download_ctc_models",
            Self::Warmup { .. } => "warmup",
            Self::EouModelStatus { .. } => "eou_model_status",
            Self::DownloadEouModel { .. } => "download_eou_model",
            Self::WarmupEou { .. } => "warmup_eou",
            Self::StartStream { .. } => "start_stream",
            Self::AudioChunk { .. } => "audio_chunk",
            Self::FinalizeStream { .. } => "finalize_stream",
            Self::CancelStream { .. } => "cancel_stream",
            Self::DeleteModel { .. } => "delete_model",
            Self::Shutdown { .. } => "shutdown",
        }
    }

    pub fn request_timeout_secs(&self) -> u64 {
        match self {
            Self::LoadModel { force_download, .. } if force_download.unwrap_or(false) => {
                DOWNLOAD_MODEL_TIMEOUT_SECS
            }
            Self::LoadModel { .. } => LOAD_MODEL_TIMEOUT_SECS,
            Self::Transcribe { audio_path, .. } | Self::Diarize { audio_path } => {
                transcribe_timeout_secs(audio_path)
            }
            Self::DownloadCtcModels { .. } | Self::DownloadEouModel { .. } => {
                DOWNLOAD_MODEL_TIMEOUT_SECS
            }
            Self::Warmup { .. } => WARMUP_TIMEOUT_SECS,
            Self::WarmupEou { .. } => WARMUP_TIMEOUT_SECS,
            Self::Status { .. }
            | Self::EouModelStatus { .. }
            | Self::Shutdown { .. }
            | Self::StartStream { .. }
            | Self::AudioChunk { .. }
            | Self::FinalizeStream { .. }
            | Self::CancelStream { .. }
            | Self::DeleteModel { .. }
            | Self::UnloadModel { .. } => SHORT_REQUEST_TIMEOUT_SECS,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParakeetStreamEngine {
    SlidingWindow,
    Eou,
    /// Decode-ahead live-preview engine (plan 051, Phase 1): fresh coherent decode of
    /// the whole un-committed window on a ~1s cadence, committing only by token
    /// timestamp so boundary-cut words stay revisable. Phase-1 / bench-only: the
    /// capability flip that selects it lives in Phase 2.
    DecodeAhead,
}

fn default_stream_engine() -> ParakeetStreamEngine {
    ParakeetStreamEngine::SlidingWindow
}

#[derive(Debug, Clone, Serialize)]
pub struct ParakeetStreamConfig {
    pub chunk_seconds: f64,
    pub hypothesis_chunk_seconds: f64,
    pub left_context_seconds: f64,
    pub right_context_seconds: f64,
    pub min_context_for_confirmation: f64,
    pub confirmation_threshold: f64,
}

impl ParakeetStreamConfig {
    pub fn streaming() -> Self {
        Self {
            chunk_seconds: 11.0,
            hypothesis_chunk_seconds: 1.0,
            left_context_seconds: 2.0,
            right_context_seconds: 2.0,
            min_context_for_confirmation: 10.0,
            confirmation_threshold: 0.80,
        }
    }

    pub fn tuned_hypothesis_500ms() -> Self {
        Self {
            hypothesis_chunk_seconds: 0.5,
            ..Self::streaming()
        }
    }

    /// Live-preview window geometry (task #19). FluidAudio's sliding-window loop
    /// only processes once `chunk + right_context` audio has accumulated and then
    /// advances by `chunk` — the default 11s+2s geometry means the FIRST update
    /// lands at 13s, which is why the 042 bench saw partials only at the end (its
    /// `hypothesis_chunk_seconds` "quick feedback" knob is defined upstream but
    /// never consulted by the loop). Shrinking the geometry turns the SAME loop
    /// into a ~1s-cadence decode-ahead: first update at ~1.5s, window
    /// left+chunk+right = 4+1+0.5 = 5.5s (fits the model's fixed 15s input).
    /// Preview-only quality: the pasted text stays the batch decode at stop.
    pub fn preview_geometry() -> Self {
        Self {
            chunk_seconds: 1.0,
            hypothesis_chunk_seconds: 0.5,
            left_context_seconds: 4.0,
            right_context_seconds: 0.5,
            min_context_for_confirmation: 10.0,
            confirmation_threshold: 0.80,
        }
    }
}

fn transcribe_timeout_secs(audio_path: &str) -> u64 {
    wav_duration_seconds(Path::new(audio_path))
        .map(transcribe_timeout_secs_for_duration)
        .unwrap_or(TRANSCRIBE_TIMEOUT_SECS)
}

fn transcribe_timeout_secs_for_duration(duration_secs: f64) -> u64 {
    if !duration_secs.is_finite() || duration_secs <= 0.0 {
        return TRANSCRIBE_TIMEOUT_SECS;
    }

    (duration_secs.ceil() as u64)
        .saturating_mul(4)
        .saturating_add(60)
        .clamp(TRANSCRIBE_TIMEOUT_SECS, MAX_TRANSCRIBE_TIMEOUT_SECS)
}

pub(crate) fn wav_duration_seconds(audio_path: &Path) -> Option<f64> {
    let reader = hound::WavReader::open(audio_path).ok()?;
    let spec = reader.spec();
    if spec.sample_rate == 0 || spec.channels == 0 {
        return None;
    }

    let frames = reader.duration() as f64;
    Some(frames / f64::from(spec.sample_rate))
}

fn default_precision() -> String {
    "bf16".to_string()
}

fn default_attention() -> String {
    "full".to_string()
}

fn default_local_context() -> i32 {
    256
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParakeetResponse {
    #[serde(rename_all = "camelCase")]
    Ok {
        command: String,
        #[serde(default)]
        payload: HashMap<String, Value>,
    },
    #[serde(rename_all = "camelCase")]
    Error {
        code: String,
        message: String,
        #[serde(default)]
        details: Option<Value>,
    },
    #[serde(rename_all = "camelCase")]
    Status {
        loaded_model: Option<String>,
        #[serde(default)]
        model_version: Option<String>,
        model_path: Option<String>,
        precision: Option<String>,
        attention: Option<String>,
        #[serde(default)]
        custom_vocabulary_supported: bool,
        #[serde(default)]
        custom_vocabulary_ready: bool,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        progress: f32,
        #[serde(default)]
        phase: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    EouModelStatus {
        chunk_ms: u16,
        downloaded: bool,
        path: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    StreamStarted {},
    #[serde(rename_all = "snake_case")]
    StreamPartial {
        text: String,
        is_confirmed: bool,
        #[serde(default)]
        confidence: Option<f32>,
    },
    #[serde(rename_all = "camelCase")]
    StreamFinal { text: String },
    #[serde(rename_all = "camelCase")]
    StreamCancelled {},
    #[serde(rename_all = "camelCase")]
    Diarization {
        #[serde(default)]
        segments: Vec<ParakeetSpeakerSegment>,
    },
    #[serde(rename_all = "camelCase")]
    Transcription {
        text: String,
        #[serde(default)]
        segments: Vec<ParakeetSegment>,
        #[serde(default)]
        language: Option<String>,
        #[serde(default)]
        duration: Option<f32>,
    },
    #[serde(rename_all = "camelCase")]
    Warmed {
        warmed: bool,
        ms: u64,
        #[serde(default)]
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParakeetSegment {
    pub text: String,
    #[serde(default)]
    pub start: Option<f32>,
    #[serde(default)]
    pub end: Option<f32>,
    #[serde(default)]
    pub tokens: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParakeetSpeakerSegment {
    #[serde(rename = "speakerId")]
    pub speaker_id: String,
    pub start: f32,
    pub end: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcribe_command_serializes_custom_vocabulary_when_present() {
        let command = ParakeetCommand::Transcribe {
            audio_path: "/tmp/audio.wav".to_string(),
            language: Some("en".to_string()),
            translate_to_english: false,
            prompt: None,
            use_word_timestamps: Some(true),
            chunk_duration: None,
            overlap_duration: None,
            attention: None,
            local_attention_context: None,
            custom_vocabulary: Some(vec![
                ParakeetVocabularyTerm {
                    text: "Voicetypr".to_string(),
                    aliases: vec!["voice typer".to_string()],
                },
                ParakeetVocabularyTerm {
                    text: "Tauri".to_string(),
                    aliases: Vec::new(),
                },
            ]),
        };

        let value = serde_json::to_value(command).unwrap();
        assert_eq!(value["type"], "transcribe");
        assert_eq!(value["custom_vocabulary"][0]["text"], "Voicetypr");
        assert_eq!(value["custom_vocabulary"][0]["aliases"][0], "voice typer");
        assert!(value["custom_vocabulary"][1].get("aliases").is_none());
    }

    #[test]
    fn transcribe_command_omits_empty_custom_vocabulary() {
        let command = ParakeetCommand::Transcribe {
            audio_path: "/tmp/audio.wav".to_string(),
            language: None,
            translate_to_english: false,
            prompt: None,
            use_word_timestamps: Some(true),
            chunk_duration: None,
            overlap_duration: None,
            attention: None,
            local_attention_context: None,
            custom_vocabulary: None,
        };

        let value = serde_json::to_value(command).unwrap();
        assert!(value.get("custom_vocabulary").is_none());
    }

    #[test]
    fn ctc_download_command_serializes_snake_case_type() {
        let value = serde_json::to_value(ParakeetCommand::DownloadCtcModels {}).unwrap();
        assert_eq!(value["type"], "download_ctc_models");
    }

    #[test]
    fn request_timeout_secs_restores_per_command_bounds() {
        assert_eq!(
            ParakeetCommand::Status {}.request_timeout_secs(),
            SHORT_REQUEST_TIMEOUT_SECS
        );
        assert_eq!(
            ParakeetCommand::DownloadCtcModels {}.request_timeout_secs(),
            DOWNLOAD_MODEL_TIMEOUT_SECS
        );
        assert_eq!(
            ParakeetCommand::LoadModel {
                model_id: "parakeet-tdt-0.6b-v2".to_string(),
                model_version: None,
                force_download: None,
                local_path: None,
                cache_dir: None,
                precision: "bf16".to_string(),
                attention: "full".to_string(),
                local_attention_context: 256,
                chunk_duration: None,
                overlap_duration: None,
                eager_unload: None,
            }
            .request_timeout_secs(),
            LOAD_MODEL_TIMEOUT_SECS
        );
        assert_eq!(
            ParakeetCommand::LoadModel {
                model_id: "parakeet-tdt-0.6b-v2".to_string(),
                model_version: None,
                force_download: Some(true),
                local_path: None,
                cache_dir: None,
                precision: "bf16".to_string(),
                attention: "full".to_string(),
                local_attention_context: 256,
                chunk_duration: None,
                overlap_duration: None,
                eager_unload: None,
            }
            .request_timeout_secs(),
            DOWNLOAD_MODEL_TIMEOUT_SECS
        );
        assert_eq!(
            ParakeetCommand::Transcribe {
                audio_path: "/tmp/missing.wav".to_string(),
                language: None,
                translate_to_english: false,
                prompt: None,
                use_word_timestamps: None,
                chunk_duration: None,
                overlap_duration: None,
                attention: None,
                local_attention_context: None,
                custom_vocabulary: None,
            }
            .request_timeout_secs(),
            TRANSCRIBE_TIMEOUT_SECS
        );
    }

    #[test]
    fn status_response_defaults_custom_vocabulary_flags() {
        let response: ParakeetResponse = serde_json::from_value(serde_json::json!({
            "type": "status",
            "loadedModel": null,
            "modelPath": null,
            "precision": null,
            "attention": null
        }))
        .unwrap();

        match response {
            ParakeetResponse::Status {
                custom_vocabulary_supported,
                custom_vocabulary_ready,
                ..
            } => {
                assert!(!custom_vocabulary_supported);
                assert!(!custom_vocabulary_ready);
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[test]
    fn status_response_decodes_custom_vocabulary_flags() {
        let response: ParakeetResponse = serde_json::from_value(serde_json::json!({
            "type": "status",
            "loadedModel": null,
            "modelPath": null,
            "precision": null,
            "attention": null,
            "customVocabularySupported": true,
            "customVocabularyReady": true
        }))
        .unwrap();

        match response {
            ParakeetResponse::Status {
                custom_vocabulary_supported,
                custom_vocabulary_ready,
                ..
            } => {
                assert!(custom_vocabulary_supported);
                assert!(custom_vocabulary_ready);
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }
}
