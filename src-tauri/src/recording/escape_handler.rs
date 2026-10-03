use crate::{cancel_recording, get_recording_state, AppState, RecordingState};
use keytrigger::KeyPhase;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};

/// Handle ESC key press during recording
///
/// Implements a double-tap system:
/// 1. First ESC: Show inline hint "Press ESC again to cancel" for 2 seconds
/// 2. Second ESC within 2 seconds: Cancel recording
/// 3. Timeout after 2 seconds: Reset to single-tap mode
pub async fn handle_escape_key_press(
    app_state: &AppState,
    app_handle: &AppHandle,
    event_state: KeyPhase,
) {
    // Only react to ESC key press events (ignore key release)
    if event_state != KeyPhase::Pressed {
        log::debug!(
            "Ignoring ESC event since it is not a key press: {:?}",
            event_state
        );
        return;
    }

    // Handle ESC key for recording cancellation
    let current_state = get_recording_state(app_handle);
    log::debug!("Current recording state: {:?}", current_state);

    // Only handle ESC during recording or transcribing
    if !matches!(
        current_state,
        RecordingState::Recording
            | RecordingState::Transcribing
            | RecordingState::Starting
            | RecordingState::Stopping
    ) {
        return;
    }

    let was_pressed_once = app_state.esc_pressed_once.load(Ordering::SeqCst);

    if !was_pressed_once {
        handle_first_esc_press(app_state, app_handle).await;
    } else {
        handle_second_esc_press(app_state, app_handle).await;
    }
}

/// Handle first ESC press: Show confirmation hint and start timeout
async fn handle_first_esc_press(app_state: &AppState, app_handle: &AppHandle) {
    log::info!("First ESC press detected during recording");
    app_state.esc_pressed_once.store(true, Ordering::SeqCst);

    // Show inline hint for ESC warning (2 seconds)
    crate::observability::emit(
        "island_state",
        vec![("state", "escape_hint".into())],
        Some(crate::commands::audio::current_recording_generation()),
    );
    let phase = escape_phase(get_recording_state(app_handle));
    let _ = app_handle.emit_to(
        "pill",
        "escape-hint",
        serde_json::json!({
            "generation": crate::commands::audio::current_recording_generation(), "phase": phase
        }),
    );

    // Set timeout to reset ESC state after 2 seconds
    let app_for_timeout = app_handle.clone();
    let timeout_handle = tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        if let Some(app_state) = app_for_timeout.try_state::<AppState>() {
            app_state.esc_pressed_once.store(false, Ordering::SeqCst);
            log::debug!("ESC timeout expired, resetting state");
        } else {
            log::debug!("ESC timeout expired but AppState unavailable (app shutting down)");
        }
    });

    // Store timeout handle (abort previous one if exists)
    if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
        if let Some(old_handle) = timeout_guard.take() {
            old_handle.abort();
            log::debug!("Aborted previous ESC timeout handle");
        }
        *timeout_guard = Some(timeout_handle);
    }
}

/// Handle second ESC press: Cancel recording immediately
async fn handle_second_esc_press(app_state: &AppState, app_handle: &AppHandle) {
    log::info!("Second ESC press detected, cancelling recording");

    // Cancel timeout
    if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
        if let Some(handle) = timeout_guard.take() {
            handle.abort();
        }
    }

    // Reset ESC state
    app_state.esc_pressed_once.store(false, Ordering::SeqCst);

    // Cancel recording
    let app_for_cancel = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = cancel_recording(app_for_cancel).await {
            log::error!("Failed to cancel recording: {}", e);
        }
    });
}

fn escape_phase(state: RecordingState) -> &'static str {
    if state == RecordingState::Transcribing {
        "transcribing"
    } else {
        "recording"
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_escape_names_the_transcribing_phase_including_polish() {
        assert_eq!(escape_phase(RecordingState::Transcribing), "transcribing");
        assert_eq!(escape_phase(RecordingState::Recording), "recording");
    }
}
