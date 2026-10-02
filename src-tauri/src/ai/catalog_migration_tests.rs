use crate::migrate_ai_settings_values;
use serde_json::json;

#[test]
fn refreshed_catalog_preserves_available_saved_ids_and_flags_removed_ones() {
    for (provider, model, valid) in [
        ("openai", "gpt-4.1-nano", false),
        ("gemini", "gemini-2.5-flash-lite", true),
        ("anthropic", "claude-haiku-4-5", true),
        ("openrouter", "google/gemini-2.5-flash-lite", false),
        ("claude-code", "haiku", true),
    ] {
        let mut values = json!({
            "ai_enabled": true, "ai_provider": provider, "ai_model": model,
            "ai_models_by_provider": {provider: model}
        })
        .as_object()
        .unwrap()
        .clone();
        migrate_ai_settings_values(&mut values);
        assert_eq!(values["ai_model"], json!(if valid { model } else { "" }));
        assert_eq!(
            values
                .get("ai_model_needs_reselection")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            !valid
        );
    }
}
