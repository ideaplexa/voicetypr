use crate::recording::{start_source::StartSource, toggle::ToggleRoute};
use tauri::AppHandle;

/// Frontend toggles route through the backend so a pending start is cancelled
/// before the state is read; `start_recording` keeps its idempotent semantics.
#[tauri::command]
pub async fn toggle_recording(
    app: AppHandle,
    source: Option<StartSource>,
) -> Result<ToggleRoute, String> {
    let source = source.unwrap_or_default();
    // Only the main window's in-app hotkey fallback sends Hotkey here.
    if source == StartSource::Hotkey {
        use tauri::Manager;
        let state = app.state::<crate::AppState>();
        if !crate::recording::hotkeys::claim_toggle_window(&state) {
            log::info!("Toggle: in-app hotkey ignored; this press was already handled");
            return Ok(ToggleRoute::NoChange);
        }
        if crate::recording::onboarding_try::claim_hotkey(&app, keytrigger::KeyPhase::Pressed) {
            return Ok(ToggleRoute::NoChange);
        }
        log::info!("Toggle: in-app hotkey fallback");
    }
    crate::recording::toggle::toggle_recording(app, source).await
}

/// The main window's Esc on Windows, where the low-level hook is blind while
/// our window has focus. Shares the double-press cancel with the hook.
#[tauri::command]
pub async fn in_app_escape(app: AppHandle) {
    use tauri::Manager;
    let state = app.state::<crate::AppState>();
    crate::recording::escape_handler::handle_escape_key_press(
        &state,
        &app,
        keytrigger::KeyPhase::Pressed,
        crate::recording::escape_handler::EscapeSource::InApp,
    )
    .await;
}
