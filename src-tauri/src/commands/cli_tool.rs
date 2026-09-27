//! Install/uninstall/status for the `voicetypr` command-line launcher.
//!
//! macOS: writes a tiny shell shim to `/usr/local/bin/voicetypr` that `exec`s the in-bundle,
//! notarized binary by its absolute path (the VS Code `code` pattern). We never copy or
//! hardlink the signed Mach-O out of the bundle — Gatekeeper kills a relocated signed binary.
//! The shim survives in-place app updates because the bundle path is stable.
//!
//! Windows: the executable is already named `voicetypr.exe`, so exposing the command is just
//! a matter of putting its install directory on PATH. We do this from the app (no installer
//! changes): add/remove the directory in `HKCU\Environment\Path`, which is user-writable (no
//! elevation) and avoids the classic NSIS PATH-truncation data-loss bug, then broadcast
//! `WM_SETTINGCHANGE` so newly-opened terminals pick it up.

use serde::Serialize;

#[cfg(target_os = "macos")]
const MACOS_SHIM_PATH: &str = "/usr/local/bin/voicetypr";

#[derive(Debug, Clone, Serialize)]
pub struct CliToolStatus {
    /// `voicetypr` is reachable from a terminal (shim installed / install dir on PATH).
    pub installed: bool,
    /// Install/uninstall can be managed from inside the app. False only on unsupported
    /// platforms, in which case the UI should stay informational.
    pub manageable: bool,
    /// Where the command lives / how to invoke it, when known.
    pub path: Option<String>,
    /// Version of the running app that owns the managed command.
    pub app_version: String,
    /// Version exposed by the command when its launcher is known to target this app.
    pub command_version: Option<String>,
    /// The installed command is managed by Voicetypr and targets this app installation.
    pub compatible: bool,
    /// Actionable health detail when the command is stale, foreign, or unsupported.
    pub detail: Option<String>,
}

#[tauri::command]
pub fn cli_tool_status() -> Result<CliToolStatus, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(macos::status())
    }
    #[cfg(target_os = "windows")]
    {
        Ok(windows_path::status())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(CliToolStatus {
            installed: false,
            manageable: false,
            path: None,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            command_version: None,
            compatible: false,
            detail: Some("Command installation is not managed on this platform.".to_string()),
        })
    }
}

#[tauri::command]
pub async fn install_cli_tool() -> Result<CliToolStatus, String> {
    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(macos::install)
            .await
            .map_err(|e| format!("Install task failed: {e}"))??;
        Ok(macos::status())
    }
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(windows_path::install)
            .await
            .map_err(|e| format!("Install task failed: {e}"))??;
        Ok(windows_path::status())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Installing the voicetypr command is not supported on this platform.".to_string())
    }
}
#[tauri::command]
pub async fn repair_cli_tool() -> Result<CliToolStatus, String> {
    install_cli_tool().await
}

#[tauri::command]
pub async fn uninstall_cli_tool() -> Result<CliToolStatus, String> {
    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(macos::uninstall)
            .await
            .map_err(|e| format!("Uninstall task failed: {e}"))??;
        Ok(macos::status())
    }
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(windows_path::uninstall)
            .await
            .map_err(|e| format!("Uninstall task failed: {e}"))??;
        Ok(windows_path::status())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Uninstalling the voicetypr command is not supported on this platform.".to_string())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{CliToolStatus, MACOS_SHIM_PATH};
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::Command;

    /// Substring embedded in every shim we write. This token alone is NOT sufficient
    /// to claim ownership — `classify` matches the shim's full structural shape — but it
    /// is the human-readable anchor the marker line is built from.
    const SHIM_MARKER: &str = "managed by Voicetypr";

    /// First line of every shim we emit.
    const SHIM_SHEBANG: &str = "#!/bin/sh";

    /// Exact second line of every shim, carrying the ownership marker on its OWN
    /// dedicated line. Built from `SHIM_MARKER` so the two cannot drift apart;
    /// `build_shim` writes this verbatim and `classify` requires it verbatim, so a
    /// foreign file that merely *contains* the marker phrase (a comment, an `echo`, …)
    /// never classifies as Managed.
    fn shim_comment_line() -> String {
        format!(
            "# Voicetypr CLI launcher ({marker}; reinstall from Settings). Do not edit.",
            marker = SHIM_MARKER,
        )
    }

    /// Whether a path is safe for us to (over)write or remove: absent, already our
    /// managed shim, or something foreign we must leave untouched.
    #[derive(Debug)]
    enum Ownership {
        Absent,
        Managed,
        Foreign,
    }

    pub fn status() -> CliToolStatus {
        let current_exe = current_exe_path().ok();
        status_for(Path::new(MACOS_SHIM_PATH), current_exe.as_deref())
    }

    fn status_for(path: &Path, current_exe: Option<&str>) -> CliToolStatus {
        let ownership = classify(path);
        let installed = !matches!(ownership, Ownership::Absent);
        let manageable = !matches!(ownership, Ownership::Foreign);
        let compatible = matches!(ownership, Ownership::Managed)
            && current_exe.is_some_and(|exe| {
                std::fs::read_to_string(path).is_ok_and(|shim| shim == build_shim(exe))
            });
        let detail = match ownership {
            Ownership::Absent => None,
            Ownership::Foreign => Some(
                "Another command already uses this path. Voicetypr will not overwrite it."
                    .to_string(),
            ),
            Ownership::Managed if !compatible => Some(
                "The command points to a different Voicetypr installation. Repair it to use this app."
                    .to_string(),
            ),
            Ownership::Managed => None,
        };

        CliToolStatus {
            installed,
            manageable,
            path: installed.then(|| path.to_string_lossy().into_owned()),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            command_version: compatible.then(|| env!("CARGO_PKG_VERSION").to_string()),
            compatible,
            detail,
        }
    }

    pub fn install() -> Result<(), String> {
        // Never clobber an unrelated `voicetypr` command the user installed themselves:
        // only (over)write the path if it is absent or already our managed shim.
        match classify_path() {
            Ownership::Foreign => return Err(foreign_error()),
            Ownership::Managed | Ownership::Absent => {}
        }
        let exe_str = current_exe_path()?;

        // App Translocation gives a quarantined app a random, read-only path that vanishes;
        // a shim pointing there would break. Tell the user to install to /Applications first.
        if exe_str.contains("/AppTranslocation/") {
            return Err(
                "Move Voicetypr to your Applications folder before installing the command."
                    .to_string(),
            );
        }

        let shim = build_shim(&exe_str);

        // Stage the shim in a securely-created unique temp file (tempfile uses O_EXCL + a
        // random name, mode 0600), so a local attacker can't pre-create or swap it before the
        // privileged copy (no fixed-name TOCTOU). It is removed when `tmp` drops.
        let mut tmp = tempfile::Builder::new()
            .prefix("voicetypr-cli-shim-")
            .tempfile()
            .map_err(|e| format!("Cannot stage command: {e}"))?;
        tmp.write_all(shim.as_bytes())
            .map_err(|e| format!("Cannot stage command: {e}"))?;
        tmp.flush()
            .map_err(|e| format!("Cannot stage command: {e}"))?;
        let tmp_str = tmp
            .path()
            .to_str()
            .ok_or("Temp path is not valid UTF-8")?
            .to_string();

        // Fast path: /usr/local/bin already writable (typical on Homebrew machines).
        if try_install_direct(&tmp_str).is_ok() {
            return Ok(());
        }
        // Otherwise escalate with one admin prompt. `tmp` (and its file) stays alive until
        // this function returns, so the privileged copy can read it.
        install_with_admin(&tmp_str)
    }

    pub fn uninstall() -> Result<(), String> {
        // Only remove a path we actually own; leave a foreign `voicetypr` untouched.
        match classify_path() {
            Ownership::Absent => return Ok(()),
            Ownership::Foreign => return Err(foreign_error()),
            Ownership::Managed => {}
        }
        if std::fs::remove_file(MACOS_SHIM_PATH).is_ok() {
            return Ok(());
        }
        run_osascript(&format!("rm -f {}", sh_single_quote(MACOS_SHIM_PATH)))
    }

    fn try_install_direct(tmp: &str) -> Result<(), String> {
        std::fs::create_dir_all("/usr/local/bin").map_err(|e| e.to_string())?;
        std::fs::copy(tmp, MACOS_SHIM_PATH).map_err(|e| e.to_string())?;
        std::fs::set_permissions(MACOS_SHIM_PATH, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn install_with_admin(tmp: &str) -> Result<(), String> {
        run_osascript(&format!(
            "mkdir -p /usr/local/bin && cp {} {} && chmod 755 {}",
            sh_single_quote(tmp),
            sh_single_quote(MACOS_SHIM_PATH),
            sh_single_quote(MACOS_SHIM_PATH)
        ))
    }

    /// Run a /bin/sh command with administrator privileges via osascript (shows the standard
    /// macOS authentication dialog). `shell_script` must use only fixed/controlled paths.
    fn run_osascript(shell_script: &str) -> Result<(), String> {
        let apple = format!(
            "do shell script \"{}\" with administrator privileges",
            applescript_escape(shell_script)
        );
        let out = Command::new("osascript")
            .arg("-e")
            .arg(&apple)
            .output()
            .map_err(|e| format!("Failed to run osascript: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("-128") || err.contains("User canceled") {
            Err("Authorization was cancelled.".to_string())
        } else {
            Err(format!("Failed to install command: {}", err.trim()))
        }
    }

    fn build_shim(exe: &str) -> String {
        format!(
            "{shebang}\n{comment}\nexec \"{exe}\" \"$@\"\n",
            shebang = SHIM_SHEBANG,
            comment = shim_comment_line(),
            exe = sh_double_quote_escape(exe),
        )
    }
    fn current_exe_path() -> Result<String, String> {
        let exe =
            std::env::current_exe().map_err(|e| format!("Cannot resolve app executable: {e}"))?;
        let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
        exe.to_str()
            .map(str::to_string)
            .ok_or_else(|| "App executable path is not valid UTF-8".to_string())
    }

    /// Classify an arbitrary path by the same rules as the real shim path. Kept
    /// path-parameterized so the ownership logic can be unit-tested on temp files.
    fn classify(path: &Path) -> Ownership {
        // `symlink_metadata` (not `Path::exists`) so a dangling symlink is detected as
        // "present" and treated as foreign, rather than being silently written through.
        if std::fs::symlink_metadata(path).is_err() {
            return Ownership::Absent;
        }
        // `read_to_string` follows symlinks, so a symlink pointing at our shim still
        // reads as ours. A directory, unreadable file, or dangling symlink fails to read
        // and is conservatively classified foreign.
        match std::fs::read_to_string(path) {
            Ok(content) if is_managed_shim(&content) => Ownership::Managed,
            Ok(_) | Err(_) => Ownership::Foreign,
        }
    }

    /// Recognize the *exact shape* `build_shim` emits — not a bare substring. A foreign
    /// command that merely happens to contain the phrase "managed by Voicetypr" (e.g. a
    /// personal wrapper with the comment `# not managed by Voicetypr`, or a script that
    /// echoes it) must classify as Foreign, otherwise install/uninstall would clobber or
    /// remove a command the user placed there themselves despite the fail-closed check.
    fn is_managed_shim(content: &str) -> bool {
        // The managed shim is exactly three lines (plus a single trailing newline):
        //   #!/bin/sh
        //   # Voicetypr CLI launcher (managed by Voicetypr; reinstall from Settings). Do not edit.
        //   exec "<escaped absolute exe path>" "$@"
        // `lines()` yields exactly those three for that output and no fourth element, so
        // anything with extra/missing lines is foreign.
        let mut lines = content.lines();
        let (shebang, comment, exec, extra) =
            (lines.next(), lines.next(), lines.next(), lines.next());
        matches!(
            (shebang, comment, exec, extra),
            (Some(s), Some(c), Some(e), None)
                if s == SHIM_SHEBANG && c == shim_comment_line() && is_managed_exec_line(e)
        )
    }

    /// The shim's `exec "<exe>" "$@"` line. The exe path is arbitrary (it is the running
    /// app's absolute path), so we match its *shape* — `exec "<non-empty>" "$@"` — rather
    /// than a fixed string. `sh_double_quote_escape` escapes every `"` inside the path, so
    /// the only unescaped quotes are the delimiters we split on.
    fn is_managed_exec_line(line: &str) -> bool {
        let path = line
            .strip_prefix("exec \"")
            .and_then(|rest| rest.strip_suffix("\" \"$@\""));
        match path {
            Some(p) => !p.is_empty(),
            None => false,
        }
    }

    fn classify_path() -> Ownership {
        classify(Path::new(MACOS_SHIM_PATH))
    }

    /// Error returned when install/uninstall would touch an unmanaged `voicetypr`.
    /// Actionable, and deliberately does not echo the foreign file's contents.
    fn foreign_error() -> String {
        format!(
            "'{p}' already exists but is not the command managed by Voicetypr, so it was left \
             untouched. To let Voicetypr manage it, remove or rename it first (for example \
             `mv {p} {p}.bak`) and try again.",
            p = MACOS_SHIM_PATH
        )
    }

    /// Escape a string for inclusion inside an AppleScript double-quoted literal.
    fn applescript_escape(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// Escape a path for inclusion inside a `"..."` literal in /bin/sh.
    fn sh_double_quote_escape(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`")
    }

    /// Wrap a string as a safe single-quoted /bin/sh token (escaping embedded single quotes),
    /// for a path interpolated into a `do shell script` command.
    fn sh_single_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::fs::symlink;

        #[test]
        fn classifies_absent_managed_and_foreign_targets() {
            let dir = tempfile::tempdir().unwrap();

            // Absent: safe to install.
            assert!(matches!(
                classify(&dir.path().join("missing")),
                Ownership::Absent
            ));

            // Our own shim is recognized as managed (safe to reinstall/remove).
            let managed = dir.path().join("voicetypr");
            std::fs::write(
                &managed,
                build_shim("/Applications/Voicetypr.app/Contents/MacOS/voicetypr"),
            )
            .unwrap();
            assert!(matches!(classify(&managed), Ownership::Managed));

            // A foreign command without the marker is left untouched.
            let foreign = dir.path().join("foreign");
            std::fs::write(&foreign, "#!/bin/sh\necho hello\n").unwrap();
            assert!(matches!(classify(&foreign), Ownership::Foreign));

            // A symlink to an unrelated file is foreign.
            let link = dir.path().join("link-foreign");
            symlink(&foreign, &link).unwrap();
            assert!(matches!(classify(&link), Ownership::Foreign));

            // A symlink pointing at our shim reads as managed.
            let link_ours = dir.path().join("link-ours");
            symlink(&managed, &link_ours).unwrap();
            assert!(matches!(classify(&link_ours), Ownership::Managed));

            // A dangling symlink must NOT be silently written through -> foreign.
            let dangling = dir.path().join("dangling");
            symlink(dir.path().join("does-not-exist"), &dangling).unwrap();
            assert!(matches!(classify(&dangling), Ownership::Foreign));

            // A directory is foreign (we must not `cp` a file over it).
            let subdir = dir.path().join("subdir");
            std::fs::create_dir(&subdir).unwrap();
            assert!(matches!(classify(&subdir), Ownership::Foreign));
        }

        #[test]
        fn foreign_file_merely_containing_the_marker_phrase_is_foreign() {
            let dir = tempfile::tempdir().unwrap();

            // Regression for the original bare-`contains` ownership check, which misclassified
            // any readable file mentioning the phrase as Managed and would thus have let
            // install/uninstall clobber or delete a command the user placed there themselves.
            // Only our shim's *exact* shape may classify as Managed.
            let cases: &[(&str, &str)] = &[
                // A personal wrapper whose comment incidentally contains the phrase.
                (
                    "comment",
                    "#!/bin/sh\n# not managed by Voicetypr — my wrapper\necho hi\n",
                ),
                // A script that merely echoes the phrase at runtime.
                ("echo", "#!/bin/sh\necho \"managed by Voicetypr\"\n"),
                // Our exact marker comment, but a foreign (non-exec) body.
                (
                    "right-comment-wrong-body",
                    "#!/bin/sh\n# Voicetypr CLI launcher (managed by Voicetypr; reinstall from Settings). Do not edit.\nrm -rf ~\n",
                ),
                // The phrase embedded mid-line rather than on the dedicated marker line.
                ("mid-line", "#!/bin/sh\n# notmanaged by Voicetyprx\necho hi\n"),
            ];
            for &(name, body) in cases {
                let f = dir.path().join(name);
                std::fs::write(&f, body).unwrap();
                assert!(
                    matches!(classify(&f), Ownership::Foreign),
                    "case `{name}` must classify as Foreign, not Managed"
                );
            }
        }

        #[test]
        fn reports_current_stale_and_foreign_command_health() {
            let dir = tempfile::tempdir().unwrap();
            let command = dir.path().join("voicetypr");
            let current_exe = "/Applications/Voicetypr.app/Contents/MacOS/voicetypr";

            std::fs::write(&command, build_shim(current_exe)).unwrap();
            let current = status_for(&command, Some(current_exe));
            assert!(current.installed);
            assert!(current.manageable);
            assert!(current.compatible);
            assert_eq!(
                current.command_version.as_deref(),
                Some(env!("CARGO_PKG_VERSION"))
            );
            assert!(current.detail.is_none());

            let stale = status_for(
                &command,
                Some("/Applications/Other Voicetypr.app/Contents/MacOS/voicetypr"),
            );
            assert!(stale.installed);
            assert!(stale.manageable);
            assert!(!stale.compatible);
            assert!(stale.command_version.is_none());
            assert!(stale
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("Repair")));

            std::fs::write(&command, "#!/bin/sh\necho foreign\n").unwrap();
            let foreign = status_for(&command, Some(current_exe));
            assert!(foreign.installed);
            assert!(!foreign.manageable);
            assert!(!foreign.compatible);
            assert!(foreign
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("not overwrite")));
        }

        #[test]
        fn build_shim_carries_the_marker() {
            let shim = build_shim("/some/exe");
            assert!(shim.starts_with("#!/bin/sh\n"));
            assert!(
                shim.contains(SHIM_MARKER),
                "shim must embed the managed marker so ownership can be verified"
            );
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_path {
    use super::CliToolStatus;
    use std::path::Path;
    use winreg::enums::{RegType, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::{RegKey, RegValue};

    pub fn status() -> CliToolStatus {
        let (installed, compatible) = inspect_path().unwrap_or_default();
        CliToolStatus {
            installed,
            manageable: true,
            path: Some("voicetypr".to_string()),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            command_version: compatible.then(|| env!("CARGO_PKG_VERSION").to_string()),
            compatible,
            detail: (installed && !compatible).then(|| {
                "Another voicetypr command appears earlier on your user PATH. Repair this installation to give it priority.".to_string()
            }),
        }
    }

    fn inspect_path() -> Option<(bool, bool)> {
        let dir = install_dir().ok()?;
        let env = open_env(false).ok()?;
        let (current, _) = read_path(&env).ok()??;
        let installed = path_contains_dir(&current, &dir);
        let compatible = installed
            && resolved_command_dir(&current, &dir)
                .is_some_and(|entry| entry_matches_dir(entry, &dir));
        Some((installed, compatible))
    }

    pub fn install() -> Result<(), String> {
        let dir = install_dir()?;
        let env = open_env(true)?;
        let (current, vtype) = read_path(&env)?.unwrap_or((String::new(), RegType::REG_EXPAND_SZ));
        let new = prioritize_dir(&current, &dir);
        if new == current {
            return Ok(());
        }
        env.set_raw_value("Path", &encode_reg_sz(&new, vtype))
            .map_err(|e| format!("Cannot update PATH: {e}"))?;
        broadcast_env_change();
        Ok(())
    }

    pub fn uninstall() -> Result<(), String> {
        let dir = install_dir()?;
        let env = open_env(true)?;
        let Some((current, vtype)) = read_path(&env)? else {
            return Ok(());
        };
        if !path_contains_dir(&current, &dir) {
            return Ok(());
        }
        let kept: Vec<&str> = current
            .split(';')
            .filter(|entry| !entry_matches_dir(entry, &dir))
            .collect();
        env.set_raw_value("Path", &encode_reg_sz(&kept.join(";"), vtype))
            .map_err(|e| format!("Cannot update PATH: {e}"))?;
        broadcast_env_change();
        Ok(())
    }

    /// Directory holding `voicetypr.exe` — the entry we put on PATH.
    fn install_dir() -> Result<String, String> {
        let exe =
            std::env::current_exe().map_err(|e| format!("Cannot resolve app executable: {e}"))?;
        exe.parent()
            .ok_or("App executable has no parent directory")?
            .to_str()
            .map(str::to_string)
            .ok_or_else(|| "Install path is not valid UTF-8".to_string())
    }

    fn open_env(write: bool) -> Result<RegKey, String> {
        let flags = if write {
            KEY_READ | KEY_WRITE
        } else {
            KEY_READ
        };
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", flags)
            .map_err(|e| format!("Cannot open user environment: {e}"))
    }

    /// Read `Path` as a string with its registry type. `Ok(None)` = value absent. `Err` = the
    /// value exists but is not a string type (REG_SZ/REG_EXPAND_SZ); callers MUST NOT rewrite
    /// it then, or they would corrupt a non-string PATH by re-encoding it under that type.
    fn read_path(env: &RegKey) -> Result<Option<(String, RegType)>, String> {
        let raw = match env.get_raw_value("Path") {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        match &raw.vtype {
            RegType::REG_SZ | RegType::REG_EXPAND_SZ => {
                let vtype = raw.vtype.clone();
                Ok(Some((decode_reg_sz(&raw), vtype)))
            }
            other => Err(format!(
                "Refusing to modify PATH: HKCU\\Environment\\Path has unexpected type {other:?}"
            )),
        }
    }

    fn decode_reg_sz(value: &RegValue) -> String {
        let units: Vec<u16> = value
            .bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
            .trim_end_matches('\u{0}')
            .to_string()
    }

    fn encode_reg_sz(value: &str, vtype: RegType) -> RegValue {
        let mut units: Vec<u16> = value.encode_utf16().collect();
        units.push(0);
        RegValue {
            bytes: units.iter().flat_map(|u| u.to_le_bytes()).collect(),
            vtype,
        }
    }

    fn path_contains_dir(path: &str, dir: &str) -> bool {
        path.split(';').any(|entry| entry_matches_dir(entry, dir))
    }

    fn prioritize_dir(path: &str, dir: &str) -> String {
        let kept = path
            .split(';')
            .filter(|entry| !entry_matches_dir(entry, dir))
            .collect::<Vec<_>>()
            .join(";");
        if kept.is_empty() {
            dir.to_string()
        } else {
            format!("{dir};{kept}")
        }
    }

    fn resolved_command_dir<'a>(path: &'a str, install_dir: &str) -> Option<&'a str> {
        path.split(';').find(|entry| {
            entry_matches_dir(entry, install_dir)
                || Path::new(normalize_entry(entry))
                    .join("voicetypr.exe")
                    .is_file()
        })
    }

    fn normalize_entry(entry: &str) -> &str {
        entry
            .trim()
            .trim_matches('"')
            .trim_end_matches(|c| c == '\\' || c == '/')
    }

    /// Case-insensitive, quote- and trailing-slash-insensitive comparison of a PATH entry.
    fn entry_matches_dir(entry: &str, dir: &str) -> bool {
        let entry = normalize_entry(entry);
        !entry.is_empty() && entry.eq_ignore_ascii_case(normalize_entry(dir))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn repair_prioritizes_current_app_over_shadowing_command() {
            let root = tempfile::tempdir().unwrap();
            let foreign = root.path().join("foreign");
            let current = root.path().join("current");
            std::fs::create_dir_all(&foreign).unwrap();
            std::fs::create_dir_all(&current).unwrap();
            std::fs::write(foreign.join("voicetypr.exe"), b"foreign").unwrap();
            std::fs::write(current.join("voicetypr.exe"), b"current").unwrap();

            let foreign = foreign.to_string_lossy();
            let current = current.to_string_lossy();
            let shadowed = format!("{foreign};{current}");
            assert!(resolved_command_dir(&shadowed, &current)
                .is_some_and(|entry| entry_matches_dir(entry, &foreign)));

            let repaired = prioritize_dir(&shadowed, &current);
            assert!(resolved_command_dir(&repaired, &current)
                .is_some_and(|entry| entry_matches_dir(entry, &current)));
            assert_eq!(repaired, format!("{current};{foreign}"));
        }

        #[test]
        fn prioritizing_an_already_first_directory_is_a_noop() {
            let current = r"C:\Program Files\Voicetypr";
            let other = r"C:\Tools";
            let path = format!("{current};{other}");
            assert_eq!(prioritize_dir(&path, current), path);
        }
    }

    fn broadcast_env_change() {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        // UTF-16 "Environment" payload; must outlive the synchronous SendMessageTimeoutW call.
        let target: Vec<u16> = "Environment\0".encode_utf16().collect();
        // SAFETY: user32 FFI; target buffer is valid for the duration of this blocking call.
        unsafe {
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(target.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        }
    }
}
