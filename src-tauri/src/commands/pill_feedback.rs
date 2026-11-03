//! Terminal window lifetime, separate from text delivery and its latency.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

use super::audio::current_recording_generation;
use super::settings::{resolve_pill_indicator_mode, Settings};

mod replay;

static PENDING_HIDE: Mutex<Option<u64>> = Mutex::new(None);

pub(crate) fn pill_focus_safe() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

pub(crate) fn pill_mode<R: tauri::Runtime>(app: &AppHandle<R>) -> String {
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

/// A recovery/blocker owns the window until its card is resolved by the UI.
pub(crate) fn cancel_terminal_hide(generation: u64) {
    let mut pending = PENDING_HIDE.lock().unwrap();
    if hide_is_current(*pending, generation, current_recording_generation()) {
        *pending = None;
    }
}

fn hide_is_current(pending: Option<u64>, captured: u64, current: u64) -> bool {
    pending == Some(captured) && captured == current
}

pub(crate) fn schedule_terminal_hide(app: &AppHandle, generation: u64, outcome: &str) {
    if !keep_visible_for_terminal(app) {
        return;
    }
    let Some(timeout_ms) = terminal_timeout_ms(outcome) else {
        // Copied cards are sticky until dismissed or interrupted by a new take.
        let mut pending = PENDING_HIDE.lock().unwrap();
        if generation == current_recording_generation() {
            *pending = Some(generation);
        }
        return;
    };
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
            if !feedback_owned()
                && hide_is_current(*pending, generation, current_recording_generation())
            {
                *pending = None;
                if keep_visible_for_terminal(&hide_app) {
                    if let Some(window) = hide_app.get_webview_window("pill") {
                        crate::pill::hit_test::stop(&hide_app);
                        if let Err(error) = crate::pill::native::hide(&window) {
                            log::error!("Failed to hide terminal pill window: {}", error);
                        }
                    }
                }
            }
        });
    });
}

fn terminal_timeout_ms(outcome: &str) -> Option<u64> {
    match outcome {
        "pasted" => Some(2400),
        "copied" | "no_permission" => None,
        _ => Some(0), // Delivery failed before producing a terminal outcome.
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
    fn never_releases_even_sticky_feedback_ownership() {
        assert!(!super::owns_feedback("never", true));
        assert!(!super::owns_feedback("never", false));
        assert!(super::owns_feedback("always", true));
        assert!(super::owns_feedback("when_recording", true));
    }
    #[test]
    fn terminal_timeouts_match_frontend_feedback() {
        assert_eq!(terminal_timeout_ms("pasted"), Some(2400));
        assert_eq!(terminal_timeout_ms("copied"), None);
        assert_eq!(terminal_timeout_ms("no_permission"), None);
        assert_eq!(terminal_timeout_ms("failed"), Some(0));
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

    #[test]
    fn withdrawal_retires_only_the_matching_queued_live_note() {
        let owned = AtomicBool::new(true);
        let mut queued = vec![
            (
                "dictation-note",
                serde_json::json!({"generation":7,"kind":"mic_silent_live"}),
            ),
            (
                "dictation-note",
                serde_json::json!({"generation":8,"kind":"mic_silent_live"}),
            ),
            (
                "dictation-note",
                serde_json::json!({"generation":7,"kind":"mic_silent"}),
            ),
            (
                "dictation-recovery",
                serde_json::json!({"generation":7,"id":"kept"}),
            ),
        ];
        withdraw_queued_live_mic_silent(&mut queued, 6, false, &owned);
        assert_eq!(queued.len(), 4);
        withdraw_queued_live_mic_silent(&mut queued, 7, false, &owned);
        assert_eq!(queued.len(), 3);
        assert_eq!(queued[0].1["generation"], 8);
        assert_eq!(queued[1].1["kind"], "mic_silent");
        assert_eq!(queued[2].1["id"], "kept");
        assert!(owned.load(Ordering::SeqCst));
    }

    #[test]
    fn last_queued_warning_releases_pre_ready_but_not_renderer_ownership() {
        for ready in [false, true] {
            let owned = AtomicBool::new(true);
            let mut queued = vec![(
                "dictation-note",
                serde_json::json!({"generation":7,"kind":"mic_silent_live"}),
            )];
            withdraw_queued_live_mic_silent(&mut queued, 7, ready, &owned);
            assert!(queued.is_empty());
            assert_eq!(owned.load(Ordering::SeqCst), ready);
            // The normal terminal-hide guard is no longer suppressed before readiness.
            assert_eq!(
                !owned.load(Ordering::SeqCst) && hide_is_current(Some(7), 7, 7),
                !ready
            );
        }
    }

    #[test]
    fn recovery_retirement_is_id_scoped_and_releases_only_empty_pre_ready_ownership() {
        for ready in [false, true] {
            let owned = AtomicBool::new(true);
            let mut queued = vec![
                ("dictation-recovery", serde_json::json!({"id":"gone"})),
                ("dictation-recovery", serde_json::json!({"id":"live"})),
                ("dictation-note", serde_json::json!({"id":"gone"})),
            ];
            retire_queued_kept_id(&mut queued, "gone", ready, &owned);
            assert_eq!(queued.len(), 2);
            assert_eq!(queued[0].1["id"], "live");
            assert_eq!(queued[1].0, "dictation-note");
            assert!(owned.load(Ordering::SeqCst));
            queued.pop();
            retire_queued_kept_id(&mut queued, "live", ready, &owned);
            assert!(queued.is_empty());
            assert_eq!(owned.load(Ordering::SeqCst), ready);
        }
    }
    #[test]
    fn unmatched_withdrawal_does_not_release_other_feedback_ownership() {
        let owned = AtomicBool::new(true);
        let mut queued = Vec::new();
        withdraw_queued_live_mic_silent(&mut queued, 7, false, &owned);
        assert!(owned.load(Ordering::SeqCst));
    }
}

// Feedback owns the reused, non-activating panel across idle transitions.
static FEEDBACK_OWNED: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static QUEUED: Mutex<Vec<(&'static str, serde_json::Value)>> = Mutex::new(Vec::new());
pub(crate) fn release_feedback_ownership() {
    FEEDBACK_OWNED.store(false, Ordering::SeqCst);
}
fn owns_feedback(mode: &str, visible: bool) -> bool {
    visible && mode != "never"
}
pub(crate) fn feedback_owned() -> bool {
    FEEDBACK_OWNED.load(Ordering::SeqCst)
}
fn withdraw_queued_live_mic_silent(
    queued: &mut Vec<(&'static str, serde_json::Value)>,
    generation: u64,
    ready: bool,
    owned: &AtomicBool,
) {
    let before = queued.len();
    queued.retain(|(event, payload)| {
        *event != "dictation-note"
            || payload["generation"].as_u64() != Some(generation)
            || payload["kind"].as_str() != Some("mic_silent_live")
    });
    if !ready && queued.len() < before && queued.is_empty() {
        owned.store(false, Ordering::SeqCst);
    }
}
/// A withdrawal cannot acquire feedback ownership or show/reposition the native panel.
pub(crate) fn withdraw_live_mic_silent<R: tauri::Runtime>(app: &AppHandle<R>, generation: u64) {
    use tauri::Emitter;
    let mut queued = QUEUED.lock().unwrap();
    withdraw_queued_live_mic_silent(
        &mut queued,
        generation,
        READY.load(Ordering::SeqCst),
        &FEEDBACK_OWNED,
    );
    let _ = app.emit_to(
        "pill",
        "dictation-note-withdraw",
        serde_json::json!({"generation":generation,"kind":"mic_silent_live"}),
    );
}
pub(crate) fn send<R: tauri::Runtime>(
    app: &AppHandle<R>,
    event: &'static str,
    payload: serde_json::Value,
) {
    use tauri::Emitter;
    if app.try_state::<crate::AppState>().is_some() && pill_mode(app) == "never" {
        return;
    }
    let mut queued = QUEUED.lock().unwrap();
    // Serialize ownership with pre-ready withdrawal so a waiting send cannot lose its claim.
    let produces_ui = replay::produces_ui(event);
    if produces_ui {
        FEEDBACK_OWNED.store(true, Ordering::SeqCst);
    }
    if READY.load(Ordering::SeqCst) || app.try_state::<crate::AppState>().is_none() {
        let _ = app.emit_to("pill", event, payload);
    } else {
        if queued.len() == 32 {
            queued.remove(0);
        }
        queued.push((event, payload));
    }
    drop(queued);
    if produces_ui {
        if let Some(state) = app.try_state::<crate::AppState>() {
            if let Some(manager) = state.get_window_manager() {
                tauri::async_runtime::spawn(async move {
                    let _ = manager.show_pill_window().await;
                });
            }
        }
    }
}
#[tauri::command]
pub fn pill_feedback_ready(app: AppHandle) {
    use tauri::Emitter;
    // Match kept deletion/publication's lock order: store -> status -> queue.
    // Withdrawal removes entries under QUEUED; generation changes use PENDING_HIDE.
    let replay = crate::recording::kept::with_live_recoveries(|recoveries| {
        crate::menu::runtime::with_blocked_kind(|blocked| {
            let mut queued = QUEUED.lock().unwrap();
            let _generation = PENDING_HIDE.lock().unwrap();
            let had_queued = !queued.is_empty();
            let replay = replay::revalidate(
                std::mem::take(&mut *queued),
                current_recording_generation(),
                crate::get_recording_state(&app),
                blocked,
                recoveries,
            );
            if had_queued {
                replay::release_nonvisual_replay(&replay, &FEEDBACK_OWNED);
            }
            READY.store(true, Ordering::SeqCst);
            replay
        })
    });
    // Never hold the queue or live-state locks across renderer emits. Frontend
    // generation/id guards remain the second line for changes after this snapshot.
    for (event, payload) in replay {
        let _ = app.emit_to("pill", event, payload);
    }
}
#[tauri::command]
pub async fn pill_feedback_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    let mode = pill_mode(&app);
    FEEDBACK_OWNED.store(owns_feedback(&mode, visible), Ordering::SeqCst);
    if mode == "never" {
        return crate::commands::window::hide_pill_widget(app).await;
    }
    if !visible
        && crate::get_recording_state(&app) == crate::RecordingState::Idle
        && pill_mode(&app) == "when_recording"
    {
        crate::commands::window::hide_pill_widget(app).await?;
    }
    Ok(())
}

fn retire_queued_kept_id(
    queued: &mut Vec<(&'static str, serde_json::Value)>,
    id: &str,
    ready: bool,
    owned: &AtomicBool,
) {
    let before = queued.len();
    queued.retain(|(event, payload)| {
        *event != "dictation-recovery"
            || payload.get("id").and_then(serde_json::Value::as_str) != Some(id)
    });
    if !ready && queued.len() < before && queued.is_empty() {
        owned.store(false, Ordering::SeqCst);
    }
}
pub(crate) fn retire_kept_id(id: &str) {
    retire_queued_kept_id(
        &mut QUEUED.lock().unwrap(),
        id,
        READY.load(Ordering::SeqCst),
        &FEEDBACK_OWNED,
    );
}
#[cfg(test)]
pub(crate) fn queued_for_test() -> &'static Mutex<Vec<(&'static str, serde_json::Value)>> {
    &QUEUED
}
/// Every permanent deletion invalidates queued and already-rendered recovery ids.
pub(crate) fn expire_kept_id<R: tauri::Runtime>(app: &AppHandle<R>, id: &str) {
    use tauri::Emitter;
    retire_kept_id(id);
    let _ = app.emit_to(
        "pill",
        "kept-dictation-expired",
        serde_json::json!({"id":id}),
    );
}
pub(crate) fn renderer_closed() {
    READY.store(false, Ordering::SeqCst);
}
