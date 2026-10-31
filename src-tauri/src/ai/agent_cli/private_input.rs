//! Last-mile conversion of in-memory invocation options to argv-free content.
//! Never add dynamic input to the environment. The owning directory must stay
//! alive until the child exits (including a prepared Claude spare).
use super::*;
use std::io::Write;
use std::sync::OnceLock;
use std::time::SystemTime;

const DIRECTORY_PREFIX: &str = "voicetypr-agent-";
const STALE_AFTER: Duration = Duration::from_secs(15 * 60);
static BASE_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();

/// Set before any GUI/CLI Polish work. Filesystem cleanup never blocks startup.
pub(crate) fn initialize(local_data_directory: PathBuf) {
    let base = local_data_directory.join("agent-input");
    if BASE_DIRECTORY.set(base.clone()).is_err() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("agent-input-cleanup".to_string())
        .spawn(move || {
            let now = SystemTime::now();
            let mut removed = sweep_stale_directories(&base, now);
            let legacy_base = std::env::temp_dir();
            if legacy_base != base {
                removed += sweep_stale_directories(&legacy_base, now);
            }
            log::info!("Agent input cleanup removed {} stale directories", removed);
        });
}

fn sweep_stale_directories(base: &Path, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(base) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| {
            if !entry
                .file_name()
                .to_string_lossy()
                .starts_with(DIRECTORY_PREFIX)
            {
                return false;
            }
            // Do not follow symlinks, or remove fresh/future-dated directories
            // that may belong to an app or CLI running alongside this process.
            let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
                return false;
            };
            let stale = metadata
                .modified()
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > STALE_AFTER);
            metadata.is_dir() && stale && std::fs::remove_dir_all(entry.path()).is_ok()
        })
        .count()
}

fn prepare_base(base: &Path) -> Result<(), MappedAiProviderError> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(base).map_err(|_| failure())?;
    let metadata = std::fs::symlink_metadata(base).map_err(|_| failure())?;
    if !metadata.is_dir() {
        return Err(failure());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(base, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| failure())?;
    }
    // Windows inherits the per-user LocalAppData ACL. Explicit owner-only
    // protected DACLs need Security_Authorization features we do not enable;
    // a user-customized broad AppData ACL remains a residual privacy risk.
    Ok(())
}

pub(super) fn private_directory() -> Result<TempDir, MappedAiProviderError> {
    private_directory_in(BASE_DIRECTORY.get().map(PathBuf::as_path))
}

fn private_directory_in(base: Option<&Path>) -> Result<TempDir, MappedAiProviderError> {
    if let Some(base) = base {
        prepare_base(base)?;
    }
    // Tests and callers before startup initialization retain the legacy fallback.
    let fallback = std::env::temp_dir();
    let mut builder = tempfile::Builder::new();
    builder.prefix(DIRECTORY_PREFIX);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder
        .tempdir_in(base.unwrap_or(&fallback))
        .map_err(|_| failure())
}

pub(super) fn private_argv(
    spec: &AgentCliSpec,
    argv: &[String],
    directory: &Path,
) -> Result<Vec<String>, MappedAiProviderError> {
    let mut safe = argv.to_vec();
    match spec.system_prompt {
        SystemPromptPolicy::None => {}
        SystemPromptPolicy::Flag(flag) => {
            if let Some(index) = safe.iter().position(|arg| arg == flag) {
                let prompt = safe.get(index + 1).ok_or_else(failure)?;
                let path = write_private(directory, prompt.as_bytes())?;
                if spec.provider_id == PROVIDER_CLAUDE_CODE {
                    safe[index] = "--system-prompt-file".to_string();
                }
                // pi/omp resolve an existing file passed to --system-prompt.
                safe[index + 1] = path;
            }
        }
        SystemPromptPolicy::CodexConfig => {
            for arg in &mut safe {
                if let Some(encoded) = arg.strip_prefix("developer_instructions=") {
                    let prompt: String = serde_json::from_str(encoded).map_err(|_| failure())?;
                    let path = write_private(directory, prompt.as_bytes())?;
                    // Codex's supported instruction-file option replaces its
                    // coding instructions with our dictation instructions at
                    // system level, never demoting them to untrusted user text.
                    *arg = format!(
                        "model_instructions_file={}",
                        serde_json::to_string(&path).map_err(|_| failure())?
                    );
                }
            }
        }
    }
    Ok(safe)
}

fn write_private(directory: &Path, content: &[u8]) -> Result<String, MappedAiProviderError> {
    // NamedTempFile uses create_new and 0600 on Unix, regardless of umask.
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|_| failure())?;
    file.write_all(content).map_err(|_| failure())?;
    let (_, path) = file.keep().map_err(|_| failure())?;
    path.into_os_string().into_string().map_err(|_| failure())
}

fn failure() -> MappedAiProviderError {
    MappedAiProviderError::new(AiProviderError::Internal)
}

/// Spawn the binary, deliver `input_text` per `input_mode`, wait for exit, and
/// parse the polish result from stdout. A non-zero exit is always an error,
/// even when stdout looks like a successful JSON response.
pub(super) async fn cold_spawn_and_collect(
    spec: &AgentCliSpec,
    binary_path: &Path,
    argv: &[String],
    input_mode: InputMode,
    input_text: &str,
    output: OutputParser,
) -> Result<String, MappedAiProviderError> {
    if input_mode == InputMode::Acp {
        return opencode_acp::run(binary_path, argv, input_text).await;
    }
    let private_dir = private_directory()?;
    let safe_argv = private_input::private_argv(spec, argv, private_dir.path())?;
    let mut command = Command::new(binary_path);
    command.args(&safe_argv);

    match input_mode {
        InputMode::Stdin | InputMode::Acp => {}
        InputMode::PositionalArg => {
            // OpenCode 1.x compatibility: known local process-list exposure.
            // 2.x uses ACP below; never extend positional input to other CLIs.
            command.arg(input_text);
        }
    }
    command.env("PATH", resolved_path(false).await);
    command.env("ZSH_TMUX_AUTOSTART", "false");
    for (name, value) in spec.static_env {
        command.env(name, value);
    }
    apply_claude_effort_env(&mut command, spec, argv);
    apply_no_window(&mut command);
    #[cfg(test)]
    if input_mode != InputMode::PositionalArg {
        assert_no_canaries(&command);
    }

    let capture = run_isolated_command(
        command,
        (input_mode == InputMode::Stdin).then_some(input_text.as_bytes()),
        cold_spawn_timeout(spec.provider_id),
        MAX_CAPTURE_BYTES,
    )
    .await
    .map_err(map_process_failure)?;

    if !capture.status.success() {
        let detail = extract_process_error(output, &capture.stdout, &capture.stderr);
        log::warn!(
            "agent-cli polish exited non-zero ({}); returning bounded CLI error",
            capture.status
        );
        return Err(MappedAiProviderError::new(AiProviderError::AgentCli(
            detail,
        )));
    }

    parse_cli_output(output, capture.complete_stdout()?)
}

#[cfg(test)]
pub(super) fn assert_no_canaries(command: &Command) {
    let command = command.as_std();
    for value in command
        .get_args()
        .chain(command.get_envs().filter_map(|(_, value)| value))
    {
        for canary in ["SYSTEM_CANARY_082", "TRANSCRIPT_CANARY_082"] {
            assert!(!value.to_string_lossy().contains(canary));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_directories_use_the_configured_base_and_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("agent-input");
        let directory = private_directory_in(Some(&base)).unwrap();
        assert_eq!(directory.path().parent(), Some(base.as_path()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&base).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        let not_a_directory = root.path().join("file");
        std::fs::write(&not_a_directory, []).unwrap();
        assert!(private_directory_in(Some(&not_a_directory)).is_err());
    }

    #[test]
    fn startup_sweep_removes_only_old_matching_directories() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join(format!("{DIRECTORY_PREFIX}old"));
        let fresh = root.path().join(format!("{DIRECTORY_PREFIX}fresh"));
        let unrelated = root.path().join("unrelated");
        let matching_file = root.path().join(format!("{DIRECTORY_PREFIX}file"));
        for directory in [&old, &fresh, &unrelated] {
            std::fs::create_dir(directory).unwrap();
        }
        std::fs::write(old.join("instructions"), "private").unwrap();
        std::fs::write(&matching_file, "keep").unwrap();
        let now = SystemTime::now();
        let mut options = std::fs::File::options();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows::Win32::Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_WRITE_ATTRIBUTES,
            };
            options
                .access_mode(FILE_WRITE_ATTRIBUTES.0)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0);
        }
        for directory in [&old, &unrelated] {
            options
                .open(directory)
                .unwrap()
                .set_modified(now - STALE_AFTER - Duration::from_secs(1))
                .unwrap();
        }
        assert_eq!(sweep_stale_directories(root.path(), now), 1);
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(unrelated.exists());
        assert!(matching_file.exists());
        // Future timestamps and clock rollback also fail safe.
        assert_eq!(sweep_stale_directories(root.path(), now - STALE_AFTER), 0);
        assert!(fresh.exists());
    }

    #[test]
    fn every_spawn_variant_keeps_private_content_out_of_args_and_env() {
        for spec in [&CLAUDE_CODE_SPEC, &PI_SPEC, &OMP_SPEC, &CODEX_SPEC] {
            for fast in [false, true] {
                let directory = private_directory().unwrap();
                let argv = cold_argv_for_model_with_options(
                    spec,
                    "SYSTEM_CANARY_082\nUnicode: café",
                    "",
                    ClaudeCapabilities::default(),
                    None,
                    fast,
                );
                let safe = private_argv(spec, &argv, directory.path()).unwrap();
                let mut command = Command::new("fake-cli");
                command.args(&safe);
                for (key, value) in spec.static_env {
                    command.env(key, value);
                }
                let built = command.as_std();
                for arg in built.get_args() {
                    assert!(!arg.to_string_lossy().contains("SYSTEM_CANARY_082"));
                    assert!(!arg.to_string_lossy().contains("TRANSCRIPT_CANARY_082"));
                }
                for (_, value) in built.get_envs() {
                    assert!(!value
                        .unwrap_or_default()
                        .to_string_lossy()
                        .contains("SYSTEM_CANARY_082"));
                }
                let files = std::fs::read_dir(directory.path())
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                assert_eq!(files.len(), 1);
                let path = files[0].path();
                assert_eq!(
                    std::fs::read_to_string(&path).unwrap(),
                    "SYSTEM_CANARY_082\nUnicode: café"
                );
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(
                        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                        0o600
                    );
                    assert_eq!(
                        std::fs::metadata(directory.path())
                            .unwrap()
                            .permissions()
                            .mode()
                            & 0o777,
                        0o700
                    );
                }
                drop(directory);
                assert!(!path.exists());
            }
        }
    }
}
