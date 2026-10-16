use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command<'a> {
    Status,
    LoadModel {
        model: &'a str,
        backend: &'a str,
        model_path: &'a str,
        gpu: bool,
        threads: usize,
    },
    StartStream {
        session_id: u64,
        language: &'a str,
        mode: &'a str,
    },
    AudioChunk {
        session_id: u64,
        pcm: &'a str,
    },
    FinalizeStream {
        session_id: u64,
    },
}

// Deliberately no Debug: these messages carry private audio/transcript data.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Status {
        id: u64,
        protocol: u32,
    },
    Loaded {
        id: u64,
    },
    Ok {
        id: u64,
        samples: Option<u64>,
    },
    Partial {
        id: u64,
        session_id: u64,
        committed: String,
        tentative: String,
    },
    Final {
        id: u64,
        session_id: u64,
        #[serde(flatten)]
        result: NativeTranscript,
    },
    Error {
        id: u64,
        code: String,
    },
}

#[derive(Deserialize)]
pub struct NativeTranscript {
    pub text: String,
    pub language: Option<String>,
    pub segments: Vec<crate::transcription::TranscriptionSegment>,
    pub samples: u64,
    pub processing_ms: u64,
}

impl NativeTranscript {
    pub fn into_result(
        self,
        job: &crate::transcription::TranscriptionJob,
    ) -> crate::transcription::TranscriptionResult {
        crate::transcription::TranscriptionResult::new(job, self.text)
            .with_transcript_language(self.language)
            .with_segments(self.segments)
            .with_audio_duration_ms(Some(self.samples.saturating_mul(1000) / 16_000))
            .with_processing_duration_ms(Some(self.processing_ms))
    }
}

impl Response {
    pub fn id(&self) -> u64 {
        match self {
            Self::Status { id, .. }
            | Self::Loaded { id, .. }
            | Self::Ok { id, .. }
            | Self::Partial { id, .. }
            | Self::Final { id, .. }
            | Self::Error { id, .. } => *id,
        }
    }
}

pub fn encode_audio(samples: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pcm_matches_the_native_little_endian_contract() {
        assert_eq!(encode_audio(&[1.0, -0.5]), "AACAPwAAAL8=");
    }
    #[test]
    fn native_final_accepts_unknown_language_and_timestamps() {
        let response: Response = serde_json::from_str(r#"{"type":"final","id":3,"session_id":7,"text":"text","language":null,"segments":[{"text":"text","start_ms":null,"end_ms":null}],"samples":16000,"processing_ms":25}"#).unwrap();
        assert!(matches!(
            response,
            Response::Final {
                session_id: 7,
                result: NativeTranscript {
                    samples: 16000,
                    language: None,
                    ..
                },
                ..
            }
        ));
    }
}
