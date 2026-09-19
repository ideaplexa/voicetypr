//! Best-effort, non-blocking audio feedback for user-visible lifecycle events.

use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AudioFeedbackCue {
    RecordingStarted,
    TranscriptReady,
    PasteCompleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CueSpec {
    setting_key: &'static str,
    macos_sound_path: &'static str,
    windows_frequency_hz: u32,
    windows_duration_ms: u32,
}

const fn cue_spec(cue: AudioFeedbackCue) -> CueSpec {
    match cue {
        AudioFeedbackCue::RecordingStarted => CueSpec {
            setting_key: "play_sound_on_recording",
            macos_sound_path: "/System/Library/Sounds/Tink.aiff",
            windows_frequency_hz: 800,
            windows_duration_ms: 100,
        },
        AudioFeedbackCue::TranscriptReady => CueSpec {
            setting_key: "play_sound_on_transcription_complete",
            macos_sound_path: "/System/Library/Sounds/Pop.aiff",
            windows_frequency_hz: 600,
            windows_duration_ms: 100,
        },
        AudioFeedbackCue::PasteCompleted => CueSpec {
            setting_key: "play_sound_on_paste_success",
            macos_sound_path: "/System/Library/Sounds/Glass.aiff",
            windows_frequency_hz: 1_000,
            windows_duration_ms: 100,
        },
    }
}

fn cue_enabled(
    cue: AudioFeedbackCue,
    stored: Option<bool>,
    legacy_recording_end: Option<bool>,
) -> bool {
    match cue {
        AudioFeedbackCue::TranscriptReady => {
            crate::commands::settings::resolve_transcription_complete_sound(
                stored,
                legacy_recording_end,
            )
        }
        AudioFeedbackCue::RecordingStarted | AudioFeedbackCue::PasteCompleted => {
            stored.unwrap_or(true)
        }
    }
}

/// Starts the platform sound work and returns immediately. Feedback is
/// best-effort: store or playback-start failures are logged and never affect
/// recording, delivery, or clipboard restoration.
pub(crate) fn play_audio_feedback(app: &AppHandle, cue: AudioFeedbackCue) {
    let spec = cue_spec(cue);
    let store = match app.store("settings") {
        Ok(store) => store,
        Err(error) => {
            log::warn!(
                "Skipping {:?} audio feedback because settings are unavailable: {}",
                cue,
                error
            );
            return;
        }
    };
    let enabled = cue_enabled(
        cue,
        store
            .get(spec.setting_key)
            .and_then(|value| value.as_bool()),
        store
            .get("play_sound_on_recording_end")
            .and_then(|value| value.as_bool()),
    );
    if !enabled {
        return;
    }

    play_platform_cue(cue, spec);
}

fn play_platform_cue(cue: AudioFeedbackCue, spec: CueSpec) {
    #[cfg(target_os = "macos")]
    if let Err(error) = std::process::Command::new("afplay")
        .arg(spec.macos_sound_path)
        .spawn()
    {
        log::warn!("Failed to play {:?} audio feedback: {}", cue, error);
    }

    #[cfg(target_os = "windows")]
    {
        // Beep is synchronous, so keep its alertable wait off the calling thread.
        // Calling it directly also lets us report API playback failures instead of
        // only knowing whether an intermediary PowerShell process was launched.
        if let Err(error) = std::thread::Builder::new()
            .name("audio-feedback".to_string())
            .spawn(move || {
                // SAFETY: frequency and duration are plain values in the ranges
                // accepted by Beep and have no pointer or lifetime requirements.
                if let Err(error) = unsafe {
                    windows::Win32::System::Diagnostics::Debug::Beep(
                        spec.windows_frequency_hz,
                        spec.windows_duration_ms,
                    )
                } {
                    log::warn!(
                        "Windows Beep API failed for {:?} audio feedback ({} Hz, {} ms): {}",
                        cue,
                        spec.windows_frequency_hz,
                        spec.windows_duration_ms,
                        error
                    );
                }
            })
        {
            log::warn!("Failed to start {:?} audio feedback worker: {}", cue, error);
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = (cue, spec);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn cues_preserve_platform_sound_specs() {
        let specs = [
            cue_spec(AudioFeedbackCue::RecordingStarted),
            cue_spec(AudioFeedbackCue::TranscriptReady),
            cue_spec(AudioFeedbackCue::PasteCompleted),
        ];

        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.setting_key)
                .collect::<HashSet<_>>()
                .len(),
            3
        );
        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.macos_sound_path)
                .collect::<HashSet<_>>()
                .len(),
            3
        );
        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.windows_frequency_hz)
                .collect::<HashSet<_>>()
                .len(),
            3
        );
        assert_eq!(
            specs.map(|spec| (spec.windows_frequency_hz, spec.windows_duration_ms)),
            [(800, 100), (600, 100), (1_000, 100)]
        );
    }

    #[test]
    fn each_cue_honors_its_disabled_preference() {
        for cue in [
            AudioFeedbackCue::RecordingStarted,
            AudioFeedbackCue::TranscriptReady,
            AudioFeedbackCue::PasteCompleted,
        ] {
            assert!(!cue_enabled(cue, Some(false), Some(true)));
        }
    }

    #[test]
    fn transcript_ready_honors_legacy_disabled_preference_and_new_override() {
        assert!(!cue_enabled(
            AudioFeedbackCue::TranscriptReady,
            None,
            Some(false)
        ));
        assert!(cue_enabled(
            AudioFeedbackCue::TranscriptReady,
            Some(true),
            Some(false)
        ));
    }
}
