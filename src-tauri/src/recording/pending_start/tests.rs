use super::*;
use crate::{
    commands::{
        audio::RecordingLicenseState,
        license::{CachedLicense, RuntimeLicenseCache},
    },
    license::{recording_readiness::wait_for_recording_license, LicenseState, LicenseStatus},
};
use std::sync::atomic::{AtomicBool, Ordering};

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
    let request = state
        .pending_start
        .claim(|| state.get_current_state())
        .unwrap();
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
    let request = state
        .pending_start
        .claim(|| state.get_current_state())
        .unwrap();
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
    let request = state
        .pending_start
        .claim(|| state.get_current_state())
        .unwrap();
    assert!(state
        .pending_start
        .claim(|| state.get_current_state())
        .is_none());
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
    assert!(state
        .pending_start
        .claim(|| state.get_current_state())
        .is_none());
    assert!(request.commit(|| panic!("cannot commit twice")).is_none());
}

#[tokio::test]
async fn cancel_after_ready_validation_still_prevents_publication() {
    let state = AppState::new();
    *state.license_cache.write().await = ready();
    let request = state
        .pending_start
        .claim(|| state.get_current_state())
        .unwrap();
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
    let request = state
        .pending_start
        .claim(|| state.get_current_state())
        .unwrap();
    assert!(!state.pending_start.cancel_before_recording(&state));
    assert!(state.is_cancellation_requested());
    assert!(request.commit(|| panic!("cancelled")).is_none());
}

#[test]
fn stale_request_drop_and_commit_cannot_touch_a_new_request() {
    let requests = StartRequests::default();
    let old = requests.claim(|| RecordingState::Idle).unwrap();
    assert!(requests.cancel());
    let new = requests.claim(|| RecordingState::Idle).unwrap();
    assert!(old.commit(|| panic!("stale")).is_none());
    drop(old);
    assert_eq!(new.commit(|| true), Some(true));
}
