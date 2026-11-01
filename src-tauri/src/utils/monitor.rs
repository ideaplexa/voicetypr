use std::panic::{catch_unwind, AssertUnwindSafe};

/// Run a monitor query and return `None` if it panics on a stale monitor handle.
///
/// tao 0.34.5 builds each `Monitor` by calling `GetMonitorInfoW().unwrap()`,
/// which panics with `Os { code: 1461 }` (ERROR_INVALID_MONITOR_HANDLE) when a
/// monitor handle goes stale mid display-change (dock / undock / sleep). tauri
/// runs that conversion *inside* `current_monitor()` / `primary_monitor()` /
/// `available_monitors()` (tauri-runtime-wry `From<MonitorHandleWrapper>`), so
/// the panic surfaces during the query — not at `Monitor::size()`, which only
/// returns an already-cached field. Wrapping the whole query + field read lets a
/// transient stale handle fall back gracefully instead of crashing the app.
/// (`profile.release` is `panic = "unwind"`, so the catch is effective.)
pub fn catch_monitor_panic<T>(query: impl FnOnce() -> T) -> Option<T> {
    // This pool must surround the actual query on the main thread, not the
    // worker dispatching it. Retaining a dictionary alone does not drain its
    // autoreleased backing storage.
    #[cfg(target_os = "macos")]
    let query = || objc2::rc::autoreleasepool(|_| query());
    catch_unwind(AssertUnwindSafe(query)).ok()
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, PartialEq)]
enum DispatchError {
    Unavailable,
    Timeout,
}

#[cfg(any(target_os = "macos", test))]
fn dispatch_query<T: Send + 'static>(
    is_main: bool,
    dispatch: impl FnOnce(Box<dyn FnOnce() + Send>) -> bool,
    query: impl FnOnce() -> Option<T> + Send + 'static,
    timeout: std::time::Duration,
) -> Result<Option<T>, DispatchError> {
    if is_main {
        return Ok(catch_monitor_panic(query).flatten());
    }
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    if !dispatch(Box::new(move || {
        let _ = send.send(catch_monitor_panic(query).flatten());
    })) {
        return Err(DispatchError::Unavailable);
    }
    receive.recv_timeout(timeout).map_err(|error| match error {
        std::sync::mpsc::RecvTimeoutError::Timeout => DispatchError::Timeout,
        std::sync::mpsc::RecvTimeoutError::Disconnected => DispatchError::Unavailable,
    })
}

/// No Tokio work or locks are waited on by the main-thread closure. Only an
/// off-main caller waits, with a hard bound, including during shutdown.
#[cfg(target_os = "macos")]
pub(crate) fn on_main<T: Send + 'static>(
    app: &tauri::AppHandle,
    query: impl FnOnce() -> Option<T> + Send + 'static,
) -> Option<T> {
    on_main_result(app, query).ok().flatten()
}

#[cfg(target_os = "macos")]
fn on_main_result<T: Send + 'static>(
    app: &tauri::AppHandle,
    query: impl FnOnce() -> Option<T> + Send + 'static,
) -> Result<Option<T>, DispatchError> {
    dispatch_query(
        objc2::MainThreadMarker::new().is_some(),
        |work| app.run_on_main_thread(work).is_ok(),
        query,
        std::time::Duration::from_millis(100),
    )
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::{
        sync::Mutex,
        time::{Duration, Instant},
    };

    // Tauri Monitor is already an owned descriptor: name, rects and scale,
    // NOT a tao handle, NSScreen or other Objective-C object.
    #[derive(Clone)]
    pub(crate) struct Geometry {
        pub monitors: Vec<tauri::Monitor>,
        pub primary: tauri::Monitor,
    }
    #[derive(Default)]
    struct Cache {
        value: Option<Geometry>,
        last_attempt: Option<Instant>,
        pending: bool,
    }
    static CACHE: Mutex<Cache> = Mutex::new(Cache {
        value: None,
        last_attempt: None,
        pending: false,
    });
    const TTL: Duration = Duration::from_millis(500);

    fn should_refresh(pending: bool, last: Option<Instant>, now: Instant) -> bool {
        !pending && last.is_none_or(|last| now.duration_since(last) >= TTL)
    }

    pub(crate) fn invalidate() {
        // Keep last-known geometry as the failure fallback. Do not enqueue a
        // second refresh if one is already waiting for the main thread.
        CACHE.lock().unwrap_or_else(|e| e.into_inner()).last_attempt = None;
    }

    pub(crate) fn geometry(app: &tauri::AppHandle) -> Option<Geometry> {
        {
            let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            if !should_refresh(cache.pending, cache.last_attempt, now) {
                return cache.value.clone();
            }
            cache.pending = true;
            cache.last_attempt = Some(now);
        }
        let query_app = app.clone();
        let result = on_main_result(app, move || {
            let result = catch_monitor_panic(|| {
                Some(Geometry {
                    monitors: query_app.available_monitors().ok()?,
                    primary: query_app.primary_monitor().ok().flatten()?,
                })
            })
            .flatten();
            let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
            cache.pending = false;
            if let Some(value) = &result {
                cache.value = Some(value.clone());
            }
            result
        });
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        // A queued refresh can still complete after our bounded wait expires.
        // Keep it pending to avoid a growing queue while the main loop is busy.
        // A failed dispatch was never queued; permit a retry after the TTL.
        if matches!(result, Err(DispatchError::Unavailable)) {
            cache.pending = false;
        }
        result.ok().flatten().or_else(|| cache.value.clone())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn monitor_refresh_is_throttled_and_never_queues_behind_itself() {
            let now = Instant::now();
            assert!(should_refresh(false, None, now));
            assert!(!should_refresh(false, Some(now), now + TTL / 2));
            assert!(should_refresh(false, Some(now), now + TTL));
            assert!(!should_refresh(true, Some(now), now + TTL * 10));
        }
    }
}
#[cfg(target_os = "macos")]
pub(crate) use macos::geometry as macos_geometry;
#[cfg(target_os = "macos")]
pub(crate) use macos::invalidate as invalidate_macos_geometry;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_query_returns_owned_values_and_catches_stale_handles() {
        assert_eq!(catch_monitor_panic(|| vec![1, 2]), Some(vec![1, 2]));
        assert_eq!(catch_monitor_panic(|| panic!("stale monitor")), None::<()>);
    }

    #[test]
    fn main_thread_queries_run_directly_without_dispatch_or_wait() {
        assert_eq!(
            dispatch_query(
                true,
                |_| panic!("must not dispatch"),
                || Some(42),
                std::time::Duration::ZERO
            ),
            Ok(Some(42))
        );
    }

    #[test]
    fn worker_queries_dispatch_and_return_only_owned_results() {
        let caller = std::thread::current().id();
        let (send, receive) = std::sync::mpsc::channel();
        let result = dispatch_query(
            false,
            move |work| {
                std::thread::spawn(move || {
                    send.send(std::thread::current().id()).unwrap();
                    work();
                });
                true
            },
            || Some(vec![1, 2]),
            std::time::Duration::from_secs(1),
        );
        assert_eq!(result, Ok(Some(vec![1, 2])));
        assert_ne!(caller, receive.recv().unwrap());
    }

    #[test]
    fn monitor_dispatch_failure_and_timeout_are_bounded_and_late_delivery_is_safe() {
        let timeout = std::time::Duration::from_millis(10);
        assert_eq!(
            dispatch_query(false, |_| false, || Some(1), timeout),
            Err(DispatchError::Unavailable)
        );
        let mut queued = None;
        let start = std::time::Instant::now();
        assert_eq!(
            dispatch_query(
                false,
                |work| {
                    queued = Some(work);
                    true
                },
                || Some(1),
                timeout
            ),
            Err(DispatchError::Timeout)
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        queued.unwrap()(); // Receiver is gone; main-thread work must not block.
    }
}
