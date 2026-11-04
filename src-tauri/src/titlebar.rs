//! Align native controls with the fixed sidebar toggle, in window coordinates.
//!
//! Do not set `trafficLightPosition` in either Tauri config: tao applies that
//! inset on every redraw and would overwrite this titlebar container's frame.
use objc2_app_kit::{NSWindow, NSWindowButton, NSWindowStyleMask};

const TITLEBAR_HEIGHT: f64 = 36.0;
// AppShell's toggle is 28px tall at top 5px.
const TOGGLE_CENTER: f64 = 19.0;
const TRAFFIC_LIGHT_LEFT: f64 = 12.0;
const TOLERANCE: f64 = 0.01;

fn button_bottom(window_height: f64, button_height: f64) -> f64 {
    window_height - TOGGLE_CENTER - button_height / 2.0
}

/// Called on AppKit's main thread after showing the main window and after
/// events that may cause AppKit to lay its titlebar out again.
pub fn align(window: &tauri::WebviewWindow) {
    if objc2::MainThreadMarker::new().is_none() {
        let window = window.clone();
        let on_main = window.clone();
        let _ = window.run_on_main_thread(move || align(&on_main));
        return;
    }
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    // SAFETY: Tauri owns the NSWindow for this call; AppKit is accessed only
    // after establishing that this is the main thread.
    unsafe {
        let ns_window = &*ptr.cast::<NSWindow>();
        if ns_window
            .styleMask()
            .contains(NSWindowStyleMask::FullScreen)
        {
            return;
        }
        let Some(close) = ns_window.standardWindowButton(NSWindowButton::CloseButton) else {
            return;
        };
        let Some(minimize) = ns_window.standardWindowButton(NSWindowButton::MiniaturizeButton)
        else {
            return;
        };
        let Some(zoom) = ns_window.standardWindowButton(NSWindowButton::ZoomButton) else {
            return;
        };
        let Some(container) = close.superview().and_then(|view| view.superview()) else {
            return;
        };

        // Match the existing 36pt title strip and pin the container to the top.
        let height = ns_window.frame().size.height;
        let mut frame = container.frame();
        if (frame.size.height - TITLEBAR_HEIGHT).abs() >= TOLERANCE
            || (frame.origin.y - (height - TITLEBAR_HEIGHT)).abs() >= TOLERANCE
        {
            frame.size.height = TITLEBAR_HEIGHT;
            frame.origin.y = height - TITLEBAR_HEIGHT;
            container.setFrame(frame);
        }

        let horizontal_step = minimize.frame().origin.x - close.frame().origin.x;
        for (index, button) in [close, minimize, zoom].into_iter().enumerate() {
            let Some(parent) = button.superview() else {
                continue;
            };
            let mut rect = button.convertRect_toView(button.bounds(), None);
            rect.origin.y = button_bottom(height, rect.size.height);
            let mut origin = button.frame().origin;
            origin.x = TRAFFIC_LIGHT_LEFT + index as f64 * horizontal_step;
            origin.y = parent.convertRect_fromView(rect, None).origin.y;
            let current = button.frame().origin;
            if (origin.x - current.x).abs() >= TOLERANCE
                || (origin.y - current.y).abs() >= TOLERANCE
            {
                button.setFrameOrigin(origin);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_the_buttons_on_the_frontend_toggle() {
        let shell = include_str!("../../src/components/AppShell.tsx");
        assert!(shell.contains("top-[5px] size-7"));
        assert_eq!(button_bottom(680.0, 14.0), 654.0);
        assert_eq!(button_bottom(680.0, 16.0), 653.0);
    }

    #[test]
    fn tao_does_not_reapply_a_competing_inset() {
        for config in [
            include_str!("../tauri.conf.json"),
            include_str!("../tauri.dev.conf.json"),
        ] {
            assert!(!config.contains("\"trafficLightPosition\""));
        }
    }
}
