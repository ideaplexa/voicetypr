use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

#[derive(serde::Serialize)]
pub struct ResetResult {
    pub success: bool,
    pub errors: Vec<String>,
    pub cleared_items: Vec<String>,
}

// Permit the identifier directory itself and its children, never a user-wide
// root or its ancestors. Check resolved paths too, so symlinks cannot escape.
fn guard_reset_directory(
    path: &Path,
    identifier: &str,
    protected_roots: &[PathBuf],
) -> io::Result<()> {
    let refused = || io::Error::new(io::ErrorKind::PermissionDenied, "Unsafe reset directory");
    if identifier.is_empty()
        || Path::new(identifier).components().count() != 1
        || !matches!(
            Path::new(identifier).components().next(),
            Some(Component::Normal(_))
        )
        || !path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir))
        || path.parent().is_none()
    {
        return Err(refused());
    }
    let scope = path
        .ancestors()
        .find(|p| p.file_name() == Some(std::ffi::OsStr::new(identifier)))
        .ok_or_else(refused)?;
    let resolved = path.canonicalize()?;
    let resolved_scope = scope.canonicalize()?;
    if resolved_scope.file_name() != Some(std::ffi::OsStr::new(identifier))
        || !resolved.starts_with(&resolved_scope)
        || resolved.parent().is_none()
    {
        return Err(refused());
    }
    for root in protected_roots {
        if root.starts_with(path) {
            return Err(refused());
        }
        match root.canonicalize() {
            Ok(root) if root.starts_with(&resolved) => return Err(refused()),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(refused()),
        }
    }
    Ok(())
}

fn remove_reset_directory(
    path: &Path,
    identifier: &str,
    protected_roots: &[PathBuf],
) -> io::Result<()> {
    if guard_reset_directory(path, identifier, protected_roots).is_err() {
        log::warn!("Refused unsafe reset directory deletion");
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Unsafe reset directory",
        ));
    }
    fs::remove_dir_all(path)
}

#[tauri::command]
pub async fn reset_app_data(app: AppHandle) -> Result<ResetResult, String> {
    log::info!("Starting app data reset");

    let mut errors = Vec::new();
    let mut cleared_items = Vec::new();

    // Use the current bundle identifier so dev vs prod apps
    // clear their own OS-level data independently.
    let app_identifier = app.config().identifier.clone();
    // Resolve every safety boundary before clearing anything; fail closed if
    // the platform cannot tell us where its user-wide directories are.
    let protected_roots = [
        app.path().home_dir(),
        app.path().cache_dir(),
        app.path().data_dir(),
        app.path().config_dir(),
        app.path().local_data_dir(),
    ]
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| "Could not resolve reset safety boundaries".to_string())?;

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

    // Delete the actual store files from disk
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        let stores_dir = app_data_dir.join("stores");
        if stores_dir.exists() {
            if let Err(e) = remove_reset_directory(&stores_dir, &app_identifier, &protected_roots) {
                errors.push(format!("Failed to delete stores directory: {}", e));
            } else {
                cleared_items.push("Stores directory".to_string());
            }
        }
    }

    // 2. Delete app data directories
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        // Delete models directory
        let models_dir = app_data_dir.join("models");
        if models_dir.exists() {
            if let Err(e) = remove_reset_directory(&models_dir, &app_identifier, &protected_roots) {
                errors.push(format!("Failed to delete models directory: {}", e));
            } else {
                cleared_items.push("Downloaded models".to_string());
            }
        }

        // Delete Parakeet model directories (for Swift sidecar)
        // These might exist from old Python implementation or tracking
        let parakeet_dirs = vec![
            app_data_dir.join("parakeet-tdt-0.6b-v3"),
            app_data_dir.join("parakeet-tdt-0.6b-v2"),
        ];
        for parakeet_dir in parakeet_dirs {
            if parakeet_dir.exists() {
                if let Err(e) =
                    remove_reset_directory(&parakeet_dir, &app_identifier, &protected_roots)
                {
                    errors.push(format!("Failed to delete Parakeet directory: {}", e));
                } else {
                    cleared_items.push("Parakeet model data".to_string());
                }
            }
        }

        // Delete recordings directory
        let recordings_dir = app_data_dir.join("recordings");
        if recordings_dir.exists() {
            if let Err(e) =
                remove_reset_directory(&recordings_dir, &app_identifier, &protected_roots)
            {
                errors.push(format!("Failed to delete recordings directory: {}", e));
            } else {
                cleared_items.push("Audio recordings".to_string());
            }
        }
    }

    // 3. Clear license data from secure store
    if let Err(e) = crate::secure_store::secure_delete(&app, "license") {
        // Only push error if it's not a "store doesn't exist" error
        if !e.contains("Store access failed") {
            errors.push(format!("Failed to clear license: {}", e));
        }
    } else {
        cleared_items.push("License data".to_string());
    }

    // 3.5. Clear the secure.dat file itself
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        let secure_store_path = app_data_dir.join("secure.dat");
        if secure_store_path.exists() {
            if let Err(e) = fs::remove_file(&secure_store_path) {
                errors.push(format!("Failed to remove secure storage: {}", e));
            } else {
                cleared_items.push("Secure storage (API keys)".to_string());
            }
        }
    }

    // 4. Clear cache data (license validation cache)
    if let Ok(cache_dir) = app.path().app_cache_dir() {
        if cache_dir.exists() {
            if let Err(e) = remove_reset_directory(&cache_dir, &app_identifier, &protected_roots) {
                errors.push(format!("Failed to clear cache: {}", e));
            } else {
                cleared_items.push("Cache directory".to_string());
            }
        }
    }

    // 5. Clear app preferences
    #[cfg(target_os = "macos")]
    {
        // FluidAudio's model directories are shared with other apps. Preserve
        // them: they are outside this app's identifier-scoped directories.

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
            .args(&[
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

    // 6. Clear additional system data
    #[cfg(target_os = "macos")]
    {
        if let Ok(home_dir) = app.path().home_dir() {
            // Preserve the suffix-named .savedState directory; recursive
            // deletion is restricted to exact identifier directory boundaries.

            // Clear any logs
            let logs_path = home_dir.join("Library").join("Logs").join(&app_identifier);
            if logs_path.exists() {
                if let Err(e) =
                    remove_reset_directory(&logs_path, &app_identifier, &protected_roots)
                {
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
                if let Err(e) =
                    remove_reset_directory(&webkit_path, &app_identifier, &protected_roots)
                {
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
                if let Err(e) =
                    remove_reset_directory(&nsurlsession_path, &app_identifier, &protected_roots)
                {
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
                if let Err(e) =
                    remove_reset_directory(&logs_path, &app_identifier, &protected_roots)
                {
                    errors.push(format!("Failed to clear logs: {}", e));
                } else {
                    cleared_items.push("Application logs".to_string());
                }
            }
        }

        // Preserve the legacy suffix-named temp WebView2 directory, which is
        // outside the exact identifier directory boundary.
    }

    // 7. Reset system permissions
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

    // 8. Clear any runtime state
    use tauri::async_runtime::RwLock as AsyncRwLock;
    let whisper_state = app.state::<AsyncRwLock<crate::whisper::manager::WhisperManager>>();
    let mut whisper_manager = whisper_state.write().await;
    whisper_manager.clear_all();
    drop(whisper_manager);
    cleared_items.push("Runtime state".to_string());

    // 8.5. Clear API key cache
    if let Err(e) = crate::commands::ai::clear_all_api_key_cache() {
        errors.push(format!("Failed to clear API key cache: {}", e));
    } else {
        cleared_items.push("AI API key cache".to_string());
    }

    // 9. Refresh preferences daemon
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

    // 10. Emit reset event to frontend
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
    use super::{guard_reset_directory, remove_reset_directory};
    use std::{fs, path::Path};

    const IDENTIFIER: &str = "com.ideaplexa.voicetypr";

    #[test]
    fn reset_guard_rejects_user_wide_roots_and_filesystem_root() {
        let temp = tempfile::tempdir().unwrap();
        let roots: Vec<_> = [
            "home",
            "home/Library/Caches",
            "home/Library/Application Support",
            "home/.config",
            "home/AppData/Local",
            "home/AppData/Roaming",
        ]
        .iter()
        .map(|p| temp.path().join(p))
        .collect();
        for root in &roots {
            fs::create_dir_all(root).unwrap();
        }
        for root in &roots {
            assert!(guard_reset_directory(root, IDENTIFIER, &roots).is_err());
            // Even a misconfigured identifier matching a root cannot allow it.
            assert!(guard_reset_directory(
                root,
                root.file_name().unwrap().to_str().unwrap(),
                &roots
            )
            .is_err());
        }
        assert!(guard_reset_directory(Path::new("/"), IDENTIFIER, &roots).is_err());
        assert!(guard_reset_directory(temp.path(), IDENTIFIER, &roots).is_err());
    }

    #[test]
    fn reset_guard_accepts_identifier_directory_and_children() {
        let temp = tempfile::tempdir().unwrap();
        let roots = vec![temp.path().to_path_buf()];
        for root in ["Library/Caches", "AppData/Local", "AppData/Roaming"] {
            let app_dir = temp.path().join(root).join(IDENTIFIER);
            let child = app_dir.join("models/nested");
            fs::create_dir_all(&child).unwrap();
            assert!(guard_reset_directory(&app_dir, IDENTIFIER, &roots).is_ok());
            assert!(guard_reset_directory(&child, IDENTIFIER, &roots).is_ok());
        }
    }

    #[test]
    fn reset_guard_rejects_shared_suffixed_relative_and_traversing_paths() {
        let temp = tempfile::tempdir().unwrap();
        let roots = vec![temp.path().to_path_buf()];
        for name in [
            "FluidAudio".to_string(),
            format!("{IDENTIFIER}.savedState"),
            format!("{IDENTIFIER}.WebView2"),
            format!("{IDENTIFIER}-other"),
            IDENTIFIER.to_string(),
        ] {
            fs::create_dir_all(temp.path().join(name)).unwrap();
        }
        for name in [
            "FluidAudio".to_string(),
            format!("{IDENTIFIER}.savedState"),
            format!("{IDENTIFIER}.WebView2"),
            format!("{IDENTIFIER}-other"),
        ] {
            assert!(guard_reset_directory(&temp.path().join(name), IDENTIFIER, &roots).is_err());
        }
        assert!(guard_reset_directory(Path::new(IDENTIFIER), IDENTIFIER, &roots).is_err());
        assert!(guard_reset_directory(
            &temp.path().join(IDENTIFIER).join(".."),
            IDENTIFIER,
            &roots
        )
        .is_err());
        assert!(guard_reset_directory(&temp.path().join(IDENTIFIER), "..", &roots).is_err());
    }

    #[test]
    fn reset_guard_protects_roots_even_below_an_identifier_directory() {
        let temp = tempfile::tempdir().unwrap();
        let app_dir = temp.path().join(IDENTIFIER);
        let home = app_dir.join("home");
        fs::create_dir_all(&home).unwrap();
        assert!(guard_reset_directory(&home, IDENTIFIER, std::slice::from_ref(&home)).is_err());
        assert!(guard_reset_directory(&app_dir, IDENTIFIER, &[home]).is_err());
    }

    #[test]
    fn guarded_removal_preserves_other_apps_data() {
        let temp = tempfile::tempdir().unwrap();
        let cache_root = temp.path().join("AppData/Local");
        let app_dir = cache_root.join(IDENTIFIER);
        let other_app = cache_root.join("Browser/profile");
        fs::create_dir_all(&app_dir).unwrap();
        fs::create_dir_all(&other_app).unwrap();
        let marker = other_app.join("keep");
        fs::write(&marker, b"other app data").unwrap();
        let roots = vec![temp.path().to_path_buf(), cache_root.clone()];

        assert!(remove_reset_directory(&cache_root, IDENTIFIER, &roots).is_err());
        remove_reset_directory(&app_dir, IDENTIFIER, &roots).unwrap();
        assert!(!app_dir.exists());
        assert_eq!(fs::read(marker).unwrap(), b"other app data");
    }

    #[cfg(unix)]
    #[test]
    fn reset_guard_rejects_symlink_escapes() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let app_dir = temp.path().join(IDENTIFIER);
        let other_app = temp.path().join("other-app");
        fs::create_dir_all(&app_dir).unwrap();
        fs::create_dir_all(&other_app).unwrap();
        symlink(&other_app, app_dir.join("escape")).unwrap();
        let roots = vec![temp.path().to_path_buf()];
        assert!(guard_reset_directory(&app_dir.join("escape"), IDENTIFIER, &roots).is_err());
        assert!(remove_reset_directory(&app_dir.join("escape"), IDENTIFIER, &roots).is_err());
        assert!(other_app.exists());

        let alias = temp.path().join("alias");
        fs::create_dir_all(&alias).unwrap();
        symlink(&other_app, alias.join(IDENTIFIER)).unwrap();
        assert!(guard_reset_directory(&alias.join(IDENTIFIER), IDENTIFIER, &roots).is_err());
    }
}
