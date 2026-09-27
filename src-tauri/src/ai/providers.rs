use super::contract::{AiModel, AiProvider};

pub const PROVIDER_CUSTOM: &str = "custom";
pub const PROVIDER_OPENROUTER: &str = "openrouter";
/// Claude Code (Anthropic's local coding CLI) — subscription-authenticated via
/// the `claude` binary; no API key. Polished via cold-spawn (Phase 4C-i).
pub const PROVIDER_CLAUDE_CODE: &str = "claude-code";
/// pi — multi-provider coding CLI (pi). Subscription/authenticated via the
/// `pi` binary; no API key. Polished via cold-spawn, stdin input, JSONL output.
pub const PROVIDER_PI: &str = "pi";
/// oh-my-pi (omp) — multi-provider coding CLI. Subscription/authenticated via
/// the `omp` binary; no API key. Polished via cold-spawn, positional-arg input.
pub const PROVIDER_OMP: &str = "omp";
pub const PROVIDER_CODEX: &str = "codex";
pub const PROVIDER_DROID: &str = "droid";
pub const PROVIDER_GROK: &str = "grok";
pub const PROVIDER_OPENCODE: &str = "opencode";
pub const PROVIDER_CLINE: &str = "cline";

pub const AGENT_CLI_PROVIDER_IDS: &[&str] = &[
    PROVIDER_CLAUDE_CODE,
    PROVIDER_PI,
    PROVIDER_OMP,
    PROVIDER_CODEX,
    PROVIDER_DROID,
    PROVIDER_GROK,
    PROVIDER_OPENCODE,
    PROVIDER_CLINE,
];

pub fn launch_providers() -> Vec<AiProvider> {
    crate::ai::catalog::launch_providers()
}

pub fn recommended_models(provider_id: &str) -> Vec<AiModel> {
    crate::ai::catalog::recommended_models(provider_id)
}
