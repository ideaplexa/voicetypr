//! WKWebView skips CSS cursor updates for a non-key NSPanel:
//! WebKit Source/WebKit/UIProcess/mac/PageClientImplMac.mm,
//! isViewWindowActive() and setCursor() (key-window gate and window-under-pointer check).
//! https://github.com/WebKit/WebKit/blob/main/Source/WebKit/UIProcess/mac/PageClientImplMac.mm
use super::{hit_test, PointerState};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSCursor, NSEvent, NSWindow};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, WebviewWindow};

/// WindowServer otherwise ignores NSCursor::set from an inactive application,
/// even though currentCursor reports our cached hand. This opt-in affects only
/// our own connection. update() still limits ownership to unobscured enabled
/// buttons, and reset/exit releases it without activating the app.
pub(crate) fn configure() -> Result<(), String> {
    use std::ffi::c_void;
    type Connection = unsafe extern "C" fn() -> i32;
    type Property = unsafe extern "C" fn(i32, i32, *const c_void, *const c_void) -> i32;
    // Private WindowServer SPI, resolved defensively rather than hard-linked.
    // SAFETY: called on main after AppKit has created the panel/connection;
    // NSString is toll-free bridged to CFString, kCFBooleanTrue to CFBoolean.
    unsafe {
        let connection = libc::dlsym(libc::RTLD_DEFAULT, c"CGSMainConnectionID".as_ptr());
        let property = libc::dlsym(libc::RTLD_DEFAULT, c"CGSSetConnectionProperty".as_ptr());
        let boolean = libc::dlsym(libc::RTLD_DEFAULT, c"kCFBooleanTrue".as_ptr());
        if connection.is_null() || property.is_null() || boolean.is_null() {
            return Err("Background cursor configuration unavailable".into());
        }
        let connection = std::mem::transmute::<*mut c_void, Connection>(connection)();
        let property = std::mem::transmute::<*mut c_void, Property>(property);
        let key = objc2_foundation::NSString::from_str("SetsCursorInBackground");
        let enabled = *boolean.cast::<*const c_void>();
        let key = (&*key as *const objc2_foundation::NSString).cast();
        if property(connection, connection, key, enabled) != 0 {
            return Err("Background cursor configuration failed".into());
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
enum Change {
    Hand,
    Arrow,
    None,
}
fn change(interactive: bool, frontmost: bool, owned: bool, is_hand: bool) -> Change {
    if interactive && frontmost {
        if owned {
            Change::None
        } else {
            Change::Hand
        }
    } else if owned && is_hand {
        Change::Arrow
    } else {
        Change::None
    }
}
fn apply(change: Change) {
    match change {
        Change::Hand => NSCursor::pointingHandCursor().set(),
        Change::Arrow => NSCursor::arrowCursor().set(),
        Change::None => {}
    }
}
fn is_hand() -> bool {
    // These are AppKit's shared cursors, not newly allocated images/cursors per poll.
    std::ptr::eq(
        &*NSCursor::currentCursor(),
        &*NSCursor::pointingHandCursor(),
    )
}

/// Called only inside the existing bounded main-thread pointer query.
pub(super) fn update(
    app: &AppHandle,
    window: &WebviewWindow,
    point: Option<(f64, f64)>,
    generation: u64,
) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let state = app.state::<PointerState>();
    if state.query.generation.load(Ordering::Acquire) != generation {
        return;
    }
    let owned = state.cursor_owned.load(Ordering::Relaxed);
    let interactive = crate::pill::native::is_shown(window)
        && point
            .is_some_and(|(x, y)| hit_test(&state.regions.lock().unwrap().actions, x, y, false));
    if !interactive && !owned {
        return;
    }
    let frontmost = interactive
        && window.ns_window().is_ok_and(|raw| {
            // SAFETY: Tauri owns the NSWindow; this query runs on the AppKit main thread.
            let native = unsafe { &*raw.cast::<NSWindow>() };
            native.windowNumber()
                == NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(
                    NSEvent::mouseLocation(),
                    0,
                    mtm,
                )
        });
    state
        .cursor_owned
        .store(interactive && frontmost, Ordering::Relaxed);
    // Transition-only: no NSCursor::set while stationary/on the same enabled
    // targets. On exit, release once unless another owner changed the cursor.
    apply(change(
        interactive,
        frontmost,
        owned,
        owned && !frontmost && is_hand(),
    ));
    // stop() may invalidate the query while AppKit is answering it. Do not leave a
    // hand set by that old query after stop() already inspected cursor_owned.
    if state.query.generation.load(Ordering::Acquire) != generation
        && state.cursor_owned.swap(false, Ordering::Relaxed)
    {
        apply(change(false, false, true, is_hand()));
    }
}

pub(super) fn reset(app: &AppHandle) {
    if !app
        .state::<PointerState>()
        .cursor_owned
        .load(Ordering::Relaxed)
    {
        return;
    }
    let app = app.clone();
    let generation = app
        .state::<PointerState>()
        .query
        .generation
        .load(Ordering::Acquire);
    let dispatcher = app.clone();
    let _ = dispatcher.run_on_main_thread(move || {
        let state = app.state::<PointerState>();
        // An old hide/stop must not clear the cursor of a restarted pointer task.
        if state.query.generation.load(Ordering::Acquire) == generation
            && state.cursor_owned.swap(false, Ordering::Relaxed)
        {
            apply(change(false, false, true, is_hand()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_transitions_claim_once_and_release_only_our_unchanged_hand() {
        for ((interactive, frontmost, owned, is_hand), expected) in [
            ((true, true, false, false), Change::Hand),
            ((true, false, false, false), Change::None),
            ((false, true, false, false), Change::None),
            ((true, true, true, true), Change::None),
            ((true, true, true, false), Change::None),
            ((false, false, true, true), Change::Arrow),
            ((true, false, true, true), Change::Arrow),
            ((false, false, true, false), Change::None),
            ((false, false, false, true), Change::None),
            ((false, false, false, false), Change::None),
        ] {
            assert_eq!(change(interactive, frontmost, owned, is_hand), expected);
        }
    }
}
