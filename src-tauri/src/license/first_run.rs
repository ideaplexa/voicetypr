//! A new user's first dictations in onboarding, before any trial exists.
//! Only a device whose trial was never started qualifies, only until
//! onboarding finishes, and only a few times per device; an ended trial
//! never does. The count lives in secure storage, which a reset keeps.
use crate::commands::audio::RecordingLicenseState;
use crate::commands::license::RuntimeLicenseCache;
use crate::license::LicenseState;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

static CLAIM: Mutex<()> = Mutex::new(());

pub(crate) const FIRST_RUN_DICTATIONS: u64 = 5;
const USED_KEY: &str = "first_run_dictations_used";

pub(crate) fn allows(cache: &RuntimeLicenseCache, onboarding_completed: bool, used: u64) -> bool {
    matches!(cache, RuntimeLicenseCache::Ready(cached) if cached.status.status == LicenseState::None)
        && !onboarding_completed
        && used < FIRST_RUN_DICTATIONS
}

/// The license check for a new recording (not file transcription), with the
/// first-run allowance applied.
pub(crate) async fn validate_recording_license(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let license =
        crate::license::recording_readiness::wait_for_recording_license(&state.license_cache).await;
    if license == RecordingLicenseState::Blocked && claim(app).await {
        return Ok(());
    }
    crate::recording::island::validate_recording_license(app).await
}

/// Uses one first-run dictation when this device may still have one.
async fn claim(app: &AppHandle) -> bool {
    let state = app.state::<crate::AppState>();
    let completed = app
        .store("settings")
        .ok()
        .and_then(|store| store.get("onboarding_completed"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let cache = state.license_cache.read().await.clone();
    let Ok(_claim) = CLAIM.lock() else {
        return false;
    };
    let used = match crate::secure_store::secure_get(app, USED_KEY) {
        Ok(value) => value.and_then(|count| count.parse().ok()).unwrap_or(0),
        Err(_) => return false,
    };
    if !allows(&cache, completed, used) {
        return false;
    }
    if crate::secure_store::secure_set(app, USED_KEY, &(used + 1).to_string()).is_err() {
        log::warn!("Could not save the first-run dictation count");
        return false;
    }
    log::info!(
        "First-run dictation {} of {FIRST_RUN_DICTATIONS} before a trial",
        used + 1
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::license::CachedLicense;
    use crate::license::LicenseStatus;

    fn cache(status: LicenseState) -> RuntimeLicenseCache {
        RuntimeLicenseCache::Ready(CachedLicense::new(LicenseStatus {
            status,
            trial_days_left: None,
            license_type: None,
            license_key: None,
            expires_at: None,
            verification_state: None,
            verification_expires_at: None,
        }))
    }

    #[test]
    fn a_device_without_a_trial_can_try_dictation_during_onboarding() {
        assert!(allows(&cache(LicenseState::None), false, 0));
        assert!(allows(
            &cache(LicenseState::None),
            false,
            FIRST_RUN_DICTATIONS - 1
        ));
    }

    #[test]
    fn first_run_dictations_run_out() {
        assert!(!allows(
            &cache(LicenseState::None),
            false,
            FIRST_RUN_DICTATIONS
        ));
    }

    #[test]
    fn finished_onboarding_gets_no_free_dictation() {
        assert!(!allows(&cache(LicenseState::None), true, 0));
    }

    #[test]
    fn an_ended_trial_or_an_unknown_licence_never_gets_free_dictation() {
        assert!(!allows(&cache(LicenseState::Expired), false, 0));
        assert!(!allows(&RuntimeLicenseCache::Failed, false, 0));
        assert!(!allows(&RuntimeLicenseCache::Loading, false, 0));
    }
}
