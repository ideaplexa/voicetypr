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

