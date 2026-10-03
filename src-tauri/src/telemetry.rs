//! Coded exceptions only. No raw errors, provider bodies, panic strings or paths.
use parking_lot::Mutex;
use posthog_rs::Event;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::LazyLock;

pub const KEY_TELEMETRY_ENABLED: &str = "telemetry_enabled";
pub const KEY_TELEMETRY_INSTALL_ID: &str = "telemetry_install_id";
pub const TELEMETRY_DEFAULT_ENABLED: bool = true;
pub const CODES: &[&str] = &[
    "panic",
    "frontend_error",
    "pill_panel_conversion",
    "pill_positioning",
    "pill_show",
    "pill_hit_testing",
    "retry_failed",
    "kept_storage_failed",
    "island_action_failed",
    "model_load_failed",
    "cloud_stt_failed",
    "polish_provider_failed",
    "audio_device_failed",
    "transcription_failed",
    "paste_failed",
];
static COUNTS: LazyLock<Mutex<HashMap<String, u8>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
pub fn migrated_consent(value: Option<&Value>) -> bool {
    let Some(root) = value else {
        return TELEMETRY_DEFAULT_ENABLED;
    };
    // A saved unified preference wins after migration deletes legacy keys.
    [
        "telemetry_enabled",
        crate::product_analytics::KEY_ANALYTICS_ENABLED,
        "crash_reporting_enabled",
    ]
    .iter()
    .all(|key| root.get(*key).and_then(Value::as_bool) != Some(false))
}
pub fn is_available() -> bool {
    crate::product_analytics::is_available()
}
pub fn is_enabled() -> bool {
    crate::product_analytics::is_enabled()
}
pub fn disable_and_drop_queued() {
    crate::product_analytics::disable();
}
fn permit(counts: &mut HashMap<String, u8>, code: &str) -> bool {
    if !CODES.contains(&code) {
        return false;
    }
    let count = counts.entry(code.into()).or_default();
    if *count >= 3 {
        return false;
    }
    *count += 1;
    true
}
#[derive(Debug)]
struct AppError(&'static str);
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for AppError {}
#[derive(Default)]
pub struct ErrorContext {
    pub generation: Option<u64>,
    pub provider: Option<&'static str>,
    pub provider_code: Option<&'static str>,
    pub panic_location: Option<(String, u32, u32)>,
}
pub const PROVIDERS: &[&str] = &[
    "soniox",
    "deepgram",
    "openai",
    "groq",
    "cohere",
    "pi",
    "claude",
    "omp",
    "anthropic",
    "gemini",
    "openrouter",
    "ollama",
    "custom",
    "unknown",
];
pub const PROVIDER_CODES: &[&str] = &[
    "timeout",
    "rate_limited",
    "network",
    "unauthorized",
    "unavailable",
    "cancelled",
    "invalid_response",
    "cloud_storage_limit",
    "transport",
    "auth",
    "model_unavailable",
    "engine_failed",
    "remote_auth",
    "remote_timeout",
    "remote_connect",
    "remote_http",
    "remote_response",
    "remote_internal",
    "unknown",
];
pub fn capture_error(code: &'static str, context: ErrorContext) {
    let Some(mut options) = crate::product_analytics::exception_options(code, context.generation)
    else {
        return;
    };
    options = options.level(if code == "panic" { "fatal" } else { "error" });
    if !permit(&mut COUNTS.lock(), code) {
        return;
    }
    if let Some(provider) = context.provider.filter(|p| PROVIDERS.contains(p)) {
        let Ok(next) = options.property("provider", provider) else {
            return;
        };
        options = next;
    }
    if let Some(provider_code) = context.provider_code.filter(|p| PROVIDER_CODES.contains(p)) {
        let Ok(next) = options.property("provider_code", provider_code) else {
            return;
        };
        options = next;
    }
    if let Some((file, line, column)) = context.panic_location {
        if let Some(file) = crate_location(&file) {
            let Ok(next) = options
                .property("panic_file", file)
                .and_then(|o| o.property("panic_line", line))
                .and_then(|o| o.property("panic_column", column))
            else {
                return;
            };
            options = next.level("fatal");
        }
    }
    let _ = posthog_rs::capture_exception_with(&AppError(code), options);
}
pub fn current_context() -> ErrorContext {
    ErrorContext {
        generation: Some(crate::commands::audio::current_recording_generation()),
        ..Default::default()
    }
}
pub fn capture_frontend_error(_name: Option<&str>, _message: &str) {
    capture_error("frontend_error", current_context());
}
pub fn capture_transcription_failure(
    engine: &str,
    _model: &str,
    _backend: Option<&str>,
    failure_class: &str,
    _duration_ms: Option<u64>,
    generation: u64,
) {
    let provider = PROVIDERS.iter().copied().find(|p| *p == engine);
    capture_error(
        if provider.is_some() {
            "cloud_stt_failed"
        } else {
            "transcription_failed"
        },
        ErrorContext {
            provider,
            provider_code: Some(
                PROVIDER_CODES
                    .iter()
                    .copied()
                    .find(|c| *c == failure_class)
                    .unwrap_or("unknown"),
            ),
            generation: Some(generation),
            ..Default::default()
        },
    );
}
pub fn capture_model_load_failure(_model_name: &str) {
    capture_error("model_load_failed", current_context());
}

/// Rebuild stamped helper payloads and reject the SDK's raw panic duplicate.
/// No raw exception chain or debug-image paths survive.
pub(crate) fn scrub_exception_event(event: Event) -> Option<Event> {
    if !crate::product_analytics::exception_allowed(&event) {
        return None;
    }
    // The SDK duplicate has no capture-time consent epoch. Only our stamped
    // helper event can cross the queue boundary, including for panic capture.
    let code = event.properties().get("code")?.as_str()?;
    let panic = code == "panic";
    if !CODES.contains(&code) {
        return None;
    }
    let install_id = crate::product_analytics::install_id()?;
    let fallback_trace = if panic {
        crate::observability::trace_id(crate::commands::audio::current_recording_generation())
    } else {
        None
    };
    Some(rebuild_exception(
        &event,
        code,
        panic,
        install_id,
        fallback_trace,
    ))
}
fn rebuild_exception(
    event: &Event,
    code: &str,
    panic: bool,
    install_id: String,
    fallback_trace: Option<String>,
) -> Event {
    let mut clean = Event::new("$exception".into(), install_id);
    let _ = clean.insert_prop("$process_person_profile", false);
    let _ = clean.insert_prop("$geoip_disable", true);
    let _ = clean.insert_prop("app_version", env!("CARGO_PKG_VERSION"));
    let _ = clean.insert_prop("release_channel", crate::release_channel::RELEASE_CHANNEL);
    let _ = clean.insert_prop("os", std::env::consts::OS);
    let _ = clean.insert_prop("arch", std::env::consts::ARCH);
    let _ = clean.insert_prop("code", code);
    let _ = clean.insert_prop("$exception_fingerprint", code);
    let _ = clean.insert_prop("$exception_level", if panic { "fatal" } else { "error" });
    let mut item = json!({"type": if panic {"panic"} else {"AppError"}, "value":code,
        "mechanism":{"type":if panic {"panic"} else {"generic"},"handled":!panic,"synthetic":false}});
    let mut frames = scrub_frames(event);
    if panic {
        if let Some(file) = event
            .properties()
            .get("panic_file")
            .or_else(|| event.properties().get("$exception_panic_file"))
            .and_then(Value::as_str)
        {
            let mut location = Event::new(String::from("$exception"), String::from("location"));
            let _ = location.insert_prop(
                "$exception_list",
                json!([{"stacktrace":{"frames":[{
                    "filename":file, "lineno":event.properties().get("panic_line").or_else(|| event.properties().get("$exception_panic_line"))
                }]}}]),
            );
            frames.extend(
                scrub_frames(&location)
                    .into_iter()
                    .filter(|f| f.get("filename").is_some()),
            );
        }
    }
    if !frames.is_empty() {
        item["stacktrace"] = json!({"type":"raw","frames":frames});
    }
    let _ = clean.insert_prop("$exception_list", vec![item]);
    for (key, vocabulary) in [("provider", PROVIDERS), ("provider_code", PROVIDER_CODES)] {
        if let Some(value) = event
            .properties()
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| vocabulary.contains(v))
        {
            let _ = clean.insert_prop(key, value);
        }
    }
    let id = event
        .properties()
        .get("dictation_id")
        .and_then(Value::as_str)
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .filter(|id| id.get_version_num() == 4)
        .map(|id| id.to_string())
        .or(fallback_trace);
    if let Some(id) = id {
        let _ = clean.insert_prop("dictation_id", id);
    }
    clean
}
fn scrub_frames(event: &Event) -> Vec<Value> {
    event
        .properties()
        .get("$exception_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("stacktrace")?.get("frames")?.as_array())
        .flatten()
        .take(128)
        .map(|frame| {
            let mut clean = json!({"lang":"rust","platform":"native","in_app":false});
            if let Some(function) = frame.get("function").and_then(Value::as_str).filter(|s| {
                (s.starts_with("voicetypr::") || s.starts_with("voicetypr_lib::"))
                    && s.len() <= 512
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_:<> ,&()".contains(c))
            }) {
                clean["function"] = function.into();
            }
            if let Some(filename) = frame.get("filename").and_then(Value::as_str) {
                if let Some(relative) = crate_location(filename) {
                    clean["filename"] = relative.into();
                    clean["in_app"] = true.into();
                }
            }
            for key in ["lineno", "colno"] {
                if let Some(n) = frame.get(key).and_then(Value::as_u64) {
                    clean[key] = n.into();
                }
            }
            clean
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_consent_is_on() {
        assert!(migrated_consent(None));
    }
    #[test]
    fn either_off_stays_off() {
        for key in [
            "telemetry_enabled",
            "analytics_enabled",
            "crash_reporting_enabled",
        ] {
            assert!(!migrated_consent(Some(&json!({key:false}))));
        }
    }
    #[test]
    fn both_on_stays_on() {
        assert!(migrated_consent(Some(
            &json!({"telemetry_enabled":true,"analytics_enabled":true})
        )));
    }
    #[test]
    fn malformed_consent_defaults_on() {
        assert!(migrated_consent(Some(&json!({"telemetry_enabled":"no"}))));
    }
    #[test]
    fn dedupe_max_three() {
        let mut counts = HashMap::new();
        for _ in 0..3 {
            assert!(permit(&mut counts, "panic"));
        }
        assert!(!permit(&mut counts, "panic"));
    }
    #[test]
    fn dedupe_is_per_code() {
        let mut counts = HashMap::new();
        for _ in 0..3 {
            permit(&mut counts, "panic");
        }
        assert!(permit(&mut counts, "retry_failed"));
    }
    #[test]
    fn unknown_codes_rejected() {
        assert!(!permit(&mut HashMap::new(), "/Users/words"));
    }
    fn frame_event(frame: Value) -> Event {
        let mut event = Event::new(String::from("$exception"), String::from("id"));
        event
            .insert_prop(
                "$exception_list",
                json!([{"value":"private panic words","stacktrace":{"frames":[frame]}}]),
            )
            .unwrap();
        event
    }
    #[test]
    fn panic_payload_not_in_frames() {
        assert!(
            !serde_json::to_string(&scrub_frames(&frame_event(json!({}))))
                .unwrap()
                .contains("private")
        );
    }
    #[test]
    fn absolute_paths_dropped() {
        assert!(!serde_json::to_string(&scrub_frames(&frame_event(
            json!({"filename":"/Users/alice/secret.rs"})
        )))
        .unwrap()
        .contains("Users"));
    }
    #[test]
    fn windows_paths_dropped() {
        assert!(!serde_json::to_string(&scrub_frames(&frame_event(
            json!({"filename":"C:\\Users\\alice\\secret.rs"})
        )))
        .unwrap()
        .contains("alice"));
    }
    #[test]
    fn crate_relative_location_retained() {
        assert_eq!(
            scrub_frames(&frame_event(
                json!({"filename":"/Users/build/src-tauri/src/lib.rs","lineno":42})
            ))[0]["filename"],
            "src/lib.rs"
        );
    }
    #[test]
    fn dependency_paths_dropped() {
        assert!(scrub_frames(&frame_event(
            json!({"filename":"/Users/build/.cargo/registry/src/lib.rs"})
        ))[0]
            .get("filename")
            .is_none());
    }
    #[test]
    fn frame_locals_dropped() {
        assert!(
            scrub_frames(&frame_event(json!({"vars":{"text":"words"}})))[0]
                .get("vars")
                .is_none()
        );
    }
    #[test]
    fn unsafe_symbols_dropped() {
        assert!(
            scrub_frames(&frame_event(json!({"function":"private/path"})))[0]
                .get("function")
                .is_none()
        );
    }
    #[test]
    fn safe_symbols_retained() {
        assert_eq!(
            scrub_frames(&frame_event(json!({"function":"voicetypr::run"})))[0]["function"],
            "voicetypr::run"
        );
    }
    #[test]
    fn traversal_dropped() {
        assert!(
            scrub_frames(&frame_event(json!({"filename":"src/../../secret"})))[0]
                .get("filename")
                .is_none()
        );
    }
    #[test]
    fn codes_are_fixed_content_free_values() {
        for code in CODES {
            assert!(code.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }
    #[test]
    fn provider_bodies_are_not_context() {
        assert!(!PROVIDER_CODES.contains(&"private words"));
    }
}
pub fn native_error(code: &'static str) {
    if !is_enabled() {
        return;
    }
    static NATIVE_COUNTS: LazyLock<Mutex<HashMap<&'static str, u8>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    if !crate::observability::NATIVE_CODES.contains(&code) {
        return;
    }
    let mut counts = NATIVE_COUNTS.lock();
    let count = counts.entry(code).or_default();
    if *count >= 3 {
        return;
    }
    *count += 1;
    drop(counts);
    crate::observability::emit(
        "pill_native_error",
        vec![("code", code.into())],
        Some(crate::commands::audio::current_recording_generation()),
    );
    let error = match code {
        "panel_conversion" => "pill_panel_conversion",
        "positioning" => "pill_positioning",
        "show" => "pill_show",
        _ => "pill_hit_testing",
    };
    capture_error(error, current_context());
}

pub(crate) fn crate_location(filename: &str) -> Option<String> {
    let normalized = filename.replace(char::from(92), "/");
    let relative = normalized
        .rsplit_once("/src-tauri/src/")
        .map(|(_, r)| format!("src/{r}"))
        .or_else(|| normalized.starts_with("src/").then(|| normalized.clone()))?;
    if relative.len() > 160
        || relative.contains("..")
        || !relative
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/_-.".contains(c))
    {
        return None;
    }
    Some(relative)
}
pub(crate) fn local_panic_record(file: Option<(&str, u32)>, time: &str) -> String {
    let location = file
        .and_then(|(file, line)| crate_location(file).map(|file| format!("{file}:{line}")))
        .unwrap_or_else(|| "unknown".to_string());
    format!("Panic at {location}\nTime: {time}\n")
}

pub fn install_coded_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut context = current_context();
        context.panic_location = info
            .location()
            .map(|l| (l.file().to_owned(), l.line(), l.column()));
        capture_error("panic", context);
        // The SDK hook performs a bounded flush; its raw duplicate is dropped.
        previous(info);
    }));
}

/// Shared by generic settings migration and the dedicated consent command.
/// Generic saves preserve consent and never acknowledge on the user's behalf.
pub fn save_consent_values<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    enabled: bool,
    acknowledge: bool,
) -> Option<String> {
    let existing = store
        .get(crate::product_analytics::KEY_ANALYTICS_INSTALL_ID)
        .or_else(|| store.get(KEY_TELEMETRY_INSTALL_ID))
        .and_then(|v| v.as_str().and_then(|s| uuid::Uuid::parse_str(s).ok()))
        .filter(|id| id.get_version_num() == 4)
        .map(|id| id.to_string());
    let id = if enabled {
        existing.or_else(|| acknowledge.then(|| uuid::Uuid::new_v4().to_string()))
    } else {
        None
    };
    store.set(KEY_TELEMETRY_ENABLED, json!(enabled));
    if acknowledge {
        store.set(
            crate::product_analytics::KEY_PRIVACY_CONSENT_VERSION,
            json!(crate::product_analytics::PRIVACY_CONSENT_VERSION),
        );
    }
    for key in [
        crate::product_analytics::KEY_ANALYTICS_ENABLED,
        crate::product_analytics::KEY_ANALYTICS_INSTALL_ID,
        "crash_reporting_enabled",
    ] {
        store.delete(key);
    }
    if let Some(id) = &id {
        store.set(KEY_TELEMETRY_INSTALL_ID, json!(id));
    } else {
        store.delete(KEY_TELEMETRY_INSTALL_ID);
    }
    id
}
#[cfg(test)]
mod migration_tests {
    use super::*;
    #[test]
    fn migration_deletes_old_keys_and_preserves_either_off() {
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store =
            tauri_plugin_store::StoreBuilder::new(app.handle(), dir.path().join("settings"))
                .build()
                .unwrap();
        for (analytics, crashes) in [(true, true), (true, false), (false, true), (false, false)] {
            store.clear();
            store.set("analytics_enabled", json!(analytics));
            store.set("telemetry_enabled", json!(crashes));
            store.set(
                "analytics_install_id",
                json!(uuid::Uuid::new_v4().to_string()),
            );
            let root: serde_json::Map<String, Value> = store.entries().into_iter().collect();
            let enabled = migrated_consent(Some(&Value::Object(root)));
            assert_eq!(enabled, analytics && crashes);
            save_consent_values(&store, enabled, true);
            assert_eq!(
                store.get("telemetry_enabled"),
                Some(json!(analytics && crashes))
            );
            for key in [
                "analytics_enabled",
                "analytics_install_id",
                "crash_reporting_enabled",
            ] {
                assert!(store.get(key).is_none());
            }
            assert_eq!(store.get("telemetry_install_id").is_some(), enabled);
            store.save().unwrap();
            let saved: Value =
                serde_json::from_slice(&std::fs::read(dir.path().join("settings")).unwrap())
                    .unwrap();
            assert!(saved.get("analytics_enabled").is_none());
        }
    }
    #[test]
    fn generic_save_does_not_acknowledge_or_enable_a_legacy_opt_out() {
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store =
            tauri_plugin_store::StoreBuilder::new(app.handle(), dir.path().join("settings"))
                .build()
                .unwrap();
        store.set("analytics_enabled", json!(false));
        save_consent_values(&store, false, false);
        assert!(store.get("privacy_consent_version").is_none());
        assert_eq!(store.get("telemetry_enabled"), Some(json!(false)));
        assert!(store.get("telemetry_install_id").is_none());
    }
}

#[cfg(test)]
mod payload_tests {
    use super::*;
    #[test]
    fn every_exception_code_serializes_without_untrusted_sdk_fields() {
        let id = uuid::Uuid::new_v4().to_string();
        for code in CODES {
            let mut event = Event::new(String::from("$exception"), String::from("/Users/alice"));
            event.insert_prop("$exception_list",json!([{"type":"private words", "value":"private transcript",
                "stacktrace":{"frames":[{"filename":"/Users/alice/private","function":"private/path",
                    "vars":{"prompt":"private prompt"},"context_line":"private transcript"}]}}])).unwrap();
            for key in [
                "transcript",
                "audio",
                "clipboard",
                "prompt",
                "key",
                "path",
                "app_name",
                "window_title",
                "$debug_images",
                "provider",
                "provider_code",
            ] {
                event
                    .insert_prop(key, "private words /Users/alice My App")
                    .unwrap();
            }
            let clean = rebuild_exception(&event, code, *code == "panic", id.clone(), None);
            let serialized = serde_json::to_string(&clean).unwrap();
            assert!(!serialized.contains("private"), "{code}");
            assert!(!serialized.contains("alice"), "{code}");
            assert_eq!(clean.properties()["code"], *code);
            assert_eq!(clean.properties()["$exception_list"][0]["value"], *code);
        }
    }
    #[test]
    fn panic_payload_is_replaced_and_location_is_crate_relative() {
        let mut event = Event::new(String::from("$exception"), String::from("id"));
        event
            .insert_prop(
                "$exception_list",
                json!([{"value":"real dictation private words"}]),
            )
            .unwrap();
        event
            .insert_prop(
                "$exception_panic_file",
                "/Users/alice/build/src-tauri/src/commands/audio.rs",
            )
            .unwrap();
        event.insert_prop("$exception_panic_line", 123).unwrap();
        let clean = rebuild_exception(
            &event,
            "panic",
            true,
            uuid::Uuid::new_v4().to_string(),
            None,
        );
        let serialized = serde_json::to_string(&clean).unwrap();
        assert!(!serialized.contains("dictation private"));
        assert!(!serialized.contains("alice"));
        assert_eq!(
            clean.properties()["$exception_list"][0]["stacktrace"]["frames"][0]["filename"],
            "src/commands/audio.rs"
        );
    }
    #[test]
    fn exception_keeps_the_explicit_trace_without_using_a_person_id() {
        let id = uuid::Uuid::new_v4().to_string();
        let install = uuid::Uuid::new_v4().to_string();
        let mut event = Event::new("$exception".into(), install.clone());
        event.insert_prop("dictation_id", &id).unwrap();
        let clean = rebuild_exception(&event, "retry_failed", false, install.clone(), None);
        assert_eq!(clean.properties()["dictation_id"], id);
        let serialized = serde_json::to_value(clean).unwrap();
        assert_eq!(serialized["distinct_id"], install);
    }
}

#[cfg(test)]
mod local_panic_tests {
    use super::*;
    #[test]
    fn crash_file_has_only_relative_location_and_time() {
        let record = local_panic_record(
            Some(("/Users/private/project/src-tauri/src/commands/audio.rs", 42)),
            "2026-10-03T00:00:00Z",
        );
        assert_eq!(
            record,
            "Panic at src/commands/audio.rs:42\nTime: 2026-10-03T00:00:00Z\n"
        );
        assert!(!record.contains("/Users/"));
        assert!(!record.contains("Full info"));
        assert_eq!(
            local_panic_record(Some(("/private/dependency.rs", 1)), "time"),
            "Panic at unknown\nTime: time\n"
        );
    }
}
