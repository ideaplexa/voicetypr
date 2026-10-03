use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::{AppHandle, Manager};
#[cfg(target_os = "windows")]
use tauri_plugin_store::StoreExt;

use crate::commands::settings::read_whisper_speed_mode;
use crate::parakeet::manager::ParakeetManager;
use crate::parakeet::messages::ParakeetSegment;
use crate::remote::settings::RemoteSettings;
use crate::transcription::TranscriptionSegment;
use crate::whisper::cache::TranscriberCache;
use crate::whisper::manager::WhisperManager;
use crate::whisper::transcriber::WhisperTranscriptionOutput;

pub(crate) fn seconds_to_duration_ms(duration_seconds: Option<f32>) -> Option<u64> {
    // Guard non-finite/negative timestamps: an infinite segment time otherwise
    // saturates the `as u64` cast to u64::MAX. Matches the stricter sidecar copy.
    duration_seconds.and_then(|seconds| {
        if seconds.is_finite() && seconds >= 0.0 {
            Some((seconds * 1000.0) as u64)
        } else {
            None
        }
    })
}

pub(crate) fn transcription_watchdog_budget(audio_duration_ms: Option<u64>) -> Duration {
    const MIN_SECONDS: u64 = 180;
    const MAX_SECONDS: u64 = 30 * 60;

    let budget_seconds = audio_duration_ms
        .map(|duration_ms| duration_ms.saturating_add(999) / 1000)
        .map(|duration_seconds| duration_seconds.saturating_mul(4).saturating_add(60))
        .unwrap_or(MIN_SECONDS)
        .clamp(MIN_SECONDS, MAX_SECONDS);

    Duration::from_secs(budget_seconds)
}

pub(crate) fn parakeet_segments_to_transcription_segments(
    segments: Vec<ParakeetSegment>,
) -> Vec<TranscriptionSegment> {
    segments
        .into_iter()
        .map(|segment| TranscriptionSegment {
            text: segment.text,
            start_ms: seconds_to_duration_ms(segment.start),
            end_ms: seconds_to_duration_ms(segment.end),
            speaker_id: None,
        })
        .collect()
}

#[cfg(target_os = "windows")]
const DEFAULT_TRANSCRIPTION_ACCELERATION: &str = "auto";
#[cfg(target_os = "windows")]
fn normalize_transcription_acceleration(value: Option<&str>) -> String {
    match value {
        Some("cpu") => "cpu".to_string(),
        Some("gpu") => "gpu".to_string(),
        _ => DEFAULT_TRANSCRIPTION_ACCELERATION.to_string(),
    }
}
#[cfg(target_os = "windows")]
pub(crate) async fn transcription_acceleration_mode(app: &AppHandle) -> String {
    if let Ok(store) = app.store("settings") {
        let value = store
            .get("transcription_acceleration")
            .and_then(|v| v.as_str().map(str::to_owned));
        return normalize_transcription_acceleration(value.as_deref());
    }
    DEFAULT_TRANSCRIPTION_ACCELERATION.to_string()
}

// TODO(arch-review): collapse into a request struct when the whisper dispatch
// seam is restructured (audio_ctx/speed-mode params pushed this over 8 args).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn transcribe_whisper_with_acceleration<F>(
    app: &AppHandle,
    model_path: &Path,
    audio_path: &Path,
    language: Option<&str>,
    translate: bool,
    initial_prompt: Option<&str>,
    audio_ctx: Option<i32>,
    speed_mode_override: Option<bool>,
    should_cancel: F,
) -> Result<WhisperTranscriptionOutput, String>
where
    F: Fn() -> bool + Clone + Send + 'static,
{
    #[cfg(target_os = "windows")]
    let generation = crate::commands::audio::current_recording_generation();
    let speed_mode = speed_mode_override.unwrap_or_else(|| read_whisper_speed_mode(app));

    #[cfg(target_os = "windows")]
    let mode = transcription_acceleration_mode(app).await;

    #[cfg(target_os = "windows")]
    let mut preserve_gpu_status = false;

    #[cfg(target_os = "windows")]
    if mode != "cpu" && audio_ctx.is_none() {
        if should_cancel() {
            return Err("Transcription cancelled".to_string());
        }

        let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        let status = gpu_client.status().await;
        let should_try_gpu = mode == "gpu" || status.gpu_available != Some(false);

        if should_try_gpu {
            // Plan 060.1: mark the ACTUAL attempt BEFORE the await. Success-only
            // marking left the backend tag stale through the whole sidecar
            // attempt, so a failure event (or a later read) could attribute this
            // recording to whatever backend the PREVIOUS run used.
            crate::whisper::transcriber::record_attempt_backend("sidecar");
            let gpu_result = gpu_client
                .transcribe(
                    app,
                    crate::whisper::gpu_sidecar::GpuTranscribeRequest {
                        model_path,
                        audio_path,
                        language,
                        translate,
                        initial_prompt,
                        mode: &mode,
                    },
                )
                .await;

            match gpu_result {
                Ok(output) => return Ok(output),
                Err(error)
                    if error == "Transcription cancelled"
                        || error == crate::whisper::gpu_sidecar::SIDECAR_ABORT_ERROR =>
                {
                    // User cancel / watchdog abort: not a GPU fault. Surface the
                    // canonical cancellation — no CPU re-run, no GPU-status change.
                    gpu_client.abort_active_process().await;
                    return Err("Transcription cancelled".to_string());
                }
                Err(error) => {
                    crate::recording::island::note(
                        app,
                        generation,
                        crate::recording::island::Note::GpuFallback {
                            engine_short: "Whisper".into(),
                        },
                    );
                    preserve_gpu_status = true;
                    log::warn!(
                        "GPU sidecar failed, unloading sidecar before CPU fallback: {error}"
                    );
                    gpu_client.abort_active_process().await;
                }
            }
        } else {
            preserve_gpu_status = true;
            log::info!("Skipping Vulkan sidecar in auto mode after previous GPU failure");
        }
    }

    // Plan 060.1: the CPU attempt's mark — placed BEFORE model init and the
    // CPU transcription, so a failure anywhere below attributes "cpu" (or the
    // sidecar above), never a previous recording's backend.
    #[cfg(target_os = "windows")]
    crate::whisper::transcriber::record_attempt_backend("cpu");

    let transcriber = {
        let cache_state = app.state::<AsyncMutex<TranscriberCache>>();
        let mut cache = cache_state.lock().await;
        cache.get_or_create(model_path, speed_mode)?
    };
    // Plan 060.1: warm-cache attempts use the loaded instance's backend.
    // Cold initialization records each selected backend before attempting it,
    // so a failed initialization retains the final attempted backend as well.
    crate::whisper::transcriber::record_attempt_backend(transcriber.backend());

    let audio_path = audio_path.to_path_buf();
    let language = language.map(str::to_owned);
    let initial_prompt = initial_prompt.map(str::to_owned);
    let should_cancel_for_decode = should_cancel.clone();
    let result = tokio::task::spawn_blocking(move || {
        transcriber.transcribe_with_metadata_with_prompt(
            &audio_path,
            language.as_deref(),
            translate,
            initial_prompt.as_deref(),
            audio_ctx,
            should_cancel_for_decode,
        )
    })
    .await
    .map_err(|error| format!("Whisper transcription worker failed: {error}"))?;

    #[cfg(target_os = "windows")]
    {
        if result.is_ok() && !preserve_gpu_status {
            let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
            gpu_client
                .set_cpu_status(&mode, "Last transcription used CPU mode.")
                .await;
        }
    }

    result
}

#[derive(Clone)]
pub(crate) enum ActiveEngineSelection {
    Whisper {
        model_name: String,
        model_path: PathBuf,
    },
    Parakeet {
        model_name: String,
    },
    Cloud {
        provider: crate::cloud_stt::CloudProvider,
        model_name: String,
    },
    Remote {
        server_id: String,
        server_name: String,
        host: String,
        port: u16,
        password: Option<String>,
    },
}

impl ActiveEngineSelection {
    pub(crate) fn engine_name(&self) -> &'static str {
        match self {
            ActiveEngineSelection::Whisper { .. } => "whisper",
            ActiveEngineSelection::Parakeet { .. } => "parakeet",
            ActiveEngineSelection::Cloud { provider, .. } => provider.id(),
            ActiveEngineSelection::Remote { .. } => "remote",
        }
    }

    pub(crate) const fn analytics_kind(&self) -> crate::product_analytics::EngineKind {
        match self {
            Self::Whisper { .. } => crate::product_analytics::EngineKind::Whisper,
            Self::Parakeet { .. } => crate::product_analytics::EngineKind::Parakeet,
            Self::Cloud { .. } => crate::product_analytics::EngineKind::Cloud,
            Self::Remote { .. } => crate::product_analytics::EngineKind::Remote,
        }
    }

    pub(crate) const fn route(&self) -> &'static str {
        match self {
            Self::Whisper { .. } | Self::Parakeet { .. } => "local",
            Self::Cloud { .. } => "cloud",
            Self::Remote { .. } => "remote",
        }
    }

    pub(crate) fn model_name(&self) -> &str {
        match self {
            ActiveEngineSelection::Whisper { model_name, .. } => model_name,
            ActiveEngineSelection::Parakeet { model_name } => model_name,
            ActiveEngineSelection::Cloud { model_name, .. } => model_name,
            ActiveEngineSelection::Remote { server_name, .. } => server_name,
        }
    }
}

fn should_use_active_remote(engine_hint: Option<&str>) -> bool {
    engine_hint.is_none()
}

pub(crate) async fn resolve_engine_for_model(
    app: &AppHandle,
    model_name: &str,
    engine_hint: Option<&str>,
) -> Result<ActiveEngineSelection, String> {
    let remote_settings = app.state::<AsyncMutex<RemoteSettings>>();
    let active_remote = {
        let settings = remote_settings.lock().await;
        settings.get_active_connection().cloned()
    };

    if should_use_active_remote(engine_hint) {
        if let Some(remote_conn) = active_remote {
            if matches!(
                remote_conn.status,
                crate::remote::settings::ConnectionStatus::Online
            ) {
                return Ok(ActiveEngineSelection::Remote {
                    server_id: remote_conn.id.clone(),
                    server_name: remote_conn.display_name(),
                    host: remote_conn.host,
                    port: remote_conn.port,
                    password: remote_conn.password,
                });
            }

            return Err(
                "Selected remote unavailable. Reconnect or choose another source.".to_string(),
            );
        }
    }

    let whisper_state = app.state::<AsyncRwLock<WhisperManager>>();
    let parakeet_manager = app.state::<ParakeetManager>();

    match engine_hint.map(|e| e.to_lowercase()) {
        Some(ref engine) if crate::cloud_stt::CloudProvider::from_id(engine).is_some() => {
            let provider = crate::cloud_stt::CloudProvider::from_id(engine).unwrap();
            if crate::secure_store::secure_has(app, provider.key_name()).unwrap_or(false) {
                Ok(ActiveEngineSelection::Cloud {
                    provider,
                    model_name: model_name.to_string(),
                })
            } else {
                Err(format!(
                    "{} key not configured. Please configure it in Models.",
                    provider.display_name()
                ))
            }
        }
        Some(ref engine) if engine == "parakeet" => {
            let status = parakeet_manager
                .list_models()
                .into_iter()
                .find(|m| m.name == model_name);

            match status {
                Some(info) if info.downloaded => Ok(ActiveEngineSelection::Parakeet {
                    model_name: model_name.to_string(),
                }),
                Some(_) => Err(format!(
                    "Parakeet model '{}' is not downloaded. Please download it first.",
                    model_name
                )),
                None => Err(format!(
                    "Parakeet model '{}' not found in registry.",
                    model_name
                )),
            }
        }
        Some(ref engine) if engine == "whisper" || engine == "whisper.cpp" => {
            let path = whisper_state
                .read()
                .await
                .get_model_path(model_name)
                .ok_or_else(|| format!("Whisper model '{}' not found", model_name))?;

            Ok(ActiveEngineSelection::Whisper {
                model_name: model_name.to_string(),
                model_path: path,
            })
        }
        Some(engine) => Err(format!("Unknown model engine '{}'.", engine)),
        None => {
            if let Some(provider) = crate::cloud_stt::CloudProvider::from_id(model_name) {
                if crate::secure_store::secure_has(app, provider.key_name()).unwrap_or(false) {
                    return Ok(ActiveEngineSelection::Cloud {
                        provider,
                        model_name: model_name.to_string(),
                    });
                } else {
                    return Err(format!(
                        "{} key not configured. Please configure it in Models.",
                        provider.display_name()
                    ));
                }
            }
            if let Some(path) = whisper_state.read().await.get_model_path(model_name) {
                return Ok(ActiveEngineSelection::Whisper {
                    model_name: model_name.to_string(),
                    model_path: path,
                });
            }

            let status = parakeet_manager
                .list_models()
                .into_iter()
                .find(|m| m.name == model_name);

            if let Some(info) = status {
                if info.downloaded {
                    return Ok(ActiveEngineSelection::Parakeet {
                        model_name: model_name.to_string(),
                    });
                } else {
                    return Err(format!(
                        "Model '{}' is a Parakeet model but not downloaded. Please download it first.",
                        model_name
                    ));
                }
            }

            Err(format!(
                "Model '{}' not found in Whisper or Parakeet registries",
                model_name
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{should_use_active_remote, transcription_watchdog_budget, ActiveEngineSelection};

    #[test]
    fn active_engine_routes_are_stable_for_evidence_logs() {
        let selections = [
            ActiveEngineSelection::Whisper {
                model_name: "base".to_string(),
                model_path: std::path::PathBuf::new(),
            },
            ActiveEngineSelection::Parakeet {
                model_name: "parakeet".to_string(),
            },
            ActiveEngineSelection::Cloud {
                provider: crate::cloud_stt::CloudProvider::Openai,
                model_name: "gpt-4o-mini-transcribe".to_string(),
            },
            ActiveEngineSelection::Remote {
                server_id: "server".to_string(),
                server_name: "Remote".to_string(),
                host: "127.0.0.1".to_string(),
                port: 47842,
                password: None,
            },
        ];

        assert_eq!(selections[0].route(), "local");
        assert_eq!(selections[1].route(), "local");
        assert_eq!(selections[2].route(), "cloud");
        assert_eq!(selections[3].route(), "remote");
    }

    #[test]
    fn explicit_engine_hint_bypasses_active_remote() {
        assert!(!should_use_active_remote(Some("whisper")));
        assert!(!should_use_active_remote(Some("parakeet")));
        assert!(!should_use_active_remote(Some("soniox")));
    }

    #[test]
    fn missing_engine_hint_allows_active_remote() {
        assert!(should_use_active_remote(None));
    }

    #[test]
    fn transcription_watchdog_budget_defaults_to_minimum_for_unknown_duration() {
        assert_eq!(
            transcription_watchdog_budget(None),
            std::time::Duration::from_secs(180)
        );
    }

    #[test]
    fn transcription_watchdog_budget_floors_to_minimum_for_short_audio() {
        assert_eq!(
            transcription_watchdog_budget(Some(10_000)),
            std::time::Duration::from_secs(180)
        );
    }

    #[test]
    fn transcription_watchdog_budget_scales_duration_and_adds_sixty_seconds() {
        assert_eq!(
            transcription_watchdog_budget(Some(60_000)),
            std::time::Duration::from_secs(300)
        );
    }

    #[test]
    fn transcription_watchdog_budget_ceilings_partial_seconds_and_clamps_maximum() {
        assert_eq!(
            transcription_watchdog_budget(Some(60_001)),
            std::time::Duration::from_secs(304)
        );
        assert_eq!(
            transcription_watchdog_budget(Some(600_000)),
            std::time::Duration::from_secs(1800)
        );
    }
}
