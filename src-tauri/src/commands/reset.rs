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

// Compare components rather than string prefixes. Windows canonical paths can
// differ only in ASCII casing; keep Unix comparisons byte-for-byte.
fn reset_path_starts_with(path: &Path, base: &Path, ignore_ascii_case: bool) -> bool {
    let mut components = path.components();
    base.components().all(|expected| {
        components.next().is_some_and(|actual| {
            let actual = actual.as_os_str().as_encoded_bytes();
            let expected = expected.as_os_str().as_encoded_bytes();
            if ignore_ascii_case {
                actual.eq_ignore_ascii_case(expected)
            } else {
                actual == expected
            }
        })
    })
}

fn reset_directory_present(path: &Path) -> bool {
    // Unlike exists(), do not hide permission errors or dangling aliases.
    !matches!(fs::symlink_metadata(path), Err(e) if e.kind() == io::ErrorKind::NotFound)
}

// The OS-supplied base is trusted, including aliases at or above it. Only
// <base>/<identifier> and descendants are app-owned deletion boundaries.
fn guard_reset_directory(
    path: &Path,
    base: &Path,
    identifier: &str,
    protected_roots: &[PathBuf],
) -> io::Result<PathBuf> {
    let refused = || io::Error::new(io::ErrorKind::PermissionDenied, "Unsafe reset directory");
    if !matches!(
        identifier,
        "com.ideaplexa.voicetypr" | "com.ideaplexa.voicetypr.dev"
    ) || !path.is_absolute()
        || !base.is_absolute()
        || path
            .components()
            .chain(base.components())
            .any(|c| matches!(c, Component::ParentDir))
        || !reset_path_starts_with(path, &base.join(identifier), cfg!(windows))
    {
        return Err(refused());
    }

    // Resolve the trusted base first, never treating a user-profile junction
    // or macOS /var alias as an alias inside app storage.
    let resolved_base = base.canonicalize()?;
    let scope = resolved_base.join(identifier);
    let relative: PathBuf = path.components().skip(base.components().count()).collect();
    let target = resolved_base.join(relative);
    for ancestor in target.ancestors() {
        if reset_path_starts_with(&resolved_base, ancestor, cfg!(windows)) {
            break;
        }
        let metadata = fs::symlink_metadata(ancestor)?;
        #[cfg(windows)]
        let alias = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
        };
        #[cfg(not(windows))]
        let alias = metadata.is_symlink();
        if alias || !metadata.is_dir() {
            return Err(refused());
        }
    }
    let resolved_scope = scope.canonicalize()?;
    let resolved = target.canonicalize()?;
    if !reset_path_starts_with(&resolved_scope, &scope, cfg!(windows))
        || resolved_scope.components().count() != scope.components().count()
        || !reset_path_starts_with(&resolved, &resolved_scope, cfg!(windows))
        || resolved.parent().is_none()
    {
        return Err(refused());
    }
    for root in protected_roots {
        match root.canonicalize() {
            Ok(root) if reset_path_starts_with(&root, &resolved, cfg!(windows)) => {
                return Err(refused());
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound && !reset_directory_present(root) => {}
            Err(_) => return Err(refused()),
        }
    }
    Ok(resolved)
}

fn guard_reset_app_data(
    path: &Path,
    base: &Path,
    identifier: &str,
    protected_roots: &[PathBuf],
) -> io::Result<PathBuf> {
    // Store APIs and secure_delete mutate files before recursive cleanup runs.
    // Their base must be the exact app root, not merely a child in its scope.
    if path.components().count() != base.components().count() + 1 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Unsafe app data directory",
        ));
    }
    guard_reset_directory(path, base, identifier, protected_roots)
}

fn guard_reset_cache(
    path: &Path,
    base: &Path,
    identifier: &str,
    protected_roots: &[PathBuf],
    resolved_data: &Path,
) -> io::Result<()> {
    if !reset_directory_present(path) {
        return Ok(());
    }
    let resolved = guard_reset_directory(path, base, identifier, protected_roots)?;
    if reset_path_starts_with(&resolved, resolved_data, cfg!(windows))
        || reset_path_starts_with(resolved_data, &resolved, cfg!(windows))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Cache overlaps app data",
        ));
    }
    Ok(())
}

fn remove_reset_directory(
    path: &Path,
    base: &Path,
    identifier: &str,
    protected_roots: &[PathBuf],
) -> io::Result<()> {
    if !reset_directory_present(path) {
        return Ok(());
    }
    if guard_reset_directory(path, base, identifier, protected_roots).is_err() {
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
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "Could not resolve app storage".to_string())?;
    let data_base = app_data_dir
        .parent()
        .ok_or_else(|| "Could not resolve app storage base".to_string())?;
    let resolved_data =
        guard_reset_app_data(&app_data_dir, data_base, &app_identifier, &protected_roots)
            .map_err(|_| "Refusing reset outside app-owned storage".to_string())?;
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|_| "Could not resolve app cache".to_string())?;
    let cache_base = cache_dir
        .parent()
        .ok_or_else(|| "Could not resolve app cache base".to_string())?;
    guard_reset_cache(
        &cache_dir,
        cache_base,
        &app_identifier,
        &protected_roots,
        &resolved_data,
    )
    .map_err(|_| "Refusing reset with unsafe app cache".to_string())?;

    #[cfg(windows)]
    let local_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "Could not resolve local app storage".to_string())?;
    #[cfg(windows)]
    let local_data_base = local_data_dir
        .parent()
        .ok_or_else(|| "Could not resolve local app storage base".to_string())?;

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
    {
        let stores_dir = app_data_dir.join("stores");
        if reset_directory_present(&stores_dir) {
            if let Err(e) =
                remove_reset_directory(&stores_dir, data_base, &app_identifier, &protected_roots)
            {
                errors.push(format!("Failed to delete stores directory: {}", e));
            } else {
                cleared_items.push("Stores directory".to_string());
            }
        }
    }

    // 2. Delete app data directories
    {
        // Delete models directory
        let models_dir = app_data_dir.join("models");
        if reset_directory_present(&models_dir) {
            if let Err(e) =
                remove_reset_directory(&models_dir, data_base, &app_identifier, &protected_roots)
            {
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
            if reset_directory_present(&parakeet_dir) {
                if let Err(e) = remove_reset_directory(
                    &parakeet_dir,
                    data_base,
                    &app_identifier,
                    &protected_roots,
                ) {
                    errors.push(format!("Failed to delete Parakeet directory: {}", e));
                } else {
                    cleared_items.push("Parakeet model data".to_string());
                }
            }
        }

        // Delete recordings directory
        let recordings_dir = app_data_dir.join("recordings");
        if reset_directory_present(&recordings_dir) {
            if let Err(e) = remove_reset_directory(
                &recordings_dir,
                data_base,
                &app_identifier,
                &protected_roots,
            ) {
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
    {
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
    if reset_directory_present(&cache_dir) {
        // Recheck overlap immediately before deletion as well as before stores
        // are mutated. A cache alias must never remove the data/secure store.
        let clear_cache = guard_reset_cache(
            &cache_dir,
            cache_base,
            &app_identifier,
            &protected_roots,
            &app_data_dir
                .canonicalize()
                .map_err(|_| "Could not resolve app storage")?,
        )
        .and_then(|()| {
            remove_reset_directory(&cache_dir, cache_base, &app_identifier, &protected_roots)
        });
        if let Err(e) = clear_cache {
            errors.push(format!("Failed to clear cache: {}", e));
        } else {
            cleared_items.push("Cache directory".to_string());
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
            let logs_base = home_dir.join("Library").join("Logs");
            let logs_path = logs_base.join(&app_identifier);
            if reset_directory_present(&logs_path) {
                if let Err(e) = remove_reset_directory(
                    &logs_path,
                    &logs_base,
                    &app_identifier,
                    &protected_roots,
                ) {
                    errors.push(format!("Failed to clear logs: {}", e));
                } else {
                    cleared_items.push("Application logs".to_string());
                }
            }

            // Clear WebKit data if any
            let webkit_base = home_dir.join("Library").join("WebKit");
            let webkit_path = webkit_base.join(&app_identifier);
            if reset_directory_present(&webkit_path) {
                if let Err(e) = remove_reset_directory(
                    &webkit_path,
                    &webkit_base,
                    &app_identifier,
                    &protected_roots,
                ) {
                    errors.push(format!("Failed to clear WebKit data: {}", e));
                } else {
                    cleared_items.push("WebKit data".to_string());
                }
            }

            // Clear NSURLSession downloads cache
            let nsurlsession_base = home_dir
                .join("Library")
                .join("Caches")
                .join("com.apple.nsurlsessiond")
                .join("Downloads");
            let nsurlsession_path = nsurlsession_base.join(&app_identifier);
            if reset_directory_present(&nsurlsession_path) {
                if let Err(e) = remove_reset_directory(
                    &nsurlsession_path,
                    &nsurlsession_base,
                    &app_identifier,
                    &protected_roots,
                ) {
                    errors.push(format!("Failed to clear download cache: {}", e));
                } else {
                    cleared_items.push("Download cache".to_string());
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Clear logs from AppData\Local, trusting the OS-provided local base.
        let logs_path = local_data_dir.join("logs");
        if reset_directory_present(&logs_path) {
            if let Err(e) = remove_reset_directory(
                &logs_path,
                local_data_base,
                &app_identifier,
                &protected_roots,
            ) {
                errors.push(format!("Failed to clear logs: {}", e));
            } else {
                cleared_items.push("Application logs".to_string());
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
    use super::{
        guard_reset_app_data, guard_reset_cache, guard_reset_directory, remove_reset_directory,
        reset_directory_present, reset_path_starts_with,
    };
    use std::{fs, path::Path};

    const IDENTIFIER: &str = "com.ideaplexa.voicetypr";

    #[test]
    fn reset_path_comparison_respects_component_boundaries_and_case_mode() {
        let path = Path::new("/Storage/App/Models");
        assert!(reset_path_starts_with(
            path,
            Path::new("/Storage/App"),
            false
        ));
        assert!(!reset_path_starts_with(
            path,
            Path::new("/storage/app"),
            false
        ));
        assert!(reset_path_starts_with(
            path,
            Path::new("/storage/app"),
            true
        ));
        assert!(!reset_path_starts_with(
            path,
            Path::new("/storage/ap"),
            true
        ));
        assert!(!reset_path_starts_with(
            Path::new("/Storage/App"),
            path,
            true
        ));
        // Only ASCII casing is folded, even on Windows.
        assert!(!reset_path_starts_with(
            Path::new("/Äpp"),
            Path::new("/äpp"),
            true
        ));
    }

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
            assert!(guard_reset_directory(root, root, IDENTIFIER, &roots).is_err());
            assert!(guard_reset_directory(
                root,
                root.parent().unwrap(),
                root.file_name().unwrap().to_str().unwrap(),
                &roots,
            )
            .is_err());
        }
        assert!(guard_reset_directory(Path::new("/"), temp.path(), IDENTIFIER, &roots).is_err());
        assert!(guard_reset_directory(temp.path(), temp.path(), IDENTIFIER, &roots).is_err());
    }

    #[test]
    fn reset_guard_accepts_identifier_directory_and_children() {
        let temp = tempfile::tempdir().unwrap();
        let roots = vec![temp.path().to_path_buf()];
        for root in ["Library/Caches", "AppData/Local", "AppData/Roaming"] {
            let base = temp.path().join(root);
            let app_dir = base.join(IDENTIFIER);
            let child = app_dir.join("models/nested");
            fs::create_dir_all(&child).unwrap();
            assert!(guard_reset_directory(&app_dir, &base, IDENTIFIER, &roots).is_ok());
            assert!(guard_reset_directory(&child, &base, IDENTIFIER, &roots).is_ok());
            assert!(guard_reset_app_data(&app_dir, &base, IDENTIFIER, &roots).is_ok());
            assert!(guard_reset_app_data(&child, &base, IDENTIFIER, &roots).is_err());
        }
    }

    #[test]
    fn reset_guard_rejects_shared_suffixed_relative_and_traversing_paths() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        for name in [
            "FluidAudio".to_string(),
            format!("{IDENTIFIER}.savedState"),
            format!("{IDENTIFIER}.WebView2"),
            format!("{IDENTIFIER}-other"),
            format!("nested/{IDENTIFIER}"),
        ] {
            let path = base.join(name);
            fs::create_dir_all(&path).unwrap();
            assert!(guard_reset_directory(&path, base, IDENTIFIER, &[]).is_err());
        }
        fs::create_dir_all(base.join(IDENTIFIER)).unwrap();
        assert!(guard_reset_directory(Path::new(IDENTIFIER), base, IDENTIFIER, &[]).is_err());
        assert!(
            guard_reset_directory(&base.join(IDENTIFIER).join(".."), base, IDENTIFIER, &[])
                .is_err()
        );
        assert!(guard_reset_directory(&base.join(IDENTIFIER), base, "..", &[]).is_err());
        assert!(guard_reset_directory(
            &base.join(IDENTIFIER),
            &base.join(IDENTIFIER).join(".."),
            IDENTIFIER,
            &[],
        )
        .is_err());
    }

    #[test]
    fn reset_guard_protects_roots_even_below_an_identifier_directory() {
        let temp = tempfile::tempdir().unwrap();
        let app_dir = temp.path().join(IDENTIFIER);
        let home = app_dir.join("home");
        fs::create_dir_all(&home).unwrap();
        assert!(
            guard_reset_directory(&home, temp.path(), IDENTIFIER, std::slice::from_ref(&home),)
                .is_err()
        );
        assert!(guard_reset_directory(&app_dir, temp.path(), IDENTIFIER, &[home]).is_err());
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

        assert!(remove_reset_directory(&cache_root, &cache_root, IDENTIFIER, &roots).is_err());
        remove_reset_directory(&app_dir, &cache_root, IDENTIFIER, &roots).unwrap();
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
        for path in [app_dir.join("escape"), app_dir.join("escape/child")] {
            fs::create_dir_all(other_app.join("child")).unwrap();
            assert!(guard_reset_directory(&path, temp.path(), IDENTIFIER, &[]).is_err());
            assert!(remove_reset_directory(&path, temp.path(), IDENTIFIER, &[]).is_err());
        }
        assert!(other_app.exists());
        let internal = app_dir.join("models");
        fs::create_dir_all(&internal).unwrap();
        let alias = app_dir.join("internal-alias");
        symlink(&internal, &alias).unwrap();
        assert!(guard_reset_directory(&alias, temp.path(), IDENTIFIER, &[]).is_err());
    }

    #[test]
    fn rejected_app_storage_preserves_stores_and_secrets() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        for relative in [
            "other-app".to_string(),
            format!("{IDENTIFIER}/stores"),
            "com.ideaplexa.voicetypr.dev".to_string(),
        ] {
            let data = home.join(relative);
            fs::create_dir_all(&data).unwrap();
            for name in ["settings", "transcriptions", "secure.dat"] {
                fs::write(data.join(name), b"must stay untouched").unwrap();
            }
            let reset = || -> std::io::Result<()> {
                guard_reset_app_data(&data, home, IDENTIFIER, &[home.to_path_buf()])?;
                fs::write(data.join("settings"), b"cleared")?;
                fs::write(data.join("transcriptions"), b"cleared")?;
                fs::remove_file(data.join("secure.dat"))
            };
            assert!(reset().is_err());
            for name in ["settings", "transcriptions", "secure.dat"] {
                assert_eq!(fs::read(data.join(name)).unwrap(), b"must stay untouched");
            }
        }
        assert!(guard_reset_app_data(&home.join(IDENTIFIER), home, IDENTIFIER, &[]).is_ok());
    }

    #[test]
    fn reset_guard_fails_closed_when_storage_cannot_be_resolved() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join(IDENTIFIER);
        assert!(guard_reset_app_data(&data, temp.path(), IDENTIFIER, &[]).is_err());
        assert!(!data.exists());
        fs::write(&data, b"not a directory").unwrap();
        assert!(guard_reset_app_data(&data, temp.path(), IDENTIFIER, &[]).is_err());
        assert_eq!(fs::read(data).unwrap(), b"not a directory");
    }

    #[test]
    fn nonexistent_reset_targets_are_skipped() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join(IDENTIFIER);
        assert!(!reset_directory_present(&data));
        assert!(remove_reset_directory(&data, temp.path(), IDENTIFIER, &[]).is_ok());
        assert!(guard_reset_cache(&data, temp.path(), IDENTIFIER, &[], temp.path()).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_same_named_app_root_aliases() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let real_parent = temp.path().join("real");
        let data = real_parent.join(IDENTIFIER);
        fs::create_dir_all(&data).unwrap();
        fs::write(data.join("secure.dat"), b"keep secrets").unwrap();
        let alias = temp.path().join(IDENTIFIER);
        symlink(&data, &alias).unwrap();
        assert!(guard_reset_app_data(&alias, temp.path(), IDENTIFIER, &[]).is_err());
        assert!(remove_reset_directory(&alias, temp.path(), IDENTIFIER, &[]).is_err());
        assert_eq!(fs::read(data.join("secure.dat")).unwrap(), b"keep secrets");
    }

    #[cfg(unix)]
    #[test]
    fn accepts_symlinked_storage_base_without_precanonicalizing_test_roots() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let real_base = temp.path().join("real_base");
        let link_base = temp.path().join("link_base");
        fs::create_dir_all(real_base.join(IDENTIFIER).join("models")).unwrap();
        symlink(&real_base, &link_base).unwrap();
        let app_dir = link_base.join(IDENTIFIER);
        assert!(guard_reset_app_data(&app_dir, &link_base, IDENTIFIER, &[]).is_ok());
        assert!(
            guard_reset_directory(&app_dir.join("models"), &link_base, IDENTIFIER, &[]).is_ok()
        );
        remove_reset_directory(&app_dir.join("models"), &link_base, IDENTIFIER, &[]).unwrap();
        assert!(!real_base.join(IDENTIFIER).join("models").exists());
        // Aliases above, not only at, the trusted base are also accepted.
        fs::create_dir_all(real_base.join("nested").join(IDENTIFIER)).unwrap();
        let nested_base = link_base.join("nested");
        assert!(
            guard_reset_app_data(&nested_base.join(IDENTIFIER), &nested_base, IDENTIFIER, &[],)
                .is_ok()
        );
    }

    #[test]
    fn reset_cache_rejects_equal_containing_and_contained_data_paths() {
        let temp = tempfile::tempdir().unwrap();
        let outer = temp.path().join(IDENTIFIER);
        let inner_base = outer.join("nested");
        let inner = inner_base.join(IDENTIFIER);
        fs::create_dir_all(&inner).unwrap();
        let outer_resolved = outer.canonicalize().unwrap();
        let inner_resolved = inner.canonicalize().unwrap();
        assert!(guard_reset_cache(&outer, temp.path(), IDENTIFIER, &[], &outer_resolved).is_err());
        assert!(guard_reset_cache(&outer, temp.path(), IDENTIFIER, &[], &inner_resolved).is_err());
        assert!(guard_reset_cache(&inner, &inner_base, IDENTIFIER, &[], &outer_resolved).is_err());
    }

    #[test]
    fn reset_cache_accepts_separate_data_and_cache_directories() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data").join(IDENTIFIER);
        let cache_base = temp.path().join("cache");
        let cache = cache_base.join(IDENTIFIER);
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(&cache).unwrap();
        let data = data.canonicalize().unwrap();
        assert!(guard_reset_cache(&cache, &cache_base, IDENTIFIER, &[], &data).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn reset_cache_rejects_base_alias_into_data_and_identifier_alias() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let data_base = temp.path().join("data");
        let data = data_base.join(IDENTIFIER);
        fs::create_dir_all(&data).unwrap();
        fs::write(data.join("secure.dat"), b"keep secrets").unwrap();
        let cache_base = temp.path().join("cache");
        symlink(&data_base, &cache_base).unwrap();
        let resolved_data = data.canonicalize().unwrap();
        assert!(guard_reset_cache(
            &cache_base.join(IDENTIFIER),
            &cache_base,
            IDENTIFIER,
            &[],
            &resolved_data,
        )
        .is_err());
        let other_base = temp.path().join("other-cache");
        fs::create_dir_all(&other_base).unwrap();
        symlink(&data, other_base.join(IDENTIFIER)).unwrap();
        assert!(guard_reset_cache(
            &other_base.join(IDENTIFIER),
            &other_base,
            IDENTIFIER,
            &[],
            &resolved_data,
        )
        .is_err());
        assert_eq!(fs::read(data.join("secure.dat")).unwrap(), b"keep secrets");
    }

    #[cfg(unix)]
    #[test]
    fn reset_guard_rejects_canonicalization_failure_and_dangling_aliases() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let looping_base = temp.path().join("loop");
        symlink(&looping_base, &looping_base).unwrap();
        let target = looping_base.join(IDENTIFIER);
        assert!(reset_directory_present(&target));
        assert!(guard_reset_app_data(&target, &looping_base, IDENTIFIER, &[]).is_err());
        assert!(guard_reset_cache(&target, &looping_base, IDENTIFIER, &[], temp.path()).is_err());
        assert!(remove_reset_directory(&target, &looping_base, IDENTIFIER, &[]).is_err());

        let dangling = temp.path().join(IDENTIFIER);
        symlink(temp.path().join("missing"), &dangling).unwrap();
        assert!(reset_directory_present(&dangling));
        assert!(guard_reset_app_data(&dangling, temp.path(), IDENTIFIER, &[]).is_err());
        assert!(remove_reset_directory(&dangling, temp.path(), IDENTIFIER, &[]).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn rejects_junction_app_roots_but_accepts_junction_storage_bases() {
        let temp = tempfile::tempdir().unwrap();
        let real_base = temp.path().join("real");
        let data = real_base.join(IDENTIFIER);
        fs::create_dir_all(&data).unwrap();
        fs::write(data.join("secure.dat"), b"keep secrets").unwrap();
        let alias = temp.path().join(IDENTIFIER);
        let parent_alias = temp.path().join("linked-parent");
        for (link, target) in [(&alias, &data), (&parent_alias, &real_base)] {
            assert!(std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .unwrap()
                .status
                .success());
        }
        assert!(guard_reset_app_data(&alias, temp.path(), IDENTIFIER, &[]).is_err());
        assert!(guard_reset_app_data(
            &parent_alias.join(IDENTIFIER),
            &parent_alias,
            IDENTIFIER,
            &[],
        )
        .is_ok());
        assert!(guard_reset_cache(
            &parent_alias.join(IDENTIFIER),
            &parent_alias,
            IDENTIFIER,
            &[],
            &data.canonicalize().unwrap(),
        )
        .is_err());
        assert_eq!(fs::read(data.join("secure.dat")).unwrap(), b"keep secrets");
    }
}
