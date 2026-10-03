//! Tray actions share the app's command paths; never log menu IDs or content.
use crate::{
    ai::prompts::{EnhancementOptions, EnhancementPreset},
    commands::settings::{get_settings, save_settings, Settings},
};
use tauri::{AppHandle, Emitter, Manager};
pub fn style_label(preset: EnhancementPreset) -> &'static str {
    match preset {
        EnhancementPreset::PersonalDictation => "Off",
        EnhancementPreset::CleanDictation => "Clean",
        EnhancementPreset::Writing => "Writing",
        EnhancementPreset::Notes => "Notes",
        EnhancementPreset::Message => "Message",
        EnhancementPreset::Code => "Code",
    }
}
pub fn style_preset(label: &str) -> Option<EnhancementPreset> {
    [
        EnhancementPreset::PersonalDictation,
        EnhancementPreset::CleanDictation,
        EnhancementPreset::Writing,
        EnhancementPreset::Notes,
        EnhancementPreset::Message,
        EnhancementPreset::Code,
    ]
    .into_iter()
    .find(|p| style_label(*p) == label)
}
pub fn apply_quick_setting(settings: &mut Settings, id: &str) -> bool {
    if let Some(language) = id.strip_prefix("language_") {
        settings.speech_language = language.into();
    } else {
        match id {
            "live_preview" => {
                settings.transcription_mode = if settings.transcription_mode == "live_preview" {
                    "regular"
                } else {
                    "live_preview"
                }
                .into()
            }
            "recording_mode_toggle" => settings.recording_mode = "toggle".into(),
            "recording_mode_push_to_talk" => settings.recording_mode = "push_to_talk".into(),
            _ => return false,
        }
    }
    true
}
#[derive(Clone, serde::Serialize)]
struct MainNavigate {
    screen: &'static str,
    pane: Option<&'static str>,
    source: Option<&'static str>,
}
fn destination(id: &str) -> Option<MainNavigate> {
    let (screen, pane, source) = match id {
        "nav_home" => ("home", None, None),
        "nav_insights" => ("insights", None, None),
        "nav_settings" => ("settings", Some("general"), None),
        "nav_polish" => ("polish", None, None),
        "nav_models" => ("transcription", None, Some("local")),
        "nav_transcription" => ("transcription", None, None),
        "nav_file" => ("audio", None, None),
        "nav_help" => ("help", None, None),
        _ => return None,
    };
    Some(MainNavigate {
        screen,
        pane,
        source,
    })
}
pub fn handle(app: &AppHandle, id: &str) {
    let app = app.clone();
    let id = id.to_owned();
    tauri::async_runtime::spawn(async move {
        if run(app.clone(), &id).await.is_err() {
            // Keep error payloads content-free, including clipboard/plugin errors.
            let _ = app.emit(
                "tray-action-error",
                "Could not complete the menu action. Please try again.",
            );
        }
    });
}
pub(crate) async fn run(app: AppHandle, id: &str) -> Result<(), String> {
    if let Some(destination) = destination(id) {
        crate::commands::window::focus_main_window(app.clone()).await?;
        return app
            .emit_to("main", "main-navigate", destination)
            .map_err(|_| "Navigation failed".into());
    }
    if id == "quit" {
        app.exit(0);
        return Ok(());
    }
    if id == "check_updates" {
        crate::commands::window::focus_main_window(app.clone()).await?;
        return app
            .emit_to("main", "tray-check-updates", ())
            .map_err(|_| "Update check failed".into());
    }
    if id == "dictate" {
        if app.state::<crate::AppState>().get_current_state() == crate::RecordingState::Recording {
            crate::commands::audio::stop_recording(
                app.clone(),
                app.state::<crate::commands::audio::RecorderState>(),
            )
            .await?;
        } else {
            crate::commands::audio::start_recording(
                app.clone(),
                app.state::<crate::commands::audio::RecorderState>(),
            )
            .await?;
        }
        return Ok(());
    }
    if id == "fix" {
        if let Some(action) = super::runtime::fix_action() {
            Box::pin(crate::recording::island::island_action(app, action)).await?;
        }
        return Ok(());
    }
    if let Some(id) = id.strip_prefix("retry_") {
        return crate::recording::kept::retry_kept_dictation(app, id.into(), None).await;
    }
    if let Some(id) = id.strip_prefix("discard_") {
        crate::recording::kept::discard_kept_dictation(app, id.into());
        return Ok(());
    }
    if let Some(model) = id.strip_prefix("model_") {
        return crate::commands::settings::set_model_from_tray(app, model.into()).await;
    }
    if let Some(mic) = id.strip_prefix("microphone_") {
        return crate::commands::settings::set_audio_device(
            app,
            (mic != "default").then(|| mic.into()),
        )
        .await;
    }
    if let Some(style) = id.strip_prefix("style_").and_then(style_preset) {
        let enabled = style.requires_ai_formatting();
        let ai = crate::commands::ai::get_ai_settings(app.clone()).await?;
        if enabled && !crate::commands::ai::has_ai_model_and_key(&app)? {
            return Err("Configure Polish first".into());
        }
        if ai.enabled != enabled {
            crate::commands::ai::update_ai_settings(enabled, ai.provider, ai.model, app.clone())
                .await?;
        }
        crate::commands::ai::update_enhancement_options(
            EnhancementOptions { preset: style },
            app.clone(),
        )
        .await?;
        let _ = app.emit("ai-enabled-changed", style.requires_ai_formatting());
        return Ok(());
    }
    if id == "copy_last_transcription" || id == "paste_last" || id.starts_with("recent_copy_") {
        let text = if let Some(timestamp) = id.strip_prefix("recent_copy_") {
            use tauri_plugin_store::StoreExt;
            app.store("transcriptions")
                .map_err(|_| "History unavailable")?
                .get(timestamp)
                .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(str::to_owned))
        } else {
            crate::commands::shortcuts::latest_copyable_transcription_text(&app).await?
        };
        if let Some(text) = text {
            if id == "paste_last" {
                crate::commands::text::insert_text(app, text).await?;
            } else {
                crate::commands::text::copy_text_to_clipboard(text).await?;
            }
        }
        return Ok(());
    }
    let mut settings = get_settings(app.clone()).await?;
    if apply_quick_setting(&mut settings, id) {
        save_settings(app, settings, None).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_settings_use_app_settings_keys() {
        let mut s = Settings::default();
        for (id, mode) in [
            ("recording_mode_push_to_talk", "push_to_talk"),
            ("recording_mode_toggle", "toggle"),
        ] {
            assert!(apply_quick_setting(&mut s, id));
            assert_eq!(s.recording_mode, mode);
        }
        assert!(apply_quick_setting(&mut s, "language_fr"));
        assert_eq!(s.speech_language, "fr");
        assert!(apply_quick_setting(&mut s, "live_preview"));
        assert_eq!(s.transcription_mode, "live_preview");
        assert!(apply_quick_setting(&mut s, "live_preview"));
        assert_eq!(s.transcription_mode, "regular");
        assert!(!apply_quick_setting(&mut s, "unknown"));
    }
    #[test]
    fn all_polish_styles_map_to_real_presets() {
        for style in ["Off", "Clean", "Writing", "Notes", "Message", "Code"] {
            let preset = style_preset(style).unwrap();
            assert_eq!(style_label(preset), style);
            assert_eq!(preset.requires_ai_formatting(), style != "Off");
        }
        assert!(style_preset("unknown").is_none());
    }
    #[test]
    fn navigation_destinations_match_existing_screens() {
        assert_eq!(destination("nav_file").unwrap().screen, "audio");
        assert_eq!(destination("nav_insights").unwrap().screen, "insights");
        assert_eq!(destination("nav_settings").unwrap().screen, "settings");
        assert_eq!(destination("nav_models").unwrap().source, Some("local"));
        assert_eq!(destination("nav_settings").unwrap().pane, Some("general"));
        assert_eq!(destination("nav_polish").unwrap().screen, "polish");
        assert!(destination("unknown").is_none());
    }
}
