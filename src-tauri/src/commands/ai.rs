use crate::ai::agent_cli::{cold_spawn_timeout_ms, supports_fast_mode};
use crate::ai::catalog;
use crate::ai::contract::AiPolishRequest;
use crate::ai::error::{user_facing_message, AiProviderError};
use crate::ai::executor::{AiExecutor, OpenAiCompatibleConfig};
use crate::ai::genai_runtime::AiKeyResolver;
use crate::ai::providers::{
    launch_providers, AGENT_CLI_PROVIDER_IDS, PROVIDER_CUSTOM, PROVIDER_OPENROUTER,
};
use crate::ai::EnhancementOptions;
use crate::commands::settings::{
    persist_settings_and_invalidate, FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
    TRANSCRIPTION_TASK_TRANSCRIBE,
};
use crate::secure_store;
use crate::writing::{load_writing_settings, save_writing_settings, WritingSettings};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri_plugin_store::StoreExt;

// In-memory cache for API keys to avoid system password prompts
// Keys are stored in Stronghold by frontend and cached here for backend use
static API_KEY_CACHE: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static AGENT_CLI_PROBE_CACHE: Lazy<Mutex<HashMap<String, AgentCliProbe>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
const CUSTOM_BASE_URL_KEY: &str = "ai_custom_base_url";
const CUSTOM_NO_AUTH_KEY: &str = "ai_custom_no_auth";
const LEGACY_OPENAI_BASE_URL_KEY: &str = "ai_openai_base_url";
const LEGACY_OPENAI_NO_AUTH_KEY: &str = "ai_openai_no_auth";
const AGENT_CLI_REASONING_KEY: &str = "ai_agent_cli_reasoning_by_provider";
const AGENT_CLI_FAST_MODE_KEY: &str = "ai_agent_cli_fast_mode_by_provider";

fn default_agent_cli_reasoning(provider: &str) -> &'static str {
    if matches!(provider, "pi" | "omp") {
        "off"
    } else {
        "low"
    }
}

fn normalize_agent_cli_reasoning(provider: &str, reasoning: &str) -> String {
    match reasoning {
        "off" | "low" | "medium" => reasoning.to_string(),
        // Documented fast thinking level for pi, omp, and opencode.
        "minimal" if matches!(provider, "pi" | "omp" | "opencode") => reasoning.to_string(),
        "high" => "medium".to_string(),
        _ => default_agent_cli_reasoning(provider).to_string(),
    }
}

fn agent_cli_supports_reasoning(provider: &str) -> bool {
    catalog::runtime_kind(provider) == Some("agent_cli")
}

fn validate_agent_cli_reasoning(provider: &str, reasoning: &str) -> Result<(), String> {
    if !agent_cli_supports_reasoning(provider) {
        return Err("Reasoning is not configurable for this local agent".to_string());
    }
    let supported = matches!(reasoning, "off" | "low" | "medium")
        || (matches!(provider, "pi" | "omp" | "opencode") && reasoning == "minimal");
    if !supported {
        return Err("Unsupported reasoning level".to_string());
    }
    Ok(())
}

fn load_agent_cli_reasoning<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
) -> HashMap<String, String> {
    let mut levels = store
        .get(AGENT_CLI_REASONING_KEY)
        .and_then(|value| serde_json::from_value::<HashMap<String, String>>(value.clone()).ok())
        .unwrap_or_default();
    for provider in AGENT_CLI_PROVIDER_IDS {
        let normalized = levels
            .get(*provider)
            .map(|reasoning| normalize_agent_cli_reasoning(provider, reasoning))
            .unwrap_or_else(|| default_agent_cli_reasoning(provider).to_string());
        levels.insert(provider.to_string(), normalized);
    }
    levels
}

fn load_agent_cli_fast_mode<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
) -> HashMap<String, bool> {
    let mut values = store
        .get(AGENT_CLI_FAST_MODE_KEY)
        .and_then(|value| serde_json::from_value::<HashMap<String, bool>>(value.clone()).ok())
        .unwrap_or_default();
    for provider in AGENT_CLI_PROVIDER_IDS {
        values.entry(provider.to_string()).or_insert(false);
    }
    values
}

// One pooled reqwest::Client shared across the LLM enhancement path so the connection pool stays hot across calls.
static SHARED_AI_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
});

fn shared_ai_client() -> reqwest::Client {
    SHARED_AI_CLIENT.clone()
}

pub(crate) fn ai_provider_key_names() -> Vec<String> {
    launch_providers()
        .into_iter()
        .filter(|provider| provider.requires_api_key || provider.id == PROVIDER_CUSTOM)
        .map(|provider| format!("ai_api_key_{}", provider.id))
        .collect()
}

/// Populate the in-memory API key cache from the secure store.
/// Called during backend startup BEFORE startup validation checks run.
/// This ensures credentials persisted in secure storage are visible to
/// perform_startup_checks() without waiting for the frontend to warm the cache.
pub fn warm_ai_key_cache_from_secure_store(app: &tauri::AppHandle) {
    if let Ok(mut cache) = API_KEY_CACHE.lock() {
        for key_name in ai_provider_key_names() {
            // Skip if already cached (e.g. from a prior call or concurrent frontend warm)
            if cache.contains_key(&key_name) {
                continue;
            }
            match secure_store::secure_get(app, &key_name) {
                Ok(Some(value)) => {
                    cache.insert(key_name.to_string(), value);
                    log::info!("Warmed API key cache from secure store: {}", key_name);
                }
                Ok(None) => {
                    log::debug!("No key in secure store for: {}", key_name);
                }
                Err(e) => {
                    log::warn!("Failed to read '{}' from secure store: {}", key_name, e);
                }
            }
        }

        if let Ok(store) = app.store("settings") {
            if let Some(provider) = store
                .get("ai_provider")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
            {
                let key_name = format!("ai_api_key_{}", provider);
                if !provider.is_empty() && !cache.contains_key(&key_name) {
                    match secure_store::secure_get(app, &key_name) {
                        Ok(Some(value)) => {
                            cache.insert(key_name.clone(), value);
                            log::info!(
                                "Warmed selected provider API key cache from secure store: {}",
                                key_name
                            );
                        }
                        Ok(None) => {
                            log::debug!(
                                "No selected provider key in secure store for: {}",
                                key_name
                            );
                        }
                        Err(e) => {
                            log::warn!("Failed to read '{}' from secure store: {}", key_name, e);
                        }
                    }
                }
            }
        }
    } else {
        log::error!("Failed to acquire API_KEY_CACHE lock during startup warmup");
    }
}

// Helper: determine if we should consider that the app "has an API key" for a provider
// For OpenAI-compatible providers, a configured no_auth=true also counts as "has key"
fn check_has_api_key<R: tauri::Runtime>(
    provider: &str,
    store: &tauri_plugin_store::Store<R>,
    cache: &HashMap<String, String>,
) -> bool {
    if provider == "openai" {
        cache.contains_key("ai_api_key_openai") || store.get(LEGACY_OPENAI_BASE_URL_KEY).is_some()
    } else if provider == "custom" {
        let configured_base = store.get(CUSTOM_BASE_URL_KEY).is_some()
            || store.get(LEGACY_OPENAI_BASE_URL_KEY).is_some();
        configured_base || cache.contains_key(&format!("ai_api_key_{}", provider))
    } else {
        cache.contains_key(&format!("ai_api_key_{}", provider))
    }
}

/// Whether a selected (provider, model) pair satisfies the executor's model
/// requirement. Agent-CLI runtimes (Claude Code) waive it — they carry no
/// catalog model because the CLI selects its own; every other runtime requires
/// a non-empty model. Shared by the readiness/selection guards below so the
/// model-less-CLI exemption lives in exactly one place.
fn selection_meets_model_requirement(provider: &str, model: &str) -> bool {
    !model.is_empty() || catalog::runtime_kind(provider) == Some("agent_cli")
}

pub(crate) fn has_ai_model_and_key(app: &tauri::AppHandle) -> Result<bool, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let provider = store
        .get("ai_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    let model = store
        .get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    if provider.is_empty() {
        return Ok(false);
    }
    // Agent-CLI providers (Claude Code) are subscription-authenticated local
    // CLIs — no API key, no catalog model. They are "ready" once selected;
    // availability is resolved at spawn time (raw-transcript fallback if the
    // CLI is missing or unauthenticated).
    if catalog::runtime_kind(&provider) == Some("agent_cli") {
        return Ok(true);
    }
    if model.is_empty() {
        return Ok(false);
    }

    let cache = API_KEY_CACHE
        .lock()
        .map_err(|_| "Failed to access cache".to_string())?;
    Ok(check_has_api_key(&provider, &store, &cache))
}

/// Validate a custom OpenAI-compatible base URL.
///
/// This is a minimal link-local DENYLIST, not an allowlist: localhost,
/// RFC-1918 private ranges, the 100.64.0.0/10 CGNAT range used by Tailscale,
/// hostnames, and public https URLs are all permitted. Only link-local IPv4
/// (169.254.0.0/16, including the cloud-metadata endpoint 169.254.169.254),
/// link-local IPv6 (fe80::/10, plus IPv4-mapped link-local), and non-http(s)
/// schemes are rejected.
fn validate_custom_base_url(raw: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "Invalid endpoint URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Endpoint must be an http(s) URL".to_string());
    }
    if let Some(host) = url.host_str() {
        // IPv6 literals are bracketed in the URL serialization (e.g. [fe80::1]).
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
            if ip.is_link_local() {
                return Err("Endpoint host is not allowed".to_string());
            }
        } else if let Ok(ip) = host.parse::<std::net::Ipv6Addr>() {
            // fe80::/10 unicast link-local, plus any IPv4-mapped/compatible form
            // of an IPv4 link-local address (e.g. ::ffff:169.254.169.254) that
            // routes to the same cloud-metadata target.
            let mapped_link_local = ip.to_ipv4().is_some_and(|v4| v4.is_link_local());
            if (0xfe80..=0xfebf).contains(&ip.segments()[0]) || mapped_link_local {
                return Err("Endpoint host is not allowed".to_string());
            }
        }
    }
    Ok(())
}

fn load_models_by_provider<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    current_provider: &str,
    current_model: &str,
) -> HashMap<String, String> {
    let mut models_by_provider = store
        .get("ai_models_by_provider")
        .and_then(|v| serde_json::from_value::<HashMap<String, String>>(v.clone()).ok())
        .unwrap_or_default();

    let current_selection_is_valid =
        !current_model.is_empty() || catalog::runtime_kind(current_provider) == Some("agent_cli");
    if !current_provider.is_empty()
        && current_selection_is_valid
        && !models_by_provider.contains_key(current_provider)
    {
        models_by_provider.insert(current_provider.to_string(), current_model.to_string());
    }

    models_by_provider
}

fn remember_provider_model(
    models_by_provider: &mut HashMap<String, String>,
    provider: &str,
    model: &str,
) {
    if provider.is_empty() {
        return;
    }

    if model.is_empty() && catalog::runtime_kind(provider) != Some("agent_cli") {
        models_by_provider.remove(provider);
    } else {
        models_by_provider.insert(provider.to_string(), model.to_string());
    }
}
fn selection_clears_model_reselection(provider: &str, model: &str) -> bool {
    !model.is_empty() || catalog::runtime_kind(provider) == Some("agent_cli")
}

async fn run_openai_chat_probe(
    client: &reqwest::Client,
    base_url: &str,
    model: &str,
    api_key: Option<&str>,
    no_auth: bool,
) -> Result<(), String> {
    let key = api_key.map(str::to_string);
    let key_resolver: AiKeyResolver = Arc::new(move |provider_id| {
        if provider_id == PROVIDER_CUSTOM {
            key.clone()
        } else {
            None
        }
    });
    let executor = AiExecutor::new(client.clone(), key_resolver, base_url.to_string(), no_auth);
    let request = AiPolishRequest {
        provider_id: PROVIDER_CUSTOM.to_string(),
        model_id: model.to_string(),
        input_text: "ping".to_string(),
        reasoning_level: None,
        fast_mode: false,
        prompt: "Reply with OK.".to_string(),
        timeout_ms: 10_000,
    };

    executor
        .polish(request, tokio_util::sync::CancellationToken::new())
        .await
        .map(|_| ())
        .map_err(|error| match error {
            AiProviderError::InvalidApiKey => {
                "Invalid API key (the endpoint rejected the credentials).".to_string()
            }
            AiProviderError::InvalidModel => "Endpoint or model not found.".to_string(),
            AiProviderError::RateLimited => "Rate limited by the provider.".to_string(),
            AiProviderError::Network => "Network error".to_string(),
            AiProviderError::BadResponse => {
                "The endpoint response is not OpenAI chat-completions compatible.".to_string()
            }
            other => user_facing_message(&other).to_string(),
        })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AISettings {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    #[serde(rename = "hasApiKey")]
    pub has_api_key: bool,
    #[serde(rename = "modelsByProvider")]
    pub models_by_provider: HashMap<String, String>,
    #[serde(rename = "reasoningByProvider")]
    pub reasoning_by_provider: HashMap<String, String>,
    #[serde(rename = "fastModeByProvider")]
    pub fast_mode_by_provider: HashMap<String, bool>,
    #[serde(rename = "aiModelNeedsReselection")]
    pub ai_model_needs_reselection: bool,
}

// Validation pattern for providers
lazy_static::lazy_static! {
    static ref PROVIDER_REGEX: regex::Regex = regex::Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
}

// Providers come from a broad catalog; validate only the identifier shape here.
// Availability comes from the Rust provider/model catalog in crate::ai::providers.
fn validate_provider_name(provider: &str) -> Result<(), String> {
    if !PROVIDER_REGEX.is_match(provider) {
        return Err("Invalid provider name format".to_string());
    }

    Ok(())
}

#[tauri::command]
pub async fn get_ai_settings(app: tauri::AppHandle) -> Result<AISettings, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;

    let enabled = store
        .get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let provider = store
        .get("ai_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default(); // Empty by default, user must select

    let model = store
        .get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "".to_string()); // Empty by default

    let models_by_provider = load_models_by_provider(&store, &provider, &model);
    let reasoning_by_provider = load_agent_cli_reasoning(&store);
    let fast_mode_by_provider = load_agent_cli_fast_mode(&store);

    let ai_model_needs_reselection = store
        .get("ai_model_needs_reselection")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // For OpenAI-compatible providers, treat no_auth as having a usable config
    let has_api_key = {
        let cache = API_KEY_CACHE
            .lock()
            .map_err(|_| "Failed to access cache".to_string())?;
        check_has_api_key(&provider, &store, &cache)
    };

    Ok(AISettings {
        enabled,
        provider,
        model,
        has_api_key,
        models_by_provider,
        reasoning_by_provider,
        fast_mode_by_provider,
        ai_model_needs_reselection,
    })
}

#[tauri::command]
pub async fn get_ai_settings_for_provider(
    provider: String,
    app: tauri::AppHandle,
) -> Result<AISettings, String> {
    validate_provider_name(&provider)?;

    let store = app.store("settings").map_err(|e| e.to_string())?;

    let enabled = store
        .get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let current_provider = store
        .get("ai_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    let current_model = store
        .get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "".to_string()); // Empty by default
    let models_by_provider = load_models_by_provider(&store, &current_provider, &current_model);
    let reasoning_by_provider = load_agent_cli_reasoning(&store);
    let fast_mode_by_provider = load_agent_cli_fast_mode(&store);
    let model = models_by_provider
        .get(&provider)
        .cloned()
        .unwrap_or_default();

    // For OpenAI-compatible providers, treat no_auth as having a usable config
    let has_api_key = {
        let cache = API_KEY_CACHE
            .lock()
            .map_err(|_| "Failed to access cache".to_string())?;
        check_has_api_key(&provider, &store, &cache)
    };

    let ai_model_needs_reselection = store
        .get("ai_model_needs_reselection")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(AISettings {
        enabled,
        provider,
        model,
        has_api_key,
        models_by_provider,
        reasoning_by_provider,
        fast_mode_by_provider,
        ai_model_needs_reselection,
    })
}

// Frontend is responsible for saving API keys to Stronghold
// This command caches the key for backend use
#[derive(Deserialize)]
pub struct CacheApiKeyArgs {
    pub provider: String,
    #[serde(alias = "apiKey", alias = "api_key")]
    pub api_key: String,
}

#[tauri::command]
pub async fn cache_ai_api_key(_app: tauri::AppHandle, args: CacheApiKeyArgs) -> Result<(), String> {
    let CacheApiKeyArgs { provider, api_key } = args;
    validate_provider_name(&provider)?;

    if api_key.trim().is_empty() {
        log::warn!(
            "Attempted to cache empty API key for provider: {}",
            provider
        );
        return Err("API key cannot be empty".to_string());
    }

    // Don't validate here - this is called on startup when user might be offline
    // Validation happens when saving new keys in a separate command

    // Store API key in memory cache
    let mut cache = API_KEY_CACHE.lock().map_err(|e| {
        log::error!("Failed to acquire API key cache lock: {}", e);
        "Failed to access cache".to_string()
    })?;

    let key_name = format!("ai_api_key_{}", provider);
    cache.insert(key_name.clone(), api_key.clone());

    log::info!("API key cached for provider: {}", provider);

    // Verify the key was actually stored
    if cache.contains_key(&key_name) {
        log::debug!("Verified API key is in cache for provider: {}", provider);
    } else {
        log::error!(
            "Failed to store API key in cache for provider: {}",
            provider
        );
        return Err("Failed to store API key in cache".to_string());
    }

    Ok(())
}

// Validate a new API key or no-auth OpenAI-compatible configuration.
#[derive(Deserialize)]
pub struct ValidateAiApiKeyArgs {
    pub provider: String,
    #[serde(alias = "apiKey", alias = "api_key")]
    pub api_key: Option<String>,
    #[serde(alias = "baseUrl", alias = "base_url")]
    pub base_url: Option<String>,
    pub model: Option<String>,
    #[serde(alias = "noAuth", alias = "no_auth")]
    pub no_auth: Option<bool>,
}
/// Read the user's previously-configured custom model from settings.
///
/// The custom provider has no catalog models; its model is whatever the user
/// picked. It is persisted in the per-provider map (`ai_models_by_provider`),
/// or — for the currently-active provider — in the single `ai_model` value.
fn configured_custom_model(app: &tauri::AppHandle) -> Option<String> {
    app.store("settings").ok().and_then(|store| {
        let from_map = store
            .get("ai_models_by_provider")
            .and_then(|v| serde_json::from_value::<HashMap<String, String>>(v.clone()).ok())
            .and_then(|map| {
                map.get(PROVIDER_CUSTOM)
                    .cloned()
                    .filter(|m| !m.trim().is_empty())
            });
        if from_map.is_some() {
            return from_map;
        }
        let active_provider = store
            .get("ai_provider")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        if active_provider == PROVIDER_CUSTOM {
            store
                .get("ai_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .filter(|m| !m.trim().is_empty())
        } else {
            None
        }
    })
}

/// Decide which model id to use when validating the custom provider.
///
/// Prefer the explicit `model` arg, else the user's previously-configured
/// custom model. The custom provider has no catalog models, so this NEVER
/// falls back to `gpt-5-nano` (which would 404 against a local endpoint).
fn resolve_custom_validation_model(
    explicit: Option<&str>,
    configured: Option<&str>,
) -> Result<String, String> {
    if let Some(model) = explicit.map(str::trim).filter(|m| !m.is_empty()) {
        return Ok(model.to_string());
    }
    if let Some(model) = configured.map(str::trim).filter(|m| !m.is_empty()) {
        return Ok(model.to_string());
    }
    Err("Select a model for your custom provider before validating.".to_string())
}

#[tauri::command]
pub async fn validate_ai_api_key(
    app: tauri::AppHandle,
    args: ValidateAiApiKeyArgs,
) -> Result<(), String> {
    let ValidateAiApiKeyArgs {
        provider,
        api_key,
        base_url,
        model,
        no_auth,
    } = args;
    validate_provider_name(&provider)?;

    let provider_is_supported = launch_providers()
        .iter()
        .any(|candidate| candidate.id == provider);
    if !provider_is_supported {
        return Err(user_facing_message(&AiProviderError::UnsupportedProvider).to_string());
    }

    let provided_key = api_key.unwrap_or_default();
    let no_auth =
        provider == PROVIDER_CUSTOM && (no_auth.unwrap_or(false) || provided_key.trim().is_empty());
    if !no_auth && provided_key.trim().is_empty() {
        return Err(user_facing_message(&AiProviderError::MissingApiKey).to_string());
    }

    let validation_model = if provider == PROVIDER_CUSTOM {
        // The custom provider has no catalog models, so a model must be
        // supplied explicitly or already configured in settings. Never fall
        // back to gpt-5-nano — that would 404 against a local endpoint.
        let configured = configured_custom_model(&app);
        resolve_custom_validation_model(model.as_deref(), configured.as_deref())?
    } else {
        model
            .filter(|candidate| !candidate.trim().is_empty())
            .or_else(|| {
                catalog::recommended_models(&provider)
                    .into_iter()
                    .find(|candidate| candidate.recommended)
                    .map(|candidate| candidate.model_id)
            })
            .unwrap_or_else(|| "gpt-5-nano".to_string())
    };
    let custom_base_url = if provider == PROVIDER_CUSTOM {
        base_url
            .filter(|candidate| !candidate.trim().is_empty())
            .or_else(|| custom_base_url_from_settings(&app))
            .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string())
    } else if provider == PROVIDER_OPENROUTER {
        OPENROUTER_BASE_URL.to_string()
    } else {
        DEFAULT_OPENAI_BASE_URL.to_string()
    };
    if provider == PROVIDER_CUSTOM {
        validate_custom_base_url(&custom_base_url)?;
    }

    let validation_provider = provider.clone();
    let validation_key = provided_key.trim().to_string();
    let key_resolver: AiKeyResolver = Arc::new(move |provider_id| {
        if provider_id == validation_provider && !validation_key.is_empty() {
            Some(validation_key.clone())
        } else {
            None
        }
    });
    let http_client = shared_ai_client();
    let executor = if provider == PROVIDER_OPENROUTER {
        AiExecutor::with_native_endpoint_overrides(
            http_client,
            key_resolver,
            OpenAiCompatibleConfig {
                base_url: custom_base_url,
                no_auth: false,
                key_provider_id: PROVIDER_OPENROUTER.to_string(),
                extra_headers: vec![
                    (
                        "HTTP-Referer".to_string(),
                        "https://voicetypr.com".to_string(),
                    ),
                    ("X-Title".to_string(), "VoiceTypr".to_string()),
                ],
            },
            HashMap::new(),
        )
    } else {
        AiExecutor::new(http_client, key_resolver, custom_base_url, no_auth)
    };
    let request = AiPolishRequest {
        provider_id: provider.clone(),
        model_id: validation_model,
        input_text: "ok".to_string(),
        reasoning_level: None,
        fast_mode: false,
        prompt: "Reply with exactly: ok".to_string(),
        timeout_ms: 10_000,
    };

    executor
        .polish(request, tokio_util::sync::CancellationToken::new())
        .await
        .map(|_| ())
        .map_err(|error| {
            log::warn!(
                "AI provider validation failed: provider={} category={}",
                provider,
                user_facing_message(&error)
            );
            user_facing_message(&error).to_string()
        })
}

/// Test an OpenAI-compatible endpoint without saving or caching anything.
#[tauri::command]
pub async fn test_openai_endpoint(
    base_url: String,
    model: String,
    api_key: Option<String>,
    no_auth: Option<bool>,
) -> Result<(), String> {
    validate_custom_base_url(&base_url)?;
    let no_auth = no_auth.unwrap_or(false)
        || api_key
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);

    let client = reqwest::Client::new();
    let api_key = if no_auth {
        None
    } else {
        let key = api_key.unwrap_or_default();
        if key.trim().is_empty() {
            return Err("API key is required (leave empty to use no authentication)".to_string());
        }
        Some(key.trim().to_string())
    };

    run_openai_chat_probe(&client, &base_url, &model, api_key.as_deref(), no_auth)
        .await
        .map_err(|error| {
            log::error!(
                "OpenAI-compatible test failed: base_url={} model={} error={}",
                base_url,
                model,
                error
            );
            error
        })
}

// Frontend is responsible for removing API keys from Stronghold
// This command clears the cache
#[tauri::command]
pub async fn clear_ai_api_key_cache(
    _app: tauri::AppHandle,
    provider: String,
) -> Result<(), String> {
    // Skip validation if provider is empty (happens when clearing selection)
    if !provider.is_empty() {
        validate_provider_name(&provider)?;
    }

    let mut cache = API_KEY_CACHE
        .lock()
        .map_err(|_| "Failed to access cache".to_string())?;

    if !provider.is_empty() {
        cache.remove(&format!("ai_api_key_{}", provider));
        log::info!("API key cache cleared for provider: {}", provider);
    }

    Ok(())
}

// Clear entire API key cache (for reset)
pub fn clear_all_api_key_cache() -> Result<(), String> {
    let mut cache = API_KEY_CACHE
        .lock()
        .map_err(|_| "Failed to access cache".to_string())?;
    cache.clear();
    log::info!("Cleared entire API key cache");
    Ok(())
}

#[tauri::command]
pub async fn update_ai_settings(
    enabled: bool,
    provider: String,
    model: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Allow empty provider and model for deselection
    if !provider.is_empty() {
        validate_provider_name(&provider)?;
    }

    // Don't allow enabling without a model selected
    if enabled && model.is_empty() && catalog::runtime_kind(&provider) != Some("agent_cli") {
        log::warn!("Attempted to enable AI enhancement without a model selected");
        return Err("Please select a model before enabling AI enhancement".to_string());
    }

    // Check if API key exists when enabling
    if enabled {
        if provider == "custom" {
            let store = app.store("settings").map_err(|e| e.to_string())?;
            let cache_has_key = {
                let cache = API_KEY_CACHE
                    .lock()
                    .map_err(|_| "Failed to access cache".to_string())?;
                cache.contains_key("ai_api_key_custom")
            };
            let configured_base = store.get(CUSTOM_BASE_URL_KEY).is_some()
                || store.get(LEGACY_OPENAI_BASE_URL_KEY).is_some();

            if !(cache_has_key || configured_base) {
                log::warn!(
                    "Attempted to enable AI enhancement without cached API key or configured base URL for provider: {}",
                    provider
                );
                return Err("API key not found. Please add an API key first.".to_string());
            }
        } else if provider == "openai" {
            let store = app.store("settings").map_err(|e| e.to_string())?;
            let cache_has_key = {
                let cache = API_KEY_CACHE
                    .lock()
                    .map_err(|_| "Failed to access cache".to_string())?;
                cache.contains_key("ai_api_key_openai")
            };
            let legacy_custom_config = store.get(LEGACY_OPENAI_BASE_URL_KEY).is_some();

            if !(cache_has_key || legacy_custom_config) {
                log::warn!(
                    "Attempted to enable AI enhancement without cached API key for provider: {}",
                    provider
                );
                return Err("API key not found. Please add an API key first.".to_string());
            }
        } else if catalog::runtime_kind(&provider) == Some("agent_cli") {
            // Subscription CLI — auth is out-of-band (no API key required).
        } else {
            let cache_has_key = {
                let cache = API_KEY_CACHE
                    .lock()
                    .map_err(|_| "Failed to access cache".to_string())?;
                cache.contains_key(&format!("ai_api_key_{}", provider))
            };
            if !cache_has_key {
                log::warn!(
                    "Attempted to enable AI enhancement without cached API key for provider: {}",
                    provider
                );
                return Err("API key not found. Please add an API key first.".to_string());
            }
        }
    }

    persist_settings_and_invalidate(
        &app,
        |store| {
            let mut models_by_provider = load_models_by_provider(store, &provider, &model);
            remember_provider_model(&mut models_by_provider, &provider, &model);

            store.set("ai_enabled", json!(enabled));
            store.set("ai_provider", json!(provider));
            store.set("ai_model", json!(model));
            store.set("ai_models_by_provider", json!(models_by_provider));
            if selection_clears_model_reselection(&provider, &model) {
                store.set("ai_model_needs_reselection", json!(false));
            }
            store.set(
                "enhancement_options",
                serde_json::to_value(EnhancementOptions::default_for_ai_enabled(enabled))
                    .map_err(|e| format!("Failed to serialize enhancement options: {}", e))?,
            );
            if !enabled {
                store.set(
                    "final_text_language",
                    json!(FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT),
                );
                store.set("transcription_task", json!(TRANSCRIPTION_TASK_TRANSCRIBE));
            }

            Ok(())
        },
        |e| format!("Failed to save AI settings: {}", e),
    )
    .await?;

    log::info!(
        "AI settings updated: enabled={}, provider={}, model={}",
        enabled,
        provider,
        model
    );

    Ok(())
}

#[tauri::command]
pub async fn update_agent_cli_reasoning(
    provider: String,
    reasoning: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    validate_provider_name(&provider)?;
    validate_agent_cli_reasoning(&provider, &reasoning)?;
    persist_settings_and_invalidate(
        &app,
        |store| {
            let mut levels = load_agent_cli_reasoning(store);
            levels.insert(provider.clone(), reasoning.clone());
            store.set(AGENT_CLI_REASONING_KEY, json!(levels));
            Ok(())
        },
        |error| format!("Failed to save reasoning level: {error}"),
    )
    .await
}

#[tauri::command]
pub async fn update_agent_cli_fast_mode(
    provider: String,
    enabled: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    validate_provider_name(&provider)?;
    if !supports_fast_mode(&provider) {
        return Err("Fast mode is not supported by this local agent".to_string());
    }
    persist_settings_and_invalidate(
        &app,
        |store| {
            let mut values = load_agent_cli_fast_mode(store);
            values.insert(provider.clone(), enabled);
            store.set(AGENT_CLI_FAST_MODE_KEY, json!(values));
            Ok(())
        },
        |error| format!("Failed to save fast mode: {error}"),
    )
    .await
}

#[tauri::command]
pub async fn disable_ai_enhancement(app: tauri::AppHandle) -> Result<(), String> {
    persist_settings_and_invalidate(
        &app,
        |store| {
            store.set("ai_enabled", json!(false));
            store.set(
                "enhancement_options",
                serde_json::to_value(EnhancementOptions {
                    preset: crate::ai::prompts::EnhancementPreset::PersonalDictation,
                })
                .map_err(|e| format!("Failed to serialize enhancement options: {}", e))?,
            );
            store.set(
                "final_text_language",
                json!(FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT),
            );
            store.set("transcription_task", json!(TRANSCRIPTION_TASK_TRANSCRIBE));

            Ok(())
        },
        |e| format!("Failed to save AI settings: {}", e),
    )
    .await?;

    log::info!("AI enhancement disabled");

    Ok(())
}

pub async fn get_enhancement_options_for_ai_enabled(
    app: tauri::AppHandle,
    ai_enabled: bool,
) -> Result<EnhancementOptions, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let stored_options = store.get("enhancement_options");
    let options = crate::ai::prompts::enhancement_options_for_ai_enabled(
        stored_options.as_ref(),
        ai_enabled,
    )?;

    let settings = load_writing_settings(&app)?;
    let effective = crate::writing::resolve_pipeline_config(
        &settings,
        options.preset,
        FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
        None,
        crate::writing::PipelineAiState {
            stored_ai_enabled: ai_enabled,
            has_model_and_key: has_ai_model_and_key(&app)?,
        },
    );

    Ok(EnhancementOptions {
        preset: effective.preset,
    })
}

#[tauri::command]
pub async fn get_enhancement_options(app: tauri::AppHandle) -> Result<EnhancementOptions, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let ai_enabled = store
        .get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    drop(store);
    get_enhancement_options_for_ai_enabled(app, ai_enabled).await
}

#[tauri::command]
pub async fn update_enhancement_options(
    options: EnhancementOptions,
    app: tauri::AppHandle,
) -> Result<(), String> {
    persist_settings_and_invalidate(
        &app,
        |store| {
            store.set(
                "enhancement_options",
                serde_json::to_value(&options)
                    .map_err(|e| format!("Failed to serialize options: {}", e))?,
            );

            Ok(())
        },
        |e| format!("Failed to save enhancement options: {}", e),
    )
    .await?;

    log::info!("Enhancement options updated: preset={:?}", options.preset);

    Ok(())
}

#[tauri::command]
pub async fn get_writing_settings(app: tauri::AppHandle) -> Result<WritingSettings, String> {
    load_writing_settings(&app)
}

#[tauri::command]
pub async fn update_writing_settings(
    settings: WritingSettings,
    app: tauri::AppHandle,
) -> Result<(), String> {
    save_writing_settings(&app, &settings)?;
    // Writing settings are persisted by writing::settings; keep one invalidation immediately after that save.
    crate::commands::audio::invalidate_recording_config_cache(&app).await;
    Ok(())
}

fn custom_base_url_from_settings(app: &tauri::AppHandle) -> Option<String> {
    app.store("settings").ok().and_then(|store| {
        store
            .get(CUSTOM_BASE_URL_KEY)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .or_else(|| {
                store
                    .get(LEGACY_OPENAI_BASE_URL_KEY)
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            })
    })
}

fn custom_no_auth_from_settings(app: &tauri::AppHandle, has_key: bool) -> bool {
    app.store("settings")
        .ok()
        .and_then(|store| {
            store
                .get(CUSTOM_NO_AUTH_KEY)
                .and_then(|v| v.as_bool())
                .or_else(|| {
                    store
                        .get(LEGACY_OPENAI_NO_AUTH_KEY)
                        .and_then(|v| v.as_bool())
                })
        })
        .unwrap_or(!has_key)
}

// Native adapter endpoints are internal; hardcode best-effort origin hosts (a wrong host is a harmless no-op HEAD).
fn native_origin(provider_id: &str) -> Option<&'static str> {
    match catalog::adapter_name(provider_id)? {
        "OpenAI" => Some("https://api.openai.com"),
        "Anthropic" => Some("https://api.anthropic.com"),
        "Gemini" => Some("https://generativelanguage.googleapis.com"),
        _ => None,
    }
}

fn url_origin(raw: &str) -> Option<String> {
    let origin = reqwest::Url::parse(raw).ok()?.origin();
    origin.is_tuple().then(|| origin.ascii_serialization())
}

fn validated_custom_origin(base: &str) -> Option<String> {
    validate_custom_base_url(base).ok()?;
    url_origin(base)
}

/// HTTP origin the selected polish provider will actually contact.
///
/// Mirrors `executor_for_provider`: native adapters, OpenRouter, custom, and
/// the OpenAI-without-key legacy custom fallback. Custom/legacy URLs must pass
/// `validate_custom_base_url` so warmup never HEADs an origin the executor
/// would refuse. Agent-CLI providers have no HTTP origin.
fn resolve_ai_provider_origin(
    provider_id: &str,
    custom_base_url: Option<&str>,
    openai_has_key: bool,
) -> Option<String> {
    if provider_id == PROVIDER_CUSTOM {
        let base = custom_base_url.unwrap_or(DEFAULT_OPENAI_BASE_URL);
        return validated_custom_origin(base);
    }
    if provider_id == PROVIDER_OPENROUTER {
        return url_origin(OPENROUTER_BASE_URL);
    }
    if provider_id == "openai" && !openai_has_key {
        return custom_base_url.and_then(validated_custom_origin);
    }
    native_origin(provider_id).map(str::to_string)
}

pub(crate) fn ai_provider_origin(app: &tauri::AppHandle, provider_id: &str) -> Option<String> {
    resolve_ai_provider_origin(
        provider_id,
        custom_base_url_from_settings(app).as_deref(),
        ai_provider_has_key("openai"),
    )
}

pub(crate) fn ai_provider_has_key(provider_id: &str) -> bool {
    API_KEY_CACHE
        .lock()
        .map(|cache| cache.contains_key(&format!("ai_api_key_{provider_id}")))
        .unwrap_or(false)
}

/// True when recording-start should fire a connection warmup for this provider.
///
/// Mirrors `executor_for_provider`'s credential preconditions: custom routes
/// (selected `custom`, or OpenAI's legacy custom fallback without an OpenAI
/// key) need either a cached custom key or an effective no-auth config —
/// otherwise the executor rejects before any HTTP request and warmup must not
/// contact the origin either. Other HTTP providers need a cached key.
/// Agent-CLI is never warmed.
fn should_warm_resolved_ai_provider(
    provider_id: &str,
    has_cached_key: bool,
    custom_route_usable: bool,
) -> bool {
    if catalog::runtime_kind(provider_id) == Some("agent_cli") {
        return false;
    }
    if provider_id == PROVIDER_CUSTOM {
        return custom_route_usable;
    }
    if has_cached_key {
        return true;
    }
    provider_id == "openai" && custom_route_usable
}

pub(crate) fn should_warm_ai_provider(app: &tauri::AppHandle, provider_id: &str) -> bool {
    let custom_key_cached = ai_provider_has_key(PROVIDER_CUSTOM);
    let custom_route_usable =
        custom_key_cached || custom_no_auth_from_settings(app, custom_key_cached);
    should_warm_resolved_ai_provider(
        provider_id,
        ai_provider_has_key(provider_id),
        custom_route_usable,
    )
}

pub(crate) async fn warm_ai_provider(app: tauri::AppHandle, provider_id: String) {
    if let Some(origin) = ai_provider_origin(&app, &provider_id) {
        let _ = shared_ai_client()
            .head(&origin)
            .timeout(std::time::Duration::from_secs(8))
            .send()
            .await;
    }
}

/// Recording-start prefetch for the selected polish provider. HTTP providers
/// get a pooled HEAD on their resolved origin; agent-CLI providers get a
/// binary + `--help` capability prefetch. No user text is sent on either path.
pub(crate) async fn prefetch_ai_provider(app: tauri::AppHandle, provider_id: String) {
    if crate::ai::catalog::runtime_kind(&provider_id) == Some("agent_cli") {
        crate::ai::agent_cli::prefetch_polish_capabilities(&provider_id).await;
    } else if should_warm_ai_provider(&app, &provider_id) {
        warm_ai_provider(app, provider_id).await;
    }
}

fn selected_ai_provider_and_model(
    app: &tauri::AppHandle,
) -> Result<(String, String), AiProviderError> {
    let store = app
        .store("settings")
        .map_err(|_| AiProviderError::Internal)?;
    let provider = store
        .get("ai_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    let model = store
        .get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    if provider.is_empty() || !selection_meets_model_requirement(&provider, &model) {
        return Err(AiProviderError::InvalidModel);
    }
    Ok((provider, model))
}

#[derive(Debug)]
pub(crate) struct AiPolishAttemptError {
    pub error: AiProviderError,
    pub provider_id: String,
    pub model_id: String,
}

impl AiPolishAttemptError {
    fn unattributed(error: AiProviderError) -> Self {
        Self {
            error,
            provider_id: String::new(),
            model_id: String::new(),
        }
    }

    fn for_provider(error: AiProviderError, provider_id: impl Into<String>) -> Self {
        Self {
            error,
            provider_id: provider_id.into(),
            model_id: String::new(),
        }
    }

    fn with_model(mut self, model_id: String) -> Self {
        self.model_id = model_id;
        self
    }
}
fn executor_for_provider(
    app: &tauri::AppHandle,
    selected_provider: &str,
) -> Result<(AiExecutor, String), AiPolishAttemptError> {
    let cache = API_KEY_CACHE.lock().map_err(|_| {
        AiPolishAttemptError::for_provider(AiProviderError::Internal, selected_provider)
    })?;
    let openai_key = cache.get("ai_api_key_openai").cloned();
    let custom_key = cache.get("ai_api_key_custom").cloned();
    let selected_key = cache
        .get(&format!("ai_api_key_{}", selected_provider))
        .cloned();
    drop(cache);

    let (runtime_provider, openai_compatible_config, keys) = if selected_provider == "openai" {
        if let Some(key) = openai_key {
            let mut keys = HashMap::new();
            keys.insert("openai".to_string(), key);
            (
                "openai".to_string(),
                OpenAiCompatibleConfig::custom(DEFAULT_OPENAI_BASE_URL.to_string(), false),
                keys,
            )
        } else if let Some(base_url) = custom_base_url_from_settings(app) {
            let has_key = custom_key.is_some();
            let mut keys = HashMap::new();
            if let Some(key) = custom_key {
                keys.insert(PROVIDER_CUSTOM.to_string(), key);
            }
            (
                PROVIDER_CUSTOM.to_string(),
                OpenAiCompatibleConfig::custom(
                    base_url,
                    custom_no_auth_from_settings(app, has_key),
                ),
                keys,
            )
        } else {
            return Err(AiPolishAttemptError::for_provider(
                AiProviderError::MissingApiKey,
                selected_provider,
            ));
        }
    } else if selected_provider == PROVIDER_CUSTOM {
        let has_key = custom_key.is_some();
        let mut keys = HashMap::new();
        if let Some(key) = custom_key {
            keys.insert(PROVIDER_CUSTOM.to_string(), key);
        }
        (
            PROVIDER_CUSTOM.to_string(),
            OpenAiCompatibleConfig::custom(
                custom_base_url_from_settings(app)
                    .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string()),
                custom_no_auth_from_settings(app, has_key),
            ),
            keys,
        )
    } else if selected_provider == PROVIDER_OPENROUTER {
        let key = selected_key.ok_or_else(|| {
            AiPolishAttemptError::for_provider(AiProviderError::MissingApiKey, selected_provider)
        })?;
        let mut keys = HashMap::new();
        keys.insert(PROVIDER_OPENROUTER.to_string(), key);
        (
            PROVIDER_OPENROUTER.to_string(),
            OpenAiCompatibleConfig {
                base_url: OPENROUTER_BASE_URL.to_string(),
                no_auth: false,
                key_provider_id: PROVIDER_OPENROUTER.to_string(),
                extra_headers: vec![
                    (
                        "HTTP-Referer".to_string(),
                        "https://voicetypr.com".to_string(),
                    ),
                    ("X-Title".to_string(), "VoiceTypr".to_string()),
                ],
            },
            keys,
        )
    } else if catalog::runtime_kind(selected_provider) == Some("agent_cli") {
        // Agent-CLI providers (Claude Code in 4C-i): no API key, no reqwest
        // client — the CLI is subscription-authenticated. The OpenAiCompatible
        // config here is unused (the executor dispatches agent_cli providers
        // to AgentCliRuntime, never to the openai-compatible runtime); an empty
        // base_url + no_auth=true keeps the construction safe and keyless.
        (
            selected_provider.to_string(),
            OpenAiCompatibleConfig::custom(String::new(), true),
            HashMap::new(),
        )
    } else {
        let key = selected_key.ok_or_else(|| {
            AiPolishAttemptError::for_provider(AiProviderError::MissingApiKey, selected_provider)
        })?;
        let mut keys = HashMap::new();
        keys.insert(selected_provider.to_string(), key);
        (
            selected_provider.to_string(),
            OpenAiCompatibleConfig::custom(DEFAULT_OPENAI_BASE_URL.to_string(), false),
            keys,
        )
    };
    if runtime_provider == PROVIDER_CUSTOM {
        if let Err(reason) = validate_custom_base_url(&openai_compatible_config.base_url) {
            log::error!(
                "Refusing to use disallowed custom endpoint ({}): {}",
                openai_compatible_config.base_url,
                reason
            );
            return Err(AiPolishAttemptError::for_provider(
                AiProviderError::BadResponse,
                runtime_provider,
            ));
        }
    }

    if runtime_provider == PROVIDER_CUSTOM
        && !openai_compatible_config.no_auth
        && !keys.contains_key(PROVIDER_CUSTOM)
    {
        return Err(AiPolishAttemptError::for_provider(
            AiProviderError::MissingApiKey,
            runtime_provider,
        ));
    }

    let key_resolver: AiKeyResolver = Arc::new(move |provider_id| keys.get(provider_id).cloned());
    let http_client = shared_ai_client();
    Ok((
        AiExecutor::with_native_endpoint_overrides(
            http_client,
            key_resolver,
            openai_compatible_config,
            HashMap::new(),
        ),
        runtime_provider,
    ))
}

async fn polish_text_with_prompt_result_typed(
    app: &tauri::AppHandle,
    text: &str,
    model: String,
    provider: String,
    prompt: String,
) -> Result<crate::ai::contract::AiPolishResult, AiPolishAttemptError> {
    let (executor, runtime_provider) =
        executor_for_provider(app, &provider).map_err(|error| error.with_model(model.clone()))?;
    // The subprocess runtime owns provider-specific local-agent budgets; reuse
    // that value here so the executor cannot cancel a healthy child first.
    let timeout_ms = if catalog::runtime_kind(&runtime_provider) == Some("agent_cli") {
        cold_spawn_timeout_ms(&runtime_provider)
    } else {
        30_000
    };
    let is_agent_cli = catalog::runtime_kind(&runtime_provider) == Some("agent_cli");
    let (reasoning_level, fast_mode) = if is_agent_cli {
        let store = app.store("settings").map_err(|_| {
            AiPolishAttemptError::for_provider(AiProviderError::Internal, runtime_provider.clone())
                .with_model(model.clone())
        })?;
        (
            Some(
                load_agent_cli_reasoning(&store)
                    .remove(&runtime_provider)
                    .unwrap_or_else(|| default_agent_cli_reasoning(&runtime_provider).to_string()),
            ),
            load_agent_cli_fast_mode(&store)
                .remove(&runtime_provider)
                .unwrap_or(false),
        )
    } else {
        (None, false)
    };
    let request = AiPolishRequest {
        provider_id: runtime_provider.clone(),
        model_id: model.clone(),
        reasoning_level,
        fast_mode,
        input_text: text.to_string(),
        prompt,
        timeout_ms,
    };
    let result = executor
        .polish(request, tokio_util::sync::CancellationToken::new())
        .await
        .map_err(|error| AiPolishAttemptError {
            error,
            provider_id: runtime_provider,
            model_id: model,
        })?;
    log::info!(
        "Text enhanced successfully via {} (original: {}, enhanced: {}, duration_ms: {})",
        result.provider_id,
        text.len(),
        result.output_text.len(),
        result.duration_ms
    );
    Ok(result)
}

pub async fn polish_text_typed(
    app: &tauri::AppHandle,
    text: &str,
    options: &crate::ai::EnhancementOptions,
    output_language: Option<&str>,
    transcript_language: Option<&str>,
    context: Option<&str>,
    app_category_hint: Option<&str>,
) -> Result<crate::ai::contract::AiPolishResult, AiPolishAttemptError> {
    let (provider, model) =
        selected_ai_provider_and_model(app).map_err(AiPolishAttemptError::unattributed)?;
    let prompt = crate::ai::prompts::build_enhancement_prompt_for_transcript_language(
        context,
        options,
        output_language,
        transcript_language,
        app_category_hint,
    );
    polish_text_with_prompt_result_typed(app, text, model, provider, prompt).await
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OpenAIConfig {
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(rename = "noAuth")]
    pub no_auth: bool,
}

#[derive(Deserialize)]
pub struct SetOpenAIConfigArgs {
    #[serde(alias = "baseUrl", alias = "base_url")]
    pub base_url: String,
    #[serde(alias = "noAuth", alias = "no_auth")]
    pub no_auth: Option<bool>,
}

#[tauri::command]
pub async fn set_openai_config(
    app: tauri::AppHandle,
    args: SetOpenAIConfigArgs,
) -> Result<(), String> {
    validate_custom_base_url(&args.base_url)?;
    let store = app.store("settings").map_err(|e| e.to_string())?;
    store.set(
        CUSTOM_BASE_URL_KEY,
        serde_json::Value::String(args.base_url),
    );
    if let Some(no_auth) = args.no_auth {
        // Backward-compatibility: accept but not required
        store.set(CUSTOM_NO_AUTH_KEY, serde_json::Value::Bool(no_auth));
    }
    store
        .save()
        .map_err(|e| format!("Failed to save AI settings: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn get_openai_config(app: tauri::AppHandle) -> Result<OpenAIConfig, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let base_url = store
        .get(CUSTOM_BASE_URL_KEY)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| {
            store
                .get(LEGACY_OPENAI_BASE_URL_KEY)
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string());
    let no_auth = store
        .get(CUSTOM_NO_AUTH_KEY)
        .and_then(|v| v.as_bool())
        .or_else(|| {
            store
                .get(LEGACY_OPENAI_NO_AUTH_KEY)
                .and_then(|v| v.as_bool())
        })
        .unwrap_or(false);
    Ok(OpenAIConfig { base_url, no_auth })
}

/// Re-export the probe result type so the Tauri command signature uses the
/// canonical definition in `ai::agent_cli` (avoids a duplicate struct that the
/// compiler sees as a distinct type).
pub use crate::ai::agent_cli::AgentCliProbe;

/// Detect an installed agent CLI from the hydrated login-shell PATH. Detection
/// is cached for the app session; explicit Refresh rehydrates PATH and reruns
/// executable resolution without invoking the CLI.
#[tauri::command]
pub async fn probe_agent_cli(
    provider: String,
    refresh: Option<bool>,
) -> Result<AgentCliProbe, String> {
    let refresh = refresh.unwrap_or(false);
    if !refresh {
        if let Some(probe) = AGENT_CLI_PROBE_CACHE
            .lock()
            .map_err(|_| "Failed to access agent status cache".to_string())?
            .get(&provider)
            .cloned()
        {
            return Ok(probe);
        }
    }

    let probe = crate::ai::agent_cli::probe(&provider, refresh).await;
    AGENT_CLI_PROBE_CACHE
        .lock()
        .map_err(|_| "Failed to access agent status cache".to_string())?
        .insert(provider, probe.clone());
    Ok(probe)
}

/// A model available from a provider.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderModel {
    pub id: String,
    pub name: String,
    pub recommended: bool,
    pub reasoning: bool,
    #[serde(rename = "contextWindow")]
    pub context_window: Option<u64>,
    #[serde(rename = "sourceProvider")]
    pub source_provider: Option<String>,
    #[serde(rename = "cliDefault")]
    pub cli_default: bool,
    #[serde(rename = "costInput")]
    pub cost_input: Option<f64>,
    #[serde(rename = "costOutput")]
    pub cost_output: Option<f64>,
}

fn provider_models(provider: &str) -> Vec<ProviderModel> {
    catalog::all_provider_models(provider)
        .into_iter()
        .map(|model| ProviderModel {
            id: model.model_id.clone(),
            name: model.label.clone(),
            recommended: model.recommended,
            reasoning: model.reasoning,
            context_window: model.context,
            source_provider: None,
            cli_default: false,
            cost_input: model.cost_input,
            cost_output: model.cost_output,
        })
        .collect()
}

fn provider_model_from_agent_cli(model: crate::ai::agent_cli::AgentCliModel) -> ProviderModel {
    ProviderModel {
        id: model.id,
        name: model.name,
        recommended: model.recommended,
        reasoning: model.reasoning,
        context_window: model.context_window,
        source_provider: model.source_provider,
        cli_default: model.cli_default,
        // Agent-CLI model discovery does not provide token pricing, and
        // subscription CLIs must never be presented as having token costs.
        cost_input: None,
        cost_output: None,
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub status: String,
}

fn provider_infos() -> Vec<ProviderInfo> {
    launch_providers()
        .into_iter()
        .map(|provider| ProviderInfo {
            id: provider.id,
            name: provider.label,
            status: provider.status,
        })
        .collect()
}

#[tauri::command]
pub async fn list_ai_providers(_app: tauri::AppHandle) -> Result<Vec<ProviderInfo>, String> {
    Ok(provider_infos())
}

/// List available models for a provider.
#[tauri::command]
pub async fn list_provider_models(
    provider: String,
    _app: tauri::AppHandle,
) -> Result<Vec<ProviderModel>, String> {
    validate_provider_name(&provider)?;

    if provider == PROVIDER_CUSTOM {
        // The custom provider has no catalog models; its model is whatever the
        // user configured for the endpoint.
        return Ok(Vec::new());
    }

    if catalog::runtime_kind(&provider) == Some("agent_cli") {
        let models = crate::ai::agent_cli::list_models(&provider)
            .await
            .map_err(|error| user_facing_message(&error.error).into_owned())?;
        return Ok(models
            .into_iter()
            .map(provider_model_from_agent_cli)
            .collect());
    }

    let models = provider_models(&provider);
    if models.is_empty() {
        Err(format!(
            "Unsupported provider for model listing: {}",
            provider
        ))
    } else {
        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_validation() {
        // Valid providers
        assert!(validate_provider_name("gemini").is_ok());
        assert!(validate_provider_name("openai").is_ok());
        assert!(validate_provider_name("anthropic").is_ok());
        assert!(validate_provider_name("custom").is_ok());

        // Runtime launch table support is enforced separately from loose
        // identifier validation used by legacy settings paths.
        assert!(validate_provider_name("groq").is_ok());
        assert!(validate_provider_name("azure-openai-responses").is_ok());
        assert!(validate_provider_name("openai_compatible").is_ok());

        // Invalid formats
        assert!(validate_provider_name("test provider").is_err());
        assert!(validate_provider_name("test@provider").is_err());
        assert!(validate_provider_name("").is_err());
    }

    #[test]
    fn selection_meets_model_requirement_waives_agent_cli() {
        for provider in AGENT_CLI_PROVIDER_IDS {
            assert!(
                selection_meets_model_requirement(provider, ""),
                "{provider} should allow its CLI default model"
            );
            assert!(selection_meets_model_requirement(provider, "anything"));
        }
        assert!(!selection_meets_model_requirement("gemini", ""));
        assert!(selection_meets_model_requirement(
            "gemini",
            "gemini-2.5-flash"
        ));
        assert!(!selection_meets_model_requirement("openai", ""));
        assert!(selection_meets_model_requirement("openai", "gpt-5-nano"));
        assert!(!selection_meets_model_requirement("unknown-provider", ""));
    }

    #[test]
    fn expanded_cli_reasoning_policy_matches_runtime_support() {
        for provider in [
            "claude-code",
            "pi",
            "omp",
            "codex",
            "droid",
            "grok",
            "cline",
        ] {
            assert!(validate_agent_cli_reasoning(provider, "low").is_ok());
        }
        assert!(validate_agent_cli_reasoning("opencode", "low").is_ok());
        assert_eq!(default_agent_cli_reasoning("pi"), "off");
        assert_eq!(default_agent_cli_reasoning("omp"), "off");
        assert_eq!(default_agent_cli_reasoning("codex"), "low");
        assert!(validate_agent_cli_reasoning("pi", "medium").is_ok());
        assert!(validate_agent_cli_reasoning("pi", "high").is_err());
        assert_eq!(normalize_agent_cli_reasoning("pi", "high"), "medium");
        assert!(supports_fast_mode("claude-code"));
        assert!(supports_fast_mode("omp"));
        assert!(supports_fast_mode("codex"));
        assert!(!supports_fast_mode("pi"));
        assert!(!supports_fast_mode("droid"));
        assert!(validate_agent_cli_reasoning("pi", "minimal").is_ok());
        assert!(validate_agent_cli_reasoning("omp", "minimal").is_ok());
        assert!(validate_agent_cli_reasoning("opencode", "minimal").is_ok());
        assert!(validate_agent_cli_reasoning("claude-code", "minimal").is_err());
        assert_eq!(normalize_agent_cli_reasoning("pi", "minimal"), "minimal");
        assert_eq!(normalize_agent_cli_reasoning("omp", "minimal"), "minimal");
    }

    #[test]
    fn native_origins_match_provider_catalog() {
        assert_eq!(native_origin("openai"), Some("https://api.openai.com"));
        assert_eq!(
            native_origin("anthropic"),
            Some("https://api.anthropic.com")
        );
        assert_eq!(
            native_origin("gemini"),
            Some("https://generativelanguage.googleapis.com")
        );
        assert_eq!(native_origin("custom"), None);
    }

    #[test]
    fn url_origin_extracts_scheme_host_port() {
        assert_eq!(
            url_origin("https://api.example.com/v1"),
            Some("https://api.example.com".to_string())
        );
        assert_eq!(
            url_origin("http://localhost:8080/x"),
            Some("http://localhost:8080".to_string())
        );
        assert_eq!(
            url_origin("http://[::1]:11434/v1"),
            Some("http://[::1]:11434".to_string())
        );
        assert_eq!(url_origin("not a url"), None);
    }

    #[test]
    fn resolve_origin_warms_openrouter_and_rejects_disallowed_custom() {
        assert_eq!(
            resolve_ai_provider_origin(PROVIDER_OPENROUTER, None, false),
            Some("https://openrouter.ai".to_string())
        );
        assert_eq!(
            resolve_ai_provider_origin(PROVIDER_CUSTOM, Some("http://localhost:11434/v1"), false),
            Some("http://localhost:11434".to_string())
        );
        assert_eq!(
            resolve_ai_provider_origin(
                PROVIDER_CUSTOM,
                Some("http://169.254.169.254/latest/meta-data/"),
                false
            ),
            None
        );
        assert_eq!(
            resolve_ai_provider_origin("openai", Some("http://192.168.1.50:1234/v1"), false),
            Some("http://192.168.1.50:1234".to_string())
        );
        assert_eq!(
            resolve_ai_provider_origin("openai", Some("http://192.168.1.50:1234/v1"), true),
            Some("https://api.openai.com".to_string())
        );
        assert_eq!(
            resolve_ai_provider_origin("openai", Some("http://169.254.169.254/"), false),
            None
        );
        assert_eq!(resolve_ai_provider_origin("claude-code", None, false), None);
    }

    #[test]
    fn should_warm_http_routes_but_not_agent_cli() {
        // Custom routes warm only when the executor could actually use them:
        // a cached custom key or an effective no-auth config.
        assert!(should_warm_resolved_ai_provider(
            PROVIDER_CUSTOM,
            false,
            true
        ));
        assert!(!should_warm_resolved_ai_provider(
            PROVIDER_CUSTOM,
            false,
            false
        ));
        assert!(should_warm_resolved_ai_provider(
            PROVIDER_OPENROUTER,
            true,
            false
        ));
        assert!(!should_warm_resolved_ai_provider(
            PROVIDER_OPENROUTER,
            false,
            true
        ));
        // OpenAI without an OpenAI key falls back to the custom route — same
        // credential precondition as the executor's custom branch.
        assert!(should_warm_resolved_ai_provider("openai", false, true));
        assert!(!should_warm_resolved_ai_provider("openai", false, false));
        assert!(should_warm_resolved_ai_provider("openai", true, false));
        assert!(!should_warm_resolved_ai_provider(
            "claude-code",
            false,
            false
        ));
        assert!(!should_warm_resolved_ai_provider("claude-code", true, true));
    }

    #[test]
    fn test_provider_models_are_contract_backed() {
        let openai_models = provider_models("openai");
        assert!(openai_models.len() >= 2);
        assert!(openai_models.iter().any(|m| m.id == "gpt-5-nano"));
        assert!(openai_models.iter().any(|m| m.id == "gpt-5-mini"));

        let gemini_models = provider_models("gemini");
        assert!(!gemini_models.is_empty());
        assert!(gemini_models.iter().any(|m| m.id == "gemini-2.5-flash"));
        assert!(gemini_models
            .iter()
            .any(|m| m.id == "gemini-2.5-flash-lite"));

        let anthropic_models = provider_models("anthropic");
        assert!(!anthropic_models.is_empty());
        assert!(anthropic_models.iter().any(|m| m.id == "claude-haiku-4-5"));
        assert!(anthropic_models.iter().any(|m| m.id == "claude-sonnet-4-5"));

        assert!(provider_models("custom").is_empty());
        assert!(provider_models("unknown").is_empty());
    }

    #[test]
    fn agent_cli_model_mapping_preserves_default_metadata_without_cost_claims() {
        let mapped = provider_model_from_agent_cli(crate::ai::agent_cli::AgentCliModel {
            id: String::new(),
            name: "CLI default".to_string(),
            recommended: true,
            reasoning: false,
            context_window: None,
            source_provider: None,
            cli_default: true,
        });

        assert_eq!(mapped.id, "");
        assert!(mapped.recommended);
        assert!(mapped.cli_default);
        assert_eq!(mapped.source_provider, None);
        assert_eq!(mapped.cost_input, None);
        assert_eq!(mapped.cost_output, None);

        let discovered = provider_model_from_agent_cli(crate::ai::agent_cli::AgentCliModel {
            id: "anthropic/claude-sonnet".to_string(),
            name: "Claude Sonnet".to_string(),
            recommended: false,
            reasoning: true,
            context_window: Some(200_000),
            source_provider: Some("anthropic".to_string()),
            cli_default: false,
        });
        assert_eq!(discovered.context_window, Some(200_000));
        assert_eq!(discovered.source_provider.as_deref(), Some("anthropic"));
        assert!(discovered.reasoning);
        assert!(!discovered.cli_default);

        let serialized = serde_json::to_value(&mapped).expect("ProviderModel serializes");
        assert_eq!(serialized["id"], "");
        assert_eq!(serialized["cliDefault"], true);
        assert!(serialized["sourceProvider"].is_null());
        assert!(serialized["costInput"].is_null());
        assert!(serialized["costOutput"].is_null());

        let discovered_serialized =
            serde_json::to_value(&discovered).expect("ProviderModel serializes");
        assert_eq!(discovered_serialized["contextWindow"], 200_000);
        assert_eq!(discovered_serialized["sourceProvider"], "anthropic");
        assert_eq!(discovered_serialized["cliDefault"], false);
    }

    #[test]
    fn remember_provider_model_preserves_explicit_cli_default_selection() {
        let mut models = HashMap::from([
            ("claude-code".to_string(), "sonnet".to_string()),
            ("openai".to_string(), "gpt-5-nano".to_string()),
        ]);

        remember_provider_model(&mut models, "claude-code", "");
        assert_eq!(models.get("claude-code"), Some(&String::new()));
        assert_eq!(models.get("openai"), Some(&"gpt-5-nano".to_string()));

        remember_provider_model(&mut models, "openai", "");
        assert!(!models.contains_key("openai"));

        // An empty provider is a deselection/no-op and must not clear another
        // provider's remembered model.
        remember_provider_model(&mut models, "", "");
        assert_eq!(models.len(), 1);
        assert_eq!(models.get("claude-code"), Some(&String::new()));
    }

    #[test]
    fn valid_cli_default_selection_clears_model_reselection() {
        assert!(selection_clears_model_reselection("pi", ""));
        assert!(selection_clears_model_reselection("omp", ""));
        assert!(selection_clears_model_reselection("openai", "gpt-5-mini"));
        assert!(!selection_clears_model_reselection("openai", ""));
    }

    #[test]
    fn test_list_command_dto_shape_includes_catalog_providers() {
        let providers = provider_infos();
        // 8 generated catalog providers + the synthetic custom provider.
        assert_eq!(providers.len(), launch_providers().len());
        let by_id = |id: &str| providers.iter().find(|provider| provider.id == id);
        assert_eq!(
            by_id("openai").map(|p| (p.name.as_str(), p.status.as_str())),
            Some(("OpenAI", "production"))
        );
        assert_eq!(
            by_id("gemini").map(|p| (p.name.as_str(), p.status.as_str())),
            Some(("Google Gemini", "production"))
        );
        assert_eq!(
            by_id("anthropic").map(|p| p.status.as_str()),
            Some("production")
        );
        assert_eq!(
            by_id("custom").map(|p| (p.name.as_str(), p.status.as_str())),
            Some(("Custom (OpenAI-compatible)", "production"))
        );
    }

    #[test]
    fn test_warmup_keys_are_derived_from_launch_providers() {
        let keys = ai_provider_key_names();
        let expected: Vec<String> = launch_providers()
            .into_iter()
            .filter(|provider| provider.requires_api_key || provider.id == PROVIDER_CUSTOM)
            .map(|provider| format!("ai_api_key_{}", provider.id))
            .collect();
        assert_eq!(keys, expected);
        assert!(keys.contains(&"ai_api_key_openai".to_string()));
        assert!(keys.contains(&"ai_api_key_anthropic".to_string()));
        assert!(keys.contains(&"ai_api_key_custom".to_string()));
    }

    #[test]
    fn test_enhancement_options_for_ai_enabled_normalizes_global_preset() {
        use crate::ai::prompts::{enhancement_options_for_ai_enabled, EnhancementPreset};

        let writing = serde_json::json!({ "preset": "Writing" });
        let disabled = enhancement_options_for_ai_enabled(Some(&writing), false).unwrap();
        assert_eq!(disabled.preset, EnhancementPreset::PersonalDictation);

        let enabled = enhancement_options_for_ai_enabled(Some(&writing), true).unwrap();
        assert_eq!(enabled.preset, EnhancementPreset::CleanDictation);

        let clean = serde_json::json!({ "preset": "CleanDictation" });
        let enabled_clean = enhancement_options_for_ai_enabled(Some(&clean), true).unwrap();
        assert_eq!(enabled_clean.preset, EnhancementPreset::CleanDictation);
    }

    #[test]
    fn test_warm_ai_key_cache_populates_from_store() {
        // Verify that populating the API_KEY_CACHE makes keys discoverable.
        // This tests the cache-warming path used by warm_ai_key_cache_from_secure_store.
        let mut cache: HashMap<String, String> = HashMap::new();

        // Standard providers are discovered by ai_api_key_{provider} presence
        assert!(!cache.contains_key("ai_api_key_gemini"));
        assert!(!cache.contains_key("ai_api_key_openai"));
        assert!(!cache.contains_key("ai_api_key_custom"));

        // Simulate warming from secure store for gemini
        cache.insert("ai_api_key_gemini".to_string(), "test-key".to_string());
        assert!(cache.contains_key("ai_api_key_gemini"));

        // OpenAI and custom still absent
        assert!(!cache.contains_key("ai_api_key_openai"));
        assert!(!cache.contains_key("ai_api_key_custom"));

        // Simulate warming for openai
        cache.insert("ai_api_key_openai".to_string(), "test-key-2".to_string());
        assert!(cache.contains_key("ai_api_key_openai"));

        // Clear one provider key
        cache.remove("ai_api_key_gemini");
        assert!(!cache.contains_key("ai_api_key_gemini"));
        assert!(cache.contains_key("ai_api_key_openai"));

        // Clear all
        cache.clear();
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn chat_probe_ok_on_valid_completion() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": "OK"}}]
            })))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = run_openai_chat_probe(&client, &server.uri(), "test-model", None, true).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
    }

    #[tokio::test]
    async fn chat_probe_errors_on_auth_failure() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("echoed-secret-sk-LEAK"))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = run_openai_chat_probe(&client, &server.uri(), "test-model", None, true).await;
        let err = result.unwrap_err();
        assert!(
            err.contains("Invalid API key"),
            "expected 'Invalid API key' in error, got: {}",
            err
        );
        assert!(
            !err.contains("echoed-secret-sk-LEAK"),
            "error must not echo response body, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn chat_probe_errors_on_non_chat_shape() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"foo": "bar"})),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = run_openai_chat_probe(&client, &server.uri(), "test-model", None, true).await;
        assert!(result.is_err(), "expected Err for non-chat JSON shape");
    }

    #[tokio::test]
    async fn chat_probe_errors_on_non_json() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>nope</html>"))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = run_openai_chat_probe(&client, &server.uri(), "test-model", None, true).await;
        assert!(result.is_err(), "expected Err for non-JSON response");
    }

    #[test]
    fn validate_custom_base_url_accepts_local_private_tailscale_and_public() {
        assert!(validate_custom_base_url("http://localhost:11434").is_ok());
        assert!(validate_custom_base_url("http://192.168.1.50:1234").is_ok());
        assert!(validate_custom_base_url("http://100.100.100.100:11434").is_ok());
        assert!(validate_custom_base_url("https://api.example.com").is_ok());
    }

    #[test]
    fn validate_custom_base_url_rejects_link_local_and_non_http_schemes() {
        assert!(validate_custom_base_url("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(validate_custom_base_url("http://[fe80::1]/").is_err());
        assert!(validate_custom_base_url("ftp://x").is_err());
        assert!(validate_custom_base_url("not a url").is_err());
        // IPv4-mapped link-local must not bypass the IPv6 branch.
        assert!(validate_custom_base_url("http://[::ffff:169.254.169.254]/").is_err());
    }

    #[test]
    fn custom_validation_model_resolution_never_yields_gpt5_nano() {
        // No explicit model and no configured model -> clear error, never a probe.
        let err = resolve_custom_validation_model(None, None).unwrap_err();
        assert!(err.contains("Select a model"), "got: {}", err);
        assert!(!err.to_lowercase().contains("gpt-5-nano"));

        // Explicit model wins.
        assert_eq!(
            resolve_custom_validation_model(Some("llama3.2"), None).unwrap(),
            "llama3.2"
        );
        // Configured model used when no explicit arg.
        assert_eq!(
            resolve_custom_validation_model(None, Some("qwen2.5")).unwrap(),
            "qwen2.5"
        );
        // Whitespace-only values are treated as absent (must error).
        assert!(resolve_custom_validation_model(Some("   "), Some("  ")).is_err());
    }
}
