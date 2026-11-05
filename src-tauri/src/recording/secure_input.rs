//! Secure Input interrupts capture, not the user's keyboard protection.
use crate::audio::recorder::CaptureInterrupt;
use std::time::{Duration, Instant};
use tauri::Manager;

/// The slot only locates a generation's latch. Each worker keeps its own latch,
/// so a retired sender or delayed system stop can never signal a replacement.
#[derive(Default)]
pub(crate) struct CaptureInterrupts(std::sync::Mutex<Option<(u64, CaptureInterrupt)>>);

impl CaptureInterrupts {
    pub(crate) fn begin(&self, generation: u64) -> CaptureInterrupt {
        let interrupt = CaptureInterrupt::default();
        let previous = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace((generation, interrupt.clone()));
        if let Some((_, retired)) = previous {
            retired.interrupt();
        }
        interrupt
    }

    fn is_interrupted(&self, generation: u64) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|(owned, signal)| *owned == generation && signal.is_interrupted())
    }

    pub(crate) fn interrupt(&self, generation: u64) -> bool {
        let interrupt = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .filter(|(owned, _)| *owned == generation)
            .map(|(_, signal)| signal.clone());
        if let Some(interrupt) = interrupt {
            interrupt.interrupt();
            true
        } else {
            false
        }
    }
}

/// A start validating across a hotkey-route change must not open the mic later.
#[derive(Clone, Copy)]
pub(crate) struct StartPermit(keytrigger::SecureInputState);
impl StartPermit {
    pub(crate) fn capture<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Self {
        Self::capture_state(&app.state::<crate::AppState>())
    }
    pub(crate) fn capture_state(state: &crate::AppState) -> Self {
        Self(state.trigger_engine.secure_input_state())
    }
    pub(crate) fn is_current<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) -> bool {
        self.is_current_state(&app.state::<crate::AppState>())
    }
    pub(crate) fn is_current_state(&self, state: &crate::AppState) -> bool {
        self.0 == state.trigger_engine.secure_input_state()
    }

    /// The final construction boundary, after asynchronous settings/remote work.
    pub(crate) fn start_if_current<R: tauri::Runtime, T, E>(
        &self,
        app: &tauri::AppHandle<R>,
        generation: u64,
        interrupt: &CaptureInterrupt,
        start: impl FnOnce() -> Result<T, E>,
    ) -> Result<Option<T>, E> {
        if !self.is_current(app)
            || interrupt.is_interrupted()
            || app.state::<crate::AppState>().is_cancellation_requested()
            || crate::commands::audio::recording_generation_is_stale(generation)
        {
            interrupt.interrupt();
            Ok(None)
        } else {
            start().map(Some)
        }
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

pub(crate) fn post_roll(
    app_state: &crate::AppState,
    generation: u64,
    requested: Duration,
) -> Duration {
    if app_state.capture_interrupts.is_interrupted(generation) {
        return Duration::ZERO;
    }
    requested
}

#[cfg(target_os = "macos")]
pub(crate) fn changed(app: &tauri::AppHandle, state: keytrigger::SecureInputState) {
    let app_state = app.state::<crate::AppState>();
    let held = clear_holds(&app_state);
    if state.enabled {
        app_state.take_recording_app_context();
    }
    let generation = crate::commands::audio::current_recording_generation();
    if interrupt_capture(&app_state, generation, state.enabled, held) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            // Stopping already owns finalization and transcription of the
            // captured prefix. Its latch cuts the tail without a second stop.
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
pub(crate) fn interrupt_capture(
    app_state: &crate::AppState,
    generation: u64,
    enabled: bool,
    held: bool,
) -> bool {
    app_state.pending_start.cancel_invalidated_input(app_state);
    let live = app_state.get_current_state();
    if live == crate::RecordingState::Starting
        || ((enabled || held)
            && matches!(
                live,
                crate::RecordingState::Recording | crate::RecordingState::Stopping
            ))
    {
        app_state.capture_interrupts.interrupt(generation);
        true
    } else {
        false
    }
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
#[path = "secure_input/cancel_tests.rs"]
mod cancel_tests;

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
        let generation = crate::commands::audio::current_recording_generation();
        let capture_interrupt = state.capture_interrupts.begin(generation);
        state
            .transition_recording_state(crate::RecordingState::Starting)
            .unwrap();
        let callback_app = app.handle().clone();
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
                    interrupt_capture(
                        &callback_app.state::<crate::AppState>(),
                        generation,
                        state.enabled,
                        false,
                    );
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
        assert!(capture_interrupt.is_interrupted());
        assert!(!state.pending_stop_after_start.load(Ordering::SeqCst));
        assert!(state.pending_stop_requested.lock().unwrap().is_none());
        // Model returning from a blocked settings/remote lookup. Even a latch
        // not yet attached to a worker must not authorize mic construction.
        let attempted: Result<Option<()>, ()> = permit.start_if_current(
            app.handle(),
            crate::commands::audio::current_recording_generation(),
            &CaptureInterrupt::default(),
            || panic!("an invalidated start must never construct a recorder"),
        );
        assert_eq!(attempted, Ok(None));
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

    #[test]
    fn interrupted_take_has_no_post_roll_but_next_take_keeps_the_user_stop_policy() {
        let state = crate::AppState::new();
        let generation = 7;
        let retired = state.capture_interrupts.begin(generation);
        state.capture_interrupts.interrupt(generation);
        assert_eq!(
            post_roll(&state, generation, Duration::from_millis(250)),
            Duration::ZERO
        );
        let next = state.capture_interrupts.begin(generation + 1);
        assert!(!state.capture_interrupts.interrupt(generation));
        retired.interrupt();
        assert!(!next.is_interrupted());
        assert_eq!(
            post_roll(&state, generation + 1, Duration::from_millis(250)),
            Duration::from_millis(250)
        );
    }
}
