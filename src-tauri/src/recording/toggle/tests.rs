use super::*;
use crate::{
    commands::{
        audio::RecordingLicenseState,
        license::{CachedLicense, RuntimeLicenseCache},
    },
    license::{recording_readiness::wait_for_recording_license, LicenseState, LicenseStatus},
    recording::secure_input::StartPermit,
};

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
async fn toggle_cancels_after_ready_validation_and_routes_published_states() {
    let state = AppState::new();
    *state.license_cache.write().await = ready();
    let request = state
        .pending_start
        .claim(
            &state,
            StartSource::Pointer,
            StartPermit::capture_state(&state),
        )
        .unwrap();
    assert_eq!(
        request
            .validate(wait_for_recording_license(&state.license_cache))
            .await,
        Some(RecordingLicenseState::Ready)
    );
    // A toggle after validation but before publication still cancels.
    assert_eq!(route(&state), ToggleRoute::CancelledPendingStart);
    drop(request);

    for (current, expected) in [
        (RecordingState::Starting, ToggleRoute::Stop),
        (RecordingState::Recording, ToggleRoute::Stop),
        (RecordingState::Stopping, ToggleRoute::Finishing),
        (RecordingState::Transcribing, ToggleRoute::Finishing),
        (RecordingState::Error, ToggleRoute::Start),
    ] {
        state.transition_recording_state(current).unwrap();
        assert_eq!(route(&state), expected, "{current:?}");
    }
}

#[test]
fn route_serializes_as_closed_snake_case_codes() {
    for (value, code) in [
        (
            ToggleRoute::CancelledPendingStart,
            "cancelled_pending_start",
        ),
        (ToggleRoute::Start, "start"),
        (ToggleRoute::Stop, "stop"),
        (ToggleRoute::Finishing, "finishing"),
        (ToggleRoute::NoChange, "no_change"),
    ] {
        assert_eq!(serde_json::to_value(value).unwrap(), code);
    }
}

#[test]
fn a_cancelled_or_retired_start_is_not_reported_as_started() {
    assert_eq!(start_result(false), ToggleRoute::NoChange);
    assert_eq!(start_result(true), ToggleRoute::Start);
}
