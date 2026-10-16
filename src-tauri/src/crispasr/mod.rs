mod download;
pub(crate) mod final_result;
pub(crate) mod manager;
pub(crate) mod messages;
pub(crate) mod models;
pub(crate) mod pcm;
pub(crate) mod sidecar;
pub(crate) mod stream;
pub use manager::CrispasrManager;

pub fn model_status(app: &tauri::AppHandle) -> Vec<crate::commands::model::UnifiedModelInfo> {
    use tauri::Manager;
    let manager = app.try_state::<CrispasrManager>();
    models::MODELS
        .iter()
        .map(|model| crate::commands::model::UnifiedModelInfo {
            name: model.id.into(),
            display_name: model.display_name.into(),
            size: model.size,
            url: model.url_for(model.filename),
            sha256: model.sha256.into(),
            downloaded: manager
                .as_ref()
                .is_some_and(|manager| manager.is_downloaded(model.id)),
            // The existing list accepts unrated models. Do not invent accuracy or
            // speed rankings from a single smoke fixture.
            speed_score: 0,
            accuracy_score: 0,
            recommended: matches!(model.id, models::DEFAULT_MODEL_ID | "confucius4-r2t2-q4_k"),
            engine: "crispasr".into(),
            kind: "local".into(),
            requires_setup: sidecar::runtime_path(app, false).is_none(),
            available_models: None,
            underlying_model: None,
            supported_languages: Some(
                model
                    .languages()
                    .iter()
                    .map(|language| language.to_string())
                    .collect(),
            ),
        })
        .collect()
}

pub fn preload(app: tauri::AppHandle, model: String) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        if let Some(manager) = app.try_state::<CrispasrManager>() {
            if manager.preload(&app, &model).await.is_err() {
                log::warn!("CrispASR background preload unavailable");
            }
        }
    });
}

pub(crate) async fn release_idle_whisper(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(cache) =
        app.try_state::<tauri::async_runtime::Mutex<crate::whisper::cache::TranscriberCache>>()
    {
        // Active CPU/Metal decodes retain their own Arc; clearing the cache
        // releases only idle weights and cannot invalidate their contexts.
        if let Ok(mut cache) = cache.try_lock() {
            cache.clear();
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(client) = app.try_state::<crate::whisper::gpu_sidecar::GpuSidecarClient>() {
        client.unload_if_idle().await;
    }
}

pub(crate) fn unload_if_unselected(app: tauri::AppHandle) {
    use tauri::Manager;
    use tauri_plugin_store::StoreExt;
    tauri::async_runtime::spawn(async move {
        if let Some(manager) = app.try_state::<CrispasrManager>() {
            manager
                .unload_when(|| {
                    app.store("settings")
                        .ok()
                        .and_then(|store| store.get("current_model_engine"))
                        != Some(serde_json::json!("crispasr"))
                })
                .await;
        }
    });
}

pub fn restore_selection(
    app: &tauri::AppHandle,
    store: &tauri_plugin_store::Store<tauri::Wry>,
    model: &str,
) -> bool {
    use tauri::Manager;
    if models::get(model).is_none() {
        store.set("current_model", serde_json::json!(""));
        store.set("current_model_engine", serde_json::json!("whisper"));
        let _ = store.save();
        return true;
    }
    if app
        .try_state::<CrispasrManager>()
        .is_some_and(|manager| manager.is_downloaded(model))
    {
        preload(app.clone(), model.to_string());
    }
    false
}
