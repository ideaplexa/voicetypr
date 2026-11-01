//! Fail-closed ownership boundary for recursive reset deletion.
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

const OUTSIDE: &str = "Refusing reset outside app-owned storage";
const UNRESOLVED: &str = "Could not validate reset storage";

// Component-aware (not string-prefix) comparisons. Test Windows' ASCII case
// folding on every host; canonical paths on Windows can change spelling.
fn within(path: &Path, root: &Path, ignore_case: bool) -> bool {
    let mut components = path.components();
    root.components().all(|expected| {
        components.next().is_some_and(|actual| {
            if ignore_case {
                actual
                    .as_os_str()
                    .as_encoded_bytes()
                    .eq_ignore_ascii_case(expected.as_os_str().as_encoded_bytes())
            } else {
                actual == expected
            }
        })
    })
}

fn same(path: &Path, other: &Path, ignore_case: bool) -> bool {
    within(path, other, ignore_case) && within(other, path, ignore_case)
}

/// Canonicalize an existing base. A missing base is skipped, but a broken
/// alias, inaccessible ancestor or other resolution failure is not "missing".
fn canonical_base(base: &Path) -> Result<Option<PathBuf>, &'static str> {
    for ancestor in base.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                let canonical = fs::canonicalize(ancestor).map_err(|_| UNRESOLVED)?;
                if !canonical.is_dir() {
                    return Err(UNRESOLVED);
                }
                return Ok((ancestor == base).then_some(canonical));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(UNRESOLVED),
        }
    }
    Err(UNRESOLVED)
}

/// `base` is a trusted OS storage base, NOT a guessed ancestor of `path`.
/// Aliases at/above it are allowed; aliases from `<base>/<identifier>` down
/// are not. Return only the canonical existing target, or None when absent.
pub(crate) fn validate(
    path: &Path,
    identifier: &str,
    base: &Path,
) -> Result<Option<PathBuf>, &'static str> {
    if !matches!(
        identifier,
        "com.ideaplexa.voicetypr" | "com.ideaplexa.voicetypr.dev"
    ) || !path.is_absolute()
        || !base.is_absolute()
        || path
            .components()
            .chain(base.components())
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(OUTSIDE);
    }
    let root = base.join(identifier);
    if !within(path, &root, cfg!(windows)) {
        return Err(OUTSIDE);
    }
    let Some(mut target) = canonical_base(base)? else {
        return Ok(None);
    };
    let canonical_root = target.join(identifier);
    for component in path.components().skip(base.components().count()) {
        target.push(component);
        match fs::symlink_metadata(&target) {
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
                if !metadata.is_dir() {
                    return Err(UNRESOLVED);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(UNRESOLVED),
        }
    }
    let canonical_target = fs::canonicalize(&target).map_err(|_| UNRESOLVED)?;
    let canonical_root = fs::canonicalize(canonical_root).map_err(|_| UNRESOLVED)?;
    if !within(&canonical_target, &canonical_root, cfg!(windows)) {
        return Err(OUTSIDE);
    }
    Ok(Some(canonical_target))
}

/// Preflight *all* storage roles before any mutation. A platform may explicitly
/// allow two roles to share the exact same OS path, never two different paths
/// aliased to the same place (e.g. Caches -> Application Support).
pub(crate) fn validate_roles(
    roots: &[PathBuf],
    identifier: &str,
    shared_roles: &[(usize, usize)],
) -> Result<(), &'static str> {
    let canonical = roots
        .iter()
        .map(|root| validate(root, identifier, root.parent().ok_or(OUTSIDE)?))
        .collect::<Result<Vec<_>, _>>()?;
    for (i, path) in canonical.iter().enumerate() {
        let Some(path) = path else { continue };
        for (j, other) in canonical.iter().enumerate().take(i) {
            let Some(other) = other else { continue };
            let shared = shared_roles.contains(&(j, i))
                && same(&roots[j], &roots[i], cfg!(windows))
                && same(path, other, cfg!(windows));
            if !shared && (within(path, other, cfg!(windows)) || within(other, path, cfg!(windows)))
            {
                return Err("Refusing reset of overlapping storage roles");
            }
        }
    }
    Ok(())
}

/// Validate a nonrecursive file sweep in an OS-provided temp base or its
/// immediate child. This does NOT authorize recursively removing either root.
/// OS aliases at/above the trusted base are allowed, but a child alias is not.
pub(crate) fn validate_file_sweep_root(path: &Path, base: &Path) -> Result<(), &'static str> {
    if !path.is_absolute()
        || !base.is_absolute()
        || path
            .components()
            .chain(base.components())
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        || !(path == base || path.parent() == Some(base))
    {
        return Err(OUTSIDE);
    }
    let Some(canonical) = canonical_base(base)? else {
        return Ok(());
    };
    if path != base {
        let child = canonical.join(path.file_name().ok_or(OUTSIDE)?);
        match fs::symlink_metadata(child) {
            Ok(metadata) => {
                #[cfg(windows)]
                let alias = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let alias = metadata.is_symlink();
                if alias || !metadata.is_dir() {
                    return Err(UNRESOLVED);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(UNRESOLVED),
        }
    }
    Ok(())
}

/// Returns whether anything was deleted. Never includes user paths in errors.
pub(crate) fn remove(path: &Path, identifier: &str, base: &Path) -> Result<bool, String> {
    let Some(path) = validate(path, identifier, base)? else {
        return Ok(false);
    };
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
    fn rejects_user_wide_roots_siblings_and_other_builds() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        for relative in [
            "",
            "another-app",
            "com.ideaplexa.voicetypr",
            "com.ideaplexa.voicetypr.dev-other",
            "FluidAudio",
        ] {
            assert!(validate(&base.join(relative), ID, base).is_err());
        }
        for path in [Path::new("/"), Path::new("relative"), Path::new("C:\\")] {
            assert!(validate(path, ID, base).is_err());
        }
        assert!(validate(&base.join(ID).join("../another-app"), ID, base).is_err());
        assert!(validate(&base.join(ID), "", base).is_err());
        assert!(validate(&base.join(ID), "../Caches", base).is_err());
        assert!(validate(&base.join(ID), ID, &base.join(ID)).is_err());
    }

    #[test]
    fn accepts_only_the_current_apps_dirs_and_preserves_siblings() {
        let temp = tempfile::tempdir().unwrap();
        for identifier in [ID, "com.ideaplexa.voicetypr"] {
            for parent in [
                "Library/Caches",
                "Library/Application Support",
                "AppData/Local",
                "AppData/Roaming",
                ".cache",
            ] {
                let base = temp.path().join(parent);
                let root = base.join(identifier);
                let sibling = base.join("other-app");
                fs::create_dir_all(root.join("models")).unwrap();
                fs::create_dir_all(&sibling).unwrap();
                fs::write(sibling.join("keep"), b"unrelated").unwrap();
                assert!(validate(&root.join("models"), identifier, &base)
                    .unwrap()
                    .is_some());
                assert!(remove(&root, identifier, &base).unwrap());
                assert!(!remove(&root, identifier, &base).unwrap());
                assert!(sibling.join("keep").exists());
            }
        }
    }

    #[test]
    fn case_folding_is_ascii_and_component_aware() {
        let root = Path::new("/Users/Test/AppData/Local/App");
        let child = Path::new("/users/test/appdata/local/APP/models");
        assert!(within(child, root, true));
        assert!(!within(child, root, false));
        assert!(!within(
            Path::new("/Users/Test/AppData/Local/App-other"),
            root,
            true
        ));
        assert!(!same(child, root, true));
        assert!(same(Path::new("/users/test/appdata/local/APP"), root, true));
        assert!(!same(Path::new("/Ä"), Path::new("/ä"), true));
    }

    #[test]
    fn rejects_resolution_failures_but_skips_missing_paths() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        fs::write(base.join("not-a-directory"), b"blocked").unwrap();
        assert!(validate(
            &base.join("not-a-directory").join(ID),
            ID,
            &base.join("not-a-directory")
        )
        .is_err());
        fs::create_dir(base.join(ID)).unwrap();
        fs::write(base.join(ID).join("blocked"), b"blocked").unwrap();
        assert!(validate(&base.join(ID).join("blocked/child"), ID, base).is_err());
        assert_eq!(validate(&base.join(ID).join("missing"), ID, base), Ok(None));
        assert_eq!(
            validate(
                &base.join("missing/base").join(ID),
                ID,
                &base.join("missing/base")
            ),
            Ok(None)
        );
    }

    #[test]
    fn explicitly_shared_roles_require_identical_os_paths() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(ID);
        fs::create_dir(&root).unwrap();
        let roots = [root.clone(), root];
        assert!(validate_roles(&roots, ID, &[]).is_err());
        assert!(validate_roles(&roots, ID, &[(0, 1)]).is_ok());
        let nested = [roots[0].clone(), roots[0].join("nested").join(ID)];
        fs::create_dir_all(&nested[1]).unwrap();
        assert!(validate_roles(&nested, ID, &[(0, 1)]).is_err());
    }

    #[test]
    fn file_sweep_preflight_accepts_only_base_and_immediate_directory_children() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        assert!(validate_file_sweep_root(base, base).is_ok());
        let child = base.join("kept-dictation");
        assert!(validate_file_sweep_root(&child, base).is_ok());
        fs::create_dir(&child).unwrap();
        assert!(validate_file_sweep_root(&child, base).is_ok());
        assert!(validate_file_sweep_root(&child.join("nested"), base).is_err());
        assert!(validate_file_sweep_root(&base.join("../outside"), base).is_err());
        fs::write(base.join("blocked"), b"not a directory").unwrap();
        assert!(validate_file_sweep_root(&base.join("blocked"), base).is_err());
        assert!(
            validate_file_sweep_root(&base.join("blocked/child"), &base.join("blocked"),).is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_sweep_preflight_allows_os_base_aliases_but_rejects_child_and_broken_aliases() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("real");
        fs::create_dir_all(base.join("kept-dictation")).unwrap();
        let os_alias = temp.path().join("os-alias");
        symlink(&base, &os_alias).unwrap();
        assert!(validate_file_sweep_root(&os_alias, &os_alias).is_ok());
        assert!(validate_file_sweep_root(&os_alias.join("kept-dictation"), &os_alias).is_ok());
        symlink(&base, base.join("child-alias")).unwrap();
        assert!(validate_file_sweep_root(&base.join("child-alias"), &base).is_err());
        let broken = temp.path().join("broken");
        symlink(temp.path().join("missing"), &broken).unwrap();
        assert!(validate_file_sweep_root(&broken, &broken).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn accepts_a_real_alias_above_the_base_without_precanonicalizing_test_roots() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real_base");
        let link = temp.path().join("link_base");
        fs::create_dir_all(real.join(ID).join("models")).unwrap();
        symlink(&real, &link).unwrap();
        assert_eq!(
            validate(&link.join(ID).join("models"), ID, &link).unwrap(),
            Some(fs::canonicalize(real.join(ID).join("models")).unwrap())
        );
        assert!(remove(&link.join(ID).join("models"), ID, &link).unwrap());
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
        let base = temp.path().join("alias");
        fs::create_dir(&base).unwrap();
        symlink(&external, base.join(ID)).unwrap();
        assert!(remove(&base.join(ID), ID, &base).is_err());
        assert!(external.join("keep").exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_cache_base_aliased_to_data_and_broken_base_resolution() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("Application Support");
        let cache = temp.path().join("Caches");
        fs::create_dir_all(data.join(ID)).unwrap();
        fs::write(data.join(ID).join("secure.dat"), b"keep").unwrap();
        symlink(&data, &cache).unwrap();
        let roots = [data.join(ID), cache.join(ID)];
        assert!(validate_roles(&roots, ID, &[]).is_err());
        assert!(validate_roles(&roots, ID, &[(0, 1)]).is_err());
        assert!(data.join(ID).join("secure.dat").exists());
        let broken = temp.path().join("broken");
        symlink(temp.path().join("absent"), &broken).unwrap();
        assert!(validate(&broken.join(ID), ID, &broken).is_err());
        assert!(validate(&broken.join("nested").join(ID), ID, &broken.join("nested")).is_err());
    }
}
