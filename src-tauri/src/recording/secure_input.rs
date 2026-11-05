//! Secure Input interrupts capture, not the user's keyboard protection.
use std::time::{Duration, Instant};
use tauri::Manager;

/// A start validating across a hotkey-route change must not open the mic later.
pub(crate) struct StartPermit(keytrigger::SecureInputState);
impl StartPermit {
    pub(crate) fn capture<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Self {
        Self(
            app.state::<crate::AppState>()
                .trigger_engine
                .secure_input_state(),
        )
    }
    pub(crate) fn is_current<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) -> bool {
        self.0
            == app
                .state::<crate::AppState>()
                .trigger_engine
                .secure_input_state()
    }

    /// Covers a route change between the final start guard and Recording.
    /// The start owns this stop; a delayed external system task never queues it.
    pub(crate) fn pending_stop<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        pending: Option<Instant>,
    ) -> Option<Instant> {
        if self.is_current(app) {
            pending
        } else {
            Some(Instant::now())
        }
    }

    pub(crate) fn stop_post_roll<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        requested: Duration,
    ) -> Duration {
        if self.is_current(app) {
            requested
        } else {
            Duration::ZERO
        }
    }
}

/// A generation-targeted system stop must NEVER set the shared Starting flag.
/// Invalidated starts clean up at their own guards (including post-publication).
/// Ordinary user stops retain their existing queue semantics.
pub(crate) fn system_stop_must_yield(
    expected: Option<u64>,
    current: u64,
    state: crate::RecordingState,
) -> bool {
    expected
        .is_some_and(|generation| generation != current || state == crate::RecordingState::Starting)
}

#[cfg(target_os = "macos")]
static INTERRUPTED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);

pub(crate) fn post_roll(generation: u64, requested: Duration) -> Duration {
    #[cfg(target_os = "macos")]
    if INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst) == generation {
        return Duration::ZERO;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = generation;
    requested
}

#[cfg(target_os = "macos")]
pub(crate) fn changed(app: &tauri::AppHandle, state: keytrigger::SecureInputState) {
    use std::sync::atomic::Ordering;
    let app_state = app.state::<crate::AppState>();
    let held = clear_holds(&app_state);
    if state.enabled {
        app_state.take_recording_app_context();
    }
    let generation = crate::commands::audio::current_recording_generation();
    if (state.enabled || held)
        && matches!(
            app_state.get_current_state(),
            crate::RecordingState::Starting | crate::RecordingState::Recording
        )
    {
        INTERRUPTED.store(generation, Ordering::SeqCst);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            // Never stop a later pointer/tray take if this task was delayed.
            if crate::commands::audio::current_recording_generation() != generation
                || !matches!(
                    crate::get_recording_state(&app),
                    crate::RecordingState::Starting | crate::RecordingState::Recording
                )
            {
                return;
            }
            let recorder = app.state::<crate::commands::audio::RecorderState>();
            if crate::commands::audio::stop_recording_with_mode(
                app.clone(),
                recorder,
                Duration::ZERO,
                Some(generation),
            )
            .await
            .is_err()
            {
                log::warn!("Secure Input: could not stop interrupted capture");
            }
        });
    }
    crate::trigger::secure_input::refresh(app);
}

#[cfg(any(target_os = "macos", test))]
fn clear_holds(app_state: &crate::AppState) -> bool {
    use std::sync::atomic::Ordering;
    let held = app_state.ptt_key_held.swap(false, Ordering::SeqCst);
    app_state.toggle_key_held.store(false, Ordering::SeqCst);
    app_state
        .active_custom_hold_bindings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    app_state
        .active_custom_pressed_bindings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    held
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{atomic::Ordering, mpsc, Arc},
        time::Duration,
    };

    #[test]
    fn reset_clears_all_hold_ownership_without_claiming_a_start() {
        let state = crate::AppState::new();
        state.ptt_key_held.store(true, Ordering::SeqCst);
        state.toggle_key_held.store(true, Ordering::SeqCst);
        state
            .active_custom_hold_bindings
            .lock()
            .unwrap()
            .insert("primary".into());
        state
            .active_custom_pressed_bindings
            .lock()
            .unwrap()
            .insert("primary".into());
        assert!(clear_holds(&state));
        assert!(!clear_holds(&state));
        assert!(!state.toggle_key_held.load(Ordering::SeqCst));
        assert!(state.active_custom_hold_bindings.lock().unwrap().is_empty());
        assert!(state
            .active_custom_pressed_bindings
            .lock()
            .unwrap()
            .is_empty());
        assert_eq!(state.get_current_state(), crate::RecordingState::Idle);
    }

    #[test]
    fn delayed_start_loses_its_permit_even_after_secure_input_turns_off_again() {
        let app = tauri::test::mock_app();
        app.manage(crate::AppState::new());
        let permit = StartPermit::capture(app.handle());
        let (tx, rx) = mpsc::channel();
        let state = app.state::<crate::AppState>();
        state
            .trigger_engine
            .start_with_source_and_status(
                Arc::new(keytrigger::engine::MockSource::new(vec![
                    (
                        keytrigger::Msg::Control(keytrigger::Control::SecureInput(true)),
                        Duration::ZERO,
                    ),
                    (
                        keytrigger::Msg::Control(keytrigger::Control::SecureInput(false)),
                        Duration::ZERO,
                    ),
                ])),
                |_| {},
                move |state| {
                    let _ = tx.send(state);
                },
            )
            .unwrap();
        let first = rx.recv_timeout(Duration::from_secs(3));
        let second = rx.recv_timeout(Duration::from_secs(3));
        state.trigger_engine.stop();
        assert!(first.unwrap().enabled);
        assert!(!second.unwrap().enabled);
        assert!(!permit.is_current(app.handle()));
        assert!(permit.pending_stop(app.handle(), None).is_some());
        assert_eq!(
            permit.stop_post_roll(app.handle(), Duration::from_millis(250)),
            Duration::ZERO
        );
        assert!(StartPermit::capture(app.handle()).is_current(app.handle()));
        assert!(StartPermit::capture(app.handle())
            .pending_stop(app.handle(), None)
            .is_none());
    }

    #[test]
    fn old_system_stop_never_queues_against_a_new_start_but_user_stop_still_can() {
        use crate::RecordingState::{Recording, Starting};
        assert!(system_stop_must_yield(Some(7), 8, Starting));
        assert!(system_stop_must_yield(Some(7), 7, Starting));
        assert!(system_stop_must_yield(Some(7), 8, Recording));
        assert!(!system_stop_must_yield(Some(7), 7, Recording));
        assert!(!system_stop_must_yield(None, 8, Starting));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn interrupted_take_has_no_post_roll_but_next_take_keeps_the_user_stop_policy() {
        let generation = u64::MAX - 10;
        INTERRUPTED.store(generation, Ordering::SeqCst);
        assert_eq!(
            post_roll(generation, Duration::from_millis(250)),
            Duration::ZERO
        );
        assert_eq!(
            post_roll(generation + 1, Duration::from_millis(250)),
            Duration::from_millis(250)
        );
        INTERRUPTED.store(u64::MAX, Ordering::SeqCst);
    }
}
