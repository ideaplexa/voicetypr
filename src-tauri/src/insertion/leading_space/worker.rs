use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::{Duration, Instant};

const TIMEOUT_BACKOFF: Duration = Duration::from_secs(30);
static LAST_TIMEOUT: Mutex<Option<Instant>> = Mutex::new(None);

fn is_backing_off(last_timeout: Option<Instant>, now: Instant) -> bool {
    last_timeout.is_some_and(|timeout| now.saturating_duration_since(timeout) < TIMEOUT_BACKOFF)
}

/// Accessibility providers are foreign code and may ignore their OS timeout.
/// Never wait longer than the delivery budget; never accumulate stuck workers.
#[derive(Default)]
pub(super) struct QueryWorker {
    busy: Arc<AtomicBool>,
}

struct Lease(Arc<AtomicBool>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl QueryWorker {
    pub(super) fn read(
        &self,
        query: impl FnOnce() -> Option<char> + Send + 'static,
        budget: Duration,
    ) -> Option<char> {
        self.read_with_backoff(query, budget, &LAST_TIMEOUT)
    }

    fn read_with_backoff(
        &self,
        query: impl FnOnce() -> Option<char> + Send + 'static,
        budget: Duration,
        last_timeout: &Mutex<Option<Instant>>,
    ) -> Option<char> {
        // One global cooldown, with no app identity or context retained. Do not
        // hold the lock while invoking foreign accessibility providers.
        if is_backing_off(
            *last_timeout
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            Instant::now(),
        ) {
            return None;
        }
        if self
            .busy
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return None;
        }
        let lease = Lease(self.busy.clone());
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("insertion-boundary".into())
            .spawn(move || {
                let _lease = lease;
                let result = query();
                let _ = sender.send(result);
            })
            .ok()?;
        // A timed-out result has no receiver and can never affect a later paste.
        match receiver.recv_timeout(budget) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                *last_timeout
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Instant::now());
                None
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_query_returns_character() {
        assert_eq!(
            QueryWorker::default().read(|| Some('o'), Duration::from_secs(1)),
            Some('o')
        );
    }

    #[test]
    fn errors_do_not_guess_context() {
        let last_timeout = Mutex::new(None);
        assert_eq!(
            QueryWorker::default().read_with_backoff(
                || None,
                Duration::from_secs(1),
                &last_timeout,
            ),
            None
        );
        assert!(last_timeout.lock().unwrap().is_none());
        assert_eq!(
            QueryWorker::default().read_with_backoff(
                || Some('o'),
                Duration::from_secs(1),
                &last_timeout,
            ),
            Some('o')
        );
    }

    #[test]
    fn timeout_discards_late_result_and_skips_busy_worker() {
        let worker = QueryWorker::default();
        let last_timeout = Mutex::new(None);
        let (release, wait) = mpsc::channel();
        assert_eq!(
            worker.read_with_backoff(
                move || {
                    wait.recv().unwrap();
                    Some('o')
                },
                Duration::from_millis(1),
                &last_timeout,
            ),
            None
        );
        assert_eq!(
            worker.read_with_backoff(|| Some('x'), Duration::from_secs(1), &Mutex::new(None),),
            None
        );
        release.send(()).unwrap();
    }

    #[test]
    fn timeout_backoff_skips_idle_workers_and_allows_retry_after_expiry() {
        let last_timeout = Mutex::new(None);
        let (release, wait) = mpsc::channel();
        assert_eq!(
            QueryWorker::default().read_with_backoff(
                move || {
                    wait.recv().unwrap();
                    Some('o')
                },
                Duration::from_millis(1),
                &last_timeout,
            ),
            None
        );
        assert!(last_timeout.lock().unwrap().is_some());
        let idle_worker = QueryWorker::default();
        assert_eq!(
            idle_worker.read_with_backoff(|| Some('x'), Duration::from_secs(1), &last_timeout,),
            None
        );
        assert!(!idle_worker.busy.load(Ordering::Acquire));
        *last_timeout.lock().unwrap() = Some(Instant::now() - TIMEOUT_BACKOFF);
        assert_eq!(
            idle_worker.read_with_backoff(|| Some('x'), Duration::from_secs(1), &last_timeout),
            Some('x')
        );
        release.send(()).unwrap();
    }

    #[test]
    fn backoff_lasts_thirty_seconds_without_sleeping() {
        let timeout = Instant::now();
        assert!(!is_backing_off(None, timeout));
        assert!(is_backing_off(Some(timeout), timeout));
        assert!(is_backing_off(
            Some(timeout),
            timeout + TIMEOUT_BACKOFF - Duration::from_nanos(1)
        ));
        assert!(!is_backing_off(Some(timeout), timeout + TIMEOUT_BACKOFF));
        assert!(!is_backing_off(
            Some(timeout),
            timeout + TIMEOUT_BACKOFF + Duration::from_secs(1)
        ));
    }
}
