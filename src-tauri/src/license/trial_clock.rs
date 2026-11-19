//! A trial stops at its expiry time even while the app stays open, without a
//! network call. The latest time this device has seen is kept in secure
//! storage, so moving the clock back cannot extend a trial: an earlier clock
//! needs a fresh license check instead.
use crate::license::{LicenseState, LicenseStatus};
use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Runtime};

pub(crate) const KEY: &str = "trial_clock_latest";
/// Small clock corrections (NTP) are not treated as moving the clock back.
const TOLERANCE_SECS: i64 = 5 * 60;
const PERSIST_EVERY_SECS: i64 = 60;

static LATEST: AtomicI64 = AtomicI64::new(i64::MIN);
static PERSISTED: AtomicI64 = AtomicI64::new(i64::MIN);
static LOADED: AtomicBool = AtomicBool::new(false);
/// The saved latest time could not be read: trials wait for an online check.
static UNREADABLE: AtomicBool = AtomicBool::new(false);
static UPDATE: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrialClock {
    Valid,
    Expired,
    /// The clock is earlier than a time already seen, or the trial's end is
    /// unknown: check the license online before honouring it.
    CheckRequired,
}

fn deadline(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|deadline| deadline.timestamp())
}

fn trial_clock_at(status: &LicenseStatus, now: i64, latest: i64, unreadable: bool) -> TrialClock {
    if status.status != LicenseState::Trial {
        return TrialClock::Valid;
    }
    let Some(deadline) = status.expires_at.as_deref().and_then(deadline) else {
        return TrialClock::CheckRequired;
    };
    if unreadable || now.saturating_add(TOLERANCE_SECS) < latest {
        return TrialClock::CheckRequired;
    }
    if now.max(latest) >= deadline {
        TrialClock::Expired
    } else {
        TrialClock::Valid
    }
}

/// Whether a cached trial may still be honoured right now.
pub(crate) fn trial_clock(status: &LicenseStatus) -> TrialClock {
    trial_clock_at(
        status,
        Utc::now().timestamp(),
        LATEST.load(Ordering::SeqCst),
        UNREADABLE.load(Ordering::SeqCst),
    )
}

/// The earliest the server's clock can be, from an active trial it reported:
/// its deadline minus the whole days it said were left.
fn server_floor(deadline: i64, days_left: i64) -> i64 {
    deadline - days_left.max(0) * 24 * 60 * 60
}

/// After the server confirmed an active trial, re-anchor the latest time seen
/// to the server's clock, so a clock that was wrong in the future can recover.
pub(crate) fn rebase_from_server<R: Runtime>(app: &AppHandle<R>, expires_at: &str, days_left: i32) {
    let Some(deadline) = deadline(expires_at) else {
        return;
    };
    let Ok(_update) = UPDATE.lock() else {
        return;
    };
    let latest = Utc::now()
        .timestamp()
        .max(server_floor(deadline, i64::from(days_left)));
    LATEST.store(latest, Ordering::SeqCst);
    LOADED.store(true, Ordering::SeqCst);
    if crate::secure_store::secure_set(app, KEY, &latest.to_string()).is_ok() {
        PERSISTED.store(latest, Ordering::SeqCst);
        UNREADABLE.store(false, Ordering::SeqCst);
    }
}

/// Records the current time as seen, loading and saving the latest time in
/// secure storage. Call before every license admission decision.
pub(crate) fn observe<R: Runtime>(app: &AppHandle<R>) {
    let Ok(_update) = UPDATE.lock() else {
        return;
    };
    if !LOADED.load(Ordering::SeqCst) {
        match crate::secure_store::secure_get(app, KEY) {
            Ok(None) => {}
            Ok(Some(saved)) => match saved.parse::<i64>() {
                Ok(saved) => {
                    LATEST.fetch_max(saved, Ordering::SeqCst);
                    PERSISTED.fetch_max(saved, Ordering::SeqCst);
                }
                Err(_) => UNREADABLE.store(true, Ordering::SeqCst),
            },
            Err(_) => UNREADABLE.store(true, Ordering::SeqCst),
        }
        LOADED.store(true, Ordering::SeqCst);
    }
    let now = Utc::now().timestamp();
    LATEST.fetch_max(now, Ordering::SeqCst);
    if now
        > PERSISTED
            .load(Ordering::SeqCst)
            .saturating_add(PERSIST_EVERY_SECS)
        && crate::secure_store::secure_set(app, KEY, &now.to_string()).is_ok()
    {
        PERSISTED.fetch_max(now, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEADLINE: &str = "2026-11-24T12:00:00Z";

    fn at(value: &str) -> i64 {
        DateTime::parse_from_rfc3339(value).unwrap().timestamp()
    }

    fn trial(expires_at: Option<&str>) -> LicenseStatus {
        LicenseStatus {
            status: LicenseState::Trial,
            trial_days_left: Some(1),
            license_type: None,
            license_key: None,
            expires_at: expires_at.map(str::to_owned),
            verification_state: None,
            verification_expires_at: None,
        }
    }

    #[test]
    fn a_trial_works_until_its_expiry_time() {
        let now = at("2026-11-24T11:59:00Z");
        assert_eq!(
            trial_clock_at(&trial(Some(DEADLINE)), now, now, false),
            TrialClock::Valid
        );
    }

    #[test]
    fn a_trial_stops_at_its_expiry_time_while_the_app_stays_open() {
        let now = at(DEADLINE);
        assert_eq!(
            trial_clock_at(&trial(Some(DEADLINE)), now, now, false),
            TrialClock::Expired
        );
        let later = at("2026-11-25T09:00:00Z");
        assert_eq!(
            trial_clock_at(&trial(Some(DEADLINE)), later, later, false),
            TrialClock::Expired
        );
    }

    #[test]
    fn moving_the_clock_back_does_not_extend_a_trial() {
        let seen = at("2026-11-25T09:00:00Z");
        let moved_back = at("2026-11-20T09:00:00Z");
        assert_eq!(
            trial_clock_at(&trial(Some(DEADLINE)), moved_back, seen, false),
            TrialClock::CheckRequired
        );
    }

    #[test]
    fn a_trial_without_a_known_end_waits_for_an_online_check() {
        let now = at("2026-11-20T09:00:00Z");
        assert_eq!(
            trial_clock_at(&trial(None), now, now, false),
            TrialClock::CheckRequired
        );
        assert_eq!(
            trial_clock_at(
                &trial(Some("Offline grace: 1 day remaining")),
                now,
                now,
                false
            ),
            TrialClock::CheckRequired
        );
    }

    #[test]
    fn an_unreadable_saved_time_waits_for_an_online_check() {
        let now = at("2026-11-20T09:00:00Z");
        assert_eq!(
            trial_clock_at(&trial(Some(DEADLINE)), now, now, true),
            TrialClock::CheckRequired
        );
    }

    #[test]
    fn the_server_bounds_how_far_back_a_trusted_clock_can_be() {
        let deadline = at(DEADLINE);
        assert_eq!(server_floor(deadline, 2), at("2026-11-22T12:00:00Z"));
        assert_eq!(server_floor(deadline, -3), deadline);
    }

    #[test]
    fn a_small_clock_correction_past_the_expiry_still_ends_the_trial() {
        let seen = at("2026-11-24T12:02:00Z");
        let corrected = at("2026-11-24T11:59:00Z");
        assert_eq!(
            trial_clock_at(&trial(Some(DEADLINE)), corrected, seen, false),
            TrialClock::Expired
        );
    }

    #[test]
    fn licenses_are_not_judged_by_the_trial_clock() {
        let mut licensed = trial(Some(DEADLINE));
        licensed.status = LicenseState::Licensed;
        let later = at("2026-12-01T00:00:00Z");
        assert_eq!(
            trial_clock_at(&licensed, later, later, false),
            TrialClock::Valid
        );
    }
}
