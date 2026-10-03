//! Contract tests for the audio hooks; kept outside the large command module.
use super::TranscriptionFailure;
use crate::cloud_stt::CloudProvider;
use crate::transcription::error::TranscriptionErrorCode;
#[test]
fn translation_failure_delivers_original_and_saves_once() {
    let job = super::build_transcription_job(
        crate::transcription::TranscriptionSource::DesktopRecording,
        "whisper",
        "tiny",
        Some("en".into()),
        false,
    );
    let result = crate::transcription::TranscriptionResult::new(&job, "Keep all my words");
    let plan = super::plan_translation_failure(&result, "es");
    assert_eq!(plan.final_text, result.raw_text);
    assert!(plan.should_deliver);
    assert_eq!(plan.save_history_entries, 1);
    assert_eq!(plan.writing_metadata.unwrap()["translation_failed"], true);
}
#[test]
fn failures_map_to_content_free_recovery_kinds() {
    use crate::recording::island::RecoveryKind;
    let cloud = super::ActiveEngineSelection::Cloud {
        provider: CloudProvider::Soniox,
        model_name: "soniox".into(),
    };
    for (code, expected) in [
        (
            TranscriptionErrorCode::TransportFailed,
            Some(RecoveryKind::NetworkOffline),
        ),
        (
            TranscriptionErrorCode::ModelUnavailable,
            Some(RecoveryKind::ModelMissing),
        ),
        (
            TranscriptionErrorCode::Timeout,
            Some(RecoveryKind::CloudFailed),
        ),
        (TranscriptionErrorCode::Cancelled, None),
    ] {
        let failure = TranscriptionFailure::Local {
            message: "private detail must not escape".into(),
            code: Some(code),
        };
        assert_eq!(
            crate::recording::island::recovery_for_failure(&failure, &cloud),
            expected
        );
    }
    assert_eq!(
        crate::recording::island::recovery_for_failure(
            &TranscriptionFailure::local("too short".into()),
            &cloud
        ),
        Some(RecoveryKind::NoSpeech)
    );
}
