//! WKWebView skips CSS cursor updates for a non-key NSPanel:
//! WebKit Source/WebKit/UIProcess/mac/PageClientImplMac.mm,
//! isViewWindowActive() and setCursor() (key-window gate and window-under-pointer check).
//! https://github.com/WebKit/WebKit/blob/main/Source/WebKit/UIProcess/mac/PageClientImplMac.mm
use super::{hit_test, PointerState};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSCursor, NSEvent, NSWindow};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, WebviewWindow};

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
    fn stationary_polls_never_reassert_the_cursor() {
        assert_eq!(change(true, true, false, false), Change::Hand);
        for _ in 0..300 {
            assert_eq!(change(true, true, true, true), Change::None);
        }
        assert_eq!(change(false, false, true, true), Change::Arrow);
        assert_eq!(change(false, false, false, false), Change::None);
    }
    #[test]
    fn non_key_island_claims_only_its_interactive_frontmost_region() {
        assert_eq!(change(true, true, false, false), Change::Hand);
        assert_eq!(change(true, false, false, false), Change::None);
        assert_eq!(change(false, true, false, false), Change::None);
        assert_eq!(change(true, true, true, true), Change::None);
        assert_eq!(change(true, true, true, false), Change::None);
    }
    #[test]
    fn leaving_hiding_or_occluding_releases_only_our_unchanged_hand() {
        assert_eq!(change(false, false, true, true), Change::Arrow);
        assert_eq!(change(true, false, true, true), Change::Arrow);
        assert_eq!(change(false, false, true, false), Change::None);
        assert_eq!(change(false, false, false, true), Change::None);
    }
}
