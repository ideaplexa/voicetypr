use super::*;
use crate::{
    commands::{
        audio::RecordingLicenseState,
        license::{CachedLicense, RuntimeLicenseCache},
    },
    license::{recording_readiness::wait_for_recording_license, LicenseState, LicenseStatus},
};
use std::sync::atomic::{AtomicBool, Ordering};

fn claim(state: &AppState) -> Option<StartRequest<'_>> {
    state.pending_start.claim(
        state,
        StartSource::Hotkey,
        StartPermit::capture_state(state),
    )
}
fn ready() -> RuntimeLicenseCache {
    RuntimeLicenseCache::Ready(CachedLicense::new(LicenseStatus {
        status: LicenseState::Licensed,
        trial_days_left: None,
        license_type: None,
        license_key: None,
        expires_at: None,
        verification_state: None,
        verification_expires_at: None,
    }))
}

#[tokio::test]
async fn second_toggle_during_loading_never_opens_the_mic() {
    let state = AppState::new();
    let request = claim(&state).unwrap();
    let opened = AtomicBool::new(false);
    let (result, ()) = tokio::join!(
        async {
            let validation = request
                .validate(wait_for_recording_license(&state.license_cache))
                .await;
            if validation.is_some() {
                request.commit(|| opened.store(true, Ordering::SeqCst));
            }
            validation
        },
        async {
            tokio::task::yield_now().await;
            // The debounced toggle/stop path cancels the pending request while Idle.
            assert!(state.pending_start.cancel());
            *state.license_cache.write().await = ready();
        }
    );
    assert!(result.is_none());
    assert!(!opened.load(Ordering::SeqCst));
    assert_eq!(state.get_current_state(), RecordingState::Idle);
}

#[tokio::test]
async fn cancel_during_loading_cannot_be_cleared_by_a_late_start() {
    let state = AppState::new();
    let request = claim(&state).unwrap();
    let opened = AtomicBool::new(false);
    let (result, ()) = tokio::join!(
        request.validate(wait_for_recording_license(&state.license_cache)),
        async {
            tokio::task::yield_now().await;
            assert!(state.pending_start.cancel_before_recording(&state));
            *state.license_cache.write().await = ready();
        }
    );
    assert!(result.is_none());
    assert!(request
        .commit(|| {
            state.clear_cancellation();
            opened.store(true, Ordering::SeqCst);
        })
        .is_none());
    assert!(state.is_cancellation_requested());
    assert!(!opened.load(Ordering::SeqCst));
}

#[tokio::test]
async fn ready_start_commits_once_and_keeps_duplicate_calls_idempotent() {
    let state = AppState::new();
    state.request_cancellation(); // Stale cancellation from a previous take.
    *state.license_cache.write().await = ready();
    let request = claim(&state).unwrap();
    assert!(claim(&state).is_none());
    assert_eq!(
        request
            .validate(wait_for_recording_license(&state.license_cache))
            .await,
        Some(RecordingLicenseState::Ready)
    );
    assert_eq!(
        request.commit(|| {
            state.clear_cancellation();
            state
                .transition_recording_state(RecordingState::Starting)
                .unwrap();
        }),
        Some(())
    );
    assert!(!state.is_cancellation_requested());
    assert!(!state.pending_start.cancel());
    assert!(claim(&state).is_none());
    assert!(request.commit(|| panic!("cannot commit twice")).is_none());
}

#[tokio::test]
async fn cancel_after_ready_validation_still_prevents_publication() {
    let state = AppState::new();
    *state.license_cache.write().await = ready();
    let request = claim(&state).unwrap();
    assert!(request
        .validate(wait_for_recording_license(&state.license_cache))
        .await
        .is_some());
    assert!(state.pending_start.cancel_before_recording(&state));
    assert!(request.commit(|| state.clear_cancellation()).is_none());
    assert!(state.is_cancellation_requested());
    assert_eq!(state.get_current_state(), RecordingState::Idle);
}

#[test]
fn cancelling_a_pending_request_with_an_existing_take_still_requires_cleanup() {
    let state = AppState::new();
    for phase in [
        RecordingState::Starting,
        RecordingState::Recording,
        RecordingState::Stopping,
        RecordingState::Transcribing,
    ] {
        state.transition_recording_state(phase).unwrap();
    }
    let request = claim(&state).unwrap();
    assert!(!state.pending_start.cancel_before_recording(&state));
    assert!(state.is_cancellation_requested());
    assert!(request.commit(|| panic!("cancelled")).is_none());
}

#[test]
fn stale_request_drop_and_commit_cannot_touch_a_new_request() {
    let state = AppState::new();
    let old = claim(&state).unwrap();
    assert!(state.pending_start.cancel());
    let new = claim(&state).unwrap();
    assert!(old.commit(|| panic!("stale")).is_none());
    drop(old);
    assert_eq!(new.commit(|| true), Some(true));
}

#[tokio::test]
async fn license_pending_start_is_retired_by_secure_input_even_after_exit() {
    use std::time::Duration;
    let state = Arc::new(AppState::new());
    let request = claim(&state).unwrap();
    let callback_state = state.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (validation, transitions) = tokio::join!(
        tokio::time::timeout(
            Duration::from_secs(2),
            request.validate(wait_for_recording_license(&state.license_cache))
        ),
        async {
            tokio::task::yield_now().await;
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
                    move |route| {
                        super::super::secure_input::interrupt_capture(
                            &callback_state,
                            audio::current_recording_generation(),
                            route.enabled,
                            false,
                        );
                        let _ = tx.send(route);
                    },
                )
                .unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                (rx.recv().await, rx.recv().await)
            })
            .await
        }
    );
    state.trigger_engine.stop();
    assert_eq!(validation.unwrap(), None);
    let (entered, exited) = transitions.unwrap();
    assert!(entered.unwrap().enabled);
    assert!(!exited.unwrap().enabled);
    *state.license_cache.write().await = ready();
    assert!(request
        .commit(|| panic!("late license readiness cannot construct a mic"))
        .is_none());
    assert!(!state.is_cancellation_requested());
    assert!(!state.pending_stop_after_start.load(Ordering::SeqCst));
    assert!(state.pending_stop_requested.lock().unwrap().is_none());

    // A delayed notification must not retire a request made in the new route.
    let replacement = claim(&state).unwrap();
    super::super::secure_input::interrupt_capture(
        &state,
        audio::current_recording_generation(),
        true,
        false,
    );
    drop(request);
    assert_eq!(replacement.commit(|| true), Some(true));
}

#[test]
fn hold_release_cancels_only_pending_hold_sources() {
    for mode in [
        crate::RecordingMode::Toggle,
        crate::RecordingMode::PushToTalk,
    ] {
        for source in [
            StartSource::Hotkey,
            StartSource::HoldHotkey,
            StartSource::Pointer,
            StartSource::Tray,
        ] {
            let state = AppState::new();
            *state.recording_mode.lock().unwrap() = mode;
            state.ptt_key_held.store(true, Ordering::SeqCst);
            let request = state
                .pending_start
                .claim(&state, source, StartPermit::capture_state(&state))
                .unwrap();
            state.ptt_key_held.store(false, Ordering::SeqCst);
            let should_cancel =
                source.blocked_after_release(mode == crate::RecordingMode::PushToTalk, false);
            assert_eq!(
                state.pending_start.cancel_released_hold(&state),
                should_cancel
            );
            assert_eq!(
                request.commit(|| true),
                if should_cancel { None } else { Some(true) }
            );
        }
    }
}
