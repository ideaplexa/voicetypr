//! Route `keytrigger::TriggerEvent`s to the shared shortcut dispatch path.

use keytrigger::TriggerEvent;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::app_state::AppState;

/// Invoked on the engine dispatcher thread for every emitted trigger event.
pub fn on_engine_event(app: &AppHandle, ev: TriggerEvent) {
    if ev.id == "island-escape-observer" {
        if ev.phase == keytrigger::KeyPhase::Pressed {
            let _ = app.emit_to("pill", "island-escape", ());
        }
        return;
    }
    let app_state = app.state::<AppState>();

    let binding = match app_state.engine_bindings.lock() {
        Ok(guard) => guard.iter().find(|b| b.id == ev.id).cloned(),
        Err(error) => {
            log::error!("keytrigger: engine_bindings lock poisoned: {}", error);
            return;
        }
    };

    // Transient window: the matcher may emit a synthetic Released for a binding
    // the app map no longer holds (it was just removed). The removal path
    // already released it, so this is safe to ignore.
    let Some(binding) = binding else {
        log::debug!("keytrigger: no engine binding for id {}", ev.id);
        return;
    };

    if ev.id == "escape-cancel" {
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let app_state = app_handle.state::<AppState>();
            crate::recording::escape_handler::handle_escape_key_press(
                &app_state,
                &app_handle,
                ev.phase,
            )
            .await;
        });
        return;
    }

    crate::recording::hotkeys::dispatch_action(
        app,
        app_state.inner(),
        &binding.id,
        binding.action,
        binding.trigger,
        ev.phase,
    );
}
