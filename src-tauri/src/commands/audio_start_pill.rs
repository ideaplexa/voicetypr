//! Early startup island lifetime, including interrupted and retired starts.
use super::*;

/// Show in the background without delaying capture. On an unsuccessful start,
/// settle the show before deciding whether live state still needs the island.
pub(super) struct EarlyPillShow {
    hide_unused: Option<Box<dyn FnOnce() + Send>>,
    handle: Option<tauri::async_runtime::JoinHandle<Result<(), String>>>,
}

impl EarlyPillShow {
    pub(super) fn spawn<R: RecordingRuntime>(app: &AppHandle<R>, generation: u64) -> Self {
        crate::perf::mark(generation, crate::perf::Stage::PillShowCalled);
        let task_app = app.clone();
        let handle = tauri::async_runtime::spawn(async move {
            if crate::perf::enabled() {
                let visible = task_app
                    .get_webview_window("pill")
                    .map(|window| crate::pill::native::is_shown(&window))
                    .unwrap_or(false);
                crate::perf::note_window_hidden(generation, !visible);
            }
            let result = if let Some(native) = R::native(&task_app) {
                let manager = native
                    .state::<AppState>()
                    .get_window_manager()
                    .ok_or_else(|| "Window manager not initialized".to_string())?;
                manager.show_pill_window_for_take(generation).await
            } else {
                // MockRuntime windows have no native UI. Keep ownership and
                // window lifetime real while testing the command headlessly.
                let state = task_app.state::<AppState>();
                state
                    .pending_start
                    .with_active(&state, generation, || {
                        task_app
                            .get_webview_window("pill")
                            .ok_or_else(|| "Mock pill unavailable".to_string())?
                            .show()
                            .map_err(|e| e.to_string())?;
                        #[cfg(test)]
                        start_command_tests::pill_shown(&task_app);
                        Ok(())
                    })
                    .unwrap_or(Ok(()))
            };
            let state = task_app.state::<AppState>();
            state.pending_start.with_active(&state, generation, || {
                crate::perf::mark(generation, crate::perf::Stage::PillShown);
            });
            result
        });
        let hide_app = app.clone();
        Self {
            hide_unused: Some(Box::new(move || {
                hide_unneeded_early_pill(&hide_app);
                #[cfg(test)]
                start_command_tests::pill_settled(&hide_app);
            })),
            handle: Some(handle),
        }
    }

    /// Recording owns the island now; report how the early show went.
    pub(super) async fn consume(mut self) -> Result<(), String> {
        match self.handle.take() {
            Some(handle) => handle.await.map_err(|error| error.to_string())?,
            None => Ok(()),
        }
    }
}

impl Drop for EarlyPillShow {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        let hide_unused = self.hide_unused.take();
        tauri::async_runtime::spawn(async move {
            let _ = handle.await;
            if let Some(hide_unused) = hide_unused {
                hide_unused();
            }
        });
    }
}

/// A card (blocked, error, recovery) or a newer take still needs the window.
pub(super) fn early_pill_unneeded(mode: &str, state: RecordingState, feedback_owned: bool) -> bool {
    mode == "when_recording"
        && !feedback_owned
        && matches!(state, RecordingState::Idle | RecordingState::Error)
}

pub(super) fn hide_unneeded_early_pill<R: RecordingRuntime>(app: &AppHandle<R>) {
    let hide_app = app.clone();
    // Decide on the native UI thread, serialized with a replacement's show.
    let _ = app.run_on_main_thread(move || {
        if !early_pill_unneeded(
            &crate::commands::pill_feedback::pill_mode(&hide_app),
            crate::get_recording_state(&hide_app),
            crate::commands::pill_feedback::feedback_owned(),
        ) {
            return;
        }
        if let Some(window) = hide_app.get_webview_window("pill") {
            let result = if let Some(native) = R::native(&hide_app) {
                crate::pill::hit_test::stop(native);
                crate::pill::native::hide(&window)
            } else {
                let result = window.hide().map_err(|e| e.to_string());
                #[cfg(test)]
                if result.is_ok() {
                    start_command_tests::pill_hidden(&hide_app);
                }
                result
            };
            if let Err(error) = result {
                log::error!("Failed to hide unused early pill window: {}", error);
            }
        }
    });
}
