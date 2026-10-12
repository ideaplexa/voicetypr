//! Cursor's documented headless contract: print + JSON, read-only ask mode,
//! sandbox enabled, explicit empty workspace. Prompt is a discrete positional
//! argument as documented at https://cursor.com/docs/cli/headless.
use super::agent_cli::{
    discover_capabilities_for_polish, named_cli_default, required_capabilities_present,
    resolve_binary_names, AgentCliModel, AgentCliSpec, BinaryResolution, InputMode, OutputParser,
    ReasoningPolicy, SystemPromptPolicy,
};
use super::error::{AiProviderError, MappedAiProviderError};
use super::providers::PROVIDER_CURSOR;
use tokio::sync::Mutex;

// Detection, model discovery and polish share this validated absolute path.
// Explicit provider Refresh invalidates it along with cached help.
static BINARY_RESOLUTION: Mutex<Option<BinaryResolution>> = Mutex::const_new(None);

pub(crate) async fn invalidate_resolution() {
    *BINARY_RESOLUTION.lock().await = None;
}

pub(super) async fn resolve_binary(refresh_path: bool) -> BinaryResolution {
    let mut cached = BINARY_RESOLUTION.lock().await;
    if !refresh_path {
        if let Some(resolution) = cached.as_ref() {
            return resolution.clone();
        }
    }
    let resolution = match resolve_binary_names([CURSOR_SPEC.binary], refresh_path).await {
        BinaryResolution::Found(path) => {
            let (_, help, _) = discover_capabilities_for_polish(&path, &CURSOR_SPEC, false).await;
            validate_binary_path(path, &help)
        }
        resolution => resolution,
    };
    *cached = Some(resolution.clone());
    resolution
}

/// Validate an already resolved cursor-agent candidate before non-help work.
pub(super) fn validate_binary_path(path: std::path::PathBuf, help: &[u8]) -> BinaryResolution {
    if binary_is_cursor(help) {
        BinaryResolution::Found(path)
    } else {
        BinaryResolution::Missing
    }
}

fn binary_is_cursor(help: &[u8]) -> bool {
    String::from_utf8_lossy(help)
        .lines()
        .any(|line| line.trim() == "Start the Cursor Agent")
        && required_capabilities_present(&CURSOR_SPEC, help)
}

pub(crate) const CURSOR_SPEC: AgentCliSpec = AgentCliSpec {
    provider_id: PROVIDER_CURSOR,
    binary: "cursor-agent",
    binary_aliases: &[],
    help_argv: &["--help"],
    cold_argv_prefix: &["-p", "--output-format", "json"],
    cold_argv_suffix: &[],
    required_isolation_args: &["--mode", "ask", "--sandbox", "enabled", "--trust"],
    required_capability_flags: &[
        "--print",
        "--output-format",
        "--mode",
        "--sandbox",
        "--trust",
        "--workspace",
        "--model",
    ],
    default_model: None,
    model_flag: Some("--model"),
    reasoning: ReasoningPolicy::None,
    system_prompt: SystemPromptPolicy::None,
    input_mode: InputMode::PositionalArg,
    output: OutputParser::CursorJson,
    static_env: &[],
};

/// Keep the workspace alive until the child exits; it starts with no project
/// rules, sources or MCP configuration.
pub(crate) fn apply_workspace(
    command: &mut tokio::process::Command,
) -> Result<tempfile::TempDir, MappedAiProviderError> {
    let directory = tempfile::TempDir::new()
        .map_err(|_| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    command.arg("--workspace").arg(directory.path());
    Ok(directory)
}

/// Place the optional model after the explicit workspace, matching the
/// documented headless invocation. Never pass a model argument through a shell.
pub(crate) fn configure_command(
    command: &mut tokio::process::Command,
    argv: &[String],
) -> Result<tempfile::TempDir, MappedAiProviderError> {
    let model_index = argv.iter().position(|arg| arg == "--model");
    if let Some(index) = model_index {
        command.args(&argv[..index]).args(&argv[index + 2..]);
    } else {
        command.args(argv);
    }
    let directory = apply_workspace(command)?;
    if let Some(index) = model_index {
        command.args(&argv[index..index + 2]);
    }
    Ok(directory)
}

/// Cursor `models` emits `id - Display Name (current, default)` rows under
/// "Available models". Strip terminal colors before parsing exact ids.
pub(crate) fn parse_models(stdout: &[u8]) -> Result<Vec<AgentCliModel>, MappedAiProviderError> {
    let text = String::from_utf8_lossy(stdout);
    let mut clean = String::with_capacity(text.len());
    let mut escape = false;
    for ch in text.chars() {
        if ch == '\u{1b}' {
            escape = true;
            continue;
        }
        if escape {
            if ch.is_ascii_alphabetic() {
                escape = false;
            }
            continue;
        }
        clean.push(ch);
    }
    let mut models = vec![named_cli_default("Cursor", None, None)];
    let mut in_models = false;
    for line in clean.lines().map(str::trim) {
        if line == "Available models" {
            in_models = true;
            continue;
        }
        if !in_models || line.is_empty() {
            continue;
        }
        if line.starts_with("Tip:") {
            break;
        }
        let (id, name) = line.split_once(" - ").unwrap_or((line, line));
        let id = id.split_whitespace().next().unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let name = name.split(" (").next().unwrap_or(name);
        models.push(AgentCliModel {
            id: id.to_string(),
            name: name.to_string(),
            recommended: false,
            reasoning: false,
            context_window: None,
            source_provider: None,
            cli_default: false,
        });
    }
    if models.len() == 1 {
        return Err(MappedAiProviderError::new(AiProviderError::BadResponse));
    }
    Ok(models)
}

/// Cursor promises a result field; unlike Claude's legacy content fallback,
/// missing or empty result is an error, not text to paste.
pub(crate) fn parse_result(stdout: &[u8]) -> Result<String, MappedAiProviderError> {
    let value: serde_json::Value = serde_json::from_slice(stdout)
        .map_err(|_| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    if value.get("is_error").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err(MappedAiProviderError::new(AiProviderError::BadResponse));
    }
    value
        .get("result")
        .and_then(serde_json::Value::as_str)
        .filter(|result| !result.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_model_listing_and_preserves_ids() {
        let models = parse_models(b"Available models\n\n\x1b[36mcomposer-2.5\x1b[0m - Composer 2.5 (current, default)\ngpt-5 - GPT-5\n\nTip: use --model <id>\n").unwrap();
        assert_eq!(models[0].id, "");
        assert_eq!(models[1].id, "composer-2.5");
        assert_eq!(models[1].name, "Composer 2.5");
        assert_eq!(models.len(), 3);
        assert!(parse_models(b"Authentication required").is_err());
    }
    #[test]
    fn requires_nonempty_result() {
        assert_eq!(
            parse_result(br#"{"result":"Ship today."}"#).unwrap(),
            "Ship today."
        );
        for payload in [
            br#"{"content":"not result"}"#.as_slice(),
            br#"{"result":""}"#,
            br#"{"result":"error","is_error":true}"#,
        ] {
            assert!(parse_result(payload).is_err());
        }
    }
}
