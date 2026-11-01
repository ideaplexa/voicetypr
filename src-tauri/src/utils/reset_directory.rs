//! Fail-closed ownership boundary for recursive reset deletion.
use std::{
    fs,
    path::{Component, Path},
};

pub(crate) fn validate(path: &Path, identifier: &str, home: &Path) -> Result<(), &'static str> {
    if !matches!(
        identifier,
        "com.ideaplexa.voicetypr" | "com.ideaplexa.voicetypr.dev"
    ) || !path.is_absolute()
        || path == home
        || path.parent().is_none()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("Refusing reset outside app-owned storage");
    }
    // The directory named EXACTLY for this build's identifier is the ownership
    // boundary. Its parent (Caches, AppData/Local, etc.) is never a target.
    // The boundary itself may be removed (app_cache_dir), as may its children.
    let root = path
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == identifier))
        .ok_or("Refusing reset outside app-owned storage")?;
    if root == home || root.parent().is_none() {
        return Err("Refusing reset outside app-owned storage");
    }
    // Reject aliases at/below the ownership boundary, including junctions on
    // Windows. Otherwise `app-id/models` could resolve to another app's data.
    // System ancestors such as macOS /var -> /private/var are not app aliases.
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                #[cfg(windows)]
                let alias = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
                };
                #[cfg(not(windows))]
                let alias = metadata.is_symlink();
                if alias {
                    return Err("Refusing reset through a storage alias");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Could not validate reset storage"),
        }
        if ancestor == root {
            break;
        }
    }
    Ok(())
}

/// Returns whether anything was deleted. Never includes user paths in errors.
pub(crate) fn remove(path: &Path, identifier: &str, home: &Path) -> Result<bool, String> {
    validate(path, identifier, home)?;
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("Could not clear app-owned storage".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "com.ideaplexa.voicetypr.dev";

    #[test]
    fn rejects_home_user_wide_roots_siblings_and_other_builds() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        for relative in [
            "",
            "Library/Caches",
            ".cache",
            "AppData/Local",
            "AppData/Roaming",
            "Library/Caches/another-app",
            "AppData/Local/another-app",
            "Library/Caches/com.ideaplexa.voicetypr",
            "Library/Caches/com.ideaplexa.voicetypr.dev-other",
            "Library/Caches/FluidAudio",
        ] {
            assert!(validate(&home.join(relative), ID, &home).is_err());
        }
        for path in [Path::new("/"), Path::new("relative"), Path::new("C:\\")] {
            assert!(validate(path, ID, &home).is_err());
        }
        #[cfg(windows)]
        for path in ["C:\\Users\\test\\AppData\\Local", "C:\\Users\\test", "C:\\"] {
            assert!(validate(Path::new(path), ID, &home).is_err());
        }
        let root = home.join("Library/Caches").join(ID);
        assert!(validate(&root.join("../another-app"), ID, &home).is_err());
        assert!(validate(&root, "", &home).is_err());
        assert!(validate(&root, "../Caches", &home).is_err());
        assert!(
            validate(&root, ID, &root).is_err(),
            "even an app-named home is protected"
        );
        assert!(validate(&root.join("models"), ID, &root).is_err());
    }

    #[test]
    fn accepts_only_the_current_apps_dirs_and_preserves_siblings() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        for identifier in [ID, "com.ideaplexa.voicetypr"] {
            for parent in [
                "Library/Caches",
                "Library/Application Support",
                "AppData/Local",
                "AppData/Roaming",
                ".cache",
            ] {
                let root = home.join(parent).join(identifier);
                let sibling = home.join(parent).join("other-app");
                fs::create_dir_all(root.join("models")).unwrap();
                fs::create_dir_all(&sibling).unwrap();
                fs::write(sibling.join("keep"), b"unrelated").unwrap();
                assert!(validate(&root.join("models"), identifier, home).is_ok());
                assert!(remove(&root, identifier, home).unwrap());
                assert!(!remove(&root, identifier, home).unwrap());
                assert!(sibling.join("keep").exists());
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_targets_and_symlinked_app_roots() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(ID);
        let external = temp.path().join("other-app");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("keep"), b"unrelated").unwrap();
        symlink(&external, root.join("models")).unwrap();
        assert!(remove(&root.join("models"), ID, temp.path()).is_err());
        assert!(validate(&root.join("models/subdir"), ID, temp.path()).is_err());
        let alias = temp.path().join("alias").join(ID);
        fs::create_dir_all(alias.parent().unwrap()).unwrap();
        symlink(&external, &alias).unwrap();
        assert!(remove(&alias, ID, temp.path()).is_err());
        assert!(external.join("keep").exists());
    }
}
