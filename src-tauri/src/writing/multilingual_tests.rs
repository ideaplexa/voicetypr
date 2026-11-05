//! Synthetic exact-byte regression fixtures. Never used as engine-accuracy evidence.
#[path = "../../crates/transcript-text/src/french_spacing_fixtures.rs"]
mod french_spacing_fixtures;

use super::format::{format_for_transcription, FormatStageSettings};
use crate::transcription::{
    TranscriptionJob, TranscriptionResult, TranscriptionSource, TranscriptionTask,
};

#[test]
fn french_spacing_survives_engine_and_shared_writing_stages() {
    for case in french_spacing_fixtures::fixtures() {
        for engine in ["whisper", "deepgram", "soniox", "fixture"] {
            for source in [
                TranscriptionSource::DesktopRecording,
                TranscriptionSource::AudioFile,
                TranscriptionSource::RemoteServer,
            ] {
                let job = TranscriptionJob {
                    source,
                    engine: engine.into(),
                    model: "fixture".into(),
                    spoken_language: Some(case.language.into()),
                    task: TranscriptionTask::Transcribe,
                };
                let normalized = transcript_text::normalize_transcript_spacing_for_language(
                    &case.input,
                    Some(case.language),
                );
                let transcription = TranscriptionResult::new(&job, normalized.to_string());
                let stage = FormatStageSettings {
                    paragraphs: false,
                    remove_fillers: true,
                };
                // Non-French negative fixtures use native casing so their byte
                // assertion isolates spacing rather than English capitalization.
                let mut transcription = transcription;
                if !transcript_text::is_french(Some(case.language)) {
                    transcription.engine = "soniox".into();
                }
                let actual = format_for_transcription(
                    &normalized,
                    &transcription,
                    Some(case.language),
                    stage,
                    &[],
                )
                .unwrap_or_else(|| normalized.to_string());
                assert_eq!(
                    actual.as_bytes(),
                    case.expected.as_bytes(),
                    "{} {} {} {}",
                    case.id,
                    engine,
                    case.engine,
                    case.flags
                );
                assert_eq!(
                    format_for_transcription(
                        &actual,
                        &transcription,
                        Some(case.language),
                        stage,
                        &[]
                    ),
                    None,
                    "{}",
                    case.id,
                );
            }
        }
    }
}
