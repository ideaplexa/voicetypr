//! Join an in-flight cold-start check without another network request or an
//! optimistic entitlement. Ordinary starts take only the existing cache read.
use crate::commands::{
    audio::{recording_license_state, RecordingLicenseState},
    license::RuntimeLicenseCache,
};
use std::time::Duration;
use tokio::sync::RwLock;

const STARTUP_WAIT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

pub(crate) async fn wait_for_recording_license(
    cache: &RwLock<RuntimeLicenseCache>,
) -> RecordingLicenseState {
    wait_with_timeout(cache, STARTUP_WAIT).await
}

async fn wait_with_timeout(
    cache: &RwLock<RuntimeLicenseCache>,
    timeout: Duration,
) -> RecordingLicenseState {
    tokio::time::timeout(timeout, async {
        loop {
            let state = recording_license_state(&*cache.read().await);
            if state != RecordingLicenseState::Loading {
                return state;
            }
            // Never hold the cache lock while waiting for its writer.
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    })
    .await
    .unwrap_or(RecordingLicenseState::Loading)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commands::license::CachedLicense,
        license::{LicenseState, LicenseStatus},
    };
    use futures_util::FutureExt;

    fn cached(status: LicenseState) -> RuntimeLicenseCache {
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

    #[tokio::test]
    async fn cold_start_waits_for_entitlement_instead_of_refusing_loading() {
        for (status, expected) in [
            (LicenseState::Licensed, RecordingLicenseState::Ready),
            (LicenseState::Trial, RecordingLicenseState::Ready),
            (LicenseState::Expired, RecordingLicenseState::Blocked),
            (LicenseState::None, RecordingLicenseState::Blocked),
        ] {
            let cache = RwLock::new(RuntimeLicenseCache::Loading);
            let (result, ()) =
                tokio::join!(wait_with_timeout(&cache, Duration::from_secs(1)), async {
                    tokio::task::yield_now().await;
                    *cache.write().await = cached(status);
                });
            assert_eq!(result, expected);
        }
    }

    #[tokio::test]
    async fn failed_check_is_not_licensed_and_a_stuck_check_is_bounded() {
        let cache = RwLock::new(RuntimeLicenseCache::Loading);
        let (result, ()) = tokio::join!(wait_with_timeout(&cache, Duration::from_secs(1)), async {
            tokio::task::yield_now().await;
            *cache.write().await = RuntimeLicenseCache::Failed;
        });
        assert_eq!(result, RecordingLicenseState::CheckFailed);
        *cache.write().await = RuntimeLicenseCache::Loading;
        assert_eq!(
            wait_with_timeout(&cache, Duration::from_millis(1)).await,
            RecordingLicenseState::Loading
        );
    }

    #[tokio::test]
    async fn hold_released_during_license_wait_still_aborts_after_validation() {
        use crate::{
            commands::audio::ptt_key_released, recording::start_source::StartSource, AppState,
            RecordingMode,
        };
        use std::sync::atomic::Ordering;
        for mode in [RecordingMode::Toggle, RecordingMode::PushToTalk] {
            let state = AppState::new();
            *state.recording_mode.lock().unwrap() = mode;
            state.ptt_key_held.store(true, Ordering::SeqCst);
            let (license, ()) =
                tokio::join!(wait_for_recording_license(&state.license_cache), async {
                    tokio::task::yield_now().await;
                    state.ptt_key_held.store(false, Ordering::SeqCst);
                    *state.license_cache.write().await = cached(LicenseState::Licensed);
                });
            assert_eq!(license, RecordingLicenseState::Ready);
            // start_recording runs this guard after validation, before opening
            // a generation or claiming ownership of a recording.
            assert!(ptt_key_released(&state, StartSource::HoldHotkey));
            assert!(!ptt_key_released(&state, StartSource::Pointer));
        }
    }

    #[tokio::test]
    async fn completed_cache_never_sleeps_and_offline_deadline_is_still_enforced() {
        let mut entry = cached(LicenseState::Licensed);
        let cache = RwLock::new(entry.clone());
        // A ready cache completes on the first poll, with no timer wake.
        assert!(wait_for_recording_license(&cache).now_or_never().is_some());
        let RuntimeLicenseCache::Ready(ref mut license) = entry else {
            unreachable!()
        };
        license.status.verification_expires_at = Some(chrono::Utc::now());
        *cache.write().await = entry;
        assert_eq!(
            wait_for_recording_license(&cache).await,
            RecordingLicenseState::VerificationRequired
        );
    }
}
