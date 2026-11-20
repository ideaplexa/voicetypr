//! Only an active trial or a license turns speech into text. The desktop
//! recording gate lives in `recording/island.rs`, the CLI's in `cli.rs` and
//! the network host's in `remote/http.rs`. This file holds onboarding's
//! First try allowance, the one exception.
#[cfg(test)]
use crate::commands::audio::recording_license_state;
use crate::commands::audio::RecordingLicenseState;
#[cfg(test)]
use crate::commands::license::{CachedLicense, RuntimeLicenseCache};
#[cfg(test)]
use crate::license::LicenseStatus;
use tauri::{AppHandle, Manager};

#[cfg(test)]
fn status_allows(status: LicenseStatus) -> bool {
    recording_license_state(&RuntimeLicenseCache::Ready(CachedLicense::new(status)))
        == RecordingLicenseState::Ready
}

/// Onboarding's First try is the one way to hear the engine before a trial or
/// license: `recording/onboarding_try.rs` records it and shows the text only
/// in the setup window. Ten tries per device, counted in secure storage, which
/// survives a settings reset and reopening onboarding.
pub(crate) const TRY_LIMIT: u32 = 10;
pub(crate) const TRY_KEY: &str = "onboarding_tries_used";
pub(crate) const TRY_USED_UP: &str =
    "The first tries on this device are used up. Start your free trial to keep dictating.";
pub(crate) const TRY_WINDOW: &str = "Try dictation from Voicetypr's setup window.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum TryAllowance {
    /// A trial or license is active: tries are not counted.
    Access,
    Tries {
        left: u32,
    },
    UsedUp,
}

/// An unreadable count is treated as used up.
fn tries_used(saved: Result<Option<String>, String>) -> Option<u32> {
    match saved {
        Ok(None) => Some(0),
        Ok(Some(value)) => value.parse().ok(),
        Err(_) => None,
    }
}

fn allowance(has_access: bool, used: Option<u32>) -> TryAllowance {
    if has_access {
        return TryAllowance::Access;
    }
    match used {
        Some(used) if used < TRY_LIMIT => TryAllowance::Tries {
            left: TRY_LIMIT - used,
        },
        _ => TryAllowance::UsedUp,
    }
}

/// A try runs only from the setup window, open and in front, before
/// onboarding is finished.
pub(crate) fn try_window_allows(
    label: &str,
    visible: bool,
    focused: bool,
    onboarding_completed: bool,
) -> bool {
    label == "main" && visible && focused && !onboarding_completed
}

pub(crate) async fn try_allowance(app: &AppHandle) -> TryAllowance {
    crate::license::trial_clock::observe(app);
    let cache = app.state::<crate::AppState>().license_cache.clone();
    let has_access = crate::license::recording_readiness::wait_for_recording_license(&cache).await
        == RecordingLicenseState::Ready;
    allowance(
        has_access,
        tries_used(crate::secure_store::secure_get(app, TRY_KEY)),
    )
}

/// Counts one try before its text can exist, from the count saved now.
/// Returns the tries left, or `None` when a trial or license made it free.
pub(crate) fn spend_try(app: &AppHandle, allowance: TryAllowance) -> Result<Option<u32>, String> {
    if allowance == TryAllowance::Access {
        return Ok(None);
    }
    match self::allowance(
        false,
        tries_used(crate::secure_store::secure_get(app, TRY_KEY)),
    ) {
        TryAllowance::Tries { left } => {
            let used = TRY_LIMIT - left + 1;
            crate::secure_store::secure_set(app, TRY_KEY, &used.to_string())
                .map_err(|_| TRY_USED_UP.to_string())?;
            Ok(Some(left - 1))
        }
        _ => Err(TRY_USED_UP.to_string()),
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
            expires_at: Some((chrono::Utc::now() + chrono::Duration::days(3)).to_rfc3339()),
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

    #[test]
    fn a_cached_trial_past_its_end_may_not_transcribe() {
        let mut ended = status(LicenseState::Trial);
        ended.expires_at = Some((chrono::Utc::now() - chrono::Duration::seconds(1)).to_rfc3339());
        assert!(!status_allows(ended));
        let mut unknown = status(LicenseState::Trial);
        unknown.expires_at = None;
        assert!(!status_allows(unknown));
    }
    #[test]
    fn a_try_without_access_is_counted_and_stops_at_ten() {
        assert_eq!(allowance(false, Some(0)), TryAllowance::Tries { left: 10 });
        assert_eq!(allowance(false, Some(9)), TryAllowance::Tries { left: 1 });
        assert_eq!(allowance(false, Some(10)), TryAllowance::UsedUp);
        assert_eq!(allowance(false, Some(40)), TryAllowance::UsedUp);
        assert_eq!(allowance(true, Some(40)), TryAllowance::Access);
    }

    /// Tries left open the setup window's First try and nothing else: the
    /// hotkey, file jobs, kept retries and History re-transcribe
    /// (`validate_recording_license`), the CLI (`cli_license_decision`) and
    /// the network host (`host_license_refusal`) all read the license cache,
    /// which knows nothing of tries.
    #[test]
    fn tries_left_open_only_the_first_try() {
        assert_eq!(allowance(false, Some(0)), TryAllowance::Tries { left: 10 });
        let unlicensed = recording_license_state(&RuntimeLicenseCache::Ready(CachedLicense::new(
            status(LicenseState::None),
        )));
        assert_eq!(unlicensed, RecordingLicenseState::Blocked);
        assert_eq!(
            crate::remote::server::host_license_refusal(unlicensed),
            Some(crate::remote::server::HOST_LICENSE_REQUIRED)
        );
    }

    #[test]
    fn an_unreadable_try_count_gives_no_try() {
        assert_eq!(tries_used(Ok(None)), Some(0));
        assert_eq!(tries_used(Ok(Some("3".into()))), Some(3));
        assert_eq!(tries_used(Ok(Some("-1".into()))), None);
        assert_eq!(tries_used(Ok(Some("lots".into()))), None);
        assert_eq!(tries_used(Err("locked".into())), None);
        assert_eq!(
            allowance(false, tries_used(Err("locked".into()))),
            TryAllowance::UsedUp
        );
    }

    #[test]
    fn a_try_runs_only_in_the_setup_window_in_front() {
        assert!(try_window_allows("main", true, true, false));
        assert!(!try_window_allows("main", true, false, false));
        assert!(!try_window_allows("main", false, true, false));
        assert!(!try_window_allows("main", true, true, true));
        assert!(!try_window_allows("pill", true, true, false));
    }

    #[test]
    fn resetting_app_data_keeps_the_try_count() {
        let data = std::path::Path::new("/data");
        let secure = data.join(crate::secure_store::SECURE_STORE_FILE);
        for (dir, _) in crate::commands::reset::app_data_directories_to_clear(data) {
            assert!(!secure.starts_with(dir));
        }
    }
}
