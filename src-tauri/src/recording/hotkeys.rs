use crate::commands::audio::{
    start_recording, stop_recording, RecorderState, PTT_START_ABORTED_AFTER_RELEASE,
};
use crate::commands::shortcuts::{
    self, hold_shortcut_transition, pressed_shortcut_should_run, CustomHoldTransition,
    ShortcutAction, ShortcutTrigger,
};
use crate::{get_recording_state, update_recording_state, AppState, RecordingState};
use keytrigger::KeyPhase;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;

/// Handle toggle mode recording (click to start/stop)
fn handle_toggle_mode(
    app: &tauri::AppHandle,
    app_state: &AppState,
    current_state: RecordingState,
    event_state: KeyPhase,
) {
    match event_state {
        KeyPhase::Released => {
            app_state.toggle_key_held.store(false, Ordering::SeqCst);
            return;
        }
        KeyPhase::Pressed => {}
    }

    if !claim_toggle_press(&app_state.toggle_key_held) {
        log::debug!("Toggle: Ignoring duplicate key press while hotkey is held");
        return;
    }

    let should_throttle = {
        let now = std::time::Instant::now();
        match app_state.last_toggle_press.lock() {
            Ok(mut last_press) => {
                if let Some(last) = *last_press {
                    if now.duration_since(last).as_millis() < 300 {
                        log::debug!("Toggle: Throttling hotkey press (too fast)");
                        true
                    } else {
                        *last_press = Some(now);
                        false
                    }
                } else {
                    *last_press = Some(now);
                    false
                }
            }
            Err(e) => {
                log::error!("Failed to lock last_toggle_press: {}", e);
                false
            }
        }
    };

    if should_throttle {
        crate::commands::audio::pill_toast(app, "Hold on...", 1000);
        return;
    }

    match current_state {
        RecordingState::Idle | RecordingState::Error => {
            log::info!("Toggle: Starting recording via hotkey");
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let recorder_state = app_handle.state::<RecorderState>();
                match start_recording(app_handle.clone(), recorder_state).await {
                    Ok(_) => log::info!("Toggle: Recording started successfully"),
                    Err(e) => {
                        log::error!("Toggle: Error starting recording: {}", e);
                        update_recording_state(&app_handle, RecordingState::Error, Some(e));
                    }
                }
            });
        }
        RecordingState::Starting => {
            log::info!("Toggle: stop requested while starting; will stop after start completes");
            app_state
                .pending_stop_after_start
                .store(true, Ordering::SeqCst);
        }
        RecordingState::Recording => {
            log::info!("Toggle: Stopping recording via hotkey");
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let recorder_state = app_handle.state::<RecorderState>();
                match stop_recording(app_handle.clone(), recorder_state).await {
                    Ok(_) => log::info!("Toggle: Recording stopped successfully"),
                    Err(e) => log::error!("Toggle: Error stopping recording: {}", e),
                }
            });
        }
        _ => log::debug!("Toggle: Ignoring hotkey in state {:?}", current_state),
    }
}

fn claim_toggle_press(toggle_key_held: &AtomicBool) -> bool {
    !toggle_key_held.swap(true, Ordering::SeqCst)
}

fn handle_hold_to_record_source(
    app: &tauri::AppHandle,
    app_state: &AppState,
    source_id: &str,
    event_state: KeyPhase,
) {
    let transition = match app_state.active_custom_hold_bindings.lock() {
        Ok(mut active_bindings) => {
            hold_shortcut_transition(&mut active_bindings, source_id, event_state)
        }
        Err(error) => {
            log::error!("Failed to lock active hold bindings: {}", error);
            CustomHoldTransition::Noop
        }
    };

    match transition {
        CustomHoldTransition::Start => {
            let current_state = get_recording_state(app);
            handle_ptt_mode(app, app_state, current_state, KeyPhase::Pressed);
        }
        CustomHoldTransition::Stop => {
            let current_state = get_recording_state(app);
            handle_ptt_mode(app, app_state, current_state, KeyPhase::Released);
        }
        CustomHoldTransition::Noop => {}
    }
}

/// Handle push-to-talk mode recording (hold to record, release to stop)
fn handle_ptt_mode(
    app: &tauri::AppHandle,
    app_state: &AppState,
    current_state: RecordingState,
    event_state: KeyPhase,
) {
    match event_state {
        KeyPhase::Pressed => {
            log::info!("PTT: Key pressed");
            app_state.ptt_key_held.store(true, Ordering::Relaxed);

            if matches!(current_state, RecordingState::Idle | RecordingState::Error) {
                log::info!("PTT: Starting recording");
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let recorder_state = app_handle.state::<RecorderState>();
                    match start_recording(app_handle.clone(), recorder_state).await {
                        Ok(_) => log::info!("PTT: Recording started successfully"),
                        Err(e) if e == PTT_START_ABORTED_AFTER_RELEASE => {
                            log::info!("PTT: Recording start cancelled after key release");
                            update_recording_state(&app_handle, RecordingState::Idle, None);
                        }
                        Err(e) => {
                            log::error!("PTT: Error starting recording: {}", e);
                            update_recording_state(&app_handle, RecordingState::Error, Some(e));
                        }
                    }
                });
            }
        }
        KeyPhase::Released => {
            log::info!("PTT: Key released");

            // Debounce: swap returns previous value, skip if already false (duplicate release)
            // This prevents race conditions when multiple release events fire rapidly
            if !app_state.ptt_key_held.swap(false, Ordering::SeqCst) {
                log::debug!("PTT: Ignoring duplicate key release");
                return;
            }

            match current_state {
                RecordingState::Recording => {
                    log::info!("PTT: Stopping recording");
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let recorder_state = app_handle.state::<RecorderState>();
                        match stop_recording(app_handle.clone(), recorder_state).await {
                            Ok(_) => log::info!("PTT: Recording stopped successfully"),
                            Err(e) => log::error!("PTT: Error stopping recording: {}", e),
                        }
                    });
                }
                RecordingState::Starting => {
                    // Key released while recording is still starting up.
                    // Set the pending flag so start_recording() can honor the stop
                    // as soon as it reaches the Recording state. This prevents
                    // recording from continuing after the user released PTT.
                    log::info!(
                        "PTT: Key released while Starting; setting pending_stop_after_start"
                    );
                    app_state
                        .pending_stop_after_start
                        .store(true, Ordering::SeqCst);
                }
                _ => {
                    log::debug!("PTT: Key released in state {:?}; no action", current_state);
                }
            }
        }
    }
}

fn should_dispatch_custom_pressed_binding(
    active_bindings: &mut std::collections::HashSet<String>,
    binding_id: &str,
    action: ShortcutAction,
    event_state: KeyPhase,
) -> bool {
    let should_run = pressed_shortcut_should_run(active_bindings, binding_id, event_state);
    should_run || (action == ShortcutAction::ToggleRecording && event_state == KeyPhase::Released)
}

/// Dispatch a shortcut action by (id, action, trigger) through the native engine.
pub(crate) fn dispatch_action(
    app: &tauri::AppHandle,
    app_state: &AppState,
    id: &str,
    action: ShortcutAction,
    trigger: ShortcutTrigger,
    event_state: KeyPhase,
) {
    if trigger == ShortcutTrigger::Pressed {
        let should_dispatch = match app_state.active_custom_pressed_bindings.lock() {
            Ok(mut active_bindings) => should_dispatch_custom_pressed_binding(
                &mut active_bindings,
                id,
                action,
                event_state,
            ),
            Err(error) => {
                log::error!("Failed to lock active custom pressed bindings: {}", error);
                false
            }
        };

        if !should_dispatch {
            return;
        }
    } else if action != ShortcutAction::HoldToRecord {
        return;
    }

    match action {
        ShortcutAction::ToggleRecording => {
            let current_state = get_recording_state(app);
            handle_toggle_mode(app, app_state, current_state, event_state);
        }
        ShortcutAction::HoldToRecord => {
            if trigger != ShortcutTrigger::Hold {
                return;
            }
            handle_hold_to_record_source(app, app_state, id, event_state);
        }
        ShortcutAction::CancelRecording => {
            if event_state == KeyPhase::Pressed {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = crate::commands::audio::cancel_recording(app_handle).await {
                        log::error!("Shortcut cancel_recording failed: {}", error);
                    }
                });
            }
        }
        ShortcutAction::CopyLastTranscription => {
            if event_state == KeyPhase::Pressed {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    match shortcuts::latest_copyable_transcription_text(&app_handle).await {
                        Ok(Some(text)) => {
                            if let Err(error) =
                                crate::commands::text::copy_text_to_clipboard(text).await
                            {
                                log::error!("Shortcut copy_last_transcription failed: {}", error);
                            }
                        }
                        Ok(None) => log::debug!(
                            "Shortcut copy_last_transcription found no copyable history item"
                        ),
                        Err(error) => {
                            log::error!("Shortcut copy_last_transcription failed: {}", error)
                        }
                    }
                });
            }
        }
        ShortcutAction::PasteLastTranscription => {
            if event_state == KeyPhase::Pressed {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    match shortcuts::latest_copyable_transcription_text(&app_handle).await {
                        Ok(Some(text)) => {
                            if let Err(error) =
                                crate::commands::text::insert_text(app_handle.clone(), text).await
                            {
                                log::error!("Shortcut paste_last_transcription failed: {}", error);
                            }
                        }
                        Ok(None) => log::debug!(
                            "Shortcut paste_last_transcription found no copyable history item"
                        ),
                        Err(error) => {
                            log::error!("Shortcut paste_last_transcription failed: {}", error)
                        }
                    }
                });
            }
        }
        ShortcutAction::ToggleAiFormatting => {
            if event_state == KeyPhase::Pressed {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = shortcuts::toggle_ai_formatting(app_handle).await {
                        log::error!("Shortcut toggle_ai_formatting failed: {e}");
                    }
                });
            }
        }
        ShortcutAction::OpenDashboard => {
            if event_state == KeyPhase::Pressed {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = crate::commands::window::focus_main_window(app_handle).await
                    {
                        log::error!("Shortcut open_dashboard failed: {}", error);
                    }
                });
            }
        }
        ShortcutAction::Unknown => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{claim_toggle_press, should_dispatch_custom_pressed_binding};
    use crate::commands::shortcuts::ShortcutAction;
    use keytrigger::KeyPhase;
    use std::{
        collections::HashSet,
        sync::atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn claim_toggle_press_blocks_repeats_until_release() {
        let held = AtomicBool::new(false);

        assert!(claim_toggle_press(&held));
        assert!(!claim_toggle_press(&held));

        held.store(false, Ordering::SeqCst);
        assert!(claim_toggle_press(&held));
    }

    #[test]
    fn custom_toggle_release_clears_held_state_for_next_press() {
        let mut active_bindings = HashSet::new();
        let held = AtomicBool::new(false);
        let binding_id = "custom-toggle";

        assert!(should_dispatch_custom_pressed_binding(
            &mut active_bindings,
            binding_id,
            ShortcutAction::ToggleRecording,
            KeyPhase::Pressed,
        ));
        assert!(claim_toggle_press(&held));

        assert!(!should_dispatch_custom_pressed_binding(
            &mut active_bindings,
            binding_id,
            ShortcutAction::ToggleRecording,
            KeyPhase::Pressed,
        ));

        assert!(should_dispatch_custom_pressed_binding(
            &mut active_bindings,
            binding_id,
            ShortcutAction::ToggleRecording,
            KeyPhase::Released,
        ));
        held.store(false, Ordering::SeqCst);

        assert!(should_dispatch_custom_pressed_binding(
            &mut active_bindings,
            binding_id,
            ShortcutAction::ToggleRecording,
            KeyPhase::Pressed,
        ));
        assert!(claim_toggle_press(&held));
    }
}
