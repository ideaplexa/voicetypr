//! A posted paste shortcut is not a receipt: native menus can consume it.
//! Fail closed to clipboard-only delivery if a menu owns keyboard input or the
//! UI thread cannot answer promptly. Never send Escape (it could hit the editor).

/// Called on the insertion worker, never on the native UI thread.
pub(crate) fn paste_ready(app: Option<&tauri::AppHandle>) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::{sync::mpsc, time::Duration};
        let Some(app) = app else { return false };
        let (tx, rx) = mpsc::sync_channel(1);
        if app
            .run_on_main_thread(move || {
                let _ = tx.send(macos_ready());
            })
            .is_err()
        {
            return false;
        }
        // Tauri's UI dispatcher may not run at all inside AppKit's menu tracking
        // loop. The queued closure only observes state, so a late reply is inert.
        rx.recv_timeout(Duration::from_millis(150)).unwrap_or(false)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetGUIThreadInfo, GUITHREADINFO, GUI_INMENUMODE, GUI_POPUPMENUMODE, GUI_SYSTEMMENUMODE,
        };
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        // Thread 0 queries the foreground thread, including the tray's modal
        // context menu. No UI dispatch is needed while TrackPopupMenu is running.
        // SAFETY: info is writable and cbSize describes its actual allocation.
        unsafe { GetGUIThreadInfo(0, &mut info) }.is_ok()
            && (info.flags & (GUI_INMENUMODE | GUI_POPUPMENUMODE | GUI_SYSTEMMENUMODE)).0 == 0
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app;
        true
    }
}

#[cfg(target_os = "macos")]
fn macos_ready() -> bool {
    use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
    use objc2_foundation::NSString;
    // SAFETY: called only by run_on_main_thread. These are public AppKit and
    // Foundation selectors, with their documented object/bool return types.
    unsafe {
        let run_loop: *mut AnyObject = msg_send![class!(NSRunLoop), currentRunLoop];
        let mode: Option<Retained<NSString>> = msg_send![run_loop, currentMode];
        let Some(mode) = mode else { return false };
        if mode.to_string() == "NSEventTrackingRunLoopMode" {
            return false;
        }
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let active: bool = msg_send![app, isActive];
        let key_window: *mut AnyObject = msg_send![app, keyWindow];
        // Closing a status menu can leave this accessory app active with no key
        // window. Tao explicitly drops Command events in that state.
        !active || !key_window.is_null()
    }
}

/// Check both sides of dispatch. If a menu appeared meanwhile, preserve the
/// clipboard and report "copied", never retry (the first paste may have landed).
pub(crate) fn dispatch_if_ready<T>(
    mut ready: impl FnMut() -> bool,
    dispatch: impl FnOnce() -> T,
) -> Option<T> {
    if !ready() {
        return None;
    }
    let result = dispatch();
    ready().then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_owns_input_or_ui_probe_times_out_so_no_shortcut_is_sent() {
        assert_eq!(
            dispatch_if_ready(|| false, || panic!("must not paste")),
            None::<()>
        );
    }

    #[test]
    fn menu_opening_during_dispatch_cannot_report_success() {
        let mut probes = [true, false].into_iter();
        assert_eq!(dispatch_if_ready(|| probes.next().unwrap(), || true), None);
    }

    #[test]
    fn unobstructed_paste_preserves_the_dispatch_result() {
        assert_eq!(dispatch_if_ready(|| true, || false), Some(false));
        assert_eq!(dispatch_if_ready(|| true, || true), Some(true));
    }
}
