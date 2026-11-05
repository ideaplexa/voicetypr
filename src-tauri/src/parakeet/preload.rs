//! Startup and selection preloads, kept out of the bootstrap and settings monoliths.
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
use crate::parakeet;
use crate::parakeet::ParakeetManager;
use crate::whisper;
use crate::whisper::cache::TranscriberCache;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
use tauri::Emitter;
use tauri::{AppHandle, Manager};
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
use tauri_plugin_store::StoreExt;

pub(crate) async fn startup_whisper(app_handle: AppHandle, current_model: String) {
    log::info!("Attempting to preload model on startup: {}", current_model);

    // Get model path from WhisperManager
    let whisper_state = app_handle.state::<AsyncRwLock<whisper::manager::WhisperManager>>();
    let model_path = {
        let manager = whisper_state.read().await;
        manager.get_model_path(&current_model)
    };

    if let Some(model_path) = model_path {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        let _local_model_gate = crate::parakeet::onnx::LOCAL_MODEL_GATE.lock().await;
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        let still_selected = || {
            app_handle.store("settings").ok().is_some_and(|store| {
                let engine = store
                    .get("current_model_engine")
                    .and_then(|v| v.as_str().map(str::to_owned));
                let model = store
                    .get("current_model")
                    .and_then(|v| v.as_str().map(str::to_owned));
                crate::parakeet::onnx::selected_model_matches(
                    engine.as_deref(),
                    model.as_deref(),
                    "whisper",
                    &current_model,
                )
            })
        };
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        if !still_selected() {
            return;
        }
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        app_handle
            .state::<parakeet::ParakeetManager>()
            .unload_onnx()
            .await;
        if crate::transcription::engines::warm_whisper_gpu_sidecar_on_model_preload(
            &app_handle,
            &model_path,
        )
        .await
        {
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            if !still_selected() {
                app_handle
                    .state::<crate::whisper::gpu_sidecar::GpuSidecarClient>()
                    .abort_active_process()
                    .await;
                return;
            }
            log::info!(
                "Successfully preloaded model '{}' in Vulkan sidecar",
                current_model
            );
        } else {
            let preload_result = {
                let cache_state = app_handle.state::<AsyncMutex<TranscriberCache>>();
                let mut cache = cache_state.lock().await;
                let speed_mode = crate::commands::settings::read_whisper_speed_mode(&app_handle);
                let result = cache.get_or_create(&model_path, speed_mode).map(|_| ());
                #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
                if !still_selected() {
                    cache.clear();
                    return;
                }
                result
            };

            match preload_result {
                Ok(()) => {
                    log::info!(
                        "Successfully preloaded model '{}' into cache",
                        current_model
                    );
                }
                Err(e) => {
                    log::warn!(
                        "Failed to preload model '{}': {}. App will continue without preloading.",
                        current_model,
                        e
                    );
                }
            }
        }
    } else {
        log::warn!(
            "Model '{}' not found in models directory, skipping preload",
            current_model
        );
    }
}

pub(crate) fn selected_in_background(app_clone: AppHandle, model_name: String) {
    tokio::spawn(async move {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        let _ = app_clone.emit("parakeet-model-warming", model_name.clone());
        let parakeet_manager = app_clone.state::<ParakeetManager>();
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        let load_result = parakeet_manager
            .preload_selected_onnx(&app_clone, &model_name)
            .await;
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
        let load_result = parakeet_manager
            .load_model(&app_clone, &model_name)
            .await
            .map(|()| crate::parakeet::manager::PreloadOutcome::Loaded);
        match load_result {
            Ok(crate::parakeet::manager::PreloadOutcome::Loaded) => {
                log::info!("Successfully preloaded new model: {}", model_name);
                match parakeet_manager.warmup(&app_clone).await {
                    Ok(Some(ms)) => log::info!(
                        "Successfully warmed new Parakeet model '{}' in {}ms",
                        model_name,
                        ms
                    ),
                    Ok(None) => {
                        log::info!("Parakeet warmup skipped for '{}'", model_name)
                    }
                    Err(e) => log::warn!("Failed to warm new Parakeet model: {}", e),
                }
            }
            Ok(crate::parakeet::manager::PreloadOutcome::Superseded) => {}
            Err(e) => log::warn!("Failed to preload new model: {}", e),
        }
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        let _ = app_clone.emit("parakeet-model-warmed", model_name);
    });
}

pub(crate) fn selected_parakeet_preload_candidate<'a>(
    engine: &str,
    selected_model: &'a str,
    downloaded: bool,
) -> Option<&'a str> {
    if engine != "parakeet" || selected_model.is_empty() || !downloaded {
        return None;
    }
    // A saved explicit selection may warm the lazy DLL, but only after admission.
    // Exercise Windows' portable admission branch in host tests too.
    #[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
    crate::onnx_runtime::require_cpu().ok()?;
    Some(selected_model)
}

pub(crate) async fn recover_downloads(app: &AppHandle) {
    // Startup checks run in a background task. Resolve interrupted downloads
    // before taking the availability snapshot or choosing a model to preload.
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        let onnx_root = app
            .path()
            .app_data_dir()
            .map(|path| path.join("models").join("parakeet-onnx"));
        match onnx_root {
            Ok(root) => {
                let recovery = parakeet::onnx::download::recover_on_startup(
                    &root,
                    &parakeet::onnx::MODEL_OPERATION_LOCK,
                )
                .await;
                // An error can follow an earlier cleanup, so refresh then too.
                if matches!(&recovery, Ok(true) | Err(_)) {
                    if let Err(error) = app.emit(
                        "model-downloaded",
                        serde_json::json!({
                            "model": parakeet::onnx::download::MODEL_ID,
                            "engine": "parakeet",
                            "refreshOnly": true
                        }),
                    ) {
                        log::warn!(
                            "Failed to refresh Parakeet model status after recovery: {error}"
                        );
                    }
                }
                if let Err(error) = recovery {
                    log::warn!("Parakeet download recovery: {error}");
                }
            }
            Err(_) => log::warn!("Cannot locate the Parakeet model directory for recovery"),
        }
    }

    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    let _ = app;
}

#[cfg(test)]
mod startup_model_decision_tests {
    use super::selected_parakeet_preload_candidate;

    #[test]
    fn preloads_only_a_downloaded_selected_parakeet_model() {
        let model = "parakeet-tdt-0.6b-v3";
        assert_eq!(
            selected_parakeet_preload_candidate("parakeet", model, true),
            crate::onnx_runtime::require_cpu().ok().map(|()| model)
        );
        assert_eq!(
            selected_parakeet_preload_candidate("parakeet", model, false),
            None
        );
        assert_eq!(
            selected_parakeet_preload_candidate("whisper", model, true),
            None
        );
        assert_eq!(
            selected_parakeet_preload_candidate("parakeet", "", true),
            None
        );
    }

    #[test]
    fn unsupported_cpu_never_admits_selected_parakeet_startup_preload() {
        crate::onnx_runtime::without_cpu(|| {
            assert_eq!(
                selected_parakeet_preload_candidate("parakeet", "parakeet-tdt-0.6b-v3", true),
                None
            );
        });
    }
}
