use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

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
        receiver.recv_timeout(budget).ok().flatten()
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
        assert_eq!(
            QueryWorker::default().read(|| None, Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn timeout_discards_late_result_and_skips_busy_worker() {
        let worker = QueryWorker::default();
        let (release, wait) = mpsc::channel();
        assert_eq!(
            worker.read(
                move || {
                    wait.recv().unwrap();
                    Some('o')
                },
                Duration::from_millis(1)
            ),
            None
        );
        assert_eq!(
            worker.read(
                || panic!("must not accumulate workers"),
                Duration::from_secs(1)
            ),
            None
        );
        release.send(()).unwrap();
    }
}
