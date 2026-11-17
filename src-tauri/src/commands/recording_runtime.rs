//! Native I/O boundary for headless command-path regression tests.
//! Production always uses Wry; MockRuntime never opens a device or native UI.
use tauri::{AppHandle, Runtime};

pub trait RecordingRuntime: Runtime {
    fn native(app: &AppHandle<Self>) -> Option<&AppHandle>;
    fn validate(
        app: &AppHandle<Self>,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;
}

impl RecordingRuntime for tauri::Wry {
    fn native(app: &AppHandle<Self>) -> Option<&AppHandle> {
        Some(app)
    }

    async fn validate(app: &AppHandle<Self>) -> Result<(), String> {
        super::audio::validate_recording_requirements(app).await
    }
}

#[cfg(test)]
impl RecordingRuntime for tauri::test::MockRuntime {
    fn native(_: &AppHandle<Self>) -> Option<&AppHandle> {
        None
    }

    async fn validate(app: &AppHandle<Self>) -> Result<(), String> {
        use tauri::Manager;
        // Device/model availability is the mocked I/O; license waiting and
        // request ownership still run through the real command.
        let state = app.state::<crate::AppState>();
        match crate::license::recording_readiness::wait_for_recording_license(&state.license_cache)
            .await
        {
            super::audio::RecordingLicenseState::Ready => Ok(()),
            _ => Err("Recording license unavailable".into()),
        }
    }
}
