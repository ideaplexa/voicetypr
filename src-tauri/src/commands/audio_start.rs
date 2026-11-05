//! Cleanup for starts that never publish Recording.
use super::{clear_pending_stop_after_start, RecorderState, MEDIA_CONTROLLER};
use crate::{AppState, RecordingState};
use tauri::{AppHandle, Manager};

/// Stop before publishing Idle; propagate failures to avoid orphaning capture.
pub(crate) fn abort_start(
    app: &AppHandle,
    capture_files: &crate::recording::storage::CaptureFiles,
) -> Result<(), String> {
    let app_state = app.state::<AppState>();
    let recorder_state_handle = app.state::<RecorderState>();
    let stop_result = recorder_state_handle
        .inner()
        .0
        .lock()
        .map_err(|e| format!("Failed to acquire recorder lock: {}", e))
        .and_then(|mut recorder| {
            if recorder.is_recording() {
                recorder.stop_recording()
            } else {
                Ok(String::new())
            }
        });

    clear_pending_stop_after_start(&app_state);
    MEDIA_CONTROLLER.resume_if_we_paused();

    let cleanup_recording_path = || {
        if let Ok(mut path_guard) = app_state.current_recording_path.lock() {
            if let Some(path) = path_guard.take() {
                crate::recording::storage::remove_finished_audio(&path);
            }
        }
    };

    capture_files.writer_stopped(
        stop_result
            .as_ref()
            .err()
            .is_some_and(|e| crate::recording::storage::writer_may_be_running(e)),
    );
    if let Err(error) = stop_result {
        cleanup_recording_path();
        return Err(error);
    }

    cleanup_recording_path();
    crate::update_recording_state(app, RecordingState::Idle, None);
    Ok(())
}
