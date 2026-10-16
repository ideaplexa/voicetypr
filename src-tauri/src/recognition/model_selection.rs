use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::parakeet;
use crate::remote::settings::{ConnectionStatus, RemoteSettings};
use crate::whisper;

/// Snapshot of recognition engine availability
#[derive(Debug, Clone, serde::Serialize)]
pub struct RecognitionAvailabilitySnapshot {
    pub whisper_available: bool,
    pub parakeet_available: bool,
    pub crispasr_available: bool,
    pub cloud_selected: bool,
    pub cloud_ready: bool,
    pub remote_selected: bool,
    pub remote_status: ConnectionStatus,
    pub remote_last_checked: u64,
    pub remote_available: bool,
}

impl RecognitionAvailabilitySnapshot {
    pub fn any_available(&self) -> bool {
        self.whisper_available
            || self.parakeet_available
            || self.crispasr_available
            || (self.cloud_selected && self.cloud_ready)
            || self.remote_available
    }
}

pub(crate) fn remote_availability_from_settings(
    remote_settings: Option<&RemoteSettings>,
) -> (bool, ConnectionStatus, u64, bool) {
    let Some(remote_settings) = remote_settings else {
        return (false, ConnectionStatus::Unknown, 0, false);
    };

    let Some(connection) = remote_settings.get_active_connection() else {
        return (false, ConnectionStatus::Unknown, 0, false);
    };

    let remote_status = connection.status.clone();
    let remote_last_checked = connection.last_checked;
    let remote_available = matches!(remote_status, ConnectionStatus::Online);

    (true, remote_status, remote_last_checked, remote_available)
}

pub async fn emit_recognition_availability(
    app: &tauri::AppHandle,
) -> RecognitionAvailabilitySnapshot {
    let availability = recognition_availability_snapshot(app).await;

    if let Err(err) = app.emit("recognition-availability", availability.clone()) {
        log::warn!("Failed to emit recognition availability event: {}", err);
    }

    availability
}

/// Get a snapshot of which recognition engines are available
pub async fn recognition_availability_snapshot(
    app: &tauri::AppHandle,
) -> RecognitionAvailabilitySnapshot {
    let whisper_available =
        if let Some(manager) = app.try_state::<AsyncRwLock<whisper::manager::WhisperManager>>() {
            manager.read().await.has_downloaded_models()
        } else {
            false
        };

    let parakeet_available =
        if let Some(parakeet_manager) = app.try_state::<parakeet::ParakeetManager>() {
            parakeet_manager
                .list_models()
                .into_iter()
                .any(|model| model.downloaded)
        } else {
            false
        };

    let (cloud_selected, cloud_ready) = match app.store("settings") {
        Ok(store) => {
            let engine = store
                .get("current_model_engine")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "whisper".to_string());
            if let Some(provider) = crate::cloud_stt::CloudProvider::from_id(&engine) {
                let has_key =
                    crate::secure_store::secure_has(app, provider.key_name()).unwrap_or(false);
                (true, has_key)
            } else {
                (false, false)
            }
        }
        Err(_) => (false, false),
    };

    let (remote_selected, remote_status, remote_last_checked, remote_available) =
        if let Some(remote_settings) = app.try_state::<AsyncMutex<RemoteSettings>>() {
            let settings = remote_settings.lock().await;
            remote_availability_from_settings(Some(&settings))
        } else {
            remote_availability_from_settings(None)
        };

    RecognitionAvailabilitySnapshot {
        whisper_available,
        parakeet_available,
        crispasr_available: app
            .try_state::<crate::crispasr::CrispasrManager>()
            .is_some_and(|manager| {
                crate::crispasr::models::MODELS
                    .iter()
                    .any(|model| manager.is_downloaded(model.id))
            })
            && crate::crispasr::sidecar::runtime_path(app, false).is_some(),
        cloud_selected,
        cloud_ready,
        remote_selected,
        remote_status,
        remote_last_checked,
        remote_available,
    }
}

#[tauri::command]
pub async fn get_recognition_availability_snapshot(
    app: tauri::AppHandle,
) -> Result<RecognitionAvailabilitySnapshot, String> {
    Ok(recognition_availability_snapshot(&app).await)
}

fn pick_best_parakeet_model(models: Vec<parakeet::ParakeetModelStatus>) -> Option<String> {
    let mut downloaded: Vec<_> = models.into_iter().filter(|m| m.downloaded).collect();
    downloaded.sort_by(|a, b| {
        b.recommended
            .cmp(&a.recommended)
            .then(b.accuracy_score.cmp(&a.accuracy_score))
            .then(a.size.cmp(&b.size))
    });
    downloaded.first().map(|m| m.name.clone())
}

async fn should_prefer_cpu_whisper_models(app: &tauri::AppHandle) -> bool {
    #[cfg(not(target_os = "windows"))]
    let _ = app;
    #[cfg(target_os = "windows")]
    {
        let mode = app
            .store("settings")
            .ok()
            .and_then(|store| {
                store
                    .get("transcription_acceleration")
                    .and_then(|value| value.as_str().map(str::to_owned))
            })
            .unwrap_or_else(|| "auto".to_string());
        if mode == "cpu" {
            true
        } else if mode == "gpu" {
            false
        } else if let Some(client) = app.try_state::<whisper::gpu_sidecar::GpuSidecarClient>() {
            client.status().await.gpu_available == Some(false)
        } else {
            false
        }
    }

    #[cfg(target_os = "macos")]
    {
        std::env::consts::ARCH != "aarch64"
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        false
    }
}

async fn pick_best_whisper_model(
    app: &tauri::AppHandle,
    manager: &AsyncRwLock<whisper::manager::WhisperManager>,
) -> Option<String> {
    let manager = manager.read().await;
    let mut downloaded: Vec<_> = manager
        .get_models_status()
        .into_iter()
        .filter(|(_, info)| info.downloaded)
        .collect();
    if should_prefer_cpu_whisper_models(app).await {
        downloaded.sort_by(|a, b| {
            b.1.recommended
                .cmp(&a.1.recommended)
                .then(b.1.speed_score.cmp(&a.1.speed_score))
                .then(b.1.accuracy_score.cmp(&a.1.accuracy_score))
                .then(a.1.size.cmp(&b.1.size))
        });
    } else {
        downloaded.sort_by(|a, b| {
            b.1.recommended
                .cmp(&a.1.recommended)
                .then(b.1.accuracy_score.cmp(&a.1.accuracy_score))
                .then(a.1.size.cmp(&b.1.size))
        });
    }
    downloaded.first().map(|(name, _)| name.clone())
}

/// Auto-select the best available model if none is currently selected.
///
/// This must not mark onboarding complete: reset/re-run onboarding should still
/// require the user to confirm setup instead of being closed by startup checks.
pub async fn auto_select_model_if_needed(
    app: &tauri::AppHandle,
    availability: &RecognitionAvailabilitySnapshot,
) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let current_model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    if !current_model.is_empty() {
        return Ok(());
    }

    // A default is a preference among usable downloads, never permission to
    // download silently or replace the saved choice handled above.
    let mut selection = app
        .try_state::<crate::crispasr::CrispasrManager>()
        .filter(|manager| {
            availability.crispasr_available
                && manager.is_downloaded(crate::crispasr::models::DEFAULT_MODEL_ID)
        })
        .map(|_| {
            (
                "crispasr".to_string(),
                crate::crispasr::models::DEFAULT_MODEL_ID.to_string(),
            )
        });

    if selection.is_none() && availability.parakeet_available {
        if let Some(parakeet_manager) = app.try_state::<parakeet::ParakeetManager>() {
            if let Some(model) = pick_best_parakeet_model(parakeet_manager.list_models()) {
                selection = Some(("parakeet".to_string(), model));
            }
        }
    }

    if selection.is_none() && availability.whisper_available {
        if let Some(whisper_state) =
            app.try_state::<AsyncRwLock<whisper::manager::WhisperManager>>()
        {
            if let Some(model) = pick_best_whisper_model(app, &whisper_state).await {
                selection = Some(("whisper".to_string(), model));
            }
        }
    }

    if selection.is_none() && availability.crispasr_available {
        if let Some(manager) = app.try_state::<crate::crispasr::CrispasrManager>() {
            if let Some(model) = crate::crispasr::models::MODELS
                .iter()
                .find(|model| manager.is_downloaded(model.id))
            {
                selection = Some(("crispasr".to_string(), model.id.to_string()));
            }
        }
    }

    if selection.is_none() && availability.cloud_selected && availability.cloud_ready {
        let engine = store
            .get("current_model_engine")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "whisper".to_string());
        selection = Some((engine.clone(), engine));
    }

    let Some((engine, model)) = selection else {
        return Ok(());
    };

    store.set("current_model", serde_json::Value::String(model.clone()));
    store.set(
        "current_model_engine",
        serde_json::Value::String(engine.clone()),
    );
    store.save().map_err(|e| e.to_string())?;
    if let Err(e) = app.emit("settings-changed", ()) {
        log::warn!(
            "Failed to emit settings-changed after auto-selection: {}",
            e
        );
    }

    log::info!(
        "Auto-selected {} model '{}' based on availability snapshot",
        engine,
        model
    );

    if let Err(e) = app.emit(
        "model-auto-selected",
        serde_json::json!({ "engine": engine, "model": model }),
    ) {
        log::warn!("Failed to emit model auto-selection event: {}", e);
    }

    let app_for_tray = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::commands::settings::update_tray_menu(app_for_tray.clone()).await {
            log::warn!("Failed to refresh tray menu after auto-selection: {}", e);
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::RecognitionAvailabilitySnapshot;
    use crate::remote::settings::{ConnectionStatus, RemoteSettings};
    #[test]
    fn remote_availability_snapshot_reports_no_active_remote_as_unresolved() {
        let settings = RemoteSettings::default();

        let (remote_selected, remote_status, remote_last_checked, remote_available) =
            super::remote_availability_from_settings(Some(&settings));

        assert!(!remote_selected);
        assert_eq!(remote_status, ConnectionStatus::Unknown);
        assert_eq!(remote_last_checked, 0);
        assert!(!remote_available);
    }

    #[test]
    fn remote_availability_snapshot_reports_structured_remote_state() {
        let mut settings = RemoteSettings::default();
        let conn = settings.add_connection(
            "192.168.1.10".to_string(),
            47842,
            None,
            Some("Remote".to_string()),
            None,
        );
        let conn_id = conn.id.clone();
        settings
            .set_active_connection(Some(conn_id.clone()))
            .unwrap();
        let active = settings
            .saved_connections
            .iter_mut()
            .find(|c| c.id == conn_id)
            .expect("active connection should exist");
        active.status = ConnectionStatus::Unknown;
        active.last_checked = 1_717_000_000_000;

        let (remote_selected, remote_status, remote_last_checked, remote_available) =
            super::remote_availability_from_settings(Some(&settings));

        assert!(remote_selected);
        assert_eq!(remote_status, ConnectionStatus::Unknown);
        assert_eq!(remote_last_checked, 1_717_000_000_000);
        assert!(!remote_available);
    }

    #[test]
    fn any_available_is_true_when_authenticated_remote_is_available() {
        let snapshot = RecognitionAvailabilitySnapshot {
            whisper_available: false,
            parakeet_available: false,
            crispasr_available: false,
            cloud_selected: false,
            cloud_ready: false,
            remote_selected: true,
            remote_status: ConnectionStatus::Online,
            remote_last_checked: 1,
            remote_available: true,
        };

        assert!(snapshot.any_available());
    }

    #[test]
    fn any_available_is_false_when_remote_is_only_selected() {
        let snapshot = RecognitionAvailabilitySnapshot {
            whisper_available: false,
            parakeet_available: false,
            crispasr_available: false,
            cloud_selected: false,
            cloud_ready: false,
            remote_selected: true,
            remote_status: ConnectionStatus::Unknown,
            remote_last_checked: 1,
            remote_available: false,
        };

        assert!(!snapshot.any_available());
    }

    #[test]
    fn any_available_is_false_when_nothing_is_ready() {
        let snapshot = RecognitionAvailabilitySnapshot {
            whisper_available: false,
            parakeet_available: false,
            crispasr_available: false,
            cloud_selected: false,
            cloud_ready: false,
            remote_selected: false,
            remote_status: ConnectionStatus::Unknown,
            remote_last_checked: 0,
            remote_available: false,
        };

        assert!(!snapshot.any_available());
    }
}
