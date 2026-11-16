//! Runs a finished dictation as a voice command when the setting is on and
//! the command is the whole dictation (rule in writing/voice_commands.rs).
//! Commands never reach the writing pipeline, Polish, the clipboard, History
//! or saved recordings.

use crate::recording::island::{self, BlockedKind, IslandAction};
use crate::transcription::TranscriptionResult;
use crate::writing::{classify_utterance, Utterance, VoiceCommand};
use tauri::{AppHandle, Manager};

/// What a finished dictation is. Always `Dictation` while the setting is off.
pub(crate) fn classify(app: &AppHandle, transcription: &TranscriptionResult) -> Utterance {
    let Ok(settings) = crate::writing::load_writing_settings(app) else {
        return Utterance::Dictation;
    };
    if !settings.editing_commands {
        return Utterance::Dictation;
    }
    let language = crate::writing::effective_transcript_language(transcription);
    classify_utterance(
        &transcription.raw_text,
        language.as_deref(),
        settings.spoken_punctuation,
    )
}

pub(crate) enum Failure {
    /// The Accessibility blocker now owns the pill.
    NoPermission,
    NotRun,
}

pub(crate) async fn run(
    app: &AppHandle,
    command: VoiceCommand,
    generation: u64,
) -> Result<(), Failure> {
    #[cfg(target_os = "macos")]
    if !super::permissions::check_accessibility_permission(app.clone())
        .await
        .unwrap_or(false)
    {
        island::blocked(
            app,
            generation,
            BlockedKind::AccessibilityOff,
            IslandAction::OpenAccessibility,
        );
        return Err(Failure::NoPermission);
    }
    let app = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        let current = || {
            if super::audio::delivery_aborted(
                app.state::<crate::AppState>().is_cancellation_requested(),
                generation,
            ) {
                return Err("Dictation discarded".to_string());
            }
            // A registered hotkey never authorizes keys into a protected field.
            if keytrigger::backend::secure_input_enabled() {
                return Err("Secure Input is on".to_string());
            }
            Ok(())
        };
        current()?;
        crate::insertion::menu_tracking::dispatch_if_ready(
            || crate::insertion::menu_tracking::paste_ready(Some(&app)),
            || crate::insertion::voice_keys::perform(command, &current),
        )
        .unwrap_or_else(|| Err("A menu owns the keyboard".to_string()))
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    result.map_err(|error| {
        log::warn!("Voice command was not run: {}", error);
        Failure::NotRun
    })
}
