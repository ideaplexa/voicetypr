pub mod capabilities;
pub(crate) mod engines;
pub mod error;
pub mod executor;
pub mod request;
pub mod stream;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionSource {
    DesktopRecording,
    AudioFile,
    AudioBytes,
    RemoteServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionTask {
    Transcribe,
    TranslateToEnglish,
}

impl TranscriptionTask {
    pub fn from_translate_to_english(translate_to_english: bool) -> Self {
        if translate_to_english {
            Self::TranslateToEnglish
        } else {
            Self::Transcribe
        }
    }

    pub fn fallback_transcript_language(self, spoken_language: Option<&str>) -> Option<String> {
        match self {
            Self::Transcribe => spoken_language.map(str::to_string),
            Self::TranslateToEnglish => Some("en".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionJob {
    pub source: TranscriptionSource,
    pub engine: String,
    pub model: String,
    pub spoken_language: Option<String>,
    pub task: TranscriptionTask,
}

impl TranscriptionJob {
    pub fn from_legacy_settings(
        source: TranscriptionSource,
        engine: impl Into<String>,
        model: impl Into<String>,
        spoken_language: Option<String>,
        translate_to_english: bool,
    ) -> Self {
        Self {
            source,
            engine: engine.into(),
            model: model.into(),
            spoken_language,
            task: TranscriptionTask::from_translate_to_english(translate_to_english),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub text: String,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionTimings {
    pub audio_duration_ms: Option<u64>,
    pub processing_duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spans_ms: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionWord {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub raw_text: String,
    pub engine: String,
    pub model: String,
    pub spoken_language: Option<String>,
    pub transcript_language: Option<String>,
    pub task: TranscriptionTask,
    pub source: TranscriptionSource,
    pub segments: Option<Vec<TranscriptionSegment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<TranscriptionWord>>,
    pub timings: TranscriptionTimings,
}

impl TranscriptionResult {
    pub fn new(job: &TranscriptionJob, raw_text: impl Into<String>) -> Self {
        Self {
            raw_text: raw_text.into(),
            engine: job.engine.clone(),
            model: job.model.clone(),
            spoken_language: job.spoken_language.clone(),
            transcript_language: job
                .task
                .fallback_transcript_language(job.spoken_language.as_deref()),
            task: job.task,
            source: job.source,
            segments: None,
            words: None,
            timings: TranscriptionTimings::default(),
        }
    }

    pub fn with_transcript_language(mut self, transcript_language: Option<String>) -> Self {
        if transcript_language.is_some() {
            self.transcript_language = transcript_language;
        }
        self
    }

    pub fn with_segments(mut self, segments: Vec<TranscriptionSegment>) -> Self {
        self.segments = Some(segments);
        self
    }

    pub fn with_audio_duration_ms(mut self, audio_duration_ms: Option<u64>) -> Self {
        self.timings.audio_duration_ms = audio_duration_ms;
        self
    }

    pub fn with_processing_duration_ms(mut self, processing_duration_ms: Option<u64>) -> Self {
        self.timings.processing_duration_ms = processing_duration_ms;
        self
    }

    pub fn with_span_timings_ms(mut self, spans_ms: serde_json::Value) -> Self {
        self.timings.spans_ms = Some(spans_ms);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_task_falls_back_to_english_transcript_language() {
        let job = TranscriptionJob::from_legacy_settings(
            TranscriptionSource::DesktopRecording,
            "whisper",
            "medium",
            Some("es".to_string()),
            true,
        );

        let result = TranscriptionResult::new(&job, "hello world");

        assert_eq!(result.spoken_language.as_deref(), Some("es"));
        assert_eq!(result.transcript_language.as_deref(), Some("en"));
        assert_eq!(result.task, TranscriptionTask::TranslateToEnglish);
    }

    #[test]
    fn test_transcribe_task_falls_back_to_spoken_language() {
        let job = TranscriptionJob::from_legacy_settings(
            TranscriptionSource::AudioFile,
            "parakeet",
            "parakeet-tdt-0.6b-v2",
            Some("fr".to_string()),
            false,
        );

        let result = TranscriptionResult::new(&job, "bonjour le monde");

        assert_eq!(result.transcript_language.as_deref(), Some("fr"));
        assert_eq!(result.task, TranscriptionTask::Transcribe);
    }

    #[test]
    fn test_explicit_transcript_language_overrides_fallback() {
        let job = TranscriptionJob::from_legacy_settings(
            TranscriptionSource::RemoteServer,
            "remote",
            "shared-model",
            Some("es".to_string()),
            false,
        );

        let result = TranscriptionResult::new(&job, "hola mundo")
            .with_transcript_language(Some("pt".to_string()))
            .with_audio_duration_ms(Some(1200))
            .with_processing_duration_ms(Some(450));

        assert_eq!(result.transcript_language.as_deref(), Some("pt"));
        assert_eq!(result.timings.audio_duration_ms, Some(1200));
        assert_eq!(result.timings.processing_duration_ms, Some(450));
    }

    #[test]
    fn test_transcription_result_serde_round_trip_with_words() {
        let job = TranscriptionJob::from_legacy_settings(
            TranscriptionSource::AudioFile,
            "whisper",
            "large-v3",
            Some("en".to_string()),
            false,
        );
        let word = TranscriptionWord {
            text: "hello".to_string(),
            start_ms: Some(100),
            end_ms: Some(600),
            speaker_id: Some("S1".to_string()),
            confidence: Some(0.95),
        };
        let result = TranscriptionResult::new(&job, "hello world")
            .with_transcript_language(Some("en".to_string()));
        let result = TranscriptionResult {
            words: Some(vec![word]),
            ..result
        };

        let json = serde_json::to_string(&result).expect("serialize");
        let back: TranscriptionResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(result, back);
        assert_eq!(back.source, TranscriptionSource::AudioFile);
        assert!(back.words.is_some());

        // Back-compat: JSON without `words` deserializes with words: None
        let legacy_json = r#"{"raw_text":"hi","engine":"whisper","model":"large-v3","spoken_language":"en","transcript_language":"en","task":"transcribe","source":"audio_file","timings":{"audio_duration_ms":null,"processing_duration_ms":null}}"#;
        let legacy: TranscriptionResult =
            serde_json::from_str(legacy_json).expect("deserialize legacy");
        assert_eq!(legacy.words, None);
    }
}
