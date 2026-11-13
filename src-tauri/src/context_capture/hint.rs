//! Legacy app hints cannot block the island or drift to a later foreground app.
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};
static BUSY: AtomicBool = AtomicBool::new(false);
struct Lease;
impl Drop for Lease {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}
pub(super) fn spawn(app: &AppHandle, generation: u64) {
    // This local NSWorkspace property is not AX IPC. The expensive hint lookup
    // runs below, after the island has already been scheduled.
    #[cfg(target_os = "macos")]
    let Some(pid) = frontmost_pid() else {
        return;
    };
    if BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let lease = Lease;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        if crate::commands::audio::recording_generation_is_stale(generation) {
            return;
        }
        #[cfg(target_os = "macos")]
        let hint = hint_for_process(pid);
        #[cfg(not(target_os = "macos"))]
        let hint = crate::writing::capture_active_app_context();
        let state = app.state::<crate::state::AppState>();
        state.pending_start.with_active(&state, generation, || {
            if let Some(hint) = hint {
                state.set_recording_app_context(hint);
            }
        });
    });
}

#[cfg(target_os = "macos")]
fn frontmost_pid() -> Option<i32> {
    use objc2::{msg_send, rc::Retained, runtime::AnyObject};
    objc2::rc::autoreleasepool(|_| {
        let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
        // SAFETY: public NSWorkspace/NSRunningApplication selectors and their
        // declared Objective-C types. This does not request AX/window contents.
        let app: Option<Retained<AnyObject>> =
            unsafe { msg_send![&*workspace, frontmostApplication] };
        let app = app?;
        let pid: i32 = unsafe { msg_send![&*app, processIdentifier] };
        (pid > 0).then_some(pid)
    })
}

#[cfg(target_os = "macos")]
fn hint_for_process(pid: i32) -> Option<crate::writing::ContextHint> {
    use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
    use objc2_foundation::NSString;
    if keytrigger::backend::secure_input_enabled() {
        return None;
    }
    objc2::rc::autoreleasepool(|_| {
        // SAFETY: public NSRunningApplication class method resolves the pinned
        // process even if it is no longer frontmost. Never fetch a window title.
        let app: Option<Retained<AnyObject>> = unsafe {
            msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid]
        };
        let app = app?;
        let name: Option<Retained<NSString>> = unsafe { msg_send![&*app, localizedName] };
        let url: Option<Retained<AnyObject>> = unsafe { msg_send![&*app, bundleURL] };
        let path: Option<Retained<NSString>> =
            url.and_then(|url| unsafe { msg_send![&*url, path] });
        if keytrigger::backend::secure_input_enabled() {
            return None;
        }
        Some(crate::writing::ContextHint {
            app_name: name.map(|s| s.to_string()),
            process_path: path.map(|s| s.to_string()),
            ..Default::default()
        })
    })
}
