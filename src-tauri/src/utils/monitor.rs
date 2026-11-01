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
    // Tauri converts tao handles on the CALLER thread. On macOS, each
    // conversion calls NSScreen::screens/deviceDescription several times.
    // Tokio workers have no AppKit event-loop pool; retaining the returned
    // dictionary does not drain its autoreleased backing storage.
    #[cfg(target_os = "macos")]
    let query = || objc2::rc::autoreleasepool(|_| query());
    catch_unwind(AssertUnwindSafe(query)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_query_returns_owned_values_and_catches_stale_handles() {
        assert_eq!(catch_monitor_panic(|| vec![1, 2]), Some(vec![1, 2]));
        assert_eq!(catch_monitor_panic(|| panic!("stale monitor")), None::<()>);
    }
}
