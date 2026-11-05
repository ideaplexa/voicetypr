//! Optional native provider warmup; never extends a retired start's lifetime.
use super::*;
use crate::audio::recorder::CaptureInterrupt;

pub(super) fn spawn(
    app: &AppHandle,
    config: &RecordingConfig,
    generation: u64,
    capture_interrupt: &CaptureInterrupt,
) {
    let state = app.state::<AppState>();
    if let Some(provider) = crate::cloud_stt::CloudProvider::from_id(&config.current_engine) {
        let app = app.clone();
        let interrupt = capture_interrupt.clone();
        state.pending_start.with_active(&state, generation, || {
            tokio::spawn(async move {
                tokio::select! {
                    biased;
                    () = interrupt.wait() => {},
                    () = async {
                        let remote_active = {
                            let remote = app.state::<AsyncMutex<RemoteSettings>>();
                            let guard = remote.lock().await;
                            guard.get_active_connection().is_some()
                        };
                        let state = app.state::<AppState>();
                        let warm = state.pending_start.with_active(&state, generation, || {
                            !remote_active && crate::secure_store::secure_has(&app, provider.key_name()).unwrap_or(false)
                        }).unwrap_or(false);
                        if warm { provider.warm_up().await; }
                    } => {},
                }
            });
        });
    }
    if config.ai_enabled && !config.ai_provider.is_empty() {
        let app = app.clone();
        let provider_id = config.ai_provider.clone();
        let interrupt = capture_interrupt.clone();
        state.pending_start.with_active(&state, generation, || {
            tokio::spawn(async move {
                tokio::select! {
                    biased;
                    () = interrupt.wait() => {},
                    () = crate::commands::ai::prefetch_ai_provider(app, provider_id, generation) => {},
                }
            });
        });
    }
}
