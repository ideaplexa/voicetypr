use crate::recording::{start_source::StartSource, toggle::ToggleRoute};
use tauri::AppHandle;

/// Frontend toggles route through the backend so a pending start is cancelled
/// before the state is read; `start_recording` keeps its idempotent semantics.
#[tauri::command]
pub async fn toggle_recording(
    app: AppHandle,
    source: Option<StartSource>,
) -> Result<ToggleRoute, String> {
    crate::recording::toggle::toggle_recording(app, source.unwrap_or_default()).await
}
