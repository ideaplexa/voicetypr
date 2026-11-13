//! Store adapters for the surrounding-text preference.
use serde_json::json;

pub(super) fn load_polish_context<R: tauri::Runtime>(store: &tauri_plugin_store::Store<R>) -> bool {
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
        let store =
            tauri_plugin_store::StoreBuilder::new(app.handle(), dir.path().join("settings"))
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
