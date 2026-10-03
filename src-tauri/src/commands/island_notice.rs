//! Closed, content-free notices for feedback outside the recovery contract.
use serde::Serialize;
use tauri::{AppHandle, Runtime};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    Finishing,
    PolishOn,
    PolishOff,
    PolishSetup,
    ShortcutsRetired,
    LongSilence,
    SilenceStopped,
    SilenceDiscarded,
    RecordingFailed,
    CopyFailed,
    TranscriptionFailed,
    HistoryRetry,
    ShortcutThrottled,
    NoSpeech,
}
pub fn notice<R: Runtime>(app: &AppHandle<R>, kind: NoticeKind) -> NoticeKind {
    crate::observability::emit(
        "island_state",
        vec![("state", serde_json::to_value(kind).unwrap())],
        Some(crate::commands::audio::current_recording_generation()),
    );
    super::pill_feedback::send(app, "island-notice", serde_json::json!({ "kind": kind }));
    kind
}
pub fn clear<R: Runtime>(app: &AppHandle<R>, kind: NoticeKind) {
    super::pill_feedback::send(
        app,
        "island-notice-clear",
        serde_json::json!({ "kind": kind }),
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    macro_rules! notice_contract {
        ($name:ident, $variant:ident, $wire:literal) => {
            #[test]
            fn $name() {
                let payload = serde_json::json!({"kind":NoticeKind::$variant});
                assert_eq!(payload, serde_json::json!({"kind":$wire}));
                assert_eq!(payload.as_object().unwrap().len(),1);
            }
        };
    }
    notice_contract!(finishing, Finishing, "finishing");
    notice_contract!(polish_on, PolishOn, "polish_on");
    notice_contract!(polish_off, PolishOff, "polish_off");
    notice_contract!(polish_setup, PolishSetup, "polish_setup");
    notice_contract!(shortcuts_retired, ShortcutsRetired, "shortcuts_retired");
    notice_contract!(long_silence, LongSilence, "long_silence");
    notice_contract!(silence_stopped, SilenceStopped, "silence_stopped");
    notice_contract!(silence_discarded, SilenceDiscarded, "silence_discarded");
    notice_contract!(recording_failed, RecordingFailed, "recording_failed");
    notice_contract!(copy_failed, CopyFailed, "copy_failed");
    notice_contract!(
        transcription_failed,
        TranscriptionFailed,
        "transcription_failed"
    );
    notice_contract!(history_retry, HistoryRetry, "history_retry");
    notice_contract!(shortcut_throttled, ShortcutThrottled, "shortcut_throttled");
    notice_contract!(no_speech, NoSpeech, "no_speech");
}
