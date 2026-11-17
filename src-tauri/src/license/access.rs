//! Only an active trial or a license turns speech into text. The desktop
//! recording gate lives in `recording/island.rs`; these cover the CLI and a
//! computer sharing its engine on the network.
use crate::commands::audio::{recording_license_state, RecordingLicenseState};
use crate::commands::license::{CachedLicense, RuntimeLicenseCache};
use crate::license::LicenseStatus;
use tauri::{AppHandle, Manager};

pub(crate) const NO_ACCESS: &str =
    "Voicetypr needs an active trial or license to transcribe. Start your free trial or add a license key in the app.";

fn status_allows(status: LicenseStatus) -> bool {
    recording_license_state(&RuntimeLicenseCache::Ready(CachedLicense::new(status)))
        == RecordingLicenseState::Ready
}

/// Checks the license now; a failed check never grants access.
pub(crate) async fn require_for_cli(app: &AppHandle) -> Result<(), String> {
    let status = crate::commands::license::check_license_status(app.clone()).await?;
    if status_allows(status) {
        Ok(())
    } else {
        Err(NO_ACCESS.to_string())
    }
}

/// Whether this computer may transcribe audio another computer sent it.
/// Waits like the desktop gate while a license check is running.
pub(crate) fn require_for_sharing(app: &AppHandle) -> Result<(), String> {
    let cache = app.state::<crate::AppState>().license_cache.clone();
    let state = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(crate::license::recording_readiness::wait_for_recording_license(&cache))
    });
    if state == RecordingLicenseState::Ready {
        Ok(())
    } else {
        Err(NO_ACCESS.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license::LicenseState;

    fn status(state: LicenseState) -> LicenseStatus {
        LicenseStatus {
            status: state,
            trial_days_left: None,
            license_type: None,
            license_key: None,
            expires_at: None,
            verification_state: None,
            verification_expires_at: None,
        }
    }

    #[test]
    fn only_a_trial_or_license_may_transcribe() {
        assert!(status_allows(status(LicenseState::Trial)));
        assert!(status_allows(status(LicenseState::Licensed)));
        assert!(!status_allows(status(LicenseState::None)));
        assert!(!status_allows(status(LicenseState::Expired)));
    }
}
