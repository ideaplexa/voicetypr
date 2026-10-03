use tauri_plugin_store::StoreExt;

pub(crate) fn migrate_ai_settings_before_key_cache<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Ok(store) = app.store("settings") else {
        log::warn!("AI settings migration skipped: settings store unavailable");
        return;
    };

    const MIGRATION_KEYS: [&str; 6] = [
        "ai_enabled",
        "ai_provider",
        "ai_model",
        "ai_models_by_provider",
        "ai_model_needs_reselection",
        "enhancement_options",
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
    let mut changed = false;

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

    if let Some(stored_options) = values.get("enhancement_options").cloned() {
        let normalized = crate::ai::prompts::enhancement_options_for_ai_enabled(
            Some(&stored_options),
            ai_enabled,
        )
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
