use crate::audio::device_watcher::try_start_device_watcher_if_ready;
use crate::commands::key_normalizer::{
    normalize_shortcut_keys, validate_key_combination_allowing_safe_single_key,
};
use crate::commands::remote::{resolve_shareable_model_config, save_remote_settings};
use crate::commands::shortcuts;
use crate::commands::updater::{UpdateChannel, UPDATE_CHANNEL_EXPLICIT_KEY};
use crate::menu::should_include_remote_connection_in_tray;
use crate::parakeet::models::AVAILABLE_MODELS;
use crate::parakeet::ParakeetManager;
use crate::remote::lifecycle::RemoteServerManager;
use crate::remote::settings::{ConnectionStatus, RemoteSettings};
use crate::whisper::languages::{validate_language, SUPPORTED_LANGUAGES};
use crate::whisper::manager::WhisperManager;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::async_runtime::Mutex as AsyncMutex;

/// Generation counter for tray menu updates to prevent race conditions.
/// Each update increments this and checks if it's still current before applying.
static TRAY_MENU_GENERATION: AtomicU64 = AtomicU64::new(0);
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

// Recording indicator offset constants (in pixels)
pub const MIN_INDICATOR_OFFSET: u32 = 10;
pub const MAX_INDICATOR_OFFSET: u32 = 50;
pub const DEFAULT_INDICATOR_OFFSET: u32 = 10;

pub const TRANSCRIPTION_TASK_TRANSCRIBE: &str = "transcribe";
pub const TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH: &str = "translate_to_english";
pub const FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT: &str = "same_as_transcript";
pub const TRANSCRIPTION_MODE_REGULAR: &str = "regular";
pub const TRANSCRIPTION_MODE_LIVE_PREVIEW: &str = "live_preview";

pub(crate) async fn persist_settings_and_invalidate<F, M>(
    app: &AppHandle,
    mutate: F,
    map_save_error: M,
) -> Result<(), String>
where
    F: FnOnce(&tauri_plugin_store::Store<tauri::Wry>) -> Result<(), String>,
    M: FnOnce(String) -> String,
{
    let store = app.store("settings").map_err(|e| e.to_string())?;
    mutate(&store)?;
    store.save().map_err(|e| map_save_error(e.to_string()))?;
    drop(store);

    crate::commands::audio::invalidate_recording_config_cache(app).await;
    Ok(())
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub hotkey: String,
    pub current_model: String,
    pub current_model_engine: String,
    pub speech_language: String,
    pub transcription_task: String,
    pub final_text_language: String,
    pub theme: String,
    // Settings disclosure mode. Post-cutover this is always "recommended".
    #[serde(default = "default_settings_mode")]
    pub settings_mode: String,
    pub transcription_cleanup_days: Option<u32>,
    pub pill_position: Option<(f64, f64)>,
    pub launch_at_startup: bool,
    pub onboarding_completed: bool,
    pub check_updates_automatically: bool,
    pub selected_microphone: Option<String>,
    // Push-to-talk support
    pub recording_mode: String, // "toggle" or "push_to_talk"
    pub use_different_ptt_key: bool,
    pub ptt_hotkey: Option<String>,
    pub keep_transcription_in_clipboard: bool,
    // Audio feedback
    #[serde(default = "default_true")]
    pub play_sound_on_recording: bool,
    #[serde(default = "default_true")]
    pub play_sound_on_transcription_complete: bool,
    #[serde(default = "default_true")]
    pub play_sound_on_paste_success: bool,
    // Pill indicator visibility mode: "never", "always", or "when_recording"
    pub pill_indicator_mode: String,
    // Pill indicator detail level: "compact" or "full"
    #[serde(default = "default_pill_indicator_style")]
    pub pill_indicator_style: String,
    // Pill indicator screen position
    pub pill_indicator_position: String,
    // Pill indicator offset from screen edge in pixels (10-50)
    pub pill_indicator_offset: u32,
    // Pause system media during recording
    pub pause_media_during_recording: bool,
    // Automatically paste transcription text into the active window
    pub auto_paste_transcription: bool,
    // Network sharing settings
    pub sharing_port: Option<u16>,
    pub sharing_password: Option<String>,
    // Recording persistence settings
    pub save_recordings: bool,
    pub recording_retention_days: Option<u32>, // None = keep forever
    // Transcription hardware acceleration: "auto" | "gpu" | "cpu"
    #[serde(default = "default_transcription_acceleration")]
    pub transcription_acceleration: String,
    // Whisper context-load speed optimization. Only applied on Apple-Silicon Metal.
    #[serde(default)]
    pub whisper_speed_mode: bool,
    // Product-facing streaming preview mode: "regular" | "live_preview"
    #[serde(default = "default_transcription_mode")]
    pub transcription_mode: String,
    // Direct-install updater channel: "stable" | "beta"
    #[serde(default = "default_update_channel")]
    pub update_channel: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "CommandOrControl+Shift+Space".to_string(),
            current_model: "".to_string(), // Empty means auto-select
            current_model_engine: "whisper".to_string(),
            speech_language: "en".to_string(),
            transcription_task: TRANSCRIPTION_TASK_TRANSCRIBE.to_string(),
            final_text_language: FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT.to_string(),
            theme: "system".to_string(),
            settings_mode: default_settings_mode(),
            transcription_cleanup_days: None, // None means keep forever
            pill_position: None,              // No saved position initially
            launch_at_startup: false,         // Default to not launching at startup
            onboarding_completed: false,      // Default to not completed
            check_updates_automatically: true, // Default to automatic update checks; installs still require confirmation
            selected_microphone: None,         // Default to system default microphone
            recording_mode: "toggle".to_string(), // Default to toggle mode for backward compatibility
            use_different_ptt_key: false,         // Default to using same key
            ptt_hotkey: Some("Alt+Space".to_string()), // Default PTT key
            keep_transcription_in_clipboard: false, // Default to restoring clipboard after paste
            play_sound_on_recording: true,
            play_sound_on_transcription_complete: true,
            play_sound_on_paste_success: true,
            pill_indicator_mode: "when_recording".to_string(), // Default to showing only when recording
            pill_indicator_style: default_pill_indicator_style(),
            pill_indicator_position: "bottom-center".to_string(), // Default to bottom center of screen
            pill_indicator_offset: DEFAULT_INDICATOR_OFFSET,
            pause_media_during_recording: false, // Default to off; user opts in
            auto_paste_transcription: true,      // Default to auto-pasting transcription
            sharing_port: Some(47842),           // Default network sharing port
            sharing_password: None,              // No password by default
            save_recordings: false,              // Default to not saving recordings
            recording_retention_days: Some(30),  // Default cleanup period when saving is enabled
            transcription_acceleration: "auto".to_string(),
            whisper_speed_mode: false,
            transcription_mode: TRANSCRIPTION_MODE_REGULAR.to_string(),
            update_channel: default_update_channel(),
        }
    }
}

fn default_pill_indicator_style() -> String {
    "compact".to_string()
}

fn default_settings_mode() -> String {
    "recommended".to_string()
}

fn normalize_settings_mode(value: Option<&str>) -> String {
    // Legacy adopter contract: pre-cutover stores may carry "advanced"; normalize it to "recommended".
    match value {
        Some("advanced") | Some("recommended") | None => default_settings_mode(),
        Some(_) => default_settings_mode(),
    }
}

fn resolve_pill_indicator_style(stored: Option<String>) -> String {
    match stored.as_deref() {
        Some("compact" | "full") => stored.unwrap_or_default(),
        _ => default_pill_indicator_style(),
    }
}

fn default_update_channel() -> String {
    UpdateChannel::from_stored(None).as_str().to_string()
}

fn update_channel_to_persist(
    stored_channel: Option<&str>,
    stored_explicit: bool,
    selected_now: bool,
    requested: UpdateChannel,
) -> Option<UpdateChannel> {
    if stored_explicit || selected_now || stored_channel == Some("beta") {
        Some(requested)
    } else {
        None
    }
}

fn default_transcription_acceleration() -> String {
    "auto".to_string()
}

fn default_transcription_mode() -> String {
    TRANSCRIPTION_MODE_REGULAR.to_string()
}

pub(crate) fn read_whisper_speed_mode(app: &AppHandle) -> bool {
    app.store("settings")
        .ok()
        .and_then(|store| store.get("whisper_speed_mode"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub fn normalize_transcription_mode(value: Option<&str>) -> String {
    match value {
        Some(TRANSCRIPTION_MODE_LIVE_PREVIEW) => TRANSCRIPTION_MODE_LIVE_PREVIEW.to_string(),
        _ => TRANSCRIPTION_MODE_REGULAR.to_string(),
    }
}

fn default_true() -> bool {
    true
}

pub fn resolve_transcription_complete_sound(
    stored: Option<bool>,
    legacy_recording_end: Option<bool>,
) -> bool {
    stored.or(legacy_recording_end).unwrap_or(true)
}

pub fn normalize_stored_transcription_acceleration(value: Option<&str>) -> String {
    match value {
        Some("gpu") => "gpu".to_string(),
        Some("cpu") => "cpu".to_string(),
        _ => "auto".to_string(),
    }
}

pub fn transcription_task_from_legacy(translate_to_english: bool) -> String {
    if translate_to_english {
        TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH.to_string()
    } else {
        TRANSCRIPTION_TASK_TRANSCRIBE.to_string()
    }
}

pub fn normalize_transcription_task(
    task: Option<&str>,
    legacy_translate_to_english: bool,
) -> String {
    match task {
        Some(TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH) => {
            TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH.to_string()
        }
        Some(TRANSCRIPTION_TASK_TRANSCRIBE) => TRANSCRIPTION_TASK_TRANSCRIBE.to_string(),
        _ => transcription_task_from_legacy(legacy_translate_to_english),
    }
}

pub fn task_uses_translate_to_english(task: &str) -> bool {
    task == TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH
}

pub fn normalize_final_text_language(value: Option<&str>, transcription_task: &str) -> String {
    if task_uses_translate_to_english(transcription_task) {
        "en".to_string()
    } else {
        match value {
            Some(FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT) | None => {
                FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT.to_string()
            }
            Some(value) => validate_language(Some(value)).to_string(),
        }
    }
}

/// Log a message from the frontend to the backend logs
#[tauri::command]
#[allow(dead_code)]
pub async fn frontend_log(message: String) {
    log::info!("[FRONTEND] {}", message);
}

/// Validate that the stored microphone selection still exists.
/// If the selected microphone is no longer available, resets to default.
/// Returns true if the microphone was reset, false if it was valid or already default.
#[tauri::command]
pub async fn validate_microphone_selection(app: AppHandle) -> Result<bool, String> {
    use crate::audio::recorder::AudioRecorder;

    let settings = get_settings(app.clone()).await?;

    // If no microphone is selected (using default), nothing to validate
    let Some(selected_mic) = settings.selected_microphone else {
        log::debug!("No microphone selected, using system default");
        return Ok(false);
    };

    // Get available devices
    let available_devices = AudioRecorder::get_devices();

    // Check if selected mic still exists
    if available_devices.contains(&selected_mic) {
        log::debug!("Selected microphone is available");
        return Ok(false);
    }

    // Selected mic no longer exists - reset to default
    log::info!(
        "Selected microphone is no longer available ({} devices available), resetting to default",
        available_devices.len()
    );

    // Clear the selection
    set_audio_device(app.clone(), None).await?;

    Ok(true)
}

pub(crate) fn resolve_pill_indicator_mode(
    stored_mode: Option<String>,
    legacy_show_pill: Option<bool>,
    default_mode: String,
) -> String {
    if let Some(mode) = stored_mode {
        return mode;
    }

    if let Some(show) = legacy_show_pill {
        if show {
            return "always".to_string();
        }
        return "when_recording".to_string();
    }

    default_mode
}

fn recording_retention_days_from_legacy_count(count: u64) -> Option<u32> {
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

pub fn model_requires_english_speech(engine: &str, model_name: &str) -> bool {
    match engine {
        "whisper" => model_name.ends_with(".en"),
        "parakeet" => model_name.contains("-v2"),
        _ => false,
    }
}

pub fn normalize_speech_language_for_model(
    engine: &str,
    model_name: &str,
    speech_language: &str,
) -> String {
    let validated = validate_language(Some(speech_language));
    match engine {
        "whisper" if model_requires_english_speech(engine, model_name) => "en".to_string(),
        "parakeet" => {
            if let Some(definition) = AVAILABLE_MODELS.iter().find(|m| m.id == model_name) {
                if definition.languages.contains(&validated) {
                    validated.to_string()
                } else {
                    definition
                        .languages
                        .first()
                        .copied()
                        .unwrap_or("en")
                        .to_string()
                }
            } else if model_requires_english_speech(engine, model_name) {
                "en".to_string()
            } else {
                validated.to_string()
            }
        }
        "soniox" => {
            const SONIOX_SUPPORTED_LANGUAGES: &[&str] = &[
                "en", "es", "fr", "de", "it", "pt", "nl", "ru", "zh", "ja", "ko", "ar", "hi", "tr",
                "pl", "sv", "no", "da", "fi", "el", "cs", "ro", "hu", "sk", "uk", "he", "id", "vi",
                "th", "ms", "tl", "fa", "ur", "bn", "ta", "te", "gu", "pa", "bg", "hr", "sr", "sl",
                "lv", "lt", "et", "is", "ca", "gl",
            ];
            if SONIOX_SUPPORTED_LANGUAGES.contains(&validated) {
                validated.to_string()
            } else {
                "en".to_string()
            }
        }
        "cohere" => {
            const COHERE_SUPPORTED_LANGUAGES: &[&str] = &[
                "en", "de", "fr", "it", "es", "pt", "el", "nl", "pl", "vi", "zh", "ar", "ja", "ko",
            ];
            if COHERE_SUPPORTED_LANGUAGES.contains(&validated) {
                validated.to_string()
            } else {
                "en".to_string()
            }
        }
        "openai" | "groq" | "deepgram" => validated.to_string(),
        _ => validated.to_string(),
    }
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let legacy_speech_language = store
        .get("language")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| Settings::default().speech_language.clone());
    let legacy_translate_to_english = store
        .get("translate_to_english")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let speech_language = store
        .get("speech_language")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or(legacy_speech_language);
    let stored_transcription_task = store
        .get("transcription_task")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let transcription_task = normalize_transcription_task(
        stored_transcription_task.as_deref(),
        legacy_translate_to_english,
    );
    let stored_final_text_language = store
        .get("final_text_language")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let final_text_language =
        normalize_final_text_language(stored_final_text_language.as_deref(), &transcription_task);
    let stored_update_channel = store
        .get("update_channel")
        .and_then(|value| value.as_str().map(str::to_owned));
    let stored_update_channel_explicit = store
        .get(UPDATE_CHANNEL_EXPLICIT_KEY)
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let settings = Settings {
        hotkey: store
            .get("hotkey")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().hotkey),
        current_model: store
            .get("current_model")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().current_model),
        current_model_engine: store
            .get("current_model_engine")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().current_model_engine.clone()),
        speech_language,
        transcription_task,
        final_text_language,
        theme: store
            .get("theme")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().theme),
        settings_mode: normalize_settings_mode(
            store
                .get("settings_mode")
                .and_then(|v| v.as_str().map(str::to_owned))
                .as_deref(),
        ),
        transcription_cleanup_days: store
            .get("transcription_cleanup_days")
            .and_then(|v| v.as_u64().map(|n| n as u32)),
        pill_position: store.get("pill_position").and_then(|v| {
            if let Some(arr) = v.as_array() {
                if arr.len() == 2 {
                    let x = arr[0].as_f64()?;
                    let y = arr[1].as_f64()?;
                    Some((x, y))
                } else {
                    None
                }
            } else {
                None
            }
        }),
        launch_at_startup: store
            .get("launch_at_startup")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().launch_at_startup),
        onboarding_completed: store
            .get("onboarding_completed")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().onboarding_completed),
        check_updates_automatically: store
            .get("check_updates_automatically")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().check_updates_automatically),
        selected_microphone: store
            .get("selected_microphone")
            .and_then(|v| v.as_str().map(|s| s.to_string())),
        recording_mode: store
            .get("recording_mode")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().recording_mode),
        use_different_ptt_key: store
            .get("use_different_ptt_key")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().use_different_ptt_key),
        ptt_hotkey: store
            .get("ptt_hotkey")
            .and_then(|v| v.as_str().map(|s| s.to_string())),
        keep_transcription_in_clipboard: store
            .get("keep_transcription_in_clipboard")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().keep_transcription_in_clipboard),
        play_sound_on_recording: store
            .get("play_sound_on_recording")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().play_sound_on_recording),
        play_sound_on_transcription_complete: resolve_transcription_complete_sound(
            store
                .get("play_sound_on_transcription_complete")
                .and_then(|v| v.as_bool()),
            store
                .get("play_sound_on_recording_end")
                .and_then(|v| v.as_bool()),
        ),
        play_sound_on_paste_success: store
            .get("play_sound_on_paste_success")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().play_sound_on_paste_success),
        // Migration: check for new pill_indicator_mode first, then fall back to old show_pill_indicator
        pill_indicator_mode: resolve_pill_indicator_mode(
            store
                .get("pill_indicator_mode")
                .and_then(|v| v.as_str().map(|s| s.to_string())),
            store.get("show_pill_indicator").and_then(|v| v.as_bool()),
            Settings::default().pill_indicator_mode,
        ),
        pill_indicator_style: resolve_pill_indicator_style(
            store
                .get("pill_indicator_style")
                .and_then(|v| v.as_str().map(str::to_owned)),
        ),
        pill_indicator_position: store
            .get("pill_indicator_position")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().pill_indicator_position),
        pill_indicator_offset: store
            .get("pill_indicator_offset")
            .and_then(|v| v.as_u64())
            .map(|v| v.clamp(MIN_INDICATOR_OFFSET as u64, MAX_INDICATOR_OFFSET as u64) as u32)
            .unwrap_or_else(|| Settings::default().pill_indicator_offset),
        pause_media_during_recording: store
            .get("pause_media_during_recording")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().pause_media_during_recording),
        auto_paste_transcription: store
            .get("auto_paste_transcription")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().auto_paste_transcription),
        sharing_port: store
            .get("sharing_port")
            .and_then(|v| v.as_u64().map(|n| n as u16))
            .or(Settings::default().sharing_port),
        sharing_password: None,
        save_recordings: store
            .get("save_recordings")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().save_recordings),
        recording_retention_days: recording_retention_days_from_store(&store),
        transcription_acceleration: normalize_stored_transcription_acceleration(
            store
                .get("transcription_acceleration")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .as_deref(),
        ),
        whisper_speed_mode: store
            .get("whisper_speed_mode")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().whisper_speed_mode),
        transcription_mode: normalize_transcription_mode(
            store
                .get("transcription_mode")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .as_deref(),
        ),
        update_channel: UpdateChannel::from_preference(
            stored_update_channel.as_deref(),
            stored_update_channel_explicit,
        )
        .as_str()
        .to_string(),
    };
    let normalized_speech_language = normalize_speech_language_for_model(
        &settings.current_model_engine,
        &settings.current_model,
        &settings.speech_language,
    );
    let mut settings = settings;
    settings.speech_language = normalized_speech_language;

    Ok(settings)
}

async fn sync_running_sharing_server_to_model(
    app: &AppHandle,
    model_name: &str,
    engine: &str,
) -> Result<(), String> {
    let server_manager = app.state::<AsyncMutex<RemoteServerManager>>();
    let mut server = server_manager.lock().await;
    if !server.is_running() {
        return Ok(());
    }

    match resolve_shareable_model_config(app, model_name, engine).await {
        Ok((model_path, resolved_engine)) => {
            server.update_model(model_path, model_name.to_string(), resolved_engine);
            Ok(())
        }
        Err(err) => {
            log::warn!(
                "Stopping sharing because the selected model is no longer shareable: {}",
                err
            );
            server.stop().await;
            drop(server);

            let remote_state = app.state::<AsyncMutex<RemoteSettings>>();
            let mut remote_settings = remote_state.lock().await;
            remote_settings.server_config.enabled = false;
            save_remote_settings(app, &remote_settings)?;

            let _ = app.emit(
                "sharing-status-changed",
                json!({
                    "enabled": false,
                    "port": null,
                    "model_name": null
                }),
            );
            Ok(())
        }
    }
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    settings: Settings,
    update_channel_explicit: Option<bool>,
) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;

    // Check if model, recording mode, onboarding, and pill indicator mode changed
    let old_model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    let old_mode = store
        .get("recording_mode")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| Settings::default().recording_mode);
    let old_onboarding_completed = store
        .get("onboarding_completed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let old_pill_indicator_mode = store
        .get("pill_indicator_mode")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| Settings::default().pill_indicator_mode);
    let old_pill_indicator_position = store
        .get("pill_indicator_position")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| Settings::default().pill_indicator_position);
    let old_pill_indicator_offset = store
        .get("pill_indicator_offset")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or_else(|| Settings::default().pill_indicator_offset);
    let old_transcription_acceleration = normalize_stored_transcription_acceleration(
        store
            .get("transcription_acceleration")
            .and_then(|v| v.as_str().map(str::to_owned))
            .as_deref(),
    );
    let stored_update_channel = store
        .get("update_channel")
        .and_then(|value| value.as_str().map(str::to_owned));
    let stored_update_channel_explicit = store
        .get(UPDATE_CHANNEL_EXPLICIT_KEY)
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let normalized_transcription_acceleration =
        normalize_stored_transcription_acceleration(Some(&settings.transcription_acceleration));
    let normalized_transcription_mode =
        normalize_transcription_mode(Some(&settings.transcription_mode));
    let normalized_update_channel = UpdateChannel::from_stored(Some(&settings.update_channel));

    let normalized_hotkey = normalize_shortcut_keys(&settings.hotkey);
    if !normalized_hotkey.is_empty() {
        validate_key_combination_allowing_safe_single_key(&normalized_hotkey)
            .map_err(|e| format!("Invalid hotkey: {}", e))?;
        crate::trigger::mapping::parse_combo(&normalized_hotkey)
            .map_err(|e| format!("Invalid hotkey '{}': {}", settings.hotkey, e))?;
    }

    let app_state = app.state::<crate::AppState>();
    let recording_mode = match settings.recording_mode.as_str() {
        "push_to_talk" => crate::RecordingMode::PushToTalk,
        _ => crate::RecordingMode::Toggle,
    };

    let ptt_hotkey_for_validation =
        if recording_mode == crate::RecordingMode::PushToTalk && settings.use_different_ptt_key {
            settings
                .ptt_hotkey
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        } else {
            None
        };
    if let Some(ptt_hotkey) = ptt_hotkey_for_validation {
        let normalized_ptt = normalize_shortcut_keys(ptt_hotkey);
        validate_key_combination_allowing_safe_single_key(&normalized_ptt)
            .map_err(|e| format!("Invalid push-to-talk hotkey: {}", e))?;
        crate::trigger::mapping::parse_combo(&normalized_ptt)
            .map_err(|e| format!("Invalid push-to-talk hotkey '{}': {}", ptt_hotkey, e))?;
    }
    shortcuts::validate_shortcut_settings(
        shortcuts::load_shortcut_settings(&app)?,
        &shortcuts::ExistingShortcutStrings {
            primary_hotkey: Some(settings.hotkey.clone()),
            ptt_hotkey: ptt_hotkey_for_validation.map(str::to_string),
        },
    )?;
    store.set("hotkey", json!(settings.hotkey));
    store.set("current_model", json!(settings.current_model));
    store.set("current_model_engine", json!(settings.current_model_engine));

    let validated_speech_language = normalize_speech_language_for_model(
        &settings.current_model_engine,
        &settings.current_model,
        &settings.speech_language,
    );
    let normalized_transcription_task =
        normalize_transcription_task(Some(&settings.transcription_task), false);
    let normalized_final_text_language = normalize_final_text_language(
        Some(&settings.final_text_language),
        &normalized_transcription_task,
    );
    store.set("speech_language", json!(validated_speech_language));
    store.set("transcription_task", json!(normalized_transcription_task));
    store.set("final_text_language", json!(normalized_final_text_language));
    // Keep legacy keys in sync during migration.
    store.set("language", json!(validated_speech_language));
    store.set(
        "translate_to_english",
        json!(task_uses_translate_to_english(
            &normalized_transcription_task
        )),
    );

    store.set("theme", json!(settings.theme));
    store.set(
        "settings_mode",
        json!(normalize_settings_mode(Some(&settings.settings_mode))),
    );
    store.set(
        "transcription_cleanup_days",
        json!(settings.transcription_cleanup_days),
    );
    store.set("launch_at_startup", json!(settings.launch_at_startup));
    store.set("onboarding_completed", json!(settings.onboarding_completed));
    store.set(
        "check_updates_automatically",
        json!(settings.check_updates_automatically),
    );
    store.set("selected_microphone", json!(settings.selected_microphone));

    // Save push-to-talk settings
    store.set("recording_mode", json!(settings.recording_mode.clone()));
    store.set(
        "use_different_ptt_key",
        json!(settings.use_different_ptt_key),
    );
    if let Some(ref ptt_hotkey) = settings.ptt_hotkey {
        store.set("ptt_hotkey", json!(ptt_hotkey));
    }
    store.set(
        "keep_transcription_in_clipboard",
        json!(settings.keep_transcription_in_clipboard),
    );
    store.set(
        "play_sound_on_recording",
        json!(settings.play_sound_on_recording),
    );
    store.set(
        "play_sound_on_transcription_complete",
        json!(settings.play_sound_on_transcription_complete),
    );
    store.set(
        "play_sound_on_paste_success",
        json!(settings.play_sound_on_paste_success),
    );
    store.delete("play_sound_on_recording_end");
    store.set("pill_indicator_mode", json!(settings.pill_indicator_mode));
    store.set(
        "pill_indicator_style",
        json!(resolve_pill_indicator_style(Some(
            settings.pill_indicator_style.clone()
        ))),
    );
    store.set(
        "pill_indicator_position",
        json!(settings.pill_indicator_position),
    );
    store.set(
        "pill_indicator_offset",
        json!(settings
            .pill_indicator_offset
            .clamp(MIN_INDICATOR_OFFSET, MAX_INDICATOR_OFFSET)),
    );
    store.set(
        "pause_media_during_recording",
        json!(settings.pause_media_during_recording),
    );
    store.set(
        "auto_paste_transcription",
        json!(settings.auto_paste_transcription),
    );
    store.set(
        "transcription_acceleration",
        json!(&normalized_transcription_acceleration),
    );
    store.set("whisper_speed_mode", json!(settings.whisper_speed_mode));
    store.set("transcription_mode", json!(&normalized_transcription_mode));
    match update_channel_to_persist(
        stored_update_channel.as_deref(),
        stored_update_channel_explicit,
        update_channel_explicit.unwrap_or(false),
        normalized_update_channel,
    ) {
        Some(channel) => {
            store.set("update_channel", json!(channel.as_str()));
            store.set(UPDATE_CHANNEL_EXPLICIT_KEY, json!(true));
        }
        None => {
            store.delete("update_channel");
            store.delete(UPDATE_CHANNEL_EXPLICIT_KEY);
        }
    }

    // Network sharing settings
    if let Some(port) = settings.sharing_port {
        store.set("sharing_port", json!(port));
    }
    store.delete("sharing_password");

    if let Some(remote_state) = app.try_state::<AsyncMutex<RemoteSettings>>() {
        let mut remote_settings = remote_state.lock().await;
        if let Some(port) = settings.sharing_port {
            remote_settings.server_config.port = port;
        }
        save_remote_settings(&app, &remote_settings)?;
    }

    // Recording persistence settings
    store.set("save_recordings", json!(settings.save_recordings));
    store.set(
        "recording_retention_days",
        recording_retention_days_to_value(settings.recording_retention_days),
    );

    // Save pill position if provided
    if let Some((x, y)) = settings.pill_position {
        store.set("pill_position", json!([x, y]));
    }

    if let Err(error) = store.save() {
        // Discard uncommitted in-memory mutations so a later
        // rebuild_engine_bindings cannot read values that failed to persist.
        let _ = store.reload();
        return Err(error.to_string());
    }

    if let Ok(mut mode_guard) = app_state.recording_mode.lock() {
        *mode_guard = recording_mode;
        log::info!("Recording mode updated to: {:?}", recording_mode);
    }

    crate::trigger::engine_host::rebuild_engine_bindings(&app);

    if old_transcription_acceleration != normalized_transcription_acceleration {
        log::info!(
            "Transcription acceleration updated: {} -> {}",
            old_transcription_acceleration,
            normalized_transcription_acceleration
        );
    }

    // This command reloads on save failure and rebuilds bindings after save; keep one explicit invalidation.
    crate::commands::audio::invalidate_recording_config_cache(&app).await;

    // Preload new model and update tray menu if model changed
    let is_parakeet_engine = settings.current_model_engine == "parakeet";
    let is_cloud_engine =
        crate::cloud_stt::CloudProvider::from_id(&settings.current_model_engine).is_some();

    if !settings.current_model.is_empty() && old_model != settings.current_model {
        use crate::commands::model::preload_model;
        use tauri::async_runtime::RwLock as AsyncRwLock;

        log::info!(
            "Model changed from '{}' to '{}', preloading new model and updating tray menu",
            old_model,
            settings.current_model
        );

        if is_parakeet_engine {
            // Warm the selected Parakeet model in the background.
            let app_clone = app.clone();
            let model_name = settings.current_model.clone();
            tokio::spawn(async move {
                #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
                let _ = app_clone.emit("parakeet-model-warming", model_name.clone());
                let parakeet_manager = app_clone.state::<ParakeetManager>();
                #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
                let load_result = parakeet_manager
                    .preload_selected_onnx(&app_clone, &model_name)
                    .await;
                #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
                let load_result = parakeet_manager
                    .load_model(&app_clone, &model_name)
                    .await
                    .map(|()| crate::parakeet::manager::PreloadOutcome::Loaded);
                match load_result {
                    Ok(crate::parakeet::manager::PreloadOutcome::Loaded) => {
                        log::info!("Successfully preloaded new model: {}", model_name);
                        match parakeet_manager.warmup(&app_clone).await {
                            Ok(Some(ms)) => log::info!(
                                "Successfully warmed new Parakeet model '{}' in {}ms",
                                model_name,
                                ms
                            ),
                            Ok(None) => {
                                log::info!("Parakeet warmup skipped for '{}'", model_name)
                            }
                            Err(e) => log::warn!("Failed to warm new Parakeet model: {}", e),
                        }
                    }
                    Ok(crate::parakeet::manager::PreloadOutcome::Superseded) => {}
                    Err(e) => log::warn!("Failed to preload new model: {}", e),
                }
                #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
                let _ = app_clone.emit("parakeet-model-warmed", model_name);
            });
        } else if !is_cloud_engine {
            // Preload the new Whisper model
            let app_clone = app.clone();
            let model_name = settings.current_model.clone();
            tokio::spawn(async move {
                let whisper_state =
                    app_clone.state::<AsyncRwLock<crate::whisper::manager::WhisperManager>>();
                match preload_model(app_clone.clone(), model_name.clone(), whisper_state).await {
                    Ok(_) => log::info!("Successfully preloaded new model: {}", model_name),
                    Err(e) => log::warn!("Failed to preload new model: {}", e),
                }
            });
        } else {
            log::info!(
                "Skipping preload for {} engine selection",
                settings.current_model_engine
            );
        }

        // Update the tray menu to reflect the new selection
        if let Err(e) = update_tray_menu(app.clone()).await {
            log::warn!("Failed to update tray menu after model change: {}", e);
            // Don't fail the whole operation if tray update fails
        }

        // Update the sharing server's model if it's running
        sync_running_sharing_server_to_model(
            &app,
            &settings.current_model,
            &settings.current_model_engine,
        )
        .await?;

        // Emit model-changed event so frontend can refresh remote server status
        if let Err(e) = app.emit(
            "model-changed",
            json!({
                "model": settings.current_model,
                "engine": settings.current_model_engine
            }),
        ) {
            log::warn!("Failed to emit model-changed event: {}", e);
        }
    }

    // If recording mode changed, refresh tray to update checked state
    if old_mode != settings.recording_mode {
        if let Err(e) = update_tray_menu(app.clone()).await {
            log::warn!("Failed to update tray menu after mode change: {}", e);
        }
    }

    // If onboarding just completed, try to start device watcher
    if !old_onboarding_completed && settings.onboarding_completed {
        log::info!("Onboarding just completed, checking if device watcher should start");
        try_start_device_watcher_if_ready(&app).await;
    }

    // Handle pill window visibility when pill_indicator_mode setting changes
    if old_pill_indicator_mode != settings.pill_indicator_mode {
        let app_state = app.state::<crate::AppState>();
        let current_state = app_state.get_current_state();
        let is_idle = matches!(current_state, crate::RecordingState::Idle);
        log::info!(
            "pill_visibility: mode change '{}' -> '{}' (state={:?})",
            old_pill_indicator_mode,
            settings.pill_indicator_mode,
            current_state
        );

        // Determine if pill should be visible based on new mode and current state
        let should_show = match settings.pill_indicator_mode.as_str() {
            "never" => false,
            "always" => true,
            "when_recording" => !is_idle, // Show only when recording
            _ => !is_idle,                // Default to when_recording behavior
        };
        log::info!(
            "pill_visibility: mode change computed should_show={}",
            should_show
        );

        if should_show {
            if let Err(e) = crate::commands::window::show_pill_widget(app.clone()).await {
                log::warn!("Failed to show pill window: {}", e);
            }
        } else if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
            log::warn!("Failed to hide pill window: {}", e);
        }
    }

    // Handle pill window position when pill_indicator_position setting changes
    // We need to recreate the pill window at the new position since repositioning doesn't work reliably
    if old_pill_indicator_position != settings.pill_indicator_position {
        log::info!(
            "Pill indicator position changed from '{}' to '{}'",
            old_pill_indicator_position,
            settings.pill_indicator_position
        );

        // Check if pill should be visible based on current mode
        let should_show = match settings.pill_indicator_mode.as_str() {
            "never" => false,
            "always" => true,
            "when_recording" => {
                let app_state = app.state::<crate::AppState>();
                let current_state = app_state.get_current_state();
                !matches!(current_state, crate::RecordingState::Idle)
            }
            _ => false,
        };

        if should_show {
            // Close and recreate the pill window at the new position
            if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
                log::warn!("Failed to hide pill window for position change: {}", e);
            }
            if let Err(e) = crate::commands::window::show_pill_widget(app.clone()).await {
                log::warn!("Failed to show pill window at new position: {}", e);
            }
            log::info!(
                "Recreated pill window at new position: {}",
                settings.pill_indicator_position
            );
        }
    }

    // Handle pill window offset change - reposition the pill window
    if old_pill_indicator_offset != settings.pill_indicator_offset {
        log::info!(
            "Pill indicator offset changed from {} to {}",
            old_pill_indicator_offset,
            settings.pill_indicator_offset
        );

        // Reposition the pill window if it's currently visible
        let app_state = app.state::<crate::AppState>();
        if let Some(window_manager) = app_state.get_window_manager() {
            if window_manager.has_pill_window() {
                window_manager
                    .reposition_floating_windows_with_position(&settings.pill_indicator_position);
                log::info!(
                    "Repositioned pill window with new offset: {}",
                    settings.pill_indicator_offset
                );
            }
        }
    }

    // Emit settings-changed event so all windows (including pill) can refresh their settings
    if let Err(e) = app.emit("settings-changed", ()) {
        log::warn!("Failed to emit settings-changed event: {}", e);
    }

    Ok(())
}

#[tauri::command]
pub async fn set_global_shortcut(app: AppHandle, shortcut: String) -> Result<(), String> {
    log::info!(
        "Updating global shortcut to: {}",
        if shortcut.is_empty() {
            "<clear>"
        } else {
            &shortcut
        }
    );

    if shortcut.len() > 100 {
        log::error!("Invalid shortcut format: too long");
        return Err("Invalid shortcut format".to_string());
    }

    let normalized_shortcut = normalize_shortcut_keys(&shortcut);
    if !normalized_shortcut.is_empty() {
        validate_key_combination_allowing_safe_single_key(&normalized_shortcut)
            .map_err(|e| format!("Invalid key combination: {}", e))?;
        crate::trigger::mapping::parse_combo(&normalized_shortcut)
            .map_err(|_| "Invalid shortcut format".to_string())?;
    }

    let store = app.store("settings").map_err(|e| {
        log::error!("Failed to get settings store: {}", e);
        "Failed to access settings store".to_string()
    })?;

    let recording_mode = store
        .get("recording_mode")
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "toggle".to_string());
    let ptt_hotkey = if recording_mode == "push_to_talk"
        && store
            .get("use_different_ptt_key")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    {
        store
            .get("ptt_hotkey")
            .and_then(|value| value.as_str().map(str::to_string))
            .filter(|value| !value.trim().is_empty())
    } else {
        None
    };

    shortcuts::validate_shortcut_settings(
        shortcuts::load_shortcut_settings(&app)?,
        &shortcuts::ExistingShortcutStrings {
            primary_hotkey: Some(shortcut.clone()),
            ptt_hotkey,
        },
    )?;

    store.set("hotkey", json!(shortcut));
    if let Err(error) = store.save() {
        // Discard the uncommitted hotkey so rebuild_engine_bindings cannot
        // apply a change the user's save failed to persist.
        let _ = store.reload();
        return Err(format!("Failed to save settings: {}", error));
    }

    crate::trigger::engine_host::rebuild_engine_bindings(&app);
    log::info!("Successfully updated global shortcut");

    Ok(())
}

#[derive(Serialize)]
pub struct LanguageInfo {
    pub code: String,
    pub name: String,
}

#[tauri::command]
pub async fn get_supported_languages() -> Result<Vec<LanguageInfo>, String> {
    let mut languages: Vec<LanguageInfo> = SUPPORTED_LANGUAGES
        .iter()
        .map(|(code, lang)| LanguageInfo {
            code: code.to_string(),
            name: lang.name.to_string(),
        })
        .collect();

    // Sort by name for better UX (auto-detect removed)
    languages.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(languages)
}

#[tauri::command]
pub async fn set_model_from_tray(app: AppHandle, model_name: String) -> Result<(), String> {
    // Check if this is a remote server selection
    if let Some(connection_id) = model_name.strip_prefix("remote_") {
        log::info!("Setting active remote server from tray: {}", connection_id);

        {
            let remote_state = app.state::<AsyncMutex<RemoteSettings>>();
            let settings = remote_state.lock().await;
            let connection = settings
                .get_connection(connection_id)
                .ok_or_else(|| format!("Connection '{}' not found", connection_id))?;

            if !should_include_remote_connection_in_tray(&connection.status) {
                return Err("Cannot select this Voicetypr instance as a remote server".to_string());
            }
        }

        let refreshed_connection =
            crate::commands::remote::refresh_saved_connection_status(&app, connection_id).await?;
        if matches!(
            refreshed_connection.status,
            ConnectionStatus::SelfConnection
        ) {
            return Err("Cannot use this Voicetypr instance as its own remote server".to_string());
        }

        // Stop network sharing if enabled (can't share while using remote)
        // and remember that it was active for auto-restore later
        if let Some(server_manager) = app.try_state::<AsyncMutex<RemoteServerManager>>() {
            let manager = server_manager.lock().await;
            if manager.get_status().enabled {
                drop(manager); // Release lock before calling stop
                log::info!("🔧 [TRAY] Stopping network sharing - selecting remote server (will remember for auto-restore)");
                let mut manager = server_manager.lock().await;
                manager.stop().await;

                // Set flag to remember sharing was active
                let remote_state = app.state::<AsyncMutex<RemoteSettings>>();
                let mut settings = remote_state.lock().await;
                settings.sharing_was_active = true;
                save_remote_settings(&app, &settings)?;
                log::info!("🔧 [TRAY] Network sharing stopped, sharing_was_active flag set");
            }
        }

        // Set the remote server as active
        let remote_state = app.state::<AsyncMutex<RemoteSettings>>();
        {
            let mut settings = remote_state.lock().await;
            settings.set_active_connection(Some(connection_id.to_string()))?;
            save_remote_settings(&app, &settings)?;
        }

        // Update the tray menu to reflect the new selection
        update_tray_menu(app.clone()).await?;

        // Emit event to update UI
        if let Err(e) = app.emit(
            "model-changed",
            json!({
                "model": model_name,
                "engine": "remote"
            }),
        ) {
            log::warn!("Failed to emit model-changed event: {}", e);
        }

        if let Err(e) = app.emit("sharing-status-changed", json!({ "refresh": true })) {
            log::warn!("Failed to emit sharing-status-changed event: {}", e);
        }

        return Ok(());
    }

    // Clear any active remote server when selecting a local model
    // and restore sharing if it was previously active
    let should_restore_sharing;
    let restore_port: u16;
    let restore_password: Option<String>;
    {
        let remote_state = app.state::<AsyncMutex<RemoteSettings>>();
        let mut settings = remote_state.lock().await;
        should_restore_sharing = settings.sharing_was_active;
        restore_port = settings.server_config.port;
        restore_password = settings.server_config.password.clone();

        if settings.active_connection_id.is_some() {
            log::info!("Clearing active remote server - switching to local model");
            settings.set_active_connection(None)?;
        }

        save_remote_settings(&app, &settings)?;
    }

    if let Err(e) = app.emit("sharing-status-changed", json!({ "refresh": true })) {
        log::warn!("Failed to emit sharing-status-changed event: {}", e);
    }

    if should_restore_sharing {
        if let Some(server_manager) = app.try_state::<AsyncMutex<RemoteServerManager>>() {
            let server_name = hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "Voicetypr Server".to_string());

            log::info!(
                "🔧 [TRAY] Auto-restoring network sharing on port {} with model {}",
                restore_port,
                model_name
            );

            let engine = if let Some(p) = crate::cloud_stt::CloudProvider::from_id(&model_name) {
                p.id().to_string()
            } else {
                let whisper_state = app.state::<tauri::async_runtime::RwLock<WhisperManager>>();
                let guard = whisper_state.read().await;
                if guard.get_models_status().contains_key(&model_name) {
                    "whisper".to_string()
                } else {
                    "parakeet".to_string()
                }
            };

            if let Ok((model_path, engine)) =
                resolve_shareable_model_config(&app, &model_name, &engine).await
            {
                let mut manager = server_manager.lock().await;
                if let Err(e) = manager
                    .start(
                        restore_port,
                        restore_password,
                        server_name,
                        model_path,
                        model_name.clone(),
                        engine,
                        Some(app.clone()),
                    )
                    .await
                {
                    log::warn!("🔧 [TRAY] Failed to auto-restore sharing: {}", e);
                } else {
                    {
                        let remote_state = app.state::<AsyncMutex<RemoteSettings>>();
                        let mut settings = remote_state.lock().await;
                        settings.sharing_was_active = false;
                        save_remote_settings(&app, &settings)?;
                    }
                    let _ = app.emit("sharing-status-changed", json!({ "refresh": true }));
                    log::info!("🔧 [TRAY] Network sharing auto-restored successfully");
                }
            } else {
                log::warn!("🔧 [TRAY] Skipping auto-restore of network sharing: selected model '{}' is not shareable", model_name);
            }
        }
    }

    // Get current settings
    let mut settings = get_settings(app.clone()).await?;

    let engine = if let Some(p) = crate::cloud_stt::CloudProvider::from_id(&model_name) {
        p.id().to_string()
    } else {
        let whisper_state = app.state::<tauri::async_runtime::RwLock<WhisperManager>>();
        let whisper_has = {
            let guard = whisper_state.read().await;
            guard.get_models_status().contains_key(&model_name)
        };

        if whisper_has {
            "whisper".to_string()
        } else {
            let parakeet_manager = app.state::<ParakeetManager>();
            let is_parakeet = parakeet_manager
                .list_models()
                .into_iter()
                .any(|m| m.name == model_name);
            if is_parakeet {
                "parakeet".to_string()
            } else {
                log::warn!(
                    "set_model_from_tray: model '{}' not found in registries; defaulting to whisper",
                    model_name
                );
                "whisper".to_string()
            }
        }
    };

    // Update the model
    settings.current_model = model_name.clone();
    settings.current_model_engine = engine.clone();
    if model_requires_english_speech(&engine, &model_name) {
        settings.speech_language = "en".to_string();
    }
    // Save settings (this will also preload the model)
    save_settings(app.clone(), settings, None).await?;

    // Keep a running sharing server truthful after the selected model changes
    sync_running_sharing_server_to_model(&app, &model_name, &engine).await?;

    // Update the tray menu to reflect the new selection
    update_tray_menu(app.clone()).await?;

    // Emit event to update UI only after successful tray menu update
    if let Err(e) = app.emit(
        "model-changed",
        json!({
            "model": model_name,
            "engine": engine
        }),
    ) {
        log::warn!("Failed to emit model-changed event: {}", e);
        // Return error to caller so they know the UI might be out of sync
        return Err(format!("Failed to emit model-changed event: {}", e));
    }

    Ok(())
}

/// Increment the tray menu generation and return the new value.
/// Used by callers who want to spawn background updates.
pub fn next_tray_menu_generation() -> u64 {
    TRAY_MENU_GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

/// Get the current tray menu generation.
pub fn current_tray_menu_generation() -> u64 {
    TRAY_MENU_GENERATION.load(Ordering::SeqCst)
}

#[tauri::command]
pub async fn update_tray_menu(app: AppHandle) -> Result<(), String> {
    update_tray_menu_with_generation(app, None).await
}

/// Update tray menu with optional generation check.
/// If generation is provided, the update will be skipped if a newer generation was requested.
pub async fn update_tray_menu_with_generation(
    app: AppHandle,
    my_generation: Option<u64>,
) -> Result<(), String> {
    use std::time::Instant;
    let start_time = Instant::now();

    let gen_info = my_generation
        .map(|g| format!(" (gen={})", g))
        .unwrap_or_default();
    log::debug!("⏱️ [TRAY TIMING] update_tray_menu called{}", gen_info);

    // Build the new menu
    log::debug!(
        "⏱️ [TRAY TIMING] Building tray menu...{} (+{}ms)",
        gen_info,
        start_time.elapsed().as_millis()
    );
    let new_menu = crate::build_tray_menu(&app)
        .await
        .map_err(|e| format!("Failed to build tray menu: {}", e))?;
    log::debug!(
        "⏱️ [TRAY TIMING] Tray menu built{} (+{}ms)",
        gen_info,
        start_time.elapsed().as_millis()
    );

    // Check if this update is still current (if generation was provided)
    if let Some(my_gen) = my_generation {
        let current_gen = current_tray_menu_generation();
        if my_gen < current_gen {
            log::debug!(
                "⏱️ [TRAY TIMING] Skipping stale tray menu update (gen={} < current={})",
                my_gen,
                current_gen
            );
            return Ok(());
        }
    }

    // Update the tray menu
    if let Some(tray) = app.tray_by_id("main") {
        log::debug!(
            "⏱️ [TRAY TIMING] Setting tray menu...{} (+{}ms)",
            gen_info,
            start_time.elapsed().as_millis()
        );
        tray.set_menu(Some(new_menu))
            .map_err(|e| format!("Failed to set tray menu: {}", e))?;
        log::debug!(
            "⏱️ [TRAY TIMING] Tray menu set{} - total: {}ms",
            gen_info,
            start_time.elapsed().as_millis()
        );
    } else {
        log::warn!("Tray icon not found");
    }

    Ok(())
}

/// Set the selected microphone device
#[tauri::command]
pub async fn set_audio_device(app: AppHandle, device_name: Option<String>) -> Result<(), String> {
    log::info!("Setting audio device to: {:?}", device_name);

    // Get current settings
    let mut settings = get_settings(app.clone()).await?;

    // Check if recording is in progress and stop it
    let recorder_state = app.state::<crate::commands::audio::RecorderState>();
    {
        let mut recorder = recorder_state
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))?;

        if recorder.is_recording() {
            log::info!("Recording in progress, stopping it before changing microphone");

            // Update state to notify UI
            crate::update_recording_state(&app, crate::RecordingState::Stopping, None);

            match recorder.stop_recording() {
                Ok(msg) => {
                    log::info!("Recording stopped: {}", msg);
                    // Update state to idle after successful stop
                    crate::update_recording_state(&app, crate::RecordingState::Idle, None);
                }
                Err(e) => {
                    log::warn!("Failed to stop recording: {}", e);
                    // Update state to error if stop failed
                    crate::update_recording_state(&app, crate::RecordingState::Error, Some(e));
                }
            }
        }
    } // Lock released here

    // Update the selected microphone
    settings.selected_microphone = device_name.clone();

    // Save the updated settings
    save_settings(app.clone(), settings, None).await?;

    // Update tray menu to reflect the change
    update_tray_menu(app.clone()).await?;

    // Emit event to notify frontend - just emit a signal, frontend will reload settings
    if let Err(e) = app.emit("audio-device-changed", ()) {
        log::warn!("Failed to emit audio-device-changed event: {}", e);
    }

    log::info!("Audio device successfully set to: {:?}", device_name);
    Ok(())
}

/// Get the current autostart status from the OS.
/// Returns the actual OS-level autostart enabled state.
#[tauri::command]
pub async fn get_autostart_status(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    autolaunch.is_enabled().map_err(|e| {
        log::warn!("Failed to check autostart status: {}", e);
        e.to_string()
    })
}

/// Set autostart enabled/disabled at the OS level and persist the actual state.
/// Returns the actual OS-level state after the mutation (may differ from requested
/// if the OS call failed).
#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();

    if enabled {
        if let Err(e) = autolaunch.enable() {
            log::warn!("Failed to enable autostart: {}", e);
        }
    } else if let Err(e) = autolaunch.disable() {
        log::warn!("Failed to disable autostart: {}", e);
    }

    // Query actual state — the OS mutation may have failed silently.
    let actual = autolaunch.is_enabled().map_err(|e| {
        log::warn!("Failed to verify autostart state after mutation: {}", e);
        e.to_string()
    })?;

    // Persist actual state to settings store.
    let store = app.store("settings").map_err(|e| e.to_string())?;
    store.set("launch_at_startup", json!(actual));

    log::info!("Autostart set: requested={}, actual={}", enabled, actual);

    Ok(actual)
}

#[tauri::command]
pub async fn get_transcription_acceleration_status(
    app: AppHandle,
) -> Result<crate::whisper::gpu_sidecar::AccelerationRuntimeStatus, String> {
    let settings = get_settings(app.clone()).await?;
    let mode =
        normalize_stored_transcription_acceleration(Some(&settings.transcription_acceleration));

    if mode == "cpu" {
        return Ok(crate::whisper::gpu_sidecar::AccelerationRuntimeStatus {
            mode,
            effective_backend: "cpu".to_string(),
            gpu_available: None,
            message: "CPU mode is selected. GPU acceleration will not be used.".to_string(),
            last_error: None,
            diagnostic_code: "cpu_selected".to_string(),
            recommended_action: "none".to_string(),
        });
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(crate::whisper::gpu_sidecar::AccelerationRuntimeStatus {
            mode,
            effective_backend: if cfg!(target_os = "macos") {
                "metal".to_string()
            } else {
                "cpu".to_string()
            },
            gpu_available: None,
            message: "This acceleration setting applies to Windows Vulkan builds.".to_string(),
            last_error: None,
            diagnostic_code: "unsupported_platform".to_string(),
            recommended_action: "none".to_string(),
        })
    }

    #[cfg(target_os = "windows")]
    {
        let client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        let mut status = client.status().await;
        status.mode = mode;
        Ok(status)
    }
}

#[tauri::command]
pub async fn test_transcription_acceleration(
    app: AppHandle,
) -> Result<crate::whisper::gpu_sidecar::AccelerationRuntimeStatus, String> {
    #[cfg(not(target_os = "windows"))]
    {
        get_transcription_acceleration_status(app).await
    }

    #[cfg(target_os = "windows")]
    {
        let settings = get_settings(app.clone()).await?;
        if settings.current_model_engine != "whisper" {
            return Err(
                "GPU acceleration testing is only available for local Whisper models.".to_string(),
            );
        }

        let whisper_state = app.state::<tauri::async_runtime::RwLock<WhisperManager>>();
        let model_path = {
            let manager = whisper_state.read().await;
            manager
                .get_model_path(&settings.current_model)
                .ok_or_else(|| {
                    "Select or download a local Whisper model before testing GPU acceleration."
                        .to_string()
                })?
        };

        let mode =
            normalize_stored_transcription_acceleration(Some(&settings.transcription_acceleration));
        let client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        // Unload the failed sidecar (mirror transcription/warm-preload) so its model doesn't stay resident.
        if let Err(error) = client.probe(&app, &model_path, &mode).await {
            client.abort_active_process().await;
            return Err(error);
        }
        Ok(client.status().await)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        get_autostart_status, normalize_settings_mode, recording_retention_days_from_legacy_count,
        recording_retention_days_to_value, resolve_pill_indicator_mode, set_autostart,
        update_channel_to_persist,
    };
    use crate::commands::updater::UpdateChannel;
    use serde_json::json;

    #[test]
    fn resolve_pill_indicator_mode_prefers_new_value() {
        let resolved = resolve_pill_indicator_mode(
            Some("always".to_string()),
            Some(false),
            "when_recording".to_string(),
        );

        assert_eq!(resolved, "always");
    }

    #[test]
    fn resolve_pill_indicator_mode_migrates_legacy_true() {
        let resolved = resolve_pill_indicator_mode(None, Some(true), "when_recording".to_string());

        assert_eq!(resolved, "always");
    }

    #[test]
    fn resolve_pill_indicator_mode_migrates_legacy_false() {
        let resolved = resolve_pill_indicator_mode(None, Some(false), "when_recording".to_string());

        assert_eq!(resolved, "when_recording");
    }

    #[test]
    fn resolve_pill_indicator_mode_uses_default() {
        let resolved = resolve_pill_indicator_mode(None, None, "when_recording".to_string());

        assert_eq!(resolved, "when_recording");
    }

    #[test]
    fn settings_mode_accepts_advanced_and_falls_back_to_recommended() {
        assert_eq!(normalize_settings_mode(Some("advanced")), "recommended");
        assert_eq!(normalize_settings_mode(Some("recommended")), "recommended");
        assert_eq!(normalize_settings_mode(Some("invalid")), "recommended");
        assert_eq!(normalize_settings_mode(None), "recommended");
    }

    /// Verify the autostart command functions exist and compile.
    /// Compilation IS the test: if the functions don't exist or have
    /// wrong signatures, the imports and generate_handler! macro will fail.
    #[test]
    fn test_autostart_commands_exist() {
        // Binding to a static reference proves the function items exist
        // at the expected path. The Tauri generate_handler! macro does
        // further compile-time signature validation in lib.rs.
        let _get = get_autostart_status;
        let _set = set_autostart;
    }

    #[test]
    fn recording_retention_days_none_saves_as_null() {
        assert_eq!(
            recording_retention_days_to_value(None),
            serde_json::Value::Null
        );
    }

    #[test]
    fn recording_retention_days_some_saves_as_number() {
        assert_eq!(recording_retention_days_to_value(Some(30)), json!(30));
    }

    #[test]
    fn recording_retention_days_migrates_legacy_counts_conservatively() {
        assert_eq!(recording_retention_days_from_legacy_count(25), Some(7));
        assert_eq!(recording_retention_days_from_legacy_count(50), Some(30));
        assert_eq!(recording_retention_days_from_legacy_count(100), Some(90));
    }

    #[test]
    fn recording_retention_days_legacy_unlimited_counts_keep_forever() {
        assert_eq!(recording_retention_days_from_legacy_count(0), None);
        assert_eq!(recording_retention_days_from_legacy_count(250), None);
        assert_eq!(recording_retention_days_from_legacy_count(1), None);
    }
    #[test]
    fn legacy_implicit_stable_channel_is_removed() {
        assert_eq!(
            update_channel_to_persist(Some("stable"), false, false, UpdateChannel::Stable,),
            None
        );
    }

    #[test]
    fn explicit_and_legacy_beta_channels_remain_authoritative() {
        assert_eq!(
            update_channel_to_persist(None, false, true, UpdateChannel::Stable),
            Some(UpdateChannel::Stable)
        );
        assert_eq!(
            update_channel_to_persist(Some("stable"), true, false, UpdateChannel::Stable,),
            Some(UpdateChannel::Stable)
        );
        assert_eq!(
            update_channel_to_persist(Some("beta"), false, false, UpdateChannel::Beta),
            Some(UpdateChannel::Beta)
        );
    }
}
