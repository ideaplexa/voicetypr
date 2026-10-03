use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::menu::{CheckMenuItem, MenuBuilder, MenuItem, PredefinedMenuItem, Submenu};
use tauri::Manager;
use tauri_plugin_store::StoreExt;

use crate::audio;
use crate::remote::settings::ConnectionStatus;
use crate::remote::settings::RemoteSettings;
use crate::whisper;

pub fn should_include_remote_connection_in_tray(status: &ConnectionStatus) -> bool {
    !matches!(status, ConnectionStatus::SelfConnection)
}

fn effective_active_remote_id(
    active_connection_id: Option<&str>,
    connections: &[(String, String, Option<String>)],
) -> Option<String> {
    active_connection_id.and_then(|id| {
        connections
            .iter()
            .any(|(cid, _, _)| cid == id)
            .then(|| id.to_string())
    })
}

/// Determines if a model should appear as selected in the tray given onboarding status
pub fn should_mark_model_selected(
    onboarding_done: bool,
    model_name: &str,
    current_model: &str,
) -> bool {
    onboarding_done && model_name == current_model
}

#[cfg(test)]
fn title_case_token(token: &str) -> String {
    if token.is_empty() {
        return String::new();
    }
    if token.len() >= 2 && token.starts_with('v') && token[1..].chars().all(|c| c.is_ascii_digit())
    {
        return token.to_ascii_lowercase();
    }
    if token.contains('_') && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return token.to_ascii_uppercase();
    }
    let mut chars = token.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
fn humanize_model_id(model_id: &str) -> String {
    let without_en = model_id.strip_suffix(".en").unwrap_or(model_id);
    let suffix = if without_en.len() != model_id.len() {
        " English"
    } else {
        ""
    };
    let name = without_en
        .split(['-', '_'])
        .filter(|token| !token.is_empty())
        .map(title_case_token)
        .collect::<Vec<_>>()
        .join(" ");
    format!("{}{}", name, suffix)
}
/// Formats the tray's model label given onboarding status and an optional resolved display name
#[cfg(test)]
pub fn format_tray_model_label(
    onboarding_done: bool,
    current_model: &str,
    resolved_display_name: Option<String>,
) -> String {
    if !onboarding_done || current_model.is_empty() {
        "Model: None".to_string()
    } else {
        let name = resolved_display_name.unwrap_or_else(|| humanize_model_id(current_model));
        format!("Model: {}", name)
    }
}

#[cfg(test)]
pub fn format_tray_polish_label(enabled: bool) -> &'static str {
    if enabled {
        "Polish: On"
    } else {
        "Polish: Off"
    }
}

fn is_copyable_transcription_entry(entry: &serde_json::Value) -> bool {
    let status = entry
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("completed");

    if matches!(status, "in_progress" | "failed") {
        return false;
    }

    entry
        .get("text")
        .and_then(|value| value.as_str())
        .is_some_and(|text| !text.trim().is_empty())
}

pub(crate) fn latest_copyable_transcription_id(
    entries: &[(String, serde_json::Value)],
) -> Option<String> {
    entries
        .iter()
        .filter(|(_, entry)| is_copyable_transcription_entry(entry))
        .max_by(|(left_ts, _), (right_ts, _)| left_ts.cmp(right_ts))
        .map(|(timestamp, _)| timestamp.clone())
}
/// Build the tray menu with all submenus (models, microphones, recent transcriptions, recording mode)
pub async fn build_tray_menu(
    app: &tauri::AppHandle,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let snapshot = snapshot(app, true).await?;
    Ok(render(app, &super::model::build(&snapshot))?)
}

pub(crate) async fn build_tray_menu_if_current(
    app: &tauri::AppHandle,
    generation: u64,
) -> Result<Option<tauri::menu::Menu<tauri::Wry>>, Box<dyn std::error::Error>> {
    if generation != crate::commands::settings::current_tray_menu_generation() {
        return Ok(None);
    }
    let snapshot = snapshot(app, true).await?;
    if generation != crate::commands::settings::current_tray_menu_generation() {
        return Ok(None);
    }
    Ok(Some(render(app, &super::model::build(&snapshot))?))
}

/// Shared tray/island data; quick settings never read transcript history.
pub(super) async fn snapshot(
    app: &tauri::AppHandle,
    include_history: bool,
) -> Result<super::model::Snapshot, Box<dyn std::error::Error>> {
    let (current_model, selected_microphone, onboarding_done, polish_enabled) = {
        match app.store("settings") {
            Ok(store) => {
                let model = store
                    .get("current_model")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_default();
                let microphone = store
                    .get("selected_microphone")
                    .and_then(|v| v.as_str().map(|s| s.to_string()));
                let onboarding_done = store
                    .get("onboarding_completed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let polish_enabled = store
                    .get("ai_enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                (model, microphone, onboarding_done, polish_enabled)
            }
            Err(_) => ("".to_string(), None, false, false),
        }
    };

    // Get remote server info (active connection and saved connections)
    // Use CACHED data - do NOT make HTTP calls here (that would block tray menu for seconds)
    // The frontend/background tasks handle status polling separately
    let (effective_active_id, _active_remote_display, active_remote_model, remote_connections) = {
        if let Some(remote_state) = app.try_state::<AsyncMutex<RemoteSettings>>() {
            let settings = remote_state.lock().await;
            // Build connections list using cached data (no HTTP calls!)
            // Include all servers with cached model info - let the user see what's available
            let mut connections: Vec<(String, String, Option<String>)> = Vec::new();
            for conn in settings.saved_connections.iter() {
                if should_include_remote_connection_in_tray(&conn.status) {
                    connections.push((conn.id.clone(), conn.display_name(), conn.model.clone()));
                }
            }

            let effective_active_id =
                effective_active_remote_id(settings.active_connection_id.as_deref(), &connections);

            // Get effective active connection info
            let active_conn_info = effective_active_id
                .as_ref()
                .and_then(|id| connections.iter().find(|(cid, _, _)| cid == id));
            let active_display = active_conn_info.map(|(_, name, _)| name.clone());
            let active_model = active_conn_info.and_then(|(_, _, model)| model.clone());

            (
                effective_active_id,
                active_display,
                active_model,
                connections,
            )
        } else {
            (None, None, None, Vec::new())
        }
    };

    let (available_models, _whisper_models_info) = {
        // Store (name, display_name, accuracy_score, speed_score) for sorting to match UI order
        let mut models: Vec<(String, String, u8, u8)> = Vec::new();
        let mut whisper_all = std::collections::HashMap::new();

        if let Some(whisper_state) =
            app.try_state::<AsyncRwLock<whisper::manager::WhisperManager>>()
        {
            let manager = whisper_state.read().await;
            whisper_all = manager.get_models_status();
            for (name, info) in whisper_all.iter() {
                if info.downloaded {
                    models.push((
                        name.clone(),
                        info.display_name.clone(),
                        info.accuracy_score,
                        info.speed_score,
                    ));
                }
            }
        } else {
            log::warn!("WhisperManager not available for tray menu");
        }

        if let Some(parakeet_manager) = app.try_state::<crate::parakeet::ParakeetManager>() {
            for m in parakeet_manager.list_models().into_iter() {
                if m.downloaded {
                    models.push((
                        m.name.clone(),
                        m.display_name.clone(),
                        m.accuracy_score,
                        m.speed_score,
                    ));
                }
            }
        } else {
            log::warn!("ParakeetManager not available for tray menu");
        }

        for provider in crate::cloud_stt::CloudProvider::ALL {
            if crate::secure_store::secure_has(app, provider.key_name()).unwrap_or(false) {
                models.push((
                    provider.id().to_string(),
                    provider.cloud_label(),
                    u8::MAX,
                    0,
                ));
            }
        }

        // Sort by accuracy_score (descending), then by speed_score (descending) as tiebreaker
        // Higher accuracy_score = better accuracy = shown first
        // Higher speed_score = faster = shown first within same accuracy
        models.sort_by(|a, b| {
            match b.2.cmp(&a.2) {
                std::cmp::Ordering::Equal => b.3.cmp(&a.3), // speed descending
                other => other,
            }
        });

        // Convert back to (name, display_name) pairs
        let models: Vec<(String, String)> = models.into_iter().map(|(n, d, _, _)| (n, d)).collect();

        // NOTE: Remote servers are added separately below with a separator
        (models, whisper_all)
    };

    let settings_store = app.store("settings")?;
    let text = |key: &str, default: &str| {
        settings_store
            .get(key)
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| default.into())
    };
    let engine_hint = if effective_active_id.is_some() {
        if crate::parakeet::models::AVAILABLE_MODELS
            .iter()
            .any(|m| Some(m.id) == active_remote_model.as_deref())
        {
            "parakeet".into()
        } else {
            "whisper".into()
        }
    } else {
        text("current_model_engine", "whisper")
    };
    let effective_model = active_remote_model.as_deref().unwrap_or(&current_model);
    let engine = if onboarding_done || effective_active_id.is_some() {
        crate::pill::context::engine_short_name(effective_model, &engine_hint)
    } else {
        "None".into()
    };
    let mut engines = available_models
        .into_iter()
        .map(|(id, _)| {
            let name = crate::pill::context::engine_short_name(&id, &id);
            let group = if crate::cloud_stt::CloudProvider::from_id(&id).is_some() {
                "cloud"
            } else {
                "local"
            };
            let selected = effective_active_id.is_none()
                && should_mark_model_selected(onboarding_done, &id, &current_model);
            (id, name, group.into(), selected)
        })
        .collect::<Vec<_>>();
    for (id, display, model) in remote_connections {
        let name = model
            .map(|m| {
                format!(
                    "{} · {}",
                    display,
                    crate::pill::context::engine_short_name(&m, &m)
                )
            })
            .unwrap_or(display);
        let selected = effective_active_id.as_deref() == Some(&id);
        engines.push((format!("remote_{id}"), name, "network".into(), selected));
    }
    let cloud_model =
        crate::cloud_stt::CloudProvider::from_id(&engine_hint).map(|p| p.selected_model(app).id);
    let languages = super::languages::supported(effective_model, &engine_hint, cloud_model);
    let mut recent = Vec::new();
    if let Some(store) = include_history
        .then(|| app.store("transcriptions").ok())
        .flatten()
    {
        let entries = crate::commands::audio::page_history_keys(store.keys(), 5)
            .into_iter()
            .filter_map(|id| store.get(&id).map(|v| (id, v)))
            .filter(|(_, v)| is_copyable_transcription_entry(v))
            .collect::<Vec<_>>();
        for (id, entry) in entries.into_iter().take(5) {
            let raw = entry["text"]
                .as_str()
                .unwrap_or_default()
                .replace(['\n', '\r', '\t'], " ");
            let mut preview = raw.chars().take(40).collect::<String>();
            if raw.chars().count() > 40 {
                preview.push('…');
            }
            let age = chrono::DateTime::parse_from_rfc3339(&id)
                .ok()
                .map(|ts| {
                    relative_time(
                        (chrono::Utc::now() - ts.with_timezone(&chrono::Utc)).num_seconds(),
                    )
                })
                .unwrap_or_else(|| "earlier".into());
            recent.push((id, format!("{preview} · {age}")));
        }
    }
    let preset = settings_store
        .get("enhancement_options")
        .and_then(|v| serde_json::from_value::<crate::ai::prompts::EnhancementOptions>(v).ok())
        .map(|o| o.preset);
    let polish = if polish_enabled {
        preset.map(super::actions::style_label).unwrap_or("Clean")
    } else {
        "Off"
    };
    let (recording, blocked) = super::runtime::snapshot();
    let shortcut = super::runtime::shortcut_text(app);
    let (kept, kept_busy) =
        if let Some((id, alternative, _, busy)) = crate::recording::kept::tray_recovery() {
            (Some((id, alternative)), busy)
        } else {
            (None, false)
        };
    let snapshot = super::model::Snapshot {
        windows: cfg!(target_os = "windows"),
        engine,
        recording,
        blocked,
        kept,
        kept_busy,
        shortcut,
        polish: polish.into(),
        engines,
        mic: selected_microphone,
        devices: audio::recorder::AudioRecorder::get_devices(),
        language: text("speech_language", "en"),
        languages,
        hold: text("recording_mode", "toggle") == "push_to_talk",
        preview: text("transcription_mode", "regular") == "live_preview",
        recent,
        updates: !crate::commands::distribution::is_store_install(),
    };
    Ok(snapshot)
}

fn render<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    items: &[super::model::Item],
) -> tauri::Result<tauri::menu::Menu<R>> {
    let menu = MenuBuilder::new(app).build()?;
    for item in items {
        menu.append(&render_item(app, item)?)?;
    }
    Ok(menu)
}
fn render_item<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    item: &super::model::Item,
) -> tauri::Result<tauri::menu::MenuItemKind<R>> {
    use super::model::Item;
    use tauri::menu::MenuItemKind;
    Ok(match item {
        Item::Separator => MenuItemKind::Predefined(PredefinedMenuItem::separator(app)?),
        Item::Action {
            id,
            label,
            enabled,
            checked: Some(checked),
            accelerator,
        } => MenuItemKind::Check(CheckMenuItem::with_id(
            app,
            id,
            label,
            *enabled,
            *checked,
            accelerator.as_deref(),
        )?),
        Item::Action {
            id,
            label,
            enabled,
            accelerator,
            ..
        } => MenuItemKind::MenuItem(MenuItem::with_id(
            app,
            id,
            label,
            *enabled,
            accelerator.as_deref(),
        )?),
        Item::Submenu { id, label, items } => {
            let submenu = Submenu::with_id(app, id, label, true)?;
            for child in items {
                submenu.append(&render_item(app, child)?)?;
            }
            MenuItemKind::Submenu(submenu)
        }
    })
}
fn relative_time(seconds: i64) -> String {
    match seconds.max(0) {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", seconds / 60),
        3600..=86399 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_active_remote_id_returns_none_when_active_remote_is_filtered_out() {
        let visible_connections = vec![("remote-1".to_string(), "Remote 1".to_string(), None)];

        assert_eq!(
            effective_active_remote_id(Some("stale-remote"), &visible_connections),
            None
        );

        assert_eq!(
            effective_active_remote_id(Some("remote-1"), &visible_connections),
            Some("remote-1".to_string())
        );
    }

    #[test]
    fn latest_copyable_transcription_id_skips_failed_and_in_progress_entries() {
        let entries = vec![
            (
                "2026-05-23T10:00:00Z".to_string(),
                serde_json::json!({
                    "text": "Older success",
                    "status": "completed",
                }),
            ),
            (
                "2026-05-23T11:00:00Z".to_string(),
                serde_json::json!({
                    "text": "Still running",
                    "status": "in_progress",
                }),
            ),
            (
                "2026-05-23T12:00:00Z".to_string(),
                serde_json::json!({
                    "text": "Failed retry",
                    "status": "failed",
                }),
            ),
        ];

        assert_eq!(
            latest_copyable_transcription_id(&entries),
            Some("2026-05-23T10:00:00Z".to_string())
        );
    }
}
