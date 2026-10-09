//! Per-provider reasoning level and fast mode for the native (genai-backed)
//! providers: openai, anthropic, gemini. Mirrors the agent-CLI maps so the
//! settings payload carries one `reasoningByProvider` / `fastModeByProvider`
//! contract for every configurable provider.
use super::catalog;
use crate::commands::settings::persist_settings_and_invalidate;
use serde_json::json;
use std::collections::HashMap;

pub const NATIVE_REASONING_KEY: &str = "ai_native_reasoning_by_provider";
pub const NATIVE_FAST_MODE_KEY: &str = "ai_native_fast_mode_by_provider";

pub const NATIVE_REASONING_LEVELS: [&str; 5] = catalog::REASONING_LEVEL_ORDER;

/// Native provider ids (genai-backed providers with a real adapter).
fn native_provider_ids() -> Vec<String> {
    catalog::launch_providers()
        .into_iter()
        .map(|provider| provider.id)
        .filter(|id| catalog::is_native_provider(id))
        .collect()
}

pub fn load_native_reasoning<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
) -> HashMap<String, String> {
    let mut levels = store
        .get(NATIVE_REASONING_KEY)
        .and_then(|value| serde_json::from_value::<HashMap<String, String>>(value.clone()).ok())
        .unwrap_or_default();
    for provider in native_provider_ids() {
        let normalized = levels
            .get(&provider)
            .filter(|level| NATIVE_REASONING_LEVELS.contains(&level.as_str()))
            .cloned()
            .unwrap_or_else(|| catalog::DEFAULT_REASONING_LEVEL.to_string());
        levels.insert(provider, normalized);
    }
    levels
}

pub fn load_native_fast_mode<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
) -> HashMap<String, bool> {
    let mut values = store
        .get(NATIVE_FAST_MODE_KEY)
        .and_then(|value| serde_json::from_value::<HashMap<String, bool>>(value.clone()).ok())
        .unwrap_or_default();
    for provider in native_provider_ids() {
        values.entry(provider.to_string()).or_insert(false);
    }
    values
}

/// Persist fast=false for a provider after a fast-tier refusal. Called from
/// the polish path when the standard-speed retry already succeeded.
pub fn persist_native_fast_mode_disabled<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    provider: &str,
) -> Result<(), String> {
    let mut values = load_native_fast_mode(store);
    values.insert(provider.to_string(), false);
    store.set(NATIVE_FAST_MODE_KEY, json!(values));
    store.save().map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_native_ai_reasoning(
    provider: String,
    reasoning: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if !catalog::is_native_provider(&provider) {
        return Err("Reasoning is not configurable for this provider".to_string());
    }
    if !NATIVE_REASONING_LEVELS.contains(&reasoning.as_str()) {
        return Err("Unsupported reasoning level".to_string());
    }
    persist_settings_and_invalidate(
        &app,
        |store| {
            let mut levels = load_native_reasoning(store);
            levels.insert(provider.clone(), reasoning.clone());
            store.set(NATIVE_REASONING_KEY, json!(levels));
            Ok(())
        },
        |error| format!("Failed to save reasoning level: {error}"),
    )
    .await
}

#[tauri::command]
pub async fn update_native_ai_fast_mode(
    provider: String,
    enabled: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if !catalog::is_native_provider(&provider) {
        return Err("Fast mode is not supported by this provider".to_string());
    }
    if !catalog::provider_has_fast_models(&provider) {
        return Err("Fast mode is not supported by this provider".to_string());
    }
    persist_settings_and_invalidate(
        &app,
        |store| {
            let mut values = load_native_fast_mode(store);
            values.insert(provider.clone(), enabled);
            store.set(NATIVE_FAST_MODE_KEY, json!(values));
            Ok(())
        },
        |error| format!("Failed to save fast mode: {error}"),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_provider_ids_cover_genai_adapters() {
        for id in ["openai", "anthropic", "gemini"] {
            assert!(
                native_provider_ids().iter().any(|p| p == id),
                "{id} should be a native provider"
            );
        }
        for id in ["custom", "openrouter", "claude-code"] {
            assert!(
                !native_provider_ids().iter().any(|p| p == id),
                "{id} must not be native"
            );
        }
    }

    #[test]
    fn fast_model_capability_matches_overlay() {
        assert!(catalog::provider_has_fast_models("openai"));
        assert!(catalog::provider_has_fast_models("anthropic"));
        assert!(!catalog::provider_has_fast_models("gemini"));
    }
}
