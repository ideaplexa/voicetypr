use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

// secure.dat is a sibling of these directories and must survive reset.
fn app_data_directories_to_clear(app_data_dir: &Path) -> [(PathBuf, &'static str); 5] {
    [
        (app_data_dir.join("stores"), "Stores directory"),
        (app_data_dir.join("models"), "Downloaded models"),
        (
            app_data_dir.join("parakeet-tdt-0.6b-v3"),
            "Parakeet model data",
        ),
        (
            app_data_dir.join("parakeet-tdt-0.6b-v2"),
            "Parakeet model data",
        ),
        (app_data_dir.join("recordings"), "Audio recordings"),
    ]
}

#[derive(serde::Serialize)]
pub struct ResetResult {
    pub success: bool,
    pub errors: Vec<String>,
    pub cleared_items: Vec<String>,
}

#[tauri::command]
pub async fn reset_app_data(app: AppHandle) -> Result<ResetResult, String> {
    log::info!("Starting app data reset");

    let mut errors = Vec::new();
    let mut cleared_items = Vec::new();

    // Use the current bundle identifier so dev vs prod apps
    // clear their own OS-level data independently.
    let app_identifier = app.config().identifier.clone();

    // 1. Clear all stores and delete the store files
    // Clear settings store
    if let Ok(store) = app.store("settings") {
        store.clear();
        if let Err(e) = store.save() {
            errors.push(format!("Failed to save cleared settings store: {}", e));
        } else {
            cleared_items.push("Settings store".to_string());
        }
    }

    // Clear transcriptions store
    if let Ok(store) = app.store("transcriptions") {
        store.clear();
        if let Err(e) = store.save() {
            errors.push(format!(
                "Failed to save cleared transcriptions store: {}",
                e
            ));
        } else {
            cleared_items.push("Transcriptions store".to_string());
        }
    }

    // Delete user-data directories only; secure.dat is a sibling file.
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        for (path, item) in app_data_directories_to_clear(&app_data_dir) {
            if path.exists() {
                if let Err(e) = fs::remove_dir_all(&path) {
                    errors.push(format!("Failed to delete {}: {}", item, e));
                } else {
                    cleared_items.push(item.to_string());
                }
            }
        }
    }

    // 2. Clear cache data (license validation cache)
    if let Ok(cache_dir) = app.path().cache_dir() {
        if cache_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&cache_dir) {
                errors.push(format!("Failed to clear cache: {}", e));
            } else {
                cleared_items.push("Cache directory".to_string());
            }
        }
    }

    // 3. Clear app preferences
    #[cfg(target_os = "macos")]
    {
        // Clear FluidAudio cached models (for Swift Parakeet sidecar)
        if let Ok(home_dir) = app.path().home_dir() {
            let fluid_audio_paths = vec![
                home_dir.join("Library/Application Support/FluidAudio"),
                home_dir.join("Library/Caches/FluidAudio"),
            ];

            for fluid_path in fluid_audio_paths {
                if fluid_path.exists() {
                    if let Err(e) = fs::remove_dir_all(&fluid_path) {
                        errors.push(format!("Failed to delete FluidAudio cache: {}", e));
                    } else {
                        cleared_items.push("FluidAudio model cache".to_string());
                    }
                }
            }
        }

        // macOS defaults system
        match std::process::Command::new("defaults")
            .arg("delete")
            .arg(&app_identifier)
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    cleared_items.push("System preferences".to_string());
                }
            }
            Err(_) => {
                // No defaults to clear is not an error
            }
        }

        // Also remove the preferences plist file
        if let Ok(home_dir) = app.path().home_dir() {
            let prefs_path = home_dir
                .join("Library")
                .join("Preferences")
                .join(format!("{}.plist", app_identifier));
            if prefs_path.exists() {
                if let Err(e) = fs::remove_file(&prefs_path) {
                    errors.push(format!("Failed to remove preferences file: {}", e));
                } else {
                    cleared_items.push("Preferences plist".to_string());
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Windows Registry cleanup
        match std::process::Command::new("reg")
            .args([
                "delete",
                &format!("HKCU\\\\Software\\\\{}", app_identifier),
                "/f",
            ])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    cleared_items.push("Registry settings".to_string());
                }
            }
            Err(_) => {
                // Registry key might not exist
            }
        }
    }

    // 4. Clear additional system data
    #[cfg(target_os = "macos")]
    {
        if let Ok(home_dir) = app.path().home_dir() {
            // Clear saved application state (window positions, etc)
            let saved_state_path = home_dir
                .join("Library")
                .join("Saved Application State")
                .join(format!("{}.savedState", app_identifier));
            if saved_state_path.exists() {
                if let Err(e) = fs::remove_dir_all(&saved_state_path) {
                    errors.push(format!("Failed to clear saved state: {}", e));
                } else {
                    cleared_items.push("Window state".to_string());
                }
            }

            // Clear any logs
            let logs_path = home_dir.join("Library").join("Logs").join(&app_identifier);
            if logs_path.exists() {
                if let Err(e) = fs::remove_dir_all(&logs_path) {
                    errors.push(format!("Failed to clear logs: {}", e));
                } else {
                    cleared_items.push("Application logs".to_string());
                }
            }

            // Clear WebKit data if any
            let webkit_path = home_dir
                .join("Library")
                .join("WebKit")
                .join(&app_identifier);
            if webkit_path.exists() {
                if let Err(e) = fs::remove_dir_all(&webkit_path) {
                    errors.push(format!("Failed to clear WebKit data: {}", e));
                } else {
                    cleared_items.push("WebKit data".to_string());
                }
            }

            // Clear NSURLSession downloads cache
            let nsurlsession_path = home_dir
                .join("Library")
                .join("Caches")
                .join("com.apple.nsurlsessiond")
                .join("Downloads")
                .join(&app_identifier);
            if nsurlsession_path.exists() {
                if let Err(e) = fs::remove_dir_all(&nsurlsession_path) {
                    errors.push(format!("Failed to clear download cache: {}", e));
                } else {
                    cleared_items.push("Download cache".to_string());
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Clear Windows app data
        if let Ok(local_data_dir) = app.path().app_local_data_dir() {
            // Clear logs from AppData\Local
            let logs_path = local_data_dir.join("logs");
            if logs_path.exists() {
                if let Err(e) = fs::remove_dir_all(&logs_path) {
                    errors.push(format!("Failed to clear logs: {}", e));
                } else {
                    cleared_items.push("Application logs".to_string());
                }
            }
        }

        // Clear Windows WebView2 cache
        if let Ok(temp_dir) = app.path().temp_dir() {
            let webview_cache = temp_dir.join(format!("{}.WebView2", app_identifier));
            if webview_cache.exists() {
                if let Err(e) = fs::remove_dir_all(&webview_cache) {
                    errors.push(format!("Failed to clear WebView2 cache: {}", e));
                } else {
                    cleared_items.push("WebView2 cache".to_string());
                }
            }
        }
    }

    // 5. Reset system permissions
    #[cfg(target_os = "macos")]
    {
        let reset_script = format!(
            "do shell script \"tccutil reset All {}\" with administrator privileges",
            app_identifier
        );

        match tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(reset_script)
            .output()
            .await
        {
            Ok(output) => {
                if output.status.success() {
                    cleared_items.push("System permissions".to_string());
                } else {
                    // User might have cancelled - not a critical error
                    log::info!("User cancelled permission reset");
                }
            }
            Err(e) => {
                errors.push(format!("Could not reset permissions: {}", e));
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Windows doesn't have centralized permissions like macOS
        cleared_items.push("System permissions (N/A on Windows)".to_string());
    }

    // 6. Clear any runtime state
    use tauri::async_runtime::RwLock as AsyncRwLock;
    let whisper_state = app.state::<AsyncRwLock<crate::whisper::manager::WhisperManager>>();
    let mut whisper_manager = whisper_state.write().await;
    whisper_manager.clear_all();
    drop(whisper_manager);
    cleared_items.push("Runtime state".to_string());

    // 6.5. Clear in-memory API key cache; persisted keys stay in secure storage.
    if let Err(e) = crate::commands::ai::clear_all_api_key_cache() {
        errors.push(format!("Failed to clear API key cache: {}", e));
    } else {
        cleared_items.push("AI API key cache".to_string());
    }

    // 7. Refresh preferences daemon
    #[cfg(target_os = "macos")]
    {
        match std::process::Command::new("killall")
            .arg("cfprefsd")
            .output()
        {
            Ok(_) => {
                log::info!("Refreshed cfprefsd");
            }
            Err(_) => {
                // Not critical
            }
        }
    }

    // 8. Emit reset event to frontend
    if let Err(e) = app.emit("app-reset", ()) {
        errors.push(format!("Failed to emit reset event: {}", e));
    }

    let success = errors.is_empty();

    if success {
        log::info!("App data reset completed successfully");
    } else {
        log::warn!("App data reset completed with {} errors", errors.len());
    }

    Ok(ResetResult {
        success,
        errors,
        cleared_items,
    })
}

#[cfg(test)]
mod tests {
    use super::app_data_directories_to_clear;
    use std::path::Path;

    #[test]
    fn reset_deletion_plan_preserves_secure_storage_and_license() {
        let app_data_dir = Path::new("/tmp/voicetypr-reset-test");
        let deletions = app_data_directories_to_clear(app_data_dir);

        assert_eq!(deletions.len(), 5);
        for (path, item) in deletions {
            assert!(path.starts_with(app_data_dir));
            assert_ne!(path, app_data_dir);
            assert!(!app_data_dir.join("secure.dat").starts_with(&path));
            assert!(!item.to_ascii_lowercase().contains("license"));
            assert!(!item.to_ascii_lowercase().contains("secure"));
            assert!(!item.to_ascii_lowercase().contains("api key"));
        }
    }
}
