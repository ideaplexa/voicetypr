//! Closed, per-dictation telemetry for the desktop stop flow.

use std::sync::atomic::Ordering as AtomicOrdering;
use std::time::Instant;
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::transcription::engines::ActiveEngineSelection;
use tauri::async_runtime::Mutex as AsyncMutex;

type DictationRoute = (
    crate::product_analytics::EngineKind,
    String,
    crate::product_analytics::DictationTransport,
);

fn recording_route(remote_online: bool, engine: &str, model: String) -> DictationRoute {
    use crate::product_analytics::{DictationTransport, EngineKind};
    if remote_online {
        (
            EngineKind::Remote,
            "other".to_string(),
            DictationTransport::Remote,
        )
    } else {
        let engine = dictation_engine_from_id(engine);
        let transport = if engine == EngineKind::Cloud {
            DictationTransport::Rest
        } else {
            DictationTransport::Local
        };
        (engine, model, transport)
    }
}

/// Owns exactly one completed event from a real stop through every early exit,
/// including an aborted transcription task. It never stores transcript text.
pub(crate) struct DictationCompletionGuard {
    pub(crate) facts: crate::product_analytics::DictationFacts,
    stop_requested: Instant,
    generation: u64,
}

impl DictationCompletionGuard {
    pub(crate) async fn new(
        app: &AppHandle,
        stop_requested: Instant,
        metrics: Option<crate::audio::recorder::CaptureAudioMetrics>,
        generation: u64,
    ) -> Self {
        use tauri_plugin_store::StoreExt;
        let app_state = app.state::<AppState>();
        let live_preview = app_state
            .recording_live_preview
            .load(AtomicOrdering::SeqCst);
        let remote_online = {
            let remote_settings =
                app.state::<AsyncMutex<crate::remote::settings::RemoteSettings>>();
            let settings = remote_settings.lock().await;
            settings.get_active_connection().is_some_and(|connection| {
                matches!(
                    connection.status,
                    crate::remote::settings::ConnectionStatus::Online
                )
            })
        };
        let (engine, model, transport) = app
            .store("settings")
            .ok()
            .map(|store| {
                let engine = store
                    .get("current_model_engine")
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_default();
                let model = store
                    .get("current_model")
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_default();
                recording_route(remote_online, &engine, model)
            })
            .unwrap_or_else(|| recording_route(remote_online, "whisper", String::new()));
        let app_category = app_state
            .recording_app_context
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(crate::writing::classify))
            .unwrap_or(crate::writing::AppCategory::Other);
        let mut guard = Self::from_snapshot(
            stop_requested,
            metrics,
            (engine, model, transport),
            live_preview,
            app_category,
        );
        guard.generation = generation;
        guard
    }

    fn from_snapshot(
        stop_requested: Instant,
        metrics: Option<crate::audio::recorder::CaptureAudioMetrics>,
        (engine, model, transport): DictationRoute,
        live_preview: bool,
        app_category: crate::writing::AppCategory,
    ) -> Self {
        Self {
            facts: crate::product_analytics::DictationFacts {
                outcome: crate::product_analytics::DictationOutcome::Cancelled,
                engine,
                model,
                transport,
                live_preview,
                recording_ms: metrics.map(|value| value.duration_ms).unwrap_or(0),
                start_to_first_audio_ms: metrics.and_then(|value| value.start_to_first_audio_ms),
                stop_to_text_ms: 0,
                post_roll_speech_detected: metrics
                    .is_some_and(|value| value.post_roll_speech_detected),
                post_roll_interrupted: metrics.is_some_and(|value| value.post_roll_interrupted),
                words: 0,
                polish: crate::product_analytics::PolishOutcome::Disabled,
                paste: crate::product_analytics::DictationPaste::Skipped,
                app_category,
            },
            stop_requested,
            generation: crate::commands::audio::current_recording_generation(),
        }
    }

    pub(crate) fn text_ready(&mut self, text: &str) {
        self.facts.stop_to_text_ms = self
            .stop_requested
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        self.facts.words = text.split_whitespace().count();
    }
}

impl Drop for DictationCompletionGuard {
    fn drop(&mut self) {
        if self.facts.stop_to_text_ms == 0 {
            self.facts.stop_to_text_ms = self
                .stop_requested
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64;
        }
        crate::product_analytics::capture_at(
            crate::product_analytics::build_dictation_completed(self.facts.clone()),
            self.generation,
        );
    }
}

pub(crate) fn dictation_engine_from_id(engine: &str) -> crate::product_analytics::EngineKind {
    match engine {
        "parakeet" => crate::product_analytics::EngineKind::Parakeet,
        "whisper" => crate::product_analytics::EngineKind::Whisper,
        id if crate::cloud_stt::CloudProvider::from_id(id).is_some() => {
            crate::product_analytics::EngineKind::Cloud
        }
        _ => crate::product_analytics::EngineKind::Whisper,
    }
}

pub(crate) fn dictation_transport(
    selection: &ActiveEngineSelection,
    ws_final_supplied_text: Option<bool>,
) -> crate::product_analytics::DictationTransport {
    use crate::product_analytics::DictationTransport;
    match selection {
        ActiveEngineSelection::Whisper { .. } | ActiveEngineSelection::Parakeet { .. } => {
            DictationTransport::Local
        }
        ActiveEngineSelection::Remote { .. } => DictationTransport::Remote,
        ActiveEngineSelection::Cloud { .. } => match ws_final_supplied_text {
            Some(true) => DictationTransport::Ws,
            Some(false) => DictationTransport::RestFallback,
            None => DictationTransport::Rest,
        },
    }
}

#[cfg(test)]
mod dictation_transport_tests {
    use super::{
        dictation_transport, recording_route, ActiveEngineSelection, DictationCompletionGuard,
    };
    use crate::product_analytics::{DictationOutcome, DictationTransport, EngineKind};

    #[tokio::test]
    async fn unfinished_task_drops_with_cancelled_outcome() {
        let guard = DictationCompletionGuard::from_snapshot(
            std::time::Instant::now(),
            None,
            recording_route(false, "whisper", "tiny".to_string()),
            false,
            crate::writing::AppCategory::Other,
        );
        let (sent, received) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            sent.send(guard.facts.outcome).unwrap();
            let _guard = guard;
            std::future::pending::<()>().await;
        });
        assert_eq!(received.await.unwrap(), DictationOutcome::Cancelled);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }

    #[test]
    fn remote_route_is_snapshotted_before_early_exit() {
        assert_eq!(
            recording_route(true, "parakeet", "local-model".to_string()),
            (
                EngineKind::Remote,
                "other".to_string(),
                DictationTransport::Remote
            )
        );
        assert_eq!(
            recording_route(false, "parakeet", "local-model".to_string()),
            (
                EngineKind::Parakeet,
                "local-model".to_string(),
                DictationTransport::Local
            )
        );
    }

    #[test]
    fn soniox_transport_follows_whether_ws_final_supplied_text() {
        let soniox = ActiveEngineSelection::Cloud {
            provider: crate::cloud_stt::CloudProvider::Soniox,
            model_name: "stt-async-v5".to_string(),
        };
        assert_eq!(
            dictation_transport(&soniox, Some(true)),
            DictationTransport::Ws
        );
        assert_eq!(
            dictation_transport(&soniox, Some(false)),
            DictationTransport::RestFallback
        );
        assert_eq!(dictation_transport(&soniox, None), DictationTransport::Rest);
    }
}
