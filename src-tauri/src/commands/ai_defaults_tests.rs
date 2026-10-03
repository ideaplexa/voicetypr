use super::*;

#[test]
fn missing_fast_mode_defaults_only_codex_on_and_preserves_saved_choices() {
    let mut values = HashMap::new();
    apply_agent_cli_fast_mode_defaults(&mut values);
    for provider in AGENT_CLI_PROVIDER_IDS {
        assert_eq!(values[*provider], *provider == "codex");
    }
    values.insert("codex".to_string(), false);
    values.insert("claude-code".to_string(), true);
    apply_agent_cli_fast_mode_defaults(&mut values);
    assert!(!values["codex"]);
    assert!(values["claude-code"]);
}

#[test]
fn codex_reasoning_defaults_to_medium_and_preserves_explicit_low() {
    assert_eq!(default_agent_cli_reasoning("codex"), "medium");
    assert_eq!(normalize_agent_cli_reasoning("codex", "low"), "low");
    assert_eq!(normalize_agent_cli_reasoning("codex", "medium"), "medium");
    assert_eq!(normalize_agent_cli_reasoning("codex", "invalid"), "medium");
}
