use tauri_plugin_store::StoreExt;

/// Set once the legacy global Polish style has been normalised (see migrate_ai_settings_values).
pub(crate) const GLOBAL_STYLE_MIGRATED_KEY: &str = "polish_global_style_migrated";

pub(crate) fn migrate_ai_settings_before_key_cache<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Ok(store) = app.store("settings") else {
        log::warn!("AI settings migration skipped: settings store unavailable");
        return;
    };

    const MIGRATION_KEYS: [&str; 7] = [
        "ai_enabled",
        "ai_provider",
        "ai_model",
        "ai_models_by_provider",
        "ai_model_needs_reselection",
        "enhancement_options",
        GLOBAL_STYLE_MIGRATED_KEY,
    ];
    let mut values = serde_json::Map::new();
    for key in MIGRATION_KEYS {
        if let Some(value) = store.get(key) {
            values.insert(key.to_string(), value.clone());
        }
    }
    let original_values = values.clone();

    if migrate_ai_settings_values(&mut values) {
        for (key, value) in values {
            store.set(key, value);
        }
        if store.save().is_err() {
            for key in MIGRATION_KEYS {
                if let Some(value) = original_values.get(key) {
                    store.set(key, value.clone());
                } else {
                    store.delete(key);
                }
            }
            log::warn!("AI settings migration save failed");
        } else {
            log::info!("AI settings migration applied");
        }
    }
}

pub(crate) fn migrate_ai_settings_values(
    values: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    let mut changed = migrate_removed_agent_cli_provider(values);

    let provider = values
        .get("ai_provider")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let migrated_provider = if provider == "google" {
        changed = true;
        "gemini".to_string()
    } else {
        provider
    };
    if values
        .get("ai_provider")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        != migrated_provider
    {
        values.insert(
            "ai_provider".to_string(),
            serde_json::Value::String(migrated_provider.clone()),
        );
        changed = true;
    }

    if let Some(models) = values
        .get_mut("ai_models_by_provider")
        .and_then(|value| value.as_object_mut())
    {
        if let Some(google_model) = models.remove("google") {
            if !models.contains_key("gemini") {
                models.insert("gemini".to_string(), google_model);
            }
            changed = true;
        }
    }

    if let Some(models) = values
        .get_mut("ai_models_by_provider")
        .and_then(|v| v.as_object_mut())
    {
        for (provider, saved) in models.iter_mut() {
            if crate::ai::catalog::is_native_provider(provider) {
                if let Some(model) = saved
                    .as_str()
                    .and_then(|model| crate::ai::catalog::resolve_model(provider, model))
                {
                    if saved.as_str() != Some(model.as_str()) {
                        *saved = serde_json::json!(model);
                        changed = true;
                    }
                }
            }
        }
    }

    let current_model = values
        .get("ai_model")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let ai_enabled = values
        .get("ai_enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !current_model.is_empty()
        && !ai_model_is_valid_for_provider(&migrated_provider, &current_model)
    {
        let replacement = crate::ai::catalog::resolve_model(&migrated_provider, &current_model)
            .filter(|model| !model.trim().is_empty());
        values.insert(
            "ai_model".to_string(),
            serde_json::json!(replacement.as_deref().unwrap_or("")),
        );
        if let Some(model) = replacement {
            let models = values
                .entry("ai_models_by_provider")
                .or_insert_with(|| serde_json::json!({}));
            if !models.is_object() {
                *models = serde_json::json!({});
            }
            models
                .as_object_mut()
                .unwrap()
                .insert(migrated_provider.clone(), serde_json::json!(model));
            values.insert(
                "ai_model_needs_reselection".to_string(),
                serde_json::json!(false),
            );
        } else if ai_enabled {
            values.insert(
                "ai_model_needs_reselection".to_string(),
                serde_json::json!(true),
            );
        }
        changed = true;
    }

    // One-time upgrade step: a global style stored by an older version may be stale
    // (the user never chose it), so it is reset to the plain on/off default once.
    // Afterwards a global style the user picks (Polish screen, tray, island) is kept.
    let legacy_style = values
        .get(GLOBAL_STYLE_MIGRATED_KEY)
        .and_then(serde_json::Value::as_bool)
        != Some(true);
    if legacy_style {
        values.insert(
            GLOBAL_STYLE_MIGRATED_KEY.to_string(),
            serde_json::json!(true),
        );
        changed = true;
    }
    if let Some(stored_options) = values.get("enhancement_options").cloned() {
        let normalized = if legacy_style {
            Ok(crate::ai::prompts::EnhancementOptions::default_for_ai_enabled(ai_enabled))
        } else {
            crate::ai::prompts::enhancement_options_for_ai_enabled(
                Some(&stored_options),
                ai_enabled,
            )
        }
        .and_then(|options| {
            serde_json::to_value(options)
                .map_err(|error| format!("Failed to serialize Polish options: {error}"))
        });
        match normalized {
            Ok(normalized) if normalized != stored_options => {
                values.insert("enhancement_options".to_string(), normalized);
                changed = true;
            }
            Ok(_) => {}
            Err(error) => log::warn!("Polish settings migration skipped: {}", error),
        }
    }

    changed
}

/// Removed CLI selections (including legacy Cursor) fall back to Claude Code.
fn migrate_removed_agent_cli_provider(
    values: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    const REMOVED_PROVIDERS: [&str; 4] = ["cline", "droid", "grok", "cursor"];
    let Some(old) = values
        .get("ai_provider")
        .and_then(|value| value.as_str())
        .filter(|provider| REMOVED_PROVIDERS.contains(provider))
        .map(str::to_string)
    else {
        return false;
    };

    let claude_model = values
        .get_mut("ai_models_by_provider")
        .and_then(|value| value.as_object_mut())
        .map(|models| {
            for provider in REMOVED_PROVIDERS {
                models.remove(provider);
            }
            models
                .get(crate::ai::providers::PROVIDER_CLAUDE_CODE)
                .and_then(|model| model.as_str())
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default();
    values.insert(
        "ai_provider".to_string(),
        serde_json::json!(crate::ai::providers::PROVIDER_CLAUDE_CODE),
    );
    values.insert("ai_model".to_string(), serde_json::json!(claude_model));
    log::info!("Polish provider {old} was removed; switched to Claude Code");
    true
}

fn ai_model_is_valid_for_provider(provider: &str, model: &str) -> bool {
    if provider == "custom" || crate::ai::catalog::runtime_kind(provider) == Some("agent_cli") {
        return !model.trim().is_empty();
    }
    crate::ai::catalog::all_provider_models(provider)
        .iter()
        .any(|candidate| candidate.model_id == model)
}
// Exercise the startup migration independently of native initialization.
#[cfg(test)]
mod ai_settings_migration_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn removed_cli_providers_keep_enabled_and_use_claude_model_memory() {
        for provider in ["cline", "droid", "grok", "cursor"] {
            for enabled in [true, false] {
                for claude_model in [None, Some("sonnet")] {
                    let mut models = json!({
                        "cline": "old-cline",
                        "droid": "old-droid",
                        "grok": "old-grok",
                        "cursor": "old-cursor",
                        "custom": "keep"
                    });
                    if let Some(model) = claude_model {
                        models["claude-code"] = json!(model);
                    }
                    let mut values = values_with_enabled(enabled, provider, "old-model", models);
                    assert!(migrate_ai_settings_values(&mut values));
                    assert_eq!(values["ai_provider"], json!("claude-code"));
                    assert_eq!(values["ai_enabled"], json!(enabled));
                    assert_eq!(values["ai_model"], json!(claude_model.unwrap_or("")));
                    assert_eq!(values["ai_models_by_provider"]["custom"], json!("keep"));
                    for removed in ["cline", "droid", "grok", "cursor"] {
                        assert!(values["ai_models_by_provider"].get(removed).is_none());
                    }
                    let migrated = values.clone();
                    assert!(!migrate_ai_settings_values(&mut values));
                    assert_eq!(values, migrated);
                }
            }
        }
    }

    fn values(
        provider: &str,
        model: &str,
        models: serde_json::Value,
    ) -> serde_json::Map<String, serde_json::Value> {
        values_with_enabled(true, provider, model, models)
    }

    fn values_with_enabled(
        enabled: bool,
        provider: &str,
        model: &str,
        models: serde_json::Value,
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut values = serde_json::Map::new();
        values.insert("ai_enabled".to_string(), json!(enabled));
        values.insert("ai_provider".to_string(), json!(provider));
        values.insert("ai_model".to_string(), json!(model));
        values.insert("ai_models_by_provider".to_string(), models);
        values
    }

    fn assert_removed_provider_migration(provider: &str) {
        for enabled in [true, false] {
            for saved_claude_model in ["", "claude-haiku-5-5"] {
                let mut values = values_with_enabled(
                    enabled,
                    provider,
                    "old-model",
                    json!({
                        "cline": "old-model",
                        "droid": "old-model",
                        "grok": "old-model",
                        "cursor": "old-model",
                        "claude-code": saved_claude_model,
                        "codex": "keep-model"
                    }),
                );
                assert!(migrate_ai_settings_values(&mut values), "{provider}");
                assert_eq!(values["ai_provider"], json!("claude-code"), "{provider}");
                assert_eq!(values["ai_model"], json!(saved_claude_model), "{provider}");
                assert_eq!(values["ai_enabled"], json!(enabled), "{provider}");
                assert_eq!(
                    values["ai_models_by_provider"],
                    json!({"claude-code": saved_claude_model, "codex": "keep-model"}),
                    "{provider}"
                );
                assert_eq!(values.get("ai_model_needs_reselection"), None, "{provider}");
                let first = values.clone();
                assert!(!migrate_ai_settings_values(&mut values), "{provider}");
                assert_eq!(values, first, "{provider}");
            }
        }
        // No saved model map is also valid: Claude Code chooses its default.
        let mut values = values(provider, "old-model", json!(null));
        assert!(migrate_ai_settings_values(&mut values), "{provider}");
        assert_eq!(values["ai_provider"], json!("claude-code"), "{provider}");
        assert_eq!(values["ai_model"], json!(""), "{provider}");
        assert_eq!(values["ai_enabled"], json!(true), "{provider}");
    }

    #[test]
    fn migrates_removed_cline_provider() {
        assert_removed_provider_migration("cline");
    }

    #[test]
    fn migrates_removed_droid_provider() {
        assert_removed_provider_migration("droid");
    }

    #[test]
    fn migrates_removed_grok_provider() {
        assert_removed_provider_migration("grok");
    }

    #[test]
    fn migrates_removed_cursor_provider_defensively() {
        assert_removed_provider_migration("cursor");
    }

    #[test]
    fn migrates_google_provider_and_model_memory_to_gemini() {
        let mut values = values(
            "google",
            "gemini-2.5-flash",
            json!({ "google": "gemini-2.5-flash" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(values["ai_provider"], json!("gemini"));
        assert_eq!(
            values["ai_models_by_provider"],
            json!({ "gemini": "gemini-2.5-flash" })
        );
        assert_eq!(values["ai_model"], json!("gemini-2.5-flash"));
        assert_eq!(values.get("ai_model_needs_reselection"), None);
    }

    #[test]
    fn migration_keeps_existing_gemini_model_memory() {
        let mut values = values(
            "google",
            "gemini-2.5-flash",
            json!({ "google": "gemini-3-flash-preview", "gemini": "gemini-2.5-flash" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(
            values["ai_models_by_provider"],
            json!({ "gemini": "gemini-2.5-flash" })
        );
        assert_eq!(values.get("ai_model_needs_reselection"), None);
    }

    #[test]
    fn migration_persists_primary_replacement_for_enabled_invalid_model() {
        let mut values = values("google", "text-bison", json!({ "google": "text-bison" }));

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(values["ai_enabled"], json!(true));
        assert_eq!(values["ai_provider"], json!("gemini"));
        let primary = crate::ai::catalog::recommended_models("gemini")[0]
            .model_id
            .clone();
        assert_eq!(values["ai_model"], json!(primary));
        assert_eq!(values["ai_models_by_provider"]["gemini"], json!(primary));
        assert_eq!(values["ai_model_needs_reselection"], json!(false));
    }

    #[test]
    fn migration_replaces_disabled_invalid_model() {
        let mut values = values_with_enabled(
            false,
            "google",
            "text-bison",
            json!({ "google": "text-bison" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(values["ai_enabled"], json!(false));
        assert_eq!(values["ai_provider"], json!("gemini"));
        assert_eq!(
            values["ai_model"],
            json!(crate::ai::catalog::recommended_models("gemini")[0].model_id)
        );
        assert_eq!(values["ai_model_needs_reselection"], json!(false));
    }

    #[test]
    fn migration_does_not_flag_empty_model() {
        let mut values = values("google", "", json!({ "google": "" }));

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(values["ai_enabled"], json!(true));
        assert_eq!(values["ai_provider"], json!("gemini"));
        assert_eq!(values["ai_model"], json!(""));
        assert_eq!(values.get("ai_model_needs_reselection"), None);
    }

    #[test]
    fn migration_keeps_custom_free_text_model() {
        let mut values = values("custom", "local-model", json!({ "custom": "local-model" }));
        values.insert(GLOBAL_STYLE_MIGRATED_KEY.to_string(), json!(true));

        assert!(!migrate_ai_settings_values(&mut values));
        assert_eq!(values["ai_model"], json!("local-model"));
    }

    #[test]
    fn migration_normalizes_legacy_global_preset_for_enabled_polish() {
        let mut values = values_with_enabled(
            true,
            "custom",
            "local-model",
            json!({ "custom": "local-model" }),
        );
        values.insert(
            "enhancement_options".to_string(),
            json!({ "preset": "Writing" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(
            values["enhancement_options"],
            json!({ "preset": "CleanDictation" })
        );
    }

    #[test]
    fn migration_keeps_a_chosen_global_style_after_the_one_time_step() {
        let mut values = values_with_enabled(
            true,
            "custom",
            "local-model",
            json!({ "custom": "local-model" }),
        );
        values.insert(
            "enhancement_options".to_string(),
            json!({ "preset": "Writing" }),
        );
        values.insert(GLOBAL_STYLE_MIGRATED_KEY.to_string(), json!(true));

        migrate_ai_settings_values(&mut values);
        assert_eq!(
            values["enhancement_options"],
            json!({ "preset": "Writing" })
        );
    }

    #[test]
    fn migration_normalizes_legacy_global_preset_for_disabled_polish() {
        let mut values = values_with_enabled(
            false,
            "custom",
            "local-model",
            json!({ "custom": "local-model" }),
        );
        values.insert(
            "enhancement_options".to_string(),
            json!({ "preset": "Writing" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(
            values["enhancement_options"],
            json!({ "preset": "PersonalDictation" })
        );
    }

    #[test]
    fn saved_haiku_stays_selected_and_native_model_memory_is_normalized_once() {
        for enabled in [false, true] {
            let mut values = values_with_enabled(
                enabled,
                "anthropic",
                "claude-haiku-5-5",
                json!({
                    "anthropic": "claude-haiku-5-5",
                    "gemini": "removed-gemini-model",
                    "openai": "removed-openai-model",
                    "claude-code": "sonnet",
                    "pi": "",
                    "omp": "",
                    "codex": "gpt-6-luna"
                }),
            );
            assert!(migrate_ai_settings_values(&mut values));
            assert_eq!(values["ai_model"], json!("claude-haiku-5-5"));
            assert_eq!(values["ai_enabled"], json!(enabled));
            assert_eq!(
                values["ai_models_by_provider"],
                json!({
                    "anthropic": "claude-haiku-5-5",
                    "gemini": "gemini-3.5-flash-lite",
                    "openai": "gpt-6-luna",
                    "claude-code": "sonnet",
                    "pi": "",
                    "omp": "",
                    "codex": "gpt-6-luna"
                })
            );
            let first = values.clone();
            assert!(!migrate_ai_settings_values(&mut values));
            assert_eq!(values, first);
        }
    }

    #[test]
    fn migration_is_idempotent_after_first_pass() {
        let mut values = values(
            "google",
            "gemini-2.5-flash",
            json!({ "google": "gemini-2.5-flash" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        let first = values.clone();
        assert!(!migrate_ai_settings_values(&mut values));
        assert_eq!(values, first);
    }
}
