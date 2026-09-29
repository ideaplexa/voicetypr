//! Terminal window lifetime, separate from text delivery and its latency.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

use super::audio::current_recording_generation;
use super::settings::{resolve_pill_indicator_mode, Settings};

static PENDING_HIDE: Mutex<Option<u64>> = Mutex::new(None);

fn pill_focus_safe() -> bool {
    // Neither platform is proven safe yet (plans/079): the pinned macOS
    // NSPanel can become key; Windows applies NOACTIVATE without checking it.
    // Keep the historical hide-before-insertion order until that is resolved.
    false
}

fn pill_mode(app: &AppHandle) -> String {
    let store = app.store("settings").ok();
    resolve_pill_indicator_mode(
        store
            .as_ref()
            .and_then(|s| s.get("pill_indicator_mode"))
            .and_then(|v| v.as_str().map(str::to_owned)),
        store
            .as_ref()
            .and_then(|s| s.get("show_pill_indicator"))
            .and_then(|v| v.as_bool()),
        Settings::default().pill_indicator_mode,
    )
}

fn defer_hide(mode: &str, focus_safe: bool) -> bool {
    mode == "when_recording" && focus_safe
}

pub(crate) fn keep_visible_for_terminal(app: &AppHandle) -> bool {
    defer_hide(&pill_mode(app), pill_focus_safe())
}

pub(crate) fn advance_recording_generation(counter: &AtomicU64) -> u64 {
    // Serialize the generation change with the native hide's final check.
    let mut pending = PENDING_HIDE.lock().unwrap();
    *pending = None;
    counter.fetch_add(1, Ordering::SeqCst) + 1
}

fn hide_is_current(pending: Option<u64>, captured: u64, current: u64) -> bool {
    pending == Some(captured) && captured == current
}

pub(crate) fn schedule_terminal_hide(app: &AppHandle, generation: u64, outcome: &str) {
    if !keep_visible_for_terminal(app) {
        return;
    }
    let timeout_ms = terminal_timeout_ms(outcome);
    {
        let mut pending = PENDING_HIDE.lock().unwrap();
        if generation != current_recording_generation() {
            return;
        }
        *pending = Some(generation);
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
        let hide_app = app.clone();
        // Check on the native UI thread too: a new recording may start while
        // the hide is queued. No show/activate call and no delivery await here.
        let _ = app.run_on_main_thread(move || {
            let mut pending = PENDING_HIDE.lock().unwrap();
            if hide_is_current(*pending, generation, current_recording_generation()) {
                *pending = None;
                if keep_visible_for_terminal(&hide_app) {
                    if let Some(window) = hide_app.get_webview_window("pill") {
                        if let Err(error) = window.hide() {
                            log::error!("Failed to hide terminal pill window: {}", error);
                        }
                    }
                }
            }
        });
    });
}

fn terminal_timeout_ms(outcome: &str) -> u64 {
    match outcome {
        "pasted" => 1200,
        "copied" => 1600,
        "no_permission" => 2500,
        _ => 0, // Delivery failed before producing a terminal outcome.
    }
}

pub(crate) fn hide_after_failed_delivery(app: &AppHandle, generation: u64) {
    // NoPermission returns an error after emitting a terminal outcome. Do not
    // replace that outcome's timeout with an immediate hide.
    if *PENDING_HIDE.lock().unwrap() != Some(generation) {
        schedule_terminal_hide(app, generation, "failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_timeouts_match_frontend_feedback() {
        assert_eq!(terminal_timeout_ms("pasted"), 1200);
        assert_eq!(terminal_timeout_ms("copied"), 1600);
        assert_eq!(terminal_timeout_ms("no_permission"), 2500);
        assert_eq!(terminal_timeout_ms("failed"), 0);
    }

    #[test]
    fn terminal_hide_requires_recording_mode_and_proven_focus_safety() {
        for mode in ["never", "always", "when_recording"] {
            for focus_safe in [false, true] {
                assert_eq!(
                    defer_hide(mode, focus_safe),
                    mode == "when_recording" && focus_safe
                );
            }
        }
    }

    #[test]
    fn new_recording_cancels_pending_hide_even_if_native_call_was_queued() {
        assert!(hide_is_current(Some(7), 7, 7));
        assert!(!hide_is_current(None, 7, 7));
        assert!(!hide_is_current(Some(7), 7, 8));
        assert!(!hide_is_current(Some(8), 7, 8));
    }
}
