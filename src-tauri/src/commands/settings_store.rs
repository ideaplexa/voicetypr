//! Store adapters for generic settings saves and the surrounding-text preference.
use serde_json::json;

pub(crate) async fn persist_generic_settings<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
) -> Result<(), String> {
    // Native Polish maps are owned by ai::native_settings and the AISettings
    // payload, not Settings. Bulk settings saves must leave those keys intact.
    let _guard = crate::commands::telemetry::CONSENT_MUTATION_LOCK
        .lock()
        .await;
    // Consent belongs to its dedicated command. Generic saves migrate stored
    // choices rather than overwriting them from a potentially stale form.
    let consent = crate::telemetry::migrated_consent(Some(&json!({
        "telemetry_enabled":store.get("telemetry_enabled"),
        "analytics_enabled":store.get("analytics_enabled"),
        "crash_reporting_enabled":store.get("crash_reporting_enabled"),
    })));
    crate::telemetry::save_consent_values(store, consent, false);
    crate::cloud_stt::migrate_soniox_model(store);
    if let Err(error) = store.save() {
        // Reload remains inside the same consent transaction.
        let _ = store.reload();
        return Err(error.to_string());
    }
    crate::commands::ai_lifecycle::invalidate();
    Ok(())
}

pub(super) fn load_polish_context<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
) -> bool {
    store
        .get("polish_context_enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
}

pub(super) fn save_polish_context<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    enabled: bool,
) {
    store.set("polish_context_enabled", json!(enabled));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polish_context_defaults_on_and_round_trips_explicit_choices() {
        let dir = tempfile::tempdir().unwrap();
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let store = tauri_plugin_store::StoreBuilder::new(app.handle(), dir.path().join("settings"))
            .build()
            .unwrap();
        assert!(load_polish_context(&store));
        store.set("polish_context_enabled", json!("invalid"));
        assert!(load_polish_context(&store));
        for enabled in [false, true] {
            save_polish_context(&store, enabled);
            store.save().unwrap();
            store.reload().unwrap();
            assert_eq!(load_polish_context(&store), enabled);
            assert_eq!(store.get("polish_context_enabled"), Some(json!(enabled)));
        }
    }
}
