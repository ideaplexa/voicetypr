//! Catalog-refresh selection guards and Claude aliases.
use super::*;

pub(super) fn apply_claude_effort_env(command: &mut Command, spec: &AgentCliSpec, argv: &[String]) {
    if spec.provider_id == PROVIDER_CLAUDE_CODE {
        // The inherited environment can outrank --effort. Override it for this
        // subprocess only, including the conservative help-probe fallback.
        let level = argv
            .windows(2)
            .find(|pair| pair[0] == "--effort")
            .map(|pair| pair[1].as_str())
            .unwrap_or("low");
        command.env("CLAUDE_CODE_EFFORT_LEVEL", level);
    }
}

pub(super) fn validate_discovered_selection(
    models: &[AgentCliModel],
    selected: &str,
) -> Result<(), MappedAiProviderError> {
    if models.iter().any(|model| model.id == selected) {
        return Ok(());
    }
    Err(MappedAiProviderError::new(AiProviderError::AgentCli(
        "The saved model is no longer available. Choose a model in Polish settings, or update your CLI default.".to_string(),
    )))
}

pub(super) fn curated_claude_models() -> Vec<AgentCliModel> {
    let mut models = vec![named_cli_default("Claude", None, None)];
    models.extend(
        [
            ("fable", "Fable 5.1"),
            ("opus", "Opus 5.5"),
            ("sonnet", "Sonnet 5.5"),
        ]
        .into_iter()
        .map(|(id, name)| AgentCliModel {
            id: id.to_string(),
            name: name.to_string(),
            recommended: false,
            reasoning: false,
            context_window: None,
            source_provider: None,
            cli_default: false,
        }),
    );
    models
}

pub(super) fn parse_pi_default_model(
    stdout: &[u8],
) -> Result<AgentCliModel, MappedAiProviderError> {
    let value = find_pi_response(stdout, PI_STATE_RESPONSE_ID)
        .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    let model = value
        .get("data")
        .and_then(|data| data.get("model"))
        .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    let model_id = model
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    if let Ok(available) = parse_pi_models(stdout) {
        let selector = format!(
            "{}/{}",
            model
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            model_id
        );
        if !available.iter().any(|model| model.id == selector) {
            return Err(MappedAiProviderError::new(AiProviderError::InvalidModel));
        }
    }
    let name = model
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| humanize_cli_model_id(model_id));
    let source_provider = model
        .get("provider")
        .and_then(Value::as_str)
        .filter(|provider| !provider.trim().is_empty())
        .map(str::to_string);
    Ok(named_cli_default("Pi", Some(&name), source_provider))
}

pub(super) fn parse_omp_default_model(
    stdout: &[u8],
    models: &[AgentCliModel],
) -> Result<AgentCliModel, MappedAiProviderError> {
    let text = String::from_utf8_lossy(stdout);
    let payload = extract_json_payload(&text).unwrap_or(text.trim());
    let value: Value = serde_json::from_str(payload)
        .map_err(|_| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    let configured = value
        .get("value")
        .and_then(|roles| roles.get("default"))
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    let selector = strip_reasoning_suffix(configured);
    let matched = models.iter().find(|model| model.id == selector);
    if matched.is_none() {
        return Err(MappedAiProviderError::new(AiProviderError::InvalidModel));
    }
    let name = matched
        .map(|model| model.name.clone())
        .unwrap_or_else(|| humanize_cli_model_id(selector));
    let source_provider = matched
        .and_then(|model| model.source_provider.clone())
        .or_else(|| {
            selector
                .split_once('/')
                .map(|(provider, _)| provider.to_string())
        });
    Ok(named_cli_default("oh-my-pi", Some(&name), source_provider))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_effort_overrides_inherited_env_without_changing_other_agents() {
        for (level, expected) in [
            (None, "low"),
            (Some("off"), "low"),
            (Some("medium"), "medium"),
        ] {
            let argv = cold_argv_for_model_with_reasoning(
                &CLAUDE_CODE_SPEC,
                "prompt",
                "",
                ClaudeCapabilities {
                    safe_mode: true,
                    effort: true,
                },
                level,
            );
            let mut command = Command::new("claude");
            apply_claude_effort_env(&mut command, &CLAUDE_CODE_SPEC, &argv);
            assert!(command
                .as_std()
                .get_envs()
                .any(|(key, value)| key == "CLAUDE_CODE_EFFORT_LEVEL"
                    && value == Some(std::ffi::OsStr::new(expected))));
        }
        let mut command = Command::new("claude");
        apply_claude_effort_env(&mut command, &CLAUDE_CODE_SPEC, &[]);
        assert!(command
            .as_std()
            .get_envs()
            .any(|(key, value)| key == "CLAUDE_CODE_EFFORT_LEVEL"
                && value == Some(std::ffi::OsStr::new("low"))));
    }

    #[test]
    fn claude_default_is_sonnet_and_saved_haiku_remains_accepted() {
        let capabilities = ClaudeCapabilities {
            safe_mode: true,
            effort: true,
        };
        for (selected, expected) in [("", "sonnet"), ("haiku", "haiku"), ("fable", "fable")] {
            let argv = cold_argv_for_model(&CLAUDE_CODE_SPEC, "prompt", selected, capabilities);
            assert!(argv
                .windows(2)
                .any(|pair| pair[0] == "--model" && pair[1] == expected));
        }
    }

    #[test]
    fn pi_and_omp_removed_selections_require_reselection() {
        let pi = br#"{"id":"voicetypr-state","data":{"model":{"provider":"openai-codex","id":"gpt-5.6-luna"}}}
{"id":"voicetypr-models","data":{"models":[{"provider":"openai-codex","id":"gpt-6-luna","name":"GPT-6 Luna"}]}}"#;
        assert_eq!(
            parse_pi_default_model(pi).unwrap_err().error,
            AiProviderError::InvalidModel
        );
        let models = parse_pi_models(pi).unwrap();
        assert!(validate_discovered_selection(&models, "openai-codex/gpt-5.6-luna").is_err());
        assert!(validate_discovered_selection(&models, "").is_err());
        assert!(validate_discovered_selection(&models, "openai-codex/gpt-6-luna").is_ok());
        assert!(parse_omp_default_model(
            br#"{"value":{"default":"openai-codex/gpt-5.6-luna:high"}}"#,
            &models
        )
        .is_err());
    }
}
