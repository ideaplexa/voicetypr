//! Tray actions share the app's command paths; never log menu IDs or content.
use crate::{
    ai::prompts::{EnhancementOptions, EnhancementPreset},
    commands::settings::{get_settings, save_settings, Settings},
};
use tauri::{AppHandle, Emitter};
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
        true
    } else {
        false
    }
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
        "nav_settings" => ("settings", Some("general"), None),
        "nav_polish" => ("polish", None, None),
        "nav_models" => ("transcription", None, Some("local")),
        "nav_transcription" => ("transcription", None, None),
        _ => return None,
    };
    Some(MainNavigate {
        screen,
        pane,
        source,
    })
}
fn retry_arguments(
    id: &str,
    alternative: Option<String>,
) -> Result<(String, Option<String>), String> {
    let engine = alternative.ok_or("Local alternative unavailable")?;
    Ok((id.into(), Some(engine)))
}
pub fn handle(app: &AppHandle, id: &str) {
    let app = app.clone();
    let id = id.to_owned();
    tauri::async_runtime::spawn(async move {
        if run(app.clone(), &id).await.is_err() {
            if id == "dictate" {
                crate::commands::island_notice::notice(
                    &app,
                    crate::commands::island_notice::NoticeKind::RecordingFailed,
                );
            }
            // Keep error payloads content-free, including clipboard/plugin errors.
            let _ = app.emit(
                "tray-action-error",
                "Could not complete the menu action. Please try again.",
            );
        }
    });
}
pub(crate) async fn run(app: AppHandle, id: &str) -> Result<(), String> {
    run_from(app, id, "tray").await
}
pub(crate) async fn run_from(app: AppHandle, id: &str, source: &str) -> Result<(), String> {
    let (result, toggle_route) = if id == "dictate" {
        match crate::recording::toggle::toggle_recording(
            app,
            crate::recording::start_source::StartSource::Tray,
        )
        .await
        {
            Ok(route) => (Ok(()), Some(route)),
            Err(error) => (Err(error), None),
        }
    } else {
        (run_inner(app, id, source).await, None)
    };
    if result.is_ok() {
        let setting = if id.starts_with("style_") {
            Some("polish")
        } else if id.starts_with("model_") {
            Some("engine")
        } else if id.starts_with("microphone_") {
            Some("mic")
        } else if id.starts_with("language_") {
            Some("language")
        } else {
            None
        };
        if let Some(setting) = setting {
            crate::observability::quick(setting, source);
        }
        let action = match id {
            "dictate" => toggle_route.and_then(toggle_action),
            "stop" => Some("stop"),
            "cancel" => Some("cancel"),
            "nav_settings" => Some("open_settings"),
            "copy_last_transcription" => Some("copy"),
            id if id.starts_with("retry_") => Some("retry"),
            id if id.starts_with("discard_") => Some("discard"),
            _ => None,
        };
        if let Some(action) = action {
            crate::observability::action(action, source);
        }
    } else {
        crate::telemetry::capture_error(
            "island_action_failed",
            crate::telemetry::current_context(),
        );
    }
    result
}

fn toggle_action(route: crate::recording::toggle::ToggleRoute) -> Option<&'static str> {
    use crate::recording::toggle::ToggleRoute;
    match route {
        ToggleRoute::Start => Some("dictate"),
        ToggleRoute::Stop => Some("stop"),
        ToggleRoute::CancelledPendingStart => Some("cancel"),
        ToggleRoute::Finishing | ToggleRoute::NoChange => None,
    }
}

async fn run_inner(app: AppHandle, id: &str, source: &str) -> Result<(), String> {
    if let Some(destination) = destination(id) {
        let mut destination = serde_json::to_value(destination).map_err(|_| "Navigation failed")?;
        destination["telemetry_source"] = serde_json::json!(source);
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
    if id == "fix" {
        if let Some(action) = super::runtime::fix_action() {
            Box::pin(crate::recording::island::island_action_from(
                app, action, source,
            ))
            .await?;
        }
        return Ok(());
    }
    if let Some(id) = id.strip_prefix("retry_") {
        let (id, engine) = retry_arguments(id, crate::recording::kept::alternative_id(id))?;
        return crate::recording::kept::retry_kept_dictation(app, id, engine).await;
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
    if id == "copy_last_transcription" {
        if let Some(text) =
            crate::commands::shortcuts::latest_copyable_transcription_text(&app).await?
        {
            crate::commands::text::copy_text_to_clipboard(text).await?;
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
    use crate::commands::license::{CachedLicense, RuntimeLicenseCache};
    use crate::license::{
        recording_readiness::wait_for_recording_license, LicenseState, LicenseStatus,
    };
    use crate::recording::{
        secure_input::StartPermit, start_source::StartSource, toggle::ToggleRoute,
    };
    #[test]
    fn a_retired_start_does_not_emit_a_late_tray_dictate_action() {
        assert_eq!(toggle_action(ToggleRoute::NoChange), None);
        assert_eq!(
            toggle_action(ToggleRoute::CancelledPendingStart),
            Some("cancel")
        );
        assert_eq!(toggle_action(ToggleRoute::Start), Some("dictate"));
    }
    #[test]
    fn tray_recovery_passes_the_kept_alternative_id() {
        for id in ["failed-soniox", "remote-offline"] {
            assert_eq!(
                retry_arguments(id, Some("small.en".into())).unwrap(),
                (id.into(), Some("small.en".into()))
            );
            assert!(
                retry_arguments(id, None).is_err(),
                "must never fall back to the failing current engine"
            );
        }
    }
    #[tokio::test]
    async fn tray_dictate_cancels_a_pending_license_wait_instead_of_starting_again() {
        let state = crate::AppState::new();
        let request = state
            .pending_start
            .claim(
                &state,
                StartSource::Tray,
                StartPermit::capture_state(&state),
            )
            .unwrap();
        let (validation, route) = tokio::join!(
            request.validate(wait_for_recording_license(&state.license_cache)),
            async {
                tokio::task::yield_now().await;
                // Still Idle: the old Idle => start branch issued a duplicate start.
                assert_eq!(state.get_current_state(), crate::RecordingState::Idle);
                let route = crate::recording::toggle::route(&state);
                // Without the cancel, a ready license would let the first start publish.
                *state.license_cache.write().await = ready();
                route
            }
        );
        assert_eq!(route, ToggleRoute::CancelledPendingStart);
        assert!(validation.is_none(), "the pending start must never publish");
        drop(request);
        assert_eq!(state.get_current_state(), crate::RecordingState::Idle);
        assert!(!state.pending_start.cancel(), "nothing may stay pending");
        // The cancel consumed that press; the next Dictate starts fresh.
        assert_eq!(crate::recording::toggle::route(&state), ToggleRoute::Start);
        state
            .transition_recording_state(crate::RecordingState::Starting)
            .unwrap();
        assert_eq!(crate::recording::toggle::route(&state), ToggleRoute::Stop);
    }
    fn ready() -> RuntimeLicenseCache {
        RuntimeLicenseCache::Ready(CachedLicense::new(LicenseStatus {
            status: LicenseState::Licensed,
            trial_days_left: None,
            license_type: None,
            license_key: None,
            expires_at: None,
            verification_state: None,
            verification_expires_at: None,
        }))
    }
    #[test]
    fn quick_settings_use_app_settings_keys() {
        let mut s = Settings::default();
        assert!(apply_quick_setting(&mut s, "language_fr"));
        assert_eq!(s.speech_language, "fr");
        // Mode and live preview are no longer menu actions.
        assert!(!apply_quick_setting(&mut s, "live_preview"));
        assert!(!apply_quick_setting(&mut s, "recording_mode_push_to_talk"));
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
        for removed in ["nav_file", "nav_insights", "nav_help"] {
            assert!(destination(removed).is_none());
        }
        assert_eq!(destination("nav_settings").unwrap().screen, "settings");
        assert_eq!(destination("nav_models").unwrap().source, Some("local"));
        assert_eq!(destination("nav_settings").unwrap().pane, Some("general"));
        assert_eq!(destination("nav_polish").unwrap().screen, "polish");
        assert!(destination("unknown").is_none());
    }
}
