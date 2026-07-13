use crate::commands::license::check_license_status_internal;
use crate::commands::settings::{
    normalize_transcription_mode, TRANSCRIPTION_MODE_LIVE_PREVIEW, TRANSCRIPTION_MODE_REGULAR,
};
use crate::emit_to_all;
use crate::license::LicenseState;
use crate::parakeet::manager::ParakeetEouModelStatus;
use crate::parakeet::{messages::ParakeetResponse, ParakeetManager, ParakeetModelStatus};
use crate::provider_capabilities::ProviderEngine;
use crate::remote::settings::RemoteSettings;
use crate::secure_store;
use crate::transcription::stream::EngineStreamCapabilities;
use crate::utils::onboarding_logger;
#[cfg(debug_assertions)]
use crate::utils::system_monitor;
use crate::whisper::manager::{ModelInfo, WhisperManager};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
#[cfg(debug_assertions)]
use std::time::Instant;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_store::StoreExt;

type ActiveDownloadsState<'a> = State<'a, Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>>;
const DEFAULT_EOU_CHUNK_MS: u16 = 320;
const EOU_MODEL_ID: &str = "parakeet-eou-live-preview";
// Used again when Parakeet's native EOU activation returns (upstream broken; plans 042/051).
#[allow(dead_code)]
const EOU_MODEL_SIZE_BYTES: u64 = 250 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ActiveStreamCapabilities {
    pub active_engine: String,
    pub capabilities: EngineStreamCapabilities,
    pub eou_model_downloaded: bool,
    pub eou_model_path: Option<String>,
    pub eou_chunk_ms: u16,
    pub transcription_mode: String,
}

fn provider_engine_from_settings(engine: &str) -> ProviderEngine {
    ProviderEngine::from_engine_str(engine).unwrap_or(ProviderEngine::Whisper)
}

async fn resolve_active_engine(app: &AppHandle) -> Result<String, String> {
    if let Some(remote_state) = app.try_state::<AsyncMutex<RemoteSettings>>() {
        let settings = remote_state.lock().await;
        if settings.get_active_connection().is_some() {
            return Ok("remote".to_string());
        }
    }

    let settings = crate::commands::settings::get_settings(app.clone()).await?;
    Ok(settings.current_model_engine)
}

fn persist_transcription_mode(app: &AppHandle, mode: &str) -> Result<(), String> {
    let store = app.store("settings").map_err(|error| error.to_string())?;
    store.set(
        "transcription_mode",
        json!(normalize_transcription_mode(Some(mode))),
    );
    store.save().map_err(|error| error.to_string())?;
    let _ = app.emit("settings-changed", json!({ "key": "transcription_mode" }));
    Ok(())
}

#[tauri::command]
pub async fn get_active_stream_capabilities(
    app: AppHandle,
    parakeet_manager: State<'_, ParakeetManager>,
) -> Result<ActiveStreamCapabilities, String> {
    let active_engine = resolve_active_engine(&app).await?;
    let provider_engine = provider_engine_from_settings(&active_engine);
    let capabilities = EngineStreamCapabilities::for_engine(provider_engine);
    let settings = crate::commands::settings::get_settings(app.clone()).await?;
    let eou_status = if provider_engine == ProviderEngine::Parakeet {
        parakeet_manager
            .eou_model_status(&app, DEFAULT_EOU_CHUNK_MS)
            .await
            .map_err(|error| error.to_string())?
    } else {
        ParakeetEouModelStatus {
            chunk_ms: DEFAULT_EOU_CHUNK_MS,
            downloaded: false,
            path: None,
        }
    };

    Ok(ActiveStreamCapabilities {
        active_engine,
        capabilities,
        eou_model_downloaded: eou_status.downloaded,
        eou_model_path: eou_status.path,
        eou_chunk_ms: eou_status.chunk_ms,
        transcription_mode: settings.transcription_mode,
    })
}

#[tauri::command]
pub async fn eou_model_status(
    app: AppHandle,
    chunk_ms: Option<u16>,
    parakeet_manager: State<'_, ParakeetManager>,
) -> Result<ParakeetEouModelStatus, String> {
    parakeet_manager
        .eou_model_status(&app, chunk_ms.unwrap_or(DEFAULT_EOU_CHUNK_MS))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn download_eou_model(
    app: AppHandle,
    chunk_ms: Option<u16>,
    parakeet_manager: State<'_, ParakeetManager>,
) -> Result<(), String> {
    let chunk_ms = chunk_ms.unwrap_or(DEFAULT_EOU_CHUNK_MS);
    let app_for_progress = app.clone();
    parakeet_manager
        .download_eou_model(&app, chunk_ms, move |downloaded, total, phase| {
            let progress = if total == 0 {
                0.0
            } else {
                (downloaded as f64 / total as f64) * 100.0
            };
            let _ = emit_to_all(
                &app_for_progress,
                "download-progress",
                json!({
                    "model": EOU_MODEL_ID,
                    "engine": "parakeet",
                    "downloaded": downloaded,
                    "total": total,
                    "progress": progress,
                    "requestId": null,
                    "phase": phase.as_deref(),
                }),
            );
        })
        .await?;
    let _ = emit_to_all(
        &app,
        "model-downloaded",
        json!({
            "model": EOU_MODEL_ID,
            "engine": "parakeet",
            "requestId": null
        }),
    );
    Ok(())
}

#[tauri::command]
pub async fn activate_live_preview(
    app: AppHandle,
    parakeet_manager: State<'_, ParakeetManager>,
) -> Result<ActiveStreamCapabilities, String> {
    persist_transcription_mode(&app, TRANSCRIPTION_MODE_REGULAR)?;

    let active_engine = resolve_active_engine(&app).await?;
    let provider_engine = provider_engine_from_settings(&active_engine);
    let capabilities = EngineStreamCapabilities::for_engine(provider_engine);
    if !capabilities.supports_streaming {
        return Err("Live preview requires a streaming-capable engine.".to_string());
    }

    // Every streaming engine now enables instantly: Whisper and Parakeet decode-ahead
    // (plans 032/051) reuse the already-loaded transcription model, and the cloud WS
    // engines need only their API key. The Parakeet EOU download+warmup that used to
    // live here returns when upstream FluidAudio fixes EOU (empty transcripts,
    // retested broken on 0.15.5 — plans/042 + 051).
    persist_transcription_mode(&app, TRANSCRIPTION_MODE_LIVE_PREVIEW)?;
    get_active_stream_capabilities(app, parakeet_manager).await
}

pub(crate) fn register_active_download(
    active_downloads: &Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>,
    model_name: &str,
    cancel_flag: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut downloads = active_downloads.lock().map_err(|e| {
        log::error!("Failed to lock active downloads for inserting: {}", e);
        "Failed to initialize download tracking".to_string()
    })?;

    if downloads.contains_key(model_name) {
        return Err(format!(
            "A download or delete operation is already in progress for '{}'",
            model_name
        ));
    }

    downloads.insert(model_name.to_string(), cancel_flag);
    Ok(())
}

pub(crate) fn clear_active_download(
    active_downloads: &Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>,
    model_name: &str,
) {
    match active_downloads.lock() {
        Ok(mut downloads) => {
            downloads.remove(model_name);
        }
        Err(e) => {
            log::warn!("Failed to lock active downloads for cleanup: {}", e);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelEngine {
    Whisper,
    Parakeet,
}

impl ModelEngine {
    fn as_str(&self) -> &'static str {
        match self {
            ModelEngine::Whisper => "whisper",
            ModelEngine::Parakeet => "parakeet",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DownloadTarget {
    engine: ModelEngine,
    size_bytes: u64,
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    model_name: String,
    request_id: Option<String>,
    whisper_state: State<'_, RwLock<WhisperManager>>,
    parakeet_manager: State<'_, ParakeetManager>,
    active_downloads: ActiveDownloadsState<'_>,
) -> Result<(), String> {
    #[cfg(debug_assertions)]
    let download_start = Instant::now();

    // Create and register cancellation flag before any await to avoid startup races.
    let cancel_flag = Arc::new(AtomicBool::new(false));
    register_active_download(&active_downloads, &model_name, cancel_flag.clone())?;

    let download_target =
        match identify_download_target(&model_name, &whisper_state, &parakeet_manager).await {
            Ok(target) => target,
            Err(error) => {
                clear_active_download(&active_downloads, &model_name);
                return Err(error);
            }
        };

    log::info!("Starting download for model: {}", model_name);

    // Monitor system resources at download start
    #[cfg(debug_assertions)]
    system_monitor::log_resources_before_operation("MODEL_DOWNLOAD");

    // Log to onboarding if in onboarding context
    let model_size_mb = download_target.size_bytes / (1024 * 1024); // bytes → MB
    onboarding_logger::with_onboarding_logger(|logger| {
        logger.log_model_download_start(&model_name, model_size_mb);
    });

    let app_handle = app.clone();

    let model_name_clone = model_name.clone();
    let request_id_for_progress = request_id.clone();

    // Create an async-safe wrapper for progress callback
    let (progress_tx, mut progress_rx) =
        tokio::sync::mpsc::unbounded_channel::<(u64, u64, Option<String>)>();

    // Spawn task to handle progress updates
    let progress_handle = tokio::spawn(async move {
        let mut verification_emitted = false;

        while let Some((downloaded, total, phase)) = progress_rx.recv().await {
            let progress = (downloaded as f64 / total as f64) * 100.0;
            log::debug!(
                "Download progress for {}: {:.1}%",
                &model_name_clone,
                progress
            );

            // Log to onboarding if active
            onboarding_logger::with_onboarding_logger(|logger| {
                logger.log_model_download_progress(&model_name_clone, progress as u8);
            });

            // Progress is already being emitted via events, no need for state storage

            if let Err(e) = emit_to_all(
                &app_handle,
                "download-progress",
                serde_json::json!({
                    "model": &model_name_clone,
                    "engine": download_target.engine.as_str(),
                    "downloaded": downloaded,
                    "total": total,
                    "progress": progress,
                    "requestId": request_id_for_progress.as_deref(),
                    "phase": phase.as_deref(),
                }),
            ) {
                log::warn!("Failed to emit download progress: {}", e);
            }

            // When download reaches 100%, emit verification event
            if progress >= 100.0 && !verification_emitted {
                verification_emitted = true;
                log::info!(
                    "Download complete, starting verification for model: {}",
                    &model_name_clone
                );
                if let Err(e) = emit_to_all(
                    &app_handle,
                    "model-verifying",
                    serde_json::json!({
                        "model": &model_name_clone,
                        "engine": download_target.engine.as_str(),
                        "requestId": request_id_for_progress.as_deref()
                    }),
                ) {
                    log::warn!("Failed to emit model-verifying event: {}", e);
                }
            }
        }
    });

    // Execute download (no retry - user can click download again if it fails)
    let download_result = if cancel_flag.load(Ordering::Relaxed) {
        log::info!("Download cancelled for model: {}", model_name);
        Err("Download cancelled by user".to_string())
    } else {
        log::info!("Starting download for model: {}", model_name);

        let progress_tx_clone = progress_tx.clone();
        let result = match download_target.engine {
            ModelEngine::Whisper => {
                let (model_info, output_path, models_dir) = {
                    let manager = whisper_state.read().await;
                    let (model_info, output_path) = manager.get_model_info(&model_name)?;
                    (model_info, output_path, manager.models_dir())
                };

                WhisperManager::download_model_file(
                    &model_info,
                    &output_path,
                    &models_dir,
                    Some(cancel_flag.clone()),
                    move |downloaded, total| {
                        let _ = progress_tx_clone.send((downloaded, total, None));
                    },
                )
                .await
            }
            ModelEngine::Parakeet => {
                parakeet_manager
                    .download_model(
                        &app,
                        &model_name,
                        Some(cancel_flag.clone()),
                        move |downloaded, total, phase| {
                            let _ = progress_tx_clone.send((downloaded, total, phase));
                        },
                    )
                    .await
            }
        };

        match &result {
            Ok(_) => {
                log::info!(
                    "Download succeeded for model {} (engine={})",
                    model_name,
                    download_target.engine.as_str()
                );
            }
            Err(e) => {
                log::error!(
                    "Download failed for model {} (engine={}): {}",
                    model_name,
                    download_target.engine.as_str(),
                    e
                );
            }
        }

        result
    };

    // Close the progress channel to signal completion
    drop(progress_tx);

    // Ensure progress handler completes
    let _ = progress_handle.await;

    log::info!("Processing download result for model: {}", model_name);
    match download_result {
        Err(ref e) if e.contains("cancelled") => {
            log::info!("Download was cancelled");
            // Emit download-cancelled event
            if let Err(e) = emit_to_all(
                &app,
                "download-cancelled",
                serde_json::json!({
                    "model": model_name,
                    "engine": download_target.engine.as_str(),
                    "requestId": request_id.as_deref()
                }),
            ) {
                log::warn!("Failed to emit download-cancelled event: {}", e);
            }
            clear_active_download(&active_downloads, &model_name);
            Err(e.clone())
        }
        Ok(_) => {
            log::info!("Download completed successfully for model: {}", model_name);

            // Monitor system resources after download completion
            #[cfg(debug_assertions)]
            system_monitor::log_resources_after_operation(
                "MODEL_DOWNLOAD",
                download_start.elapsed().as_millis() as u64,
            );

            // Log to onboarding if active
            onboarding_logger::with_onboarding_logger(|logger| {
                // Calculate duration if possible
                logger.log_model_download_complete(&model_name, 0); // TODO: track actual duration
            });

            // Refresh/verify downloaded status
            match download_target.engine {
                ModelEngine::Whisper => {
                    let verified = {
                        let mut manager = whisper_state.write().await;
                        manager.refresh_downloaded_status();
                        manager
                            .get_models_status()
                            .get(&model_name)
                            .map(|info| info.downloaded)
                            .unwrap_or(false)
                    };

                    if !verified {
                        let msg = format!(
                            "Whisper manager did not confirm '{}' as downloaded. Please try again.",
                            model_name
                        );
                        log::warn!("{}", msg);
                        if let Err(emit_err) = emit_to_all(
                            &app,
                            "download-error",
                            serde_json::json!({
                                "model": model_name,
                                "engine": download_target.engine.as_str(),
                                "requestId": request_id.as_deref(),
                                "error": msg
                            }),
                        ) {
                            log::warn!("Failed to emit download-error event: {}", emit_err);
                        }
                        clear_active_download(&active_downloads, &model_name);
                        return Err("verification_failed".to_string());
                    }
                }
                ModelEngine::Parakeet => {
                    // Verify Parakeet reports the requested model as downloaded
                    let verified = parakeet_manager
                        .list_models()
                        .into_iter()
                        .any(|m| m.name == model_name && m.downloaded);

                    if !verified {
                        let msg = format!(
                            "Parakeet sidecar did not confirm '{}' as downloaded. Please try again.",
                            model_name
                        );
                        log::warn!("{}", msg);
                        // Emit download-error event and return Err
                        if let Err(emit_err) = emit_to_all(
                            &app,
                            "download-error",
                            serde_json::json!({
                                "model": model_name,
                                "engine": download_target.engine.as_str(),
                                "requestId": request_id.as_deref(),
                                "error": msg
                            }),
                        ) {
                            log::warn!("Failed to emit download-error event: {}", emit_err);
                        }
                        clear_active_download(&active_downloads, &model_name);
                        return Err("verification_failed".to_string());
                    }
                }
            }

            // Emit success event after verification
            log::info!("Emitting model-downloaded event for {}", model_name);
            if let Err(e) = emit_to_all(
                &app,
                "model-downloaded",
                serde_json::json!({
                    "model": model_name,
                    "engine": download_target.engine.as_str(),
                    "requestId": request_id.as_deref()
                }),
            ) {
                log::warn!("Failed to emit model-downloaded event: {}", e);
            }

            // Refresh tray menu so the new model appears in the tray immediately
            if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
                log::warn!("Failed to update tray menu after model download: {}", e);
            }

            clear_active_download(&active_downloads, &model_name);
            Ok(())
        }
        Err(e) => {
            log::error!("Download failed for model {}: {}", model_name, e);

            // Log to onboarding if active
            onboarding_logger::with_onboarding_logger(|logger| {
                logger.log_model_download_failed(&model_name, &e);
            });

            // Emit download-error event
            if let Err(emit_err) = emit_to_all(
                &app,
                "download-error",
                serde_json::json!({
                    "model": model_name,
                    "engine": download_target.engine.as_str(),
                    "requestId": request_id.as_deref(),
                    "error": e.to_string()
                }),
            ) {
                log::warn!("Failed to emit download-error event: {}", emit_err);
            }

            // Progress tracking is event-based, no state cleanup needed

            clear_active_download(&active_downloads, &model_name);
            Err(e)
        }
    }
}

#[derive(serde::Serialize)]
pub struct ModelStatusResponse {
    pub models: Vec<UnifiedModelInfo>,
}

#[derive(Clone, serde::Serialize)]
pub struct UnifiedModelInfo {
    pub name: String,
    pub display_name: String,
    pub size: u64,
    pub url: String,
    pub sha256: String,
    pub downloaded: bool,
    pub speed_score: u8,
    pub accuracy_score: u8,
    pub recommended: bool,
    pub engine: String,
    pub kind: String,
    pub requires_setup: bool,
    pub underlying_model: Option<String>,
}

#[derive(serde::Serialize)]
pub struct ParakeetVocabularyStatusResponse {
    pub supported: bool,
    pub ready: bool,
}

/// Returns status of all available speech recognition models (Whisper + Parakeet).
///
/// **Platform Behavior**:
/// - **macOS (Apple Silicon)**: Returns Whisper and Parakeet models
/// - **macOS (Intel)**: Returns Whisper models only
/// - **Windows/Linux**: Returns Whisper models only (Parakeet filtered at compile time)
///
/// Parakeet models are excluded anywhere Apple Neural Engine support is unavailable.
#[tauri::command]
pub async fn get_model_status(
    whisper_state: State<'_, RwLock<WhisperManager>>,
    parakeet_manager: State<'_, ParakeetManager>,
    app: tauri::AppHandle,
) -> Result<ModelStatusResponse, String> {
    log::debug!("[GET_MODEL_STATUS] Refreshing downloaded status...");

    let whisper_models_map = {
        let mut manager = whisper_state.write().await;
        manager.refresh_downloaded_status();
        manager.get_models_status()
    };

    let mut models: Vec<UnifiedModelInfo> = whisper_models_map
        .into_iter()
        .map(|(name, info)| convert_whisper_model(name, info))
        .collect();

    let parakeet_models = parakeet_manager.list_models();
    models.extend(parakeet_models.into_iter().map(convert_parakeet_model));

    // Inject cloud providers (e.g., Soniox)
    models.extend(collect_cloud_models(&app));

    // Sort with local models first (by size), cloud models afterwards
    models.sort_by(|a, b| {
        let a_key = (a.kind == "cloud", a.size);
        let b_key = (b.kind == "cloud", b.size);
        a_key.cmp(&b_key)
    });

    log::debug!("[GET_MODEL_STATUS] Returning {} models", models.len());

    Ok(ModelStatusResponse { models })
}

#[tauri::command]
pub async fn get_parakeet_vocabulary_status(
    app: AppHandle,
    parakeet_manager: State<'_, ParakeetManager>,
) -> Result<ParakeetVocabularyStatusResponse, String> {
    match parakeet_manager.status(&app).await {
        Ok(response) => {
            let Some(status) = ParakeetManager::vocabulary_status_from_response(&response) else {
                return Err(format!("Unexpected Parakeet response: {:?}", response));
            };
            Ok(ParakeetVocabularyStatusResponse {
                supported: status.supported,
                ready: status.ready,
            })
        }
        Err(err) => Err(format!("Failed to get Parakeet vocabulary status: {}", err)),
    }
}

#[tauri::command]
pub async fn download_parakeet_vocabulary_model(
    app: AppHandle,
    parakeet_manager: State<'_, ParakeetManager>,
    active_downloads: ActiveDownloadsState<'_>,
) -> Result<(), String> {
    const MODEL_ID: &str = "parakeet-vocabulary-ctc-110m";

    let cancel_flag = Arc::new(AtomicBool::new(false));
    register_active_download(&active_downloads, MODEL_ID, cancel_flag.clone())?;

    let _ = emit_to_all(
        &app,
        "download-progress",
        serde_json::json!({
            "model": MODEL_ID,
            "engine": "parakeet",
            "downloaded": 0,
            "total": 1,
            "progress": 0.0,
            "requestId": null,
            "phase": "starting",
        }),
    );

    // The CTC sidecar command is currently a single request without a public cancel hook.
    // Register the flag anyway so cancel_download sees the same active model id and this
    // path can report cancellation consistently once the sidecar returns.
    let download_result = parakeet_manager.download_ctc_models(&app).await;

    clear_active_download(&active_downloads, MODEL_ID);

    if cancel_flag.load(Ordering::Relaxed) {
        let _ = emit_to_all(
            &app,
            "download-cancelled",
            serde_json::json!({
                "model": MODEL_ID,
                "engine": "parakeet",
                "requestId": null,
            }),
        );
        return Err("Download cancelled by user".to_string());
    }

    match download_result {
        Ok(ParakeetResponse::Ok { .. }) | Ok(ParakeetResponse::Status { .. }) => {
            let _ = emit_to_all(
                &app,
                "model-downloaded",
                serde_json::json!({
                    "model": MODEL_ID,
                    "engine": "parakeet",
                    "requestId": null,
                }),
            );
            Ok(())
        }
        Ok(ParakeetResponse::Error { code, message, .. }) => {
            let error = format!("Failed to download Parakeet vocabulary model: {code}: {message}");
            let _ = emit_to_all(
                &app,
                "download-error",
                serde_json::json!({
                    "model": MODEL_ID,
                    "engine": "parakeet",
                    "requestId": null,
                    "error": &error,
                }),
            );
            Err(error)
        }
        Ok(other) => {
            let error = format!("Unexpected Parakeet response: {:?}", other);
            let _ = emit_to_all(
                &app,
                "download-error",
                serde_json::json!({
                    "model": MODEL_ID,
                    "engine": "parakeet",
                    "requestId": null,
                    "error": &error,
                }),
            );
            Err(error)
        }
        Err(err) => {
            let error = format!("Failed to download Parakeet vocabulary model: {}", err);
            let _ = emit_to_all(
                &app,
                "download-error",
                serde_json::json!({
                    "model": MODEL_ID,
                    "engine": "parakeet",
                    "requestId": null,
                    "error": &error,
                }),
            );
            Err(error)
        }
    }
}

async fn ensure_model_is_not_currently_shared(
    app: &AppHandle,
    model_name: &str,
) -> Result<(), String> {
    let Some(server_manager) =
        app.try_state::<AsyncMutex<crate::remote::lifecycle::RemoteServerManager>>()
    else {
        return Ok(());
    };

    let status = server_manager.lock().await.get_status();
    if status.enabled && status.model_name.as_deref() == Some(model_name) {
        return Err(format!(
            "Cannot delete model '{}' while it is being shared. Stop network sharing or switch the shared model first.",
            model_name
        ));
    }

    Ok(())
}

#[tauri::command]
pub async fn delete_model(
    app: AppHandle,
    model_name: String,
    whisper_state: State<'_, RwLock<WhisperManager>>,
    parakeet_manager: State<'_, ParakeetManager>,
    active_downloads: ActiveDownloadsState<'_>,
) -> Result<(), String> {
    let operation_flag = Arc::new(AtomicBool::new(false));
    register_active_download(&active_downloads, &model_name, operation_flag)?;
    let result = async {
        let engine = determine_model_engine(&model_name, &whisper_state, &parakeet_manager).await?;
        ensure_model_is_not_currently_shared(&app, &model_name).await?;

        match engine {
            ModelEngine::Whisper => {
                let mut manager = whisper_state.write().await;
                manager.delete_model_file(&model_name)?;
            }
            ModelEngine::Parakeet => {
                parakeet_manager.delete_model(&app, &model_name).await?;
            }
        }

        Ok::<ModelEngine, String>(engine)
    }
    .await;

    let engine = match result {
        Ok(engine) => engine,
        Err(error) => {
            clear_active_download(&active_downloads, &model_name);
            return Err(error);
        }
    };

    // Emit model-deleted event
    use tauri::Emitter;
    let _ = app.emit(
        "model-deleted",
        serde_json::json!({
            "model": model_name.clone(),
            "engine": engine.as_str()
        }),
    );

    // Refresh tray menu so the deleted model is removed from tray selection
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!("Failed to update tray menu after model deletion: {}", e);
    }

    clear_active_download(&active_downloads, &model_name);
    Ok(())
}

#[tauri::command]
pub async fn list_downloaded_models(
    state: State<'_, RwLock<WhisperManager>>,
) -> Result<Vec<String>, String> {
    let manager = state.read().await;
    Ok(manager.list_downloaded_files())
}

async fn identify_download_target(
    model_name: &str,
    whisper_state: &State<'_, RwLock<WhisperManager>>,
    parakeet_manager: &ParakeetManager,
) -> Result<DownloadTarget, String> {
    let engine = determine_model_engine(model_name, whisper_state, parakeet_manager).await?;

    match engine {
        ModelEngine::Whisper => {
            let manager = whisper_state.read().await;
            if let Some(info) = manager.get_models_status().get(model_name) {
                Ok(DownloadTarget {
                    engine,
                    size_bytes: info.size,
                })
            } else {
                Err(format!(
                    "Model '{}' not found in Whisper registry",
                    model_name
                ))
            }
        }
        ModelEngine::Parakeet => {
            if let Some(definition) = parakeet_manager.get_model_definition(model_name) {
                Ok(DownloadTarget {
                    engine,
                    size_bytes: definition.estimated_size,
                })
            } else {
                Err(format!(
                    "Model '{}' not found in Parakeet registry",
                    model_name
                ))
            }
        }
    }
}

async fn determine_model_engine(
    model_name: &str,
    whisper_state: &State<'_, RwLock<WhisperManager>>,
    parakeet_manager: &ParakeetManager,
) -> Result<ModelEngine, String> {
    {
        let manager = whisper_state.read().await;
        if manager.get_models_status().contains_key(model_name) {
            return Ok(ModelEngine::Whisper);
        }
    }

    if parakeet_manager.get_model_definition(model_name).is_some() {
        return Ok(ModelEngine::Parakeet);
    }

    Err(format!("Invalid model name: {}", model_name))
}

fn convert_whisper_model(name: String, info: ModelInfo) -> UnifiedModelInfo {
    UnifiedModelInfo {
        name,
        display_name: info.display_name.clone(),
        size: info.size,
        url: info.url.clone(),
        sha256: info.sha256.clone(),
        downloaded: info.downloaded,
        speed_score: info.speed_score,
        accuracy_score: info.accuracy_score,
        recommended: info.recommended,
        engine: ModelEngine::Whisper.as_str().to_string(),
        kind: "local".to_string(),
        requires_setup: false,
        underlying_model: None,
    }
}

fn convert_parakeet_model(status: ParakeetModelStatus) -> UnifiedModelInfo {
    UnifiedModelInfo {
        name: status.name,
        display_name: status.display_name,
        size: status.size,
        url: status.url,
        sha256: status.sha256,
        downloaded: status.downloaded,
        speed_score: status.speed_score,
        accuracy_score: status.accuracy_score,
        recommended: status.recommended,
        engine: ModelEngine::Parakeet.as_str().to_string(),
        kind: "local".to_string(),
        requires_setup: false,
        underlying_model: None,
    }
}

fn collect_cloud_models(app: &AppHandle) -> Vec<UnifiedModelInfo> {
    crate::cloud_stt::CloudProvider::ALL
        .iter()
        .map(|provider| {
            let has_key =
                secure_store::secure_has(app, provider.key_name()).unwrap_or_else(|err| {
                    log::warn!(
                        "[GET_MODEL_STATUS] Failed to check {} key presence: {}",
                        provider.display_name(),
                        err
                    );
                    false
                });
            UnifiedModelInfo {
                name: provider.id().to_string(),
                display_name: provider.display_name().to_string(),
                size: 0,
                url: String::new(),
                sha256: String::new(),
                downloaded: has_key,
                speed_score: provider.speed_score(),
                accuracy_score: provider.accuracy_score(),
                recommended: matches!(provider, crate::cloud_stt::CloudProvider::Soniox),
                engine: provider.id().to_string(),
                kind: "cloud".to_string(),
                requires_setup: !has_key,
                underlying_model: Some(provider.model_name().to_string()),
            }
        })
        .collect()
}

#[tauri::command]
pub async fn cancel_download(
    model_name: String,
    active_downloads: ActiveDownloadsState<'_>,
) -> Result<(), String> {
    log::info!("Cancelling download for model: {}", model_name);

    // Set the cancellation flag
    {
        match active_downloads.lock() {
            Ok(downloads) => {
                if let Some(cancel_flag) = downloads.get(&model_name) {
                    cancel_flag.store(true, Ordering::Relaxed);
                    log::info!("Set cancellation flag for model: {}", model_name);
                } else {
                    log::warn!("No active download found for model: {}", model_name);
                    return Ok(()); // Not an error if download doesn't exist
                }
            }
            Err(e) => {
                log::error!("Failed to lock active downloads for cancellation: {}", e);
                return Err("Failed to access download tracking".to_string());
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn verify_model(
    app: AppHandle,
    model_name: String,
    state: State<'_, RwLock<WhisperManager>>,
) -> Result<(), String> {
    log::info!("Verifying model: {}", model_name);

    // Get model info and check if it exists
    let (model_info, model_path) = {
        let manager = state.read().await;
        let info = manager
            .get_models_status()
            .get(&model_name)
            .ok_or(format!("Model '{}' not found", model_name))?
            .clone();
        let path = manager
            .get_model_path(&model_name)
            .ok_or(format!("Model '{}' path not found", model_name))?;
        (info, path)
    };

    // Check if file exists
    if !model_path.exists() {
        log::warn!("Model file does not exist: {:?}", model_path);
        return Err(format!("Model file not found: {}", model_name));
    }

    // Check file size
    let metadata = tokio::fs::metadata(&model_path)
        .await
        .map_err(|e| format!("Cannot read model file metadata: {}", e))?;

    let file_size = metadata.len();
    let expected_size = model_info.size;

    // Allow 5% tolerance for size differences
    let size_tolerance = (expected_size as f64 * 0.05) as u64;
    let min_size = expected_size.saturating_sub(size_tolerance);

    if file_size < min_size {
        log::warn!(
            "Model '{}' file size {} is less than expected minimum {}",
            model_name,
            file_size,
            min_size
        );

        // Delete the corrupted file
        if let Err(e) = tokio::fs::remove_file(&model_path).await {
            log::error!("Failed to delete corrupted model file: {}", e);
        }

        // Update manager status
        {
            let mut manager = state.write().await;
            manager.refresh_downloaded_status();
        }

        return Err(format!(
            "Model '{}' is corrupted and has been deleted. Please re-download.",
            model_name
        ));
    }

    // File looks good - mark as downloaded
    {
        let mut manager = state.write().await;
        if let Some(info) = manager.get_models_status_mut().get_mut(&model_name) {
            info.downloaded = true;
        }
    }

    log::info!("Model '{}' verified successfully", model_name);

    // Emit verification success event
    let _ = app.emit("model-verified", model_name.clone());

    Ok(())
}

#[tauri::command]
pub async fn preload_model(
    app: AppHandle,
    model_name: String,
    state: State<'_, RwLock<WhisperManager>>,
) -> Result<(), String> {
    use crate::whisper::cache::TranscriberCache;
    use tauri::async_runtime::Mutex as AsyncMutex;

    // Check license status before preloading
    log::info!("[Preload] Checking license status before preload_model");
    let license_status = check_license_status_internal(&app).await?;
    if matches!(
        license_status.status,
        LicenseState::Expired | LicenseState::None
    ) {
        return Err("License required to preload models".to_string());
    }

    log::info!("Preloading model: {}", model_name);

    // Get model path
    let model_path = {
        let manager = state.read().await;
        manager
            .get_model_path(&model_name)
            .ok_or(format!("Model '{}' not found", model_name))?
    };

    // On Windows with GPU acceleration, warm the Vulkan sidecar so the first transcription
    // after a manual preload isn't slow. No-op on non-Windows / CPU mode; when it does not
    // warm, fall through to loading the CPU transcriber cache.
    if crate::commands::audio::warm_whisper_gpu_sidecar_on_model_preload(&app, &model_path).await {
        log::info!(
            "Model '{}' preloaded successfully in Vulkan sidecar",
            model_name
        );
        return Ok(());
    }

    // Load the model into the CPU transcriber cache.
    {
        let cache_state = app.state::<AsyncMutex<TranscriberCache>>();
        let mut cache = cache_state.lock().await;
        let speed_mode = crate::commands::settings::read_whisper_speed_mode(&app);
        cache.get_or_create(&model_path, speed_mode)?;
    }

    log::info!("Model '{}' preloaded successfully", model_name);

    Ok(())
}
