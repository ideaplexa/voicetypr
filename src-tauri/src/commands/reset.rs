use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::utils::reset_directory;

// secure.dat is a sibling of these directories and must survive reset.
fn app_data_directories_to_clear(app_data_dir: &Path) -> [(PathBuf, &'static str); 4] {
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
        // Audio is cleared by storage::clear_audio, which protects live writers.
    ]
}

#[cfg(any(target_os = "macos", test))]
fn macos_directories_to_clear(home: &Path, identifier: &str) -> [(PathBuf, &'static str); 3] {
    [
        (
            home.join("Library/Logs").join(identifier),
            "Application logs",
        ),
        (home.join("Library/WebKit").join(identifier), "WebKit data"),
        (
            home.join("Library/Caches/com.apple.nsurlsessiond/Downloads")
                .join(identifier),
            "Download cache",
        ),
    ]
}

#[cfg(any(target_os = "windows", test))]
fn windows_directories_to_clear(local_data: &Path) -> [(PathBuf, &'static str); 1] {
    [(local_data.join("logs"), "Application logs")]
}

fn clear_directory(
    path: &Path,
    item: &str,
    identifier: &str,
    base: &Path,
    errors: &mut Vec<String>,
    cleared_items: &mut Vec<String>,
) {
    match reset_directory::remove(path, identifier, base) {
        Ok(true) => cleared_items.push(item.to_string()),
        Ok(false) => {}
        Err(error) => errors.push(format!("Failed to clear {item}: {error}")),
    }
}

// No interactive elevation prompt: resetting this user's app permissions does
// not need administrator privileges. Bound OS commands and kill on timeout.
#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn reset_system_command(program: &str, args: &[&str]) -> Result<bool, String> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    tokio::time::timeout(Duration::from_secs(10), command.status())
        .await
        .map_err(|_| "System reset command timed out".to_string())?
        .map(|status| status.success())
        .map_err(|_| "Could not run system reset command".to_string())
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
    let identifier = app.config().identifier.clone();
    #[cfg(target_os = "macos")]
    let home = app
        .path()
        .home_dir()
        .map_err(|_| "Home directory unavailable")?;
    let data = app
        .path()
        .app_data_dir()
        .map_err(|_| "App storage unavailable")?;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|_| "App cache unavailable")?;
    let data_base = data
        .parent()
        .ok_or("App storage unavailable")?
        .to_path_buf();
    let cache_base = cache.parent().ok_or("App cache unavailable")?.to_path_buf();
    let mut roots = vec![data.clone(), cache.clone()];
    let mut directories: Vec<_> = app_data_directories_to_clear(&data)
        .into_iter()
        .map(|(path, item)| (path, item, data_base.clone()))
        .collect();
    directories.push((cache, "Cache directory", cache_base));
    #[cfg(target_os = "macos")]
    for (path, item) in macos_directories_to_clear(&home, &identifier) {
        let base = path
            .parent()
            .ok_or("App storage unavailable")?
            .to_path_buf();
        roots.push(path.clone());
        directories.push((path, item, base));
    }
    #[cfg(target_os = "windows")]
    let shared_roles = {
        let local = app
            .path()
            .app_local_data_dir()
            .map_err(|_| "App local storage unavailable")?;
        let base = local.parent().ok_or("App local storage unavailable")?;
        for (path, item) in windows_directories_to_clear(&local) {
            directories.push((path, item, base.to_path_buf()));
        }
        roots.push(local);
        // Tauri/dirs: cache_dir and data_local_dir both use LocalAppData;
        // app_data_dir uses RoamingAppData. Only cache/local may be identical.
        vec![(1, roots.len() - 1)]
    };
    #[cfg(not(target_os = "windows"))]
    let shared_roles = Vec::new();
    // Validate the data owner AND every role's canonical footprint before
    // clearing ANY stores/audio. Trusted base aliases are allowed, but a cache
    // base aliased into data must not cause secure.dat to be deleted.
    let check_identifier = identifier.clone();
    tauri::async_runtime::spawn_blocking(move || {
        reset_directory::validate_roles(&roots, &check_identifier, &shared_roles)
    })
    .await
    .map_err(|_| "Could not validate app storage")??;

    let mut errors = Vec::new();
    let mut cleared_items = Vec::new();
    match crate::recording::storage::clear_audio(&app).await {
        Ok(()) => cleared_items.push("Saved and temporary audio".to_string()),
        Err(error) => errors.push(error),
    }

    for (name, item) in [
        ("settings", "Settings store"),
        ("transcriptions", "Transcriptions store"),
    ] {
        match app.store(name) {
            Ok(store) => {
                store.clear();
                if name == "transcriptions" {
                    crate::commands::history_index::note_reset(&app);
                }
                if store.save().is_ok() {
                    cleared_items.push(item.to_string());
                } else {
                    errors.push(format!("Failed to save cleared {item}"));
                }
            }
            Err(_) => errors.push(format!("Failed to open {item}")),
        }
    }

    // FluidAudio's shared model caches are NOT Voicetypr-owned: the sidecar
    // uses Library/Application Support/FluidAudio/Models and Caches/FluidAudio
    // without an app-specific subfolder. Preserve them (and other apps' models).
    // Likewise preserve legacy <identifier>.savedState / <identifier>.WebView2
    // sibling directories: no recursive deletion outside the exact namespace.
    // Filesystem work is off the async executor.
    let delete_identifier = identifier.clone();
    let (delete_errors, deleted) = tauri::async_runtime::spawn_blocking(move || {
        let mut errors = Vec::new();
        let mut deleted = Vec::new();
        for (path, item, base) in directories {
            clear_directory(
                &path,
                item,
                &delete_identifier,
                &base,
                &mut errors,
                &mut deleted,
            );
        }
        (errors, deleted)
    })
    .await
    .map_err(|_| "Could not finish app storage reset")?;
    errors.extend(delete_errors);
    cleared_items.extend(deleted);

    #[cfg(target_os = "macos")]
    {
        // `defaults delete` updates cfprefsd itself; do not kill the user's
        // preferences daemon for every app.
        match reset_system_command("/usr/bin/defaults", &["delete", &identifier]).await {
            Ok(true) => cleared_items.push("System preferences".to_string()),
            Ok(false) => {} // No defaults domain is normal.
            Err(error) => errors.push(error),
        }
        match reset_system_command("/usr/bin/tccutil", &["reset", "All", &identifier]).await {
            Ok(true) => cleared_items.push("System permissions".to_string()),
            Ok(false) => errors.push("Could not reset system permissions".to_string()),
            Err(error) => errors.push(error),
        }
    }
    #[cfg(target_os = "windows")]
    {
        let key = format!("HKCU\\Software\\{identifier}");
        match reset_system_command("reg", &["delete", &key, "/f"]).await {
            Ok(true) => cleared_items.push("Registry settings".to_string()),
            Ok(false) => {} // A missing key is normal.
            Err(error) => errors.push(error),
        }
    }

    use tauri::async_runtime::RwLock as AsyncRwLock;
    let whisper_state = app.state::<AsyncRwLock<crate::whisper::manager::WhisperManager>>();
    match tokio::time::timeout(Duration::from_secs(10), whisper_state.write()).await {
        Ok(mut manager) => {
            manager.clear_all();
            cleared_items.push("Runtime state".to_string());
        }
        Err(_) => errors.push("Runtime state reset timed out".to_string()),
    }

    if crate::commands::ai::clear_all_api_key_cache().is_err() {
        errors.push("Failed to clear API key cache".to_string());
    } else {
        cleared_items.push("AI API key cache".to_string());
    }
    if app.emit("app-reset", ()).is_err() {
        errors.push("Failed to emit reset event".to_string());
    }
    log::info!("App data reset completed with {} errors", errors.len());
    Ok(ResetResult {
        success: errors.is_empty(),
        errors,
        cleared_items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_deletion_plan_preserves_secure_storage_and_shared_caches() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        for identifier in ["com.ideaplexa.voicetypr", "com.ideaplexa.voicetypr.dev"] {
            let data = home.join("Library/Application Support").join(identifier);
            let mut deletions = app_data_directories_to_clear(&data).to_vec();
            assert_eq!(deletions.len(), 4);
            deletions.push((
                home.join("Library/Caches").join(identifier),
                "Cache directory",
            ));
            deletions.extend(macos_directories_to_clear(home, identifier));
            let local = home.join("AppData/Local").join(identifier);
            deletions.extend(windows_directories_to_clear(&local));
            for (path, _) in deletions {
                let root = path
                    .ancestors()
                    .find(|p| p.file_name().is_some_and(|name| name == identifier))
                    .unwrap();
                assert!(
                    reset_directory::validate(&path, identifier, root.parent().unwrap()).is_ok()
                );
                for preserved in [
                    data.join("secure.dat"),
                    data.join("recordings"),
                    home.join("Library/Application Support/FluidAudio"),
                    home.join("Library/Caches/FluidAudio"),
                    home.join("Library/Caches/other-app"),
                    home.join("AppData/Local/other-app"),
                ] {
                    assert!(!preserved.starts_with(&path));
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn system_reset_commands_have_no_interactive_stdin() {
        assert!(reset_system_command("/usr/bin/true", &[]).await.unwrap());
        assert!(!reset_system_command("/usr/bin/false", &[]).await.unwrap());
        assert_eq!(
            reset_system_command("/bin/sleep", &["30"]).await,
            Err("System reset command timed out".into())
        );
    }
}
