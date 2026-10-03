//! Island problem events and recovery policies. Never serialize provider errors.
use crate::commands::audio::{
    current_recording_generation, recording_license_state, RecordingLicenseState,
    TranscriptionFailure,
};
use crate::transcription::{engines::ActiveEngineSelection, error::TranscriptionErrorCode};
use crate::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKind {
    CloudFailed,
    NetworkOffline,
    ModelMissing,
    RemoteOffline,
    NoSpeech,
    Integrity,
    MicDroppedEmpty,
}
impl RecoveryKind {
    pub fn ttl_ms(self) -> u64 {
        if self == Self::NoSpeech {
            30_000
        } else {
            600_000
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockedKind {
    LicenseCheckFailed,
    LicenseVerifyRequired,
    TrialEnded,
    NoEngine,
    CloudKeyMissing,
    CloudKeyRejected,
    SonioxStorageFull,
    MicPermissionDenied,
    MicMissing,
    MicBusy,
    AccessibilityOff,
    StartingUp,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IslandAction {
    RecheckLicense,
    OpenLicense,
    OpenModels,
    OpenCloudKeys,
    OpenStorage,
    OpenMicSettings,
    ChooseMic,
    OpenAccessibility,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolishReason {
    Timeout,
    RateLimited,
    Network,
    Guard,
    Error,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Note {
    MicDropped {
        captured_ms: u64,
    },
    MicSilent,
    TranslateFailed,
    ModelFallback {
        engine_short: String,
        alt_engine_short: String,
    },
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))] // Windows Vulkan path only.
    GpuFallback {
        engine_short: String,
    },
    PolishSkipped {
        reason: PolishReason,
    },
}
#[derive(Clone, Serialize)]
struct NoteEvent {
    generation: u64,
    #[serde(flatten)]
    note: Note,
}
#[derive(Clone, Serialize)]
struct BlockedEvent {
    generation: u64,
    kind: BlockedKind,
    action: IslandAction,
}
pub fn note(app: &AppHandle, generation: u64, note: Note) {
    if !crate::commands::audio::recording_generation_is_stale(generation) {
        let _ = app.emit_to("pill", "dictation-note", NoteEvent { generation, note });
    }
}
pub fn blocked(app: &AppHandle, generation: u64, kind: BlockedKind, action: IslandAction) {
    if !crate::commands::audio::recording_generation_is_stale(generation) {
        crate::commands::pill_feedback::cancel_terminal_hide(generation);
        let _ = app.emit_to(
            "pill",
            "dictation-blocked",
            BlockedEvent {
                generation,
                kind,
                action,
            },
        );
    }
}
pub fn mic_blocked(app: &AppHandle, generation: u64, error: &str) {
    let error = error.to_ascii_lowercase();
    let (kind, action) = if error.contains("permission") || error.contains("access") {
        (
            BlockedKind::MicPermissionDenied,
            IslandAction::OpenMicSettings,
        )
    } else if error.contains("busy") || error.contains("in use") {
        (BlockedKind::MicBusy, IslandAction::ChooseMic)
    } else {
        (BlockedKind::MicMissing, IslandAction::ChooseMic)
    };
    blocked(app, generation, kind, action);
}
pub fn polish_reason(error: &crate::ai::error::AiProviderError) -> PolishReason {
    use crate::ai::error::AiProviderError;
    match error {
        AiProviderError::Timeout => PolishReason::Timeout,
        AiProviderError::RateLimited => PolishReason::RateLimited,
        AiProviderError::Network | AiProviderError::ServiceUnavailable => PolishReason::Network,
        _ => PolishReason::Error,
    }
}
pub fn too_short<R: Runtime>(app: &AppHandle<R>, generation: u64, hold: bool) {
    let _ = app.emit_to(
        "pill",
        "recording-too-short",
        serde_json::json!({
            "generation": generation, "mode": if hold { "hold" } else { "toggle" }
        }),
    );
}

/// User-initiated navigation is the only place recovery may activate a window.
#[tauri::command]
pub async fn island_action(app: AppHandle, action: IslandAction) -> Result<(), String> {
    match action {
        IslandAction::RecheckLicense => {
            crate::commands::license::revalidate_license(app).await?;
            Ok(())
        }
        IslandAction::OpenMicSettings => crate::commands::permissions::open_microphone_settings(),
        IslandAction::OpenAccessibility => {
            crate::commands::permissions::open_accessibility_settings()
        }
        _ => {
            match action {
                IslandAction::OpenLicense => app.emit_to(
                    "main",
                    "license-required",
                    serde_json::json!({"action":"restore"}),
                ),
                IslandAction::OpenStorage => app.emit_to(
                    "main",
                    "soniox-storage-limit",
                    serde_json::json!({"autoHealed":false}),
                ),
                _ => app.emit_to("main", "island-navigate", action),
            }
            .map_err(|_| "Could not open settings".to_string())?;
            crate::commands::window::focus_main_window(app).await
        }
    }
}

// Internal delivery plan: deliberately not serializable or emitted.
#[derive(Debug)]
pub(crate) struct DesktopWritingSuccessPlan {
    pub(crate) final_text: String,
    pub(crate) writing_metadata: Option<serde_json::Value>,
    pub(crate) should_deliver: bool,
    pub(crate) save_history_entries: usize,
}

pub(crate) fn plan_translation_failure(
    transcription: &crate::transcription::TranscriptionResult,
    target_language: &str,
) -> DesktopWritingSuccessPlan {
    DesktopWritingSuccessPlan {
        final_text: transcription.raw_text.clone(),
        writing_metadata: Some(
            crate::commands::audio::build_translation_failed_history_metadata(target_language),
        ),
        should_deliver: true,
        save_history_entries: 1,
    }
}

pub fn polish_was_guarded(result: &crate::writing::WritingResult) -> bool {
    result.polish_enabled
        && result.mode != crate::ai::prompts::EnhancementPreset::PersonalDictation
        && (result.stage_timings.ai_polish_ms.is_none()
            || result
                .applied_operations
                .iter()
                .any(|op| op.kind == crate::writing::WritingOperationKind::FinalGuard))
}

pub fn emit_visible_main<S: serde::Serialize + Clone>(
    app: &AppHandle,
    event: &str,
    payload: S,
) -> Result<(), tauri::Error> {
    if app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
    {
        app.emit_to("main", event, payload)
    } else {
        Ok(())
    }
}

pub fn recovery_for_failure(
    failure: &TranscriptionFailure,
    selection: &ActiveEngineSelection,
) -> Option<RecoveryKind> {
    if matches!(
        failure,
        TranscriptionFailure::Local {
            code: Some(TranscriptionErrorCode::Cancelled),
            ..
        }
    ) {
        return None;
    }
    if let TranscriptionFailure::Local { message, .. } = failure {
        if message.contains("too short") {
            return Some(RecoveryKind::NoSpeech);
        }
    }
    if !failure.is_retryable_failure() {
        return None;
    }
    Some(match failure {
        TranscriptionFailure::Remote(_) => RecoveryKind::RemoteOffline,
        TranscriptionFailure::Local {
            code: Some(TranscriptionErrorCode::TransportFailed),
            ..
        } => RecoveryKind::NetworkOffline,
        TranscriptionFailure::Local {
            code:
                Some(
                    TranscriptionErrorCode::ModelUnavailable
                    | TranscriptionErrorCode::EngineUnavailable,
                ),
            ..
        } => RecoveryKind::ModelMissing,
        _ if matches!(selection, ActiveEngineSelection::Cloud { .. }) => RecoveryKind::CloudFailed,
        _ => RecoveryKind::Integrity,
    })
}

pub async fn validate_recording_license(app: &AppHandle) -> Result<(), String> {
    // Check cached license status (warmed during startup/license transitions - no network call)
    let app_state = app.state::<AppState>();
    let license_state = recording_license_state(&*app_state.license_cache.read().await);

    match license_state {
        RecordingLicenseState::CheckFailed => {
            log::warn!("Recording blocked: license check failed; recovery required");
            let message = "License check failed. Open License and retry, or re-enter your existing license key to activate it again.";
            blocked(
                app,
                current_recording_generation(),
                BlockedKind::LicenseCheckFailed,
                IslandAction::RecheckLicense,
            );
            let _ = emit_visible_main(
                app,
                "license-required",
                serde_json::json!({
                    "title": "License Check Failed",
                    "message": message,
                    "action": "restore"
                }),
            );
            return Err(message.to_string());
        }
        RecordingLicenseState::VerificationRequired => {
            log::warn!("Recording blocked: offline license verification window has ended");
            blocked(
                app,
                current_recording_generation(),
                BlockedKind::LicenseVerifyRequired,
                IslandAction::RecheckLicense,
            );
            let _ = emit_visible_main(
                app,
                "license-required",
                serde_json::json!({
                    "title": "License Verification Required",
                    "message": "Connect to the internet and revalidate your license to continue recording.",
                    "action": "revalidate"
                }),
            );
            return Err("License verification required to record".to_string());
        }
        RecordingLicenseState::Blocked => {
            log::warn!("Recording blocked: no active license or trial");

            blocked(
                app,
                current_recording_generation(),
                BlockedKind::TrialEnded,
                IslandAction::OpenLicense,
            );

            let _ = emit_visible_main(
                app,
                "license-required",
                serde_json::json!({
                    "title": "License Required",
                    "message": "No active license or trial was found. If you already purchased a license, re-enter your existing key here to activate it again.",
                    "action": "restore"
                }),
            );
            return Err("License required to record".to_string());
        }
        RecordingLicenseState::Ready => {}
        RecordingLicenseState::Loading => {
            log::warn!("Recording blocked: license cache not initialized yet");
            blocked(
                app,
                current_recording_generation(),
                BlockedKind::StartingUp,
                IslandAction::RecheckLicense,
            );
            return Err(
                "License status is still loading. Please try again in a moment.".to_string(),
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dictation_paths_never_focus_the_dashboard() {
        for source in [
            include_str!("../commands/audio.rs"),
            include_str!("../cloud_stt/soniox.rs"),
        ] {
            assert!(!source.contains("focus_main_window"));
        }
    }
    #[test]
    fn polish_failures_and_all_note_shapes_use_closed_codes() {
        use crate::ai::error::AiProviderError;
        for (error, reason) in [
            (AiProviderError::Timeout, "timeout"),
            (AiProviderError::RateLimited, "rate_limited"),
            (AiProviderError::Network, "network"),
            (AiProviderError::BadResponse, "error"),
        ] {
            assert_eq!(serde_json::to_value(polish_reason(&error)).unwrap(), reason);
        }
        for note in [
            Note::MicSilent,
            Note::TranslateFailed,
            Note::ModelFallback {
                engine_short: "Whisper Large".into(),
                alt_engine_short: "Whisper Tiny".into(),
            },
            Note::GpuFallback {
                engine_short: "Whisper".into(),
            },
            Note::PolishSkipped {
                reason: PolishReason::Guard,
            },
        ] {
            let value = serde_json::to_value(NoteEvent {
                generation: 42,
                note,
            })
            .unwrap();
            assert!(value.as_object().unwrap().keys().all(|key| matches!(
                key.as_str(),
                "generation" | "kind" | "reason" | "engine_short" | "alt_engine_short"
            )));
        }
    }

    #[test]
    fn note_payload_is_closed_and_content_free() {
        let value = serde_json::to_value(NoteEvent {
            generation: 42,
            note: Note::MicDropped { captured_ms: 1000 },
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"generation":42,"kind":"mic_dropped","captured_ms":1000})
        );
        assert_eq!(
            serde_json::to_value(PolishReason::RateLimited).unwrap(),
            "rate_limited"
        );
    }
}
