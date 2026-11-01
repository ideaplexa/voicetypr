use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::recording::storage::AudioResetPlan;
use crate::utils::reset_directory;

fn preflight_reset(
    roots: &[PathBuf],
    identifier: &str,
    shared_roles: &[(usize, usize)],
    directories: &[(PathBuf, &'static str, PathBuf)],
    audio: &AudioResetPlan,
) -> Result<(), &'static str> {
    reset_directory::validate_roles(roots, identifier, shared_roles)?;
    for (path, _, base) in directories {
        reset_directory::validate(path, identifier, base)?;
    }
    audio.preflight(identifier)
}

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
    let audio = AudioResetPlan::new(
        data,
        app.path()
            .temp_dir()
            .map_err(|_| "Audio temporary storage unavailable")?
            .join("kept-dictation"),
        std::env::temp_dir(),
    );
    // Validate every deletion target, audio root and role footprint before ANY
    // mutation (including cancellation of active audio). Keep the checked plan
    // rather than resolving storage paths again after clearing has begun.
    let check_identifier = identifier.clone();
    let (directories, audio) = tauri::async_runtime::spawn_blocking(move || {
        preflight_reset(
            &roots,
            &check_identifier,
            &shared_roles,
            &directories,
            &audio,
        )?;
        Ok::<_, &'static str>((directories, audio))
    })
    .await
    .map_err(|_| "Could not validate app storage")??;

    let mut errors = Vec::new();
    let mut cleared_items = Vec::new();
    match crate::recording::storage::clear_audio(&app, audio).await {
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

    #[cfg(unix)]
    fn assert_preflight_preserves_stores_and_audio(invalid: &str, alias: bool) {
        let temp = tempfile::tempdir().unwrap();
        let identifier = "com.ideaplexa.voicetypr.dev";
        let base = temp.path().join("data");
        let data = base.join(identifier);
        let cache = temp.path().join("cache").join(identifier);
        let os_temp = temp.path().join("temp");
        let external = temp.path().join("other-app");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&os_temp).unwrap();
        std::fs::create_dir_all(&external).unwrap();
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let stores: Vec<_> = ["settings", "transcriptions"]
            .into_iter()
            .map(|name| {
                let path = data.join("stores").join(name);
                let store = tauri_plugin_store::StoreBuilder::new(app.handle(), &path)
                    .build()
                    .unwrap();
                store.set("keep", serde_json::json!(name));
                store.save().unwrap();
                let original = std::fs::read(&path).unwrap();
                (store, path, original, name)
            })
            .collect();
        let audio_files = [
            data.join(if invalid == "recordings" {
                "recovery/saved.wav"
            } else {
                "recordings/saved.wav"
            }),
            data.join(if invalid == "captures" {
                "recovery/recording.wav"
            } else {
                "captures/live/recording.wav"
            }),
            external.join("recording.wav"),
            os_temp.join("kept-dictation/.tmp-recovery.wav"),
            os_temp.join(format!("voicetypr-audio-{}.wav", uuid::Uuid::new_v4())),
        ];
        for path in &audio_files {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"keep audio").unwrap();
        }
        let invalid = if invalid == "legacy" {
            let path = os_temp.join("kept-dictation");
            std::fs::remove_file(&audio_files[3]).unwrap();
            std::fs::remove_dir(&path).unwrap();
            path
        } else {
            data.join(invalid)
        };
        std::fs::create_dir_all(invalid.parent().unwrap()).unwrap();
        if alias {
            std::os::unix::fs::symlink(&external, &invalid).unwrap();
        } else {
            std::fs::write(&invalid, b"not a directory").unwrap();
        }
        let mut directories: Vec<_> = app_data_directories_to_clear(&data)
            .into_iter()
            .map(|(path, item)| (path, item, base.clone()))
            .collect();
        directories.push((
            cache.clone(),
            "Cache directory",
            cache.parent().unwrap().into(),
        ));
        let audio = AudioResetPlan::new(
            data.clone(),
            os_temp.join("kept-dictation"),
            os_temp.clone(),
        );
        let result = preflight_reset(&[data, cache], identifier, &[], &directories, &audio);
        assert!(result.is_err());
        for (store, path, original, name) in stores {
            assert_eq!(store.get("keep"), Some(serde_json::json!(name)));
            assert_eq!(std::fs::read(path).unwrap(), original);
        }
        for path in audio_files {
            // Legacy fixture was replaced by the invalid root itself.
            if path == os_temp.join("kept-dictation/.tmp-recovery.wav")
                && invalid == os_temp.join("kept-dictation")
            {
                continue;
            }
            assert_eq!(std::fs::read(path).unwrap(), b"keep audio");
        }
    }

    #[cfg(unix)]
    #[test]
    fn reset_preflight_rejects_symlinked_models_without_mutating_stores_or_audio() {
        assert_preflight_preserves_stores_and_audio("models", true);
    }

    #[cfg(unix)]
    #[test]
    fn reset_preflight_rejects_symlinked_audio_without_mutating_stores_or_audio() {
        for path in [
            "recordings",
            "recovery",
            "captures",
            "cli-recordings",
            "captures/abandoned",
            "legacy",
        ] {
            assert_preflight_preserves_stores_and_audio(path, true);
        }
    }

    #[cfg(unix)]
    #[test]
    fn reset_preflight_rejects_unresolvable_targets_without_mutating_stores_or_audio() {
        assert_preflight_preserves_stores_and_audio("models", false);
        assert_preflight_preserves_stores_and_audio("recovery", false);
    }

    #[test]
    fn reset_preflight_skips_missing_targets() {
        let temp = tempfile::tempdir().unwrap();
        let identifier = "com.ideaplexa.voicetypr.dev";
        let data = temp.path().join("data").join(identifier);
        let directories: Vec<_> = app_data_directories_to_clear(&data)
            .into_iter()
            .map(|(path, item)| (path, item, data.parent().unwrap().into()))
            .collect();
        let os_temp = temp.path().join("temp");
        let audio = AudioResetPlan::new(data.clone(), os_temp.join("kept-dictation"), os_temp);
        assert!(preflight_reset(&[data], identifier, &[], &directories, &audio).is_ok());
    }

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
