//! The one toggle route for tray, Pointer and in-app hotkey toggles. Explicit
//! `start_recording` stays idempotent; only a toggle may cancel a pending start.
use super::start_source::StartSource;
use crate::commands::recording_runtime::RecordingRuntime;
use crate::{AppState, RecordingState};
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToggleRoute {
    CancelledPendingStart,
    Start,
    Stop,
    Finishing,
    NoChange,
}

fn start_result(started: bool) -> ToggleRoute {
    if started {
        ToggleRoute::Start
    } else {
        ToggleRoute::NoChange
    }
}

/// Cancel a pending start before reading the state. A start still validating
/// (for example waiting for the license) is Idle, and a second start would only
/// be rejected by the pending claim, leaving the first one to open the mic.
pub(crate) fn route(state: &AppState) -> ToggleRoute {
    // Commit and cancellation are serialized; a request that already
    // published Starting is no longer pending and takes the ordinary stop.
    if state.pending_start.cancel() {
        return ToggleRoute::CancelledPendingStart;
    }
    match state.get_current_state() {
        RecordingState::Idle | RecordingState::Error => ToggleRoute::Start,
        RecordingState::Starting | RecordingState::Recording => ToggleRoute::Stop,
        RecordingState::Stopping | RecordingState::Transcribing => ToggleRoute::Finishing,
    }
}

pub(crate) async fn toggle_recording<R: RecordingRuntime>(
    app: AppHandle<R>,
    source: StartSource,
) -> Result<ToggleRoute, String> {
    let route = route(&app.state::<AppState>());
    match route {
        ToggleRoute::Start => {
            let started = crate::commands::audio::start_recording(
                app.clone(),
                app.state::<crate::commands::audio::RecorderState>(),
                Some(source),
            )
            .await?;
            return Ok(start_result(started));
        }
        // stop_recording queues the stop while Starting and owns StopInFlightGuard.
        ToggleRoute::Stop => {
            let app = R::native(&app).ok_or("Native capture unavailable")?;
            crate::commands::audio::stop_recording(
                app.clone(),
                app.state::<crate::commands::audio::RecorderState>(),
            )
            .await?;
        }
        ToggleRoute::Finishing => {
            crate::commands::island_notice::notice(
                &app,
                crate::commands::island_notice::NoticeKind::Finishing,
            );
        }
        ToggleRoute::CancelledPendingStart | ToggleRoute::NoChange => {}
    }
    Ok(route)
}

#[cfg(test)]
mod tests;
