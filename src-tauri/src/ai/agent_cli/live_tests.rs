use super::*;

#[tokio::test]
#[ignore = "requires VOICETYPR_AGENT_CLI_PROVIDER plus an installed, authenticated CLI"]
async fn real_configured_agent_cli_round_trip() {
    let provider = std::env::var("VOICETYPR_AGENT_CLI_PROVIDER")
        .expect("set VOICETYPR_AGENT_CLI_PROVIDER to an agent-cli provider id");
    assert!(
        spec_for(&provider).is_some(),
        "unknown agent-cli provider: {provider}"
    );
    let runtime = AgentCliRuntime::new();
    let request = AiPolishRequest {
        provider_id: provider,
        model_id: String::new(),
        reasoning_level: Some("low".to_string()),
        fast_mode: false,
        input_text: "reply okay".to_string(),
        needs_output_language_transform: false,
        prompt: "Return exactly OK. Do not use tools.".to_string(),
        timeout_ms: 9_000,
    };
    let polished = runtime
        .polish(&request)
        .await
        .expect("configured agent CLI should complete its exact adapter invocation");
    println!("adapter output: {polished:?}");
    assert!(!polished.trim().is_empty());
}

/// A real `claude` round-trip is gated behind `#[ignore]` — it requires the
/// CLI installed + authenticated and burns subscription quota, so it never
/// runs in CI. Run locally with `cargo test agent_cli -- --ignored`.
#[tokio::test]
#[ignore = "requires claude CLI installed + authenticated; not run in CI"]
async fn real_claude_code_cold_spawn_round_trip() {
    let runtime = AgentCliRuntime::new();
    let request = AiPolishRequest {
        provider_id: PROVIDER_CLAUDE_CODE.to_string(),
        model_id: String::new(),
        reasoning_level: Some("low".to_string()),
        fast_mode: false,
        input_text: "uhh so basically like um lets fix the bug".to_string(),
        needs_output_language_transform: false,
        prompt:
            "Clean up this voice dictation into clear written English. Output only the fixed text."
                .to_string(),
        timeout_ms: 9_000,
    };
    let result = runtime.polish(&request).await;
    let polished = result.expect("claude cold-spawn polish should succeed locally");
    assert!(!polished.trim().is_empty());
    println!("claude-code polished output: {polished}");
}

/// A real `pi` round-trip — gated behind `#[ignore]` (requires the CLI
/// installed + authenticated to a provider, burns quota, and the first
/// `resolve_binary` triggers the login-shell PATH probe). Pi reads stdin in
/// plain one-shot print mode. Run with `cargo test agent_cli -- --ignored`.
#[tokio::test]
#[ignore = "requires pi CLI installed + authenticated; not run in CI"]
async fn real_pi_cold_spawn_round_trip() {
    let runtime = AgentCliRuntime::new();
    let request = AiPolishRequest {
        provider_id: PROVIDER_PI.to_string(),
        model_id: String::new(),
        reasoning_level: Some("off".to_string()),
        fast_mode: false,
        input_text: "uhh so basically like um lets fix the bug".to_string(),
        needs_output_language_transform: false,
        prompt:
            "Clean up this voice dictation into clear written English. Output only the fixed text."
                .to_string(),
        timeout_ms: 9_000,
    };
    let result = runtime.polish(&request).await;
    let polished = result.expect("pi cold-spawn polish should succeed locally");
    assert!(!polished.trim().is_empty());
    println!("pi polished output: {polished}");
}

/// A real `omp` (oh-my-pi) round-trip — gated behind `#[ignore]`. Empirically
/// omp takes dictation as a positional argv arg (it did not read stdin in
/// `--mode json`). Run with `cargo test agent_cli -- --ignored`.
#[tokio::test]
#[ignore = "requires omp CLI installed + authenticated; not run in CI"]
async fn real_omp_cold_spawn_round_trip() {
    let runtime = AgentCliRuntime::new();
    let request = AiPolishRequest {
        provider_id: PROVIDER_OMP.to_string(),
        model_id: String::new(),
        reasoning_level: Some("off".to_string()),
        fast_mode: false,
        input_text: "hello; echo $HOME $(whoami)".to_string(),
        needs_output_language_transform: false,
        prompt:
            "Clean up this voice dictation into clear written English. Output only the fixed text."
                .to_string(),
        timeout_ms: 9_000,
    };
    let result = runtime.polish(&request).await;
    let polished = result.expect("omp cold-spawn polish should succeed locally");
    assert!(!polished.trim().is_empty());
    assert!(
        polished.contains("echo $HOME $(whoami)"),
        "omp must preserve shell metacharacters as literal content: {polished}"
    );
    println!("omp polished output: {polished}");
}

/// A real `droid exec` round-trip — gated behind `#[ignore]`. Verifies the
/// documented `--restrict-tools` isolation contract (an unknown
/// `--enabled-tools` would leave default tools on). Run with
/// `cargo test agent_cli -- --ignored`.
#[tokio::test]
#[ignore = "requires droid CLI installed + authenticated; not run in CI"]
async fn real_droid_cold_spawn_round_trip() {
    let runtime = AgentCliRuntime::new();
    let request = AiPolishRequest {
        provider_id: PROVIDER_DROID.to_string(),
        model_id: String::new(),
        reasoning_level: Some("low".to_string()),
        fast_mode: false,
        input_text: "hello; echo $HOME $(whoami)".to_string(),
        needs_output_language_transform: false,
        prompt:
            "Clean up this voice dictation into clear written English. Output only the fixed text."
                .to_string(),
        timeout_ms: 9_000,
    };
    let result = runtime.polish(&request).await;
    let polished = result.expect("droid cold-spawn polish should succeed locally");
    assert!(!polished.trim().is_empty());
    assert!(
        polished.contains("echo $HOME $(whoami)"),
        "droid must preserve shell metacharacters as literal content: {polished}"
    );
    println!("droid polished output: {polished}");
}
