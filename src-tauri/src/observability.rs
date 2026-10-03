//! Closed payload schemas and bounded, content-free dictation correlation.
use parking_lot::Mutex;
use posthog_rs::Event;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::LazyLock;

#[derive(Clone, Default)]
pub struct Trace {
    pub generation: u64,
    pub id: String,
    pub properties: Vec<(&'static str, Value)>,
}
static TRACES: LazyLock<Mutex<VecDeque<Trace>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));
pub fn begin(generation: u64) {
    push_trace(
        &mut TRACES.lock(),
        Trace {
            generation,
            id: uuid::Uuid::new_v4().to_string(),
            properties: vec![],
        },
    );
}
fn push_trace(traces: &mut VecDeque<Trace>, trace: Trace) {
    traces.push_back(trace);
    while traces.len() > 5 {
        traces.pop_front();
    }
}
pub fn snapshot(generation: u64) -> Option<Trace> {
    TRACES
        .lock()
        .iter()
        .find(|t| t.generation == generation)
        .cloned()
}
pub fn trace_id(generation: u64) -> Option<String> {
    TRACES
        .lock()
        .iter()
        .find(|t| t.generation == generation)
        .map(|t| t.id.clone())
}
pub fn last_ids() -> Vec<String> {
    TRACES.lock().iter().map(|t| t.id.clone()).collect()
}
pub fn update(generation: u64, key: &'static str, value: Value) {
    if let Some(t) = TRACES
        .lock()
        .iter_mut()
        .find(|t| t.generation == generation)
    {
        t.properties.retain(|(k, _)| *k != key);
        t.properties.push((key, value));
    }
}
pub fn properties(generation: u64) -> Vec<(&'static str, Value)> {
    TRACES
        .lock()
        .iter()
        .find(|t| t.generation == generation)
        .map(|t| t.properties.clone())
        .unwrap_or_default()
}
pub fn inherit(from: u64, to: u64, fallback: Option<Trace>) {
    inherit_trace(&mut TRACES.lock(), from, to, fallback);
}
fn inherit_trace(traces: &mut VecDeque<Trace>, from: u64, to: u64, fallback: Option<Trace>) {
    let original = traces
        .iter()
        .find(|t| t.generation == from)
        .cloned()
        .or(fallback);
    if let Some(mut trace) = original {
        traces.retain(|t| t.generation != to && t.id != trace.id);
        trace.generation = to;
        push_trace(traces, trace);
    }
}

pub const BLOCKED: &[&str] = &[
    "license_check_failed",
    "license_verify_required",
    "trial_ended",
    "no_engine",
    "cloud_key_missing",
    "cloud_key_rejected",
    "soniox_storage_full",
    "mic_permission_denied",
    "mic_missing",
    "mic_busy",
    "accessibility_off",
    "starting_up",
];
pub const ACTIONS: &[&str] = &[
    "open_settings",
    "recheck_license",
    "open_license",
    "open_models",
    "open_cloud_keys",
    "open_storage",
    "open_mic_settings",
    "choose_mic",
    "open_accessibility",
    "dictate",
    "stop",
    "cancel",
    "retry",
    "discard",
    "copy",
    "paste",
];
pub const RECOVERY: &[&str] = &[
    "cloud_failed",
    "network_offline",
    "model_missing",
    "remote_offline",
    "no_speech",
    "integrity",
    "mic_dropped_empty",
    "none",
];
pub const SOURCE: &[&str] = &["island", "tray", "app"];
pub const ENGINE: &[&str] = &["whisper", "parakeet", "cloud", "remote", "none"];
pub const SETTINGS: &[&str] = &[
    "polish",
    "engine",
    "mic",
    "language",
    "mode",
    "live_preview",
];
pub const NATIVE_CODES: &[&str] = &["panel_conversion", "positioning", "show", "hit_testing"];
type Schema = &'static [(&'static str, &'static [&'static str])];
pub fn schema(name: &str) -> Option<Schema> {
    Some(match name {
        "app_started" => &[
            ("version", &[]),
            ("os_version", &[]),
            ("cpu_class", &["small", "medium", "large", "unknown"]),
            ("engine_ready", &[]),
            ("pill_mode", &["always", "when_recording", "never"]),
        ],
        "app_updated" => &[("from", &[]), ("to", &[])],
        "island_state" => &[(
            "state",
            &[
                "mic_dropped",
                "mic_silent",
                "storage_failed",
                "translate_failed",
                "model_fallback",
                "gpu_fallback",
                "polish_skipped",
                "recording_too_short",
                "escape_hint",
                "recovery",
                "finishing",
                "polish_on",
                "polish_off",
                "polish_setup",
                "shortcuts_retired",
                "long_silence",
                "silence_stopped",
                "silence_discarded",
                "recording_failed",
                "copy_failed",
                "transcription_failed",
                "history_retry",
                "shortcut_throttled",
                "no_speech",
            ],
        )],
        "dictation_blocked" => &[("kind", BLOCKED)],
        "island_action" => &[("action", ACTIONS), ("source", &["island", "tray"])],
        "recovery_resolved" => &[
            ("kind", RECOVERY),
            (
                "resolution",
                &[
                    "retried_ok",
                    "retried_failed",
                    "transcribed_anyway",
                    "discarded",
                    "expired",
                ],
            ),
            ("alt_engine_kind", ENGINE),
            ("latency_ms", &[]),
        ],
        "quick_setting_changed" => &[("setting", SETTINGS), ("source", SOURCE)],
        "island_peek_opened" => &[("count", &[]), ("utc_day", &[])],
        "insights_viewed" => &[("period", &["week", "month", "all"])],
        "share_card_action" => &[("action", &["copy", "save", "post_x"])],
        "settings_opened" => &[
            (
                "pane",
                &[
                    "general",
                    "privacy",
                    "shortcuts",
                    "audio",
                    "advanced",
                    "license",
                    "about",
                    "data",
                    "appearance",
                    "storage",
                    "network",
                    "agent",
                ],
            ),
            ("source", SOURCE),
        ],
        "pill_native_error" => &[("code", NATIVE_CODES)],
        _ => return None,
    })
}
pub fn validated(event: &Event) -> Option<Vec<(&'static str, Value)>> {
    let schema = schema(event.event_name())?;
    schema
        .iter()
        .map(|(key, vocabulary)| {
            let value = event.properties().get(*key)?;
            if *key == "engine_ready" {
                value.as_bool()?;
            } else if ["version", "from", "to"].contains(key) {
                if !safe_app_version(value.as_str()?) {
                    return None;
                }
            } else if *key == "os_version" {
                let version = value.as_str()?;
                if version != "unknown" && !safe_numeric_version(version) {
                    return None;
                }
            } else if vocabulary.is_empty() {
                value.as_u64().filter(|n| *n <= 86_400_000)?;
            } else if !vocabulary.contains(&value.as_str()?) {
                return None;
            }
            Some((*key, value.clone()))
        })
        .collect()
}
pub fn emit(name: &'static str, properties: Vec<(&'static str, Value)>, generation: Option<u64>) {
    let mut event = Event::new(String::from(name), String::from("schema-check"));
    for (key, value) in &properties {
        let _ = event.insert_prop(*key, value);
    }
    let Some(properties) = validated(&event) else {
        return;
    };
    crate::product_analytics::capture_at(
        crate::product_analytics::ProductEvent::Observability { name, properties },
        generation.unwrap_or(0),
    );
}
pub fn action(action: &str, source: &str) {
    emit(
        "island_action",
        vec![("action", action.into()), ("source", source.into())],
        Some(crate::commands::audio::current_recording_generation()),
    );
}
pub fn quick(setting: &str, source: &str) {
    emit(
        "quick_setting_changed",
        vec![("setting", setting.into()), ("source", source.into())],
        None,
    );
}
static PEEKS: Mutex<(i64, u64)> = Mutex::new((0, 0));
pub fn peek() {
    if !crate::product_analytics::is_enabled() {
        return;
    }
    let day = chrono::Utc::now().timestamp() / 86400;
    let mut peeks = PEEKS.lock();
    if peeks.0 != day && peeks.1 > 0 {
        emit(
            "island_peek_opened",
            vec![
                ("count", peeks.1.into()),
                ("utc_day", (peeks.0 as u64).into()),
            ],
            None,
        );
        peeks.1 = 0;
    }
    peeks.0 = day;
    peeks.1 = peeks.1.saturating_add(1).min(86_400_000);
}
pub fn clear_peeks() {
    PEEKS.lock().1 = 0;
}
pub fn flush_peeks() {
    let (day, count) = {
        let mut peeks = PEEKS.lock();
        (peeks.0, std::mem::take(&mut peeks.1))
    };
    if count > 0 {
        emit(
            "island_peek_opened",
            vec![("count", count.into()), ("utc_day", (day as u64).into())],
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_catalogue_payload_is_closed() {
        for name in [
            "app_started",
            "app_updated",
            "island_state",
            "dictation_blocked",
            "island_action",
            "recovery_resolved",
            "quick_setting_changed",
            "island_peek_opened",
            "insights_viewed",
            "share_card_action",
            "settings_opened",
            "pill_native_error",
        ] {
            let mut event = Event::new(String::from(name), String::from("id"));
            for (key, vocabulary) in schema(name).unwrap() {
                let value = if *key == "engine_ready" {
                    Value::Bool(true)
                } else if ["version", "from", "to"].contains(key) {
                    Value::from("2.1.0-beta.4")
                } else if *key == "os_version" {
                    Value::from("15.0")
                } else if vocabulary.is_empty() {
                    Value::from(1)
                } else {
                    Value::from(vocabulary[0])
                };
                event.insert_prop(*key, value).unwrap();
            }
            let safe = validated(&event).unwrap();
            let serialized = serde_json::to_string(&safe).unwrap();
            assert!(!serialized.contains("/Users/"));
            for (key, vocabulary) in schema(name).unwrap() {
                if !vocabulary.is_empty() {
                    let mut bad = event.clone();
                    bad.insert_prop(*key, "/Users/private words window title")
                        .unwrap();
                    assert!(validated(&bad).is_none(), "{name}.{key}");
                }
            }
        }
    }
    #[test]
    fn trace_is_random_and_threads_by_generation() {
        let mut traces = VecDeque::new();
        let original = Trace {
            generation: 901,
            id: uuid::Uuid::new_v4().to_string(),
            properties: vec![],
        };
        push_trace(&mut traces, original.clone());
        assert_eq!(
            uuid::Uuid::parse_str(&original.id)
                .unwrap()
                .get_version_num(),
            4
        );
        inherit_trace(&mut traces, 901, 902, None);
        assert_eq!(traces[0].id, original.id);
        for generation in 903..910 {
            push_trace(
                &mut traces,
                Trace {
                    generation,
                    id: uuid::Uuid::new_v4().to_string(),
                    properties: vec![],
                },
            );
        }
        assert_eq!(traces.len(), 5);
        // A kept clip retains its content-free snapshot after the report ring rolls.
        inherit_trace(&mut traces, 902, 910, Some(original.clone()));
        assert_eq!(traces.back().unwrap().id, original.id);
        assert_eq!(traces.back().unwrap().generation, 910);
    }
}

pub const COMPLETION_SCHEMA: Schema = &[
    ("start_source", &["hotkey", "pointer", "tray"]),
    ("mode", &["hold", "toggle"]),
    ("start_card_shown", &[]),
    ("island_start_details", &["always", "changed", "never"]),
    (
        "language",
        &[
            "auto", "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl",
            "ar", "sv", "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da",
            "hu", "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te",
            "fa", "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne",
            "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af",
            "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo", "ht", "ps", "tk",
            "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln", "ha", "ba",
            "jw", "su", "yue", "other",
        ],
    ),
    ("polish_skip_used", &[]),
    ("polish_guard_reason", &["meta_reply", "answered", "none"]),
    ("polish_keep_words", &[]),
    (
        "polish_style",
        &["off", "clean", "writing", "notes", "message", "code"],
    ),
    ("recovery_kind", RECOVERY),
    (
        "recovery_resolution",
        &[
            "none",
            "retried_ok",
            "retried_failed",
            "transcribed_anyway",
            "discarded",
            "expired",
        ],
    ),
    ("focus_safe", &[]),
];
pub fn completion_properties(generation: u64) -> Vec<(&'static str, Value)> {
    let saved = properties(generation);
    COMPLETION_SCHEMA
        .iter()
        .map(|(key, values)| {
            let default = match *key {
                "mode" => Value::from("toggle"),
                "island_start_details" => Value::from("changed"),
                "language" => Value::from("auto"),
                "polish_guard_reason" | "recovery_kind" | "recovery_resolution" => {
                    Value::from("none")
                }
                "polish_style" => Value::from("off"),
                "focus_safe" => Value::Bool(crate::commands::pill_feedback::pill_focus_safe()),
                _ if values.is_empty() => Value::Bool(false),
                _ => Value::from(values[0]),
            };
            let value = saved
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or(default.clone());
            let valid = if values.is_empty() {
                value.is_boolean()
            } else {
                value.as_str().is_some_and(|v| values.contains(&v))
            };
            (
                *key,
                if valid {
                    value
                } else if *key == "language" {
                    Value::from("other")
                } else {
                    default
                },
            )
        })
        .collect()
}

static STARTUP: LazyLock<Mutex<Vec<(&'static str, Value)>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
pub fn startup_properties() -> Vec<(&'static str, Value)> {
    STARTUP.lock().clone()
}
pub async fn startup(app: tauri::AppHandle) {
    use tauri_plugin_store::StoreExt;
    let ready = crate::recognition::recognition_availability_snapshot(&app)
        .await
        .any_available();
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0);
    let cpu = match cores {
        0 => "unknown",
        1..=4 => "small",
        5..=8 => "medium",
        _ => "large",
    };
    let os_version = sysinfo::System::os_version()
        .filter(|v| safe_numeric_version(v))
        .unwrap_or_else(|| "unknown".into());
    let version = env!("CARGO_PKG_VERSION");
    let properties = vec![
        ("version", version.into()),
        ("os_version", os_version.into()),
        ("cpu_class", cpu.into()),
        ("engine_ready", ready.into()),
        (
            "pill_mode",
            crate::commands::pill_feedback::pill_mode(&app).into(),
        ),
    ];
    *STARTUP.lock() = properties;
    crate::product_analytics::capture_at(crate::product_analytics::ProductEvent::AppStarted, 0);
    if let Ok(store) = app.store("settings") {
        if let Some(from) = store
            .get("observability_last_version")
            .and_then(|v| v.as_str().map(str::to_owned))
        {
            if from != version && safe_app_version(&from) {
                emit(
                    "app_updated",
                    vec![("from", from.into()), ("to", version.into())],
                    None,
                );
            }
        }
        store.set("observability_last_version", serde_json::json!(version));
        let _ = store.save();
    }
}
fn safe_numeric_version(value: &str) -> bool {
    value.len() <= 32
        && !value.is_empty()
        && value
            .split('.')
            .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
}
fn safe_app_version(value: &str) -> bool {
    if value.len() > 48 {
        return false;
    }
    let (base, suffix) = value.split_once('-').unwrap_or((value, ""));
    base.split('.').count() == 3
        && safe_numeric_version(base)
        && (suffix.is_empty()
            || ["beta", "rc", "alpha", "dev"].iter().any(|channel| {
                suffix == *channel
                    || suffix
                        .strip_prefix(*channel)
                        .and_then(|rest| rest.strip_prefix('.'))
                        .is_some_and(safe_numeric_version)
            }))
}

#[cfg(test)]
mod completion_contract_tests {
    use super::*;
    #[test]
    fn completion_defaults_and_values_are_closed() {
        let properties = completion_properties(0);
        for (key, values) in COMPLETION_SCHEMA {
            let value = &properties.iter().find(|(k, _)| k == key).unwrap().1;
            if values.is_empty() {
                assert!(value.is_boolean());
            } else {
                assert!(values.contains(&value.as_str().unwrap()));
            }
        }
        assert_eq!(
            properties
                .iter()
                .find(|(k, _)| *k == "polish_guard_reason")
                .unwrap()
                .1,
            "none"
        );
        assert_eq!(
            properties
                .iter()
                .find(|(k, _)| *k == "recovery_kind")
                .unwrap()
                .1,
            "none"
        );
        assert!(!serde_json::to_string(&properties)
            .unwrap()
            .contains("/Users/"));
    }
    #[test]
    fn language_catalogue_is_closed_and_complete() {
        let languages = COMPLETION_SCHEMA
            .iter()
            .find(|(k, _)| *k == "language")
            .unwrap()
            .1;
        for code in crate::whisper::languages::SUPPORTED_LANGUAGES.keys() {
            assert!(languages.contains(code));
        }
    }
    #[test]
    fn lifecycle_versions_reject_free_text() {
        for version in [
            "/Users/alice",
            "My App",
            "2.1.0-private words",
            "2.1.0-private",
            "https://private",
        ] {
            assert!(!safe_app_version(version));
            assert!(!safe_numeric_version(version));
        }
        assert!(safe_app_version("2.1.0-beta.4"));
    }
}
