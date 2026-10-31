//! Last-mile conversion of in-memory invocation options to argv-free content.
//! Never add dynamic input to the environment. The owning directory must stay
//! alive until the child exits (including a prepared Claude spare).
use super::*;
use std::io::Write;

pub(super) fn private_directory() -> Result<TempDir, MappedAiProviderError> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("voicetypr-agent-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder.tempdir().map_err(|_| failure())
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
