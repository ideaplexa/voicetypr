//! Content-free key-press snapshot. No titles, transcript, or paths are serialized.
use crate::ai::prompts::EnhancementPreset;
use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;

static LAST: Mutex<Option<ChangeKey>> = Mutex::new(None);
#[derive(PartialEq, Eq)]
struct ChangeKey {
    style: Option<&'static str>,
    mic: String,
    engine: String,
    key_ok: bool,
}
#[derive(Clone, Serialize)]
pub struct DictationContext {
    generation: u64,
    app: App,
    polish: Polish,
    mic: Mic,
    engine: Engine,
    language: Language,
    mode: &'static str,
    changed_since_last: bool,
    show_start_card: bool,
}
#[derive(Clone, Serialize)]
struct App {
    name: String,
    icon_key: Option<String>,
}
#[derive(Clone, Serialize)]
struct Polish {
    will_run: bool,
    style: Option<&'static str>,
    key_ok: bool,
    keep_words: bool,
}
#[derive(Clone, Serialize)]
struct Mic {
    name: String,
    tooltip: String,
    ok: bool,
}
#[derive(Clone, Serialize)]
struct Engine {
    short_name: String,
    kind: &'static str,
}
#[derive(Clone, Serialize)]
struct Language {
    code: String,
    label: String,
}

pub struct Snapshot {
    payload: DictationContext,
    model: String,
    engine_hint: String,
    cloud_model: Option<String>,
    details: String,
}
/// Called once audio flows. `hint` is the app the writing pipeline pinned at
/// start, so foreground changes cannot make the card disagree with the take.
pub fn capture(app: &AppHandle, hint: Option<crate::writing::ContextHint>) -> Option<Snapshot> {
    let store = app.store("settings").ok()?;
    let text = |key: &str, default: &str| {
        store
            .get(key)
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| default.to_owned())
    };
    let hint = hint.unwrap_or_default();
    let name = hint.app_name.clone().unwrap_or_default();
    let icon_key = hint
        .process_path
        .as_deref()
        .and_then(|path| super::icons::register(app, std::path::Path::new(path)));
    let enabled = store
        .get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let key_ok = crate::commands::ai::has_ai_model_and_key(app).unwrap_or(false);
    let options = store.get("enhancement_options");
    let global = crate::ai::prompts::enhancement_options_for_ai_enabled(options.as_ref(), enabled)
        .ok()?
        .preset;
    let writing = crate::writing::load_writing_settings(app).unwrap_or_default();
    // Resolve the requested per-app style with availability enabled, then report
    // real availability separately (an amber badge must retain the selected style).
    let requested = crate::writing::resolve_pipeline_config(
        &writing,
        global,
        "same_as_transcript",
        Some(&hint),
        crate::writing::PipelineAiState {
            stored_ai_enabled: true,
            has_model_and_key: true,
        },
    )
    .preset;
    let style = style(requested);
    let selected = store
        .get("selected_microphone")
        .and_then(|v| v.as_str().map(str::to_owned));
    let host = cpal::default_host();
    // Mirror recorder fallback to the default if the selected device vanished.
    let device = selected
        .as_ref()
        .and_then(|name| {
            crate::audio::recorder::find_input_device(&host, name)
                .ok()
                .flatten()
        })
        .or_else(|| host.default_input_device());
    let mic_name = device.as_ref().and_then(|d| d.name().ok());
    let full_name = mic_name
        .clone()
        .unwrap_or_else(|| selected.unwrap_or_else(|| "Microphone".into()));
    let code = text("speech_language", &text("language", "en"));
    let label = crate::whisper::languages::SUPPORTED_LANGUAGES
        .get(code.as_str())
        .map(|language| language.name)
        .unwrap_or(if code == "auto" { "Auto" } else { &code })
        .to_owned();
    Some(Snapshot {
        payload: DictationContext {
            generation: 0,
            app: App { name, icon_key },
            polish: Polish {
                will_run: enabled && key_ok && style.is_some(),
                style,
                key_ok,
                keep_words: requested == EnhancementPreset::PersonalDictation,
            },
            mic: Mic {
                name: mic_display_name(&full_name),
                tooltip: full_name,
                ok: mic_name.is_some(),
            },
            engine: Engine {
                short_name: String::new(),
                kind: "local",
            },
            language: Language { code, label },
            mode: if text("recording_mode", "toggle") == "push_to_talk" {
                "hold"
            } else {
                "toggle"
            },
            changed_since_last: false,
            show_start_card: false,
        },
        model: text("current_model", ""),
        engine_hint: text("current_model_engine", "whisper"),
        cloud_model: crate::cloud_stt::CloudProvider::from_id(&text(
            "current_model_engine",
            "whisper",
        ))
        .or_else(|| crate::cloud_stt::CloudProvider::from_id(&text("current_model", "")))
        .map(|provider| provider.selected_model(app).id.to_owned()),
        details: text("island_start_details", "changed"),
    })
}
impl Snapshot {
    pub fn with_start_source(
        mut self,
        source: crate::recording::start_source::StartSource,
    ) -> Self {
        if source.is_toggle() {
            self.payload.mode = "toggle";
        }
        self
    }

    pub async fn emit(mut self, app: &AppHandle, generation: u64) {
        let resolved = crate::transcription::engines::resolve_engine_for_model(
            app,
            &self.model,
            Some(&self.engine_hint),
        )
        .await
        .ok();
        // Online LAN selection takes precedence in the actual recording pipeline.
        use tauri::Manager;
        let remote = app.state::<tokio::sync::Mutex<crate::remote::settings::RemoteSettings>>();
        let remote_selection = remote
            .lock()
            .await
            .get_active_connection()
            .filter(|c| matches!(c.status, crate::remote::settings::ConnectionStatus::Online))
            .map(|c| (c.id.clone(), c.model.clone().unwrap_or_default()));
        let remote_online = remote_selection.is_some();
        let remote_identity = remote_selection
            .as_ref()
            .map(|(id, model)| format!("{id}:{model}"));
        let (engine, model) = resolved
            .as_ref()
            .map(|e| (e.engine_name(), e.model_name()))
            .unwrap_or((&self.engine_hint, &self.model));
        self.payload.engine = Engine {
            short_name: if remote_online {
                engine_short_name(&remote_selection.as_ref().unwrap().1, "remote")
            } else {
                engine_short_name(model, engine)
            },
            kind: if remote_online {
                "network"
            } else if crate::cloud_stt::CloudProvider::from_id(engine).is_some() {
                "cloud"
            } else {
                "local"
            },
        };
        if crate::commands::audio::recording_generation_is_stale(generation) {
            return;
        }
        let key = ChangeKey {
            style: self.payload.polish.style,
            mic: self.payload.mic.tooltip.clone(),
            engine: format!(
                "{}:{}:{}",
                self.payload.engine.kind,
                engine,
                remote_identity
                    .as_deref()
                    .or(self.cloud_model.as_deref())
                    .unwrap_or(model)
            ),
            key_ok: self.payload.polish.key_ok,
        };
        let mut last = LAST.lock().unwrap();
        self.payload.changed_since_last = last.as_ref() != Some(&key);
        *last = Some(key);
        self.payload.generation = generation;
        if let Ok(store) = app.store("settings") {
            let shown = store
                .get("island_start_details_shown")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .min(5);
            self.payload.show_start_card =
                show_start_card(&self.details, shown, self.payload.changed_since_last);
            if shown < 5 {
                store.set("island_start_details_shown", serde_json::json!(shown + 1));
                let _ = store.save();
            }
        }
        for (key, value) in [
            ("mode", serde_json::json!(self.payload.mode)),
            (
                "start_card_shown",
                serde_json::json!(self.payload.show_start_card),
            ),
            ("island_start_details", serde_json::json!(self.details)),
            ("language", serde_json::json!(self.payload.language.code)),
            (
                "polish_keep_words",
                serde_json::json!(self.payload.polish.keep_words),
            ),
            (
                "polish_style",
                serde_json::json!(self.payload.polish.style.unwrap_or("off").to_lowercase()),
            ),
        ] {
            crate::observability::update(generation, key, value);
        }
        let _ = app.emit_to("pill", "dictation-context", self.payload);
    }
}
#[derive(Serialize)]
pub struct EffectivePolish {
    generation: u64,
    app_name: String,
    style: Option<&'static str>,
    will_run: bool,
    overridden: bool,
}
pub fn effective_polish(app: &AppHandle) -> Option<EffectivePolish> {
    use tauri::Manager;
    let hint = app
        .state::<crate::AppState>()
        .recording_app_context
        .lock()
        .ok()?
        .clone()?;
    let store = app.store("settings").ok()?;
    let enabled = store
        .get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let options = store.get("enhancement_options");
    let global = crate::ai::prompts::enhancement_options_for_ai_enabled(options.as_ref(), enabled)
        .ok()?
        .preset;
    let writing = crate::writing::load_writing_settings(app).ok()?;
    let requested = crate::writing::resolve_pipeline_config(
        &writing,
        global,
        "same_as_transcript",
        Some(&hint),
        crate::writing::PipelineAiState {
            stored_ai_enabled: true,
            has_model_and_key: true,
        },
    )
    .preset;
    let name = hint.app_name.unwrap_or_default();
    let overridden = writing.app_formatting_rules.iter().any(|rule| {
        rule.enabled
            && name
                .to_ascii_lowercase()
                .contains(&rule.app_name.trim().to_ascii_lowercase())
    });
    let style = style(requested);
    Some(EffectivePolish {
        generation: crate::commands::audio::current_recording_generation(),
        app_name: name,
        style,
        will_run: enabled
            && crate::commands::ai::has_ai_model_and_key(app).unwrap_or(false)
            && style.is_some(),
        overridden,
    })
}
fn style(preset: EnhancementPreset) -> Option<&'static str> {
    match preset {
        EnhancementPreset::PersonalDictation => None,
        EnhancementPreset::CleanDictation => Some("clean"),
        EnhancementPreset::Writing => Some("writing"),
        EnhancementPreset::Notes => Some("notes"),
        EnhancementPreset::Message => Some("message"),
        EnhancementPreset::Code => Some("code"),
    }
}
pub fn show_start_card(setting: &str, shown: u64, changed: bool) -> bool {
    match setting {
        "always" => true,
        "never" => false,
        _ => shown < 5 || changed,
    }
}
pub fn mic_display_name(name: &str) -> String {
    let prefix = name.split(" (").next().unwrap_or(name);
    if matches!(
        prefix,
        "Microphone" | "Microphone Array" | "Headset Microphone" | "Internal Microphone"
    ) {
        prefix.to_owned()
    } else {
        name.to_owned()
    }
}
/// Shared by island and tray. Catalog identifiers never become raw user-facing labels.
pub fn engine_short_name(model: &str, engine: &str) -> String {
    if engine == "remote" {
        return if model.is_empty() || model == "remote" {
            "Network".into()
        } else {
            engine_short_name(model, "")
        };
    }
    if model.starts_with("nemotron") {
        return "Nemotron".into();
    }
    if model.starts_with("parakeet-unified") {
        return "Parakeet Unified".into();
    }
    if model.ends_with("-v2") && model.starts_with("parakeet") {
        return "Parakeet V2".into();
    }
    if engine == "parakeet" || model.starts_with("parakeet") {
        return "Parakeet".into();
    }
    if let Some(provider) = crate::cloud_stt::CloudProvider::from_id(engine)
        .or_else(|| crate::cloud_stt::CloudProvider::from_id(model))
    {
        return provider.display_name().to_owned();
    }
    if model.contains("turbo") {
        return "Whisper Turbo".into();
    }
    let size = ["large", "medium", "small", "base", "tiny"]
        .into_iter()
        .find(|s| model.contains(s));
    size.map(|s| format!("Whisper {}{}", s[..1].to_uppercase(), &s[1..]))
        .unwrap_or_else(|| "Whisper".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn change_key_tracks_all_four_inputs() {
        let previous = ChangeKey {
            style: Some("clean"),
            mic: "Mic A".into(),
            engine: "local:whisper:large-v3".into(),
            key_ok: true,
        };
        let same = ChangeKey {
            style: Some("clean"),
            mic: "Mic A".into(),
            engine: "local:whisper:large-v3".into(),
            key_ok: true,
        };
        assert!(previous == same);
        for changed in [
            ChangeKey {
                style: Some("notes"),
                ..same_key()
            },
            ChangeKey {
                mic: "Mic B".into(),
                ..same_key()
            },
            ChangeKey {
                engine: "cloud:soniox:soniox".into(),
                ..same_key()
            },
            ChangeKey {
                key_ok: false,
                ..same_key()
            },
        ] {
            assert!(previous != changed);
        }
        fn same_key() -> ChangeKey {
            ChangeKey {
                style: Some("clean"),
                mic: "Mic A".into(),
                engine: "local:whisper:large-v3".into(),
                key_ok: true,
            }
        }
    }
    #[test]
    fn event_serialization_is_content_free() {
        let context = DictationContext {
            generation: 42,
            app: App {
                name: "Editor".into(),
                icon_key: Some("opaque".into()),
            },
            polish: Polish {
                will_run: true,
                style: Some("code"),
                key_ok: true,
                keep_words: false,
            },
            mic: Mic {
                name: "Microphone Array".into(),
                tooltip: "Microphone Array (Driver)".into(),
                ok: true,
            },
            engine: Engine {
                short_name: "Whisper Turbo".into(),
                kind: "local",
            },
            language: Language {
                code: "en".into(),
                label: "English".into(),
            },
            mode: "hold",
            changed_since_last: true,
            show_start_card: true,
        };
        let value = serde_json::to_value(context).unwrap();
        assert_eq!(value["generation"], 42);
        assert_eq!(
            value["app"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            ["icon_key", "name"]
        );
        assert_eq!(value["polish"]["style"], "code");
        for forbidden in [
            "title",
            "window_title",
            "path",
            "process_path",
            "transcript",
            "api_key",
        ] {
            assert!(!value.to_string().contains(forbidden));
        }
    }
    #[test]
    fn details_policy_and_display_names() {
        for count in 0..5 {
            assert!(show_start_card("changed", count, false));
        }
        assert!(!show_start_card("changed", 5, false));
        assert!(show_start_card("changed", 5, true));
        assert!(show_start_card("always", 5, false));
        assert!(!show_start_card("never", 0, true));
        assert_eq!(
            mic_display_name("Microphone Array (Intel® Smart Sound)"),
            "Microphone Array"
        );
        assert_eq!(mic_display_name("Shure (USB)"), "Shure (USB)");
        assert_eq!(
            engine_short_name("large-v3-turbo", "whisper"),
            "Whisper Turbo"
        );
        assert_eq!(
            engine_short_name("parakeet-tdt-0.6b-v3", "parakeet"),
            "Parakeet"
        );
    }
}
