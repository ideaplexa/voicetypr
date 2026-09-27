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
pub async fn build_tray_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<tauri::menu::Menu<R>, Box<dyn std::error::Error>> {
    use std::time::Instant;
    let build_start = Instant::now();
    log::debug!("⏱️ [TRAY BUILD TIMING] build_tray_menu called");

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
    log::debug!(
        "⏱️ [TRAY BUILD TIMING] Settings loaded (+{}ms)",
        build_start.elapsed().as_millis()
    );

    // Get remote server info (active connection and saved connections)
    // Use CACHED data - do NOT make HTTP calls here (that would block tray menu for seconds)
    // The frontend/background tasks handle status polling separately
    let (effective_active_id, active_remote_display, active_remote_model, remote_connections) = {
        if let Some(remote_state) = app.try_state::<AsyncMutex<RemoteSettings>>() {
            log::debug!(
                "⏱️ [TRAY BUILD TIMING] Acquiring remote_settings lock... (+{}ms)",
                build_start.elapsed().as_millis()
            );
            let settings = remote_state.lock().await;
            log::debug!(
                "⏱️ [TRAY BUILD TIMING] remote_settings lock acquired (+{}ms)",
                build_start.elapsed().as_millis()
            );
            // Build connections list using cached data (no HTTP calls!)
            // Include all servers with cached model info - let the user see what's available
            let mut connections: Vec<(String, String, Option<String>)> = Vec::new();
            for conn in settings.saved_connections.iter() {
                if should_include_remote_connection_in_tray(&conn.status) {
                    connections.push((conn.id.clone(), conn.display_name(), conn.model.clone()));
                }
            }
            log::debug!(
                "⏱️ [TRAY BUILD TIMING] Loaded {} cached connections (+{}ms)",
                connections.len(),
                build_start.elapsed().as_millis()
            );

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
    log::debug!(
        "⏱️ [TRAY BUILD TIMING] Remote server info collected (+{}ms)",
        build_start.elapsed().as_millis()
    );

    let (available_models, whisper_models_info) = {
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

    let model_submenu = if !available_models.is_empty() || !remote_connections.is_empty() {
        let mut model_items: Vec<&dyn tauri::menu::IsMenuItem<_>> = Vec::new();
        let mut model_check_items = Vec::new();
        let mut separator_items = Vec::new();
        let mut remote_header_items = Vec::new();
        let mut remote_check_items = Vec::new();

        // Add local models first
        for (model_name, display_name) in &available_models {
            // Local model - only selected if no remote is active
            let is_selected = effective_active_id.is_none()
                && should_mark_model_selected(onboarding_done, model_name, &current_model);

            let model_item = CheckMenuItem::with_id(
                app,
                format!("model_{}", model_name),
                display_name,
                true,
                is_selected,
                None::<&str>,
            )?;
            model_check_items.push(model_item);
        }

        for item in &model_check_items {
            model_items.push(item);
        }

        // Add remote servers section if any exist
        if !remote_connections.is_empty() {
            // Add separator
            let sep = PredefinedMenuItem::separator(app)?;
            separator_items.push(sep);
            for item in &separator_items {
                model_items.push(item);
            }

            // Add "Remote Voicetypr" header (disabled item)
            let header = MenuItem::with_id(
                app,
                "remote_header",
                "Remote Voicetypr",
                false,
                None::<&str>,
            )?;
            remote_header_items.push(header);
            for item in &remote_header_items {
                model_items.push(item);
            }

            // Add remote server items with format "ServerName - ModelName"
            for (conn_id, conn_display, conn_model) in &remote_connections {
                let model_id = format!("remote_{}", conn_id);
                let display = if let Some(model) = conn_model {
                    format!("{} - {}", conn_display, model)
                } else {
                    conn_display.clone()
                };
                let is_selected = effective_active_id.as_deref() == Some(conn_id.as_str());

                let remote_item = CheckMenuItem::with_id(
                    app,
                    format!("model_{}", model_id),
                    &display,
                    true,
                    is_selected,
                    None::<&str>,
                )?;
                remote_check_items.push(remote_item);
            }

            for item in &remote_check_items {
                model_items.push(item);
            }
        }

        // Resolve display name for the menu label - prioritize active remote server
        let (effective_model, resolved_display_name) = if let Some(ref remote_display) =
            active_remote_display
        {
            // Remote server is active - show "ServerName - ModelName"
            let display = if let Some(ref model) = active_remote_model {
                format!("{} - {}", remote_display, model)
            } else {
                remote_display.clone()
            };
            ("remote".to_string(), Some(display))
        } else if onboarding_done && !current_model.is_empty() {
            let display = if let Some(info) = whisper_models_info.get(&current_model) {
                Some(info.display_name.clone())
            } else if let Some(parakeet_manager) =
                app.try_state::<crate::parakeet::ParakeetManager>()
            {
                if let Some(pm) = parakeet_manager
                    .list_models()
                    .into_iter()
                    .find(|m| m.name == current_model)
                {
                    Some(pm.display_name)
                } else if let Some(p) = crate::cloud_stt::CloudProvider::from_id(&current_model) {
                    Some(p.cloud_label())
                } else {
                    Some(humanize_model_id(&current_model))
                }
            } else if let Some(p) = crate::cloud_stt::CloudProvider::from_id(&current_model) {
                Some(p.cloud_label())
            } else {
                Some(humanize_model_id(&current_model))
            };
            (current_model.clone(), display)
        } else {
            (String::new(), None)
        };

        let current_model_display = format_tray_model_label(
            onboarding_done || effective_active_id.is_some(),
            &effective_model,
            resolved_display_name,
        );

        Some(Submenu::with_id_and_items(
            app,
            "models",
            &current_model_display,
            true,
            &model_items,
        )?)
    } else {
        None
    };

    let available_devices = if onboarding_done {
        audio::recorder::AudioRecorder::get_devices()
    } else {
        Vec::new()
    };

    let microphone_submenu = if onboarding_done && !available_devices.is_empty() {
        let mut mic_items: Vec<&dyn tauri::menu::IsMenuItem<_>> = Vec::new();
        let mut mic_check_items = Vec::new();

        let default_item = CheckMenuItem::with_id(
            app,
            "microphone_default",
            "System Default",
            true,
            selected_microphone.is_none(),
            None::<&str>,
        )?;
        mic_check_items.push(default_item);

        for device_name in &available_devices {
            let is_selected = selected_microphone.as_ref() == Some(device_name);
            let mic_item = CheckMenuItem::with_id(
                app,
                format!("microphone_{}", device_name),
                device_name,
                true,
                is_selected,
                None::<&str>,
            )?;
            mic_check_items.push(mic_item);
        }

        for item in &mic_check_items {
            mic_items.push(item);
        }

        let current_mic_display = if let Some(ref mic_name) = selected_microphone {
            format!("Microphone: {}", mic_name)
        } else {
            "Microphone: Default".to_string()
        };

        Some(Submenu::with_id_and_items(
            app,
            "microphones",
            &current_mic_display,
            true,
            &mic_items,
        )?)
    } else {
        None
    };

    let polish_on_item = tauri::menu::CheckMenuItem::with_id(
        app,
        "polish_on",
        "On",
        true,
        polish_enabled,
        None::<&str>,
    )?;
    let polish_off_item = tauri::menu::CheckMenuItem::with_id(
        app,
        "polish_off",
        "Off",
        true,
        !polish_enabled,
        None::<&str>,
    )?;

    let mut recent_owned: Vec<tauri::menu::MenuItem<R>> = Vec::new();
    let mut latest_copyable_id: Option<String> = None;
    {
        if let Ok(store) = app.store("transcriptions") {
            let mut entries: Vec<(String, serde_json::Value)> = Vec::new();
            for key in store.keys() {
                if let Some(value) = store.get(&key) {
                    entries.push((key.to_string(), value));
                }
            }
            entries.sort_by(|a, b| b.0.cmp(&a.0));
            latest_copyable_id = latest_copyable_transcription_id(&entries);
            entries.truncate(5);

            for (ts, entry) in entries {
                let mut label = entry
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| {
                        let first_line = s.lines().next().unwrap_or("").trim();
                        let char_count = first_line.chars().count();
                        let mut preview: String = first_line.chars().take(40).collect();
                        if char_count > 40 {
                            preview.push('\u{2026}');
                        }
                        if preview.is_empty() {
                            "(empty)".to_string()
                        } else {
                            preview
                        }
                    })
                    .unwrap_or_else(|| "(unknown)".to_string());

                if label.is_empty() {
                    label = "(empty)".to_string();
                }

                let item = tauri::menu::MenuItem::with_id(
                    app,
                    format!("recent_copy_{}", ts),
                    label,
                    true,
                    None::<&str>,
                )?;
                recent_owned.push(item);
            }
        }
    }
    let mut recent_refs: Vec<&dyn tauri::menu::IsMenuItem<_>> = Vec::new();
    for item in &recent_owned {
        recent_refs.push(item);
    }

    let (toggle_item, ptt_item) = {
        let recording_mode = match app.store("settings") {
            Ok(store) => store
                .get("recording_mode")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "toggle".to_string()),
            Err(_) => "toggle".to_string(),
        };

        let toggle = tauri::menu::CheckMenuItem::with_id(
            app,
            "recording_mode_toggle",
            "Toggle",
            true,
            recording_mode == "toggle",
            None::<&str>,
        )?;
        let ptt = tauri::menu::CheckMenuItem::with_id(
            app,
            "recording_mode_push_to_talk",
            "Push-to-Talk",
            true,
            recording_mode == "push_to_talk",
            None::<&str>,
        )?;
        (toggle, ptt)
    };

    let copy_last_i = MenuItem::with_id(
        app,
        "copy_last_transcription",
        "Copy Last Transcription",
        latest_copyable_id.is_some(),
        None::<&str>,
    )?;
    let separator1 = PredefinedMenuItem::separator(app)?;
    let settings_i = MenuItem::with_id(app, "dashboard", "Dashboard", true, None::<&str>)?;
    let check_updates_i = if crate::commands::distribution::is_store_install() {
        None
    } else {
        Some(MenuItem::with_id(
            app,
            "check_updates",
            "Check for Updates",
            true,
            None::<&str>,
        )?)
    };
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit Voicetypr", true, None::<&str>)?;

    let mut menu_builder = MenuBuilder::new(app);

    if let Some(model_submenu) = model_submenu {
        menu_builder = menu_builder.item(&model_submenu);
    }

    if let Some(microphone_submenu) = microphone_submenu {
        menu_builder = menu_builder.item(&microphone_submenu);
    }

    let polish_items: Vec<&dyn tauri::menu::IsMenuItem<_>> =
        vec![&polish_on_item, &polish_off_item];
    let polish_submenu = Submenu::with_id_and_items(
        app,
        "polish",
        format_tray_polish_label(polish_enabled),
        true,
        &polish_items,
    )?;
    menu_builder = menu_builder.item(&polish_submenu);

    menu_builder = menu_builder.item(&copy_last_i);
    if !recent_refs.is_empty() {
        let recent_submenu =
            Submenu::with_id_and_items(app, "recent", "Recent Transcriptions", true, &recent_refs)?;
        menu_builder = menu_builder.item(&recent_submenu);
    }

    let mode_items: Vec<&dyn tauri::menu::IsMenuItem<_>> = vec![&toggle_item, &ptt_item];
    let mode_submenu =
        Submenu::with_id_and_items(app, "recording_mode", "Recording Mode", true, &mode_items)?;
    menu_builder = menu_builder.item(&mode_submenu);

    let mut menu_builder = menu_builder.item(&separator1).item(&settings_i);

    if let Some(check_updates_i) = &check_updates_i {
        menu_builder = menu_builder.item(check_updates_i);
    }

    let menu = menu_builder.item(&separator2).item(&quit_i).build()?;

    log::debug!(
        "⏱️ [TRAY BUILD TIMING] build_tray_menu COMPLETE - total: {}ms",
        build_start.elapsed().as_millis()
    );
    Ok(menu)
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
