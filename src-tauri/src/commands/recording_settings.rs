//! Recording persistence settings and legacy retention compatibility.
use serde_json::json;

pub(super) fn recording_retention_days_from_legacy_count(count: u64) -> Option<u32> {
    match count {
        0 | 250 => None,
        25 => Some(7),
        50 => Some(30),
        100 => Some(90),
        _ => None,
    }
}

pub(crate) fn recording_retention_days_from_store(
    store: &tauri_plugin_store::Store<tauri::Wry>,
) -> Option<u32> {
    if let Some(value) = store.get("recording_retention_days") {
        if value.is_null() {
            return None;
        }
        return value.as_u64().map(|n| n as u32).or(Some(30));
    }

    // Migrate old count-based retention to the new day-based policy conservatively.
    match store
        .get("recording_retention_count")
        .and_then(|value| value.as_u64())
    {
        Some(count) => recording_retention_days_from_legacy_count(count),
        None => {
            if store
                .get("recording_retention_count")
                .map(|value| value.is_null())
                .unwrap_or(false)
            {
                None
            } else {
                Some(30)
            }
        }
    }
}

pub(crate) fn recording_retention_days_to_value(days: Option<u32>) -> serde_json::Value {
    match days {
        Some(days) => json!(days),
        None => serde_json::Value::Null,
    }
}

pub(super) fn default_save_failed() -> bool {
    true
}

const MIGRATED: &str = "only_failed_audio_migrated_v1";

pub(super) fn migrate<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
) -> Result<(), String> {
    if store.get(MIGRATED).and_then(|v| v.as_bool()) != Some(true) {
        // One durable save for both the new default and its marker.
        store.set("save_failed_recordings", json!(true));
        store.set(MIGRATED, json!(true));
        if store.save().is_err() {
            store.delete(MIGRATED);
            return Err("Could not save recording preference".into());
        }
    }
    Ok(())
}

pub(super) fn load<R: tauri::Runtime>(store: &tauri_plugin_store::Store<R>) -> bool {
    if migrate(store).is_err() {
        log::warn!("Recording preference migration could not be saved; will retry");
    }
    save_failed(store)
}

fn save_failed<R: tauri::Runtime>(store: &tauri_plugin_store::Store<R>) -> bool {
    store
        .get("save_failed_recordings")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

pub(super) fn save(store: &tauri_plugin_store::Store<tauri::Wry>, settings: &super::Settings) {
    store.set("save_recordings", json!(settings.save_recordings));
    store.set(
        "save_failed_recordings",
        json!(settings.save_failed_recordings),
    );
    store.set(MIGRATED, json!(true));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_migration_save_does_not_block_settings_reads_and_can_retry() {
        let dir = tempfile::tempdir().unwrap();
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let path = dir.path().join("settings");
        let store = tauri_plugin_store::StoreBuilder::new(app.handle(), &path)
            .build()
            .unwrap();
        store.set("save_recordings", json!(false));
        std::fs::create_dir(&path).unwrap();
        assert!(load(&store));
        assert!(store.get(MIGRATED).is_none());
        std::fs::remove_dir(&path).unwrap();
        assert!(load(&store));
        store.reload().unwrap();
        assert_eq!(store.get(MIGRATED), Some(json!(true)));
    }

    #[test]
    fn migration_is_durable_once_and_preserves_savers_and_later_dont_save() {
        for saves in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let app = tauri::test::mock_builder()
                .plugin(tauri_plugin_store::Builder::default().build())
                .build(tauri::test::mock_context(tauri::test::noop_assets()))
                .unwrap();
            let store =
                tauri_plugin_store::StoreBuilder::new(app.handle(), dir.path().join("settings"))
                    .build()
                    .unwrap();
            store.set("save_recordings", json!(saves));
            store.set("recording_retention_days", json!(14));
            migrate(&store).unwrap();
            store.reload().unwrap();
            assert!(save_failed(&store));
            assert_eq!(store.get("save_recordings"), Some(json!(saves)));
            assert_eq!(store.get("recording_retention_days"), Some(json!(14)));
            store.set("save_recordings", json!(false));
            store.set("save_failed_recordings", json!(false));
            store.save().unwrap();
            store.reload().unwrap();
            migrate(&store).unwrap();
            assert!(!save_failed(&store));
            assert_eq!(store.get(MIGRATED), Some(json!(true)));
        }
    }
}
