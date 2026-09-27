//! Tauri commands for remote transcription
//!
//! These commands allow the frontend to control remote transcription
//! server mode and client mode.

use std::path::PathBuf;

use tauri::{
    async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock},
    AppHandle, Emitter, Manager, State,
};
use tauri_plugin_store::StoreExt;

use crate::remote::client::{
    self, timeout_ms_for_wav_file, RemoteClientError, RemoteServerConnection, TranscriptionRequest,
    TranscriptionSource,
};
use crate::remote::discovery::DiscoveredRemoteServer;
use crate::remote::lifecycle::{BindingResult, RemoteServerManager, SharingStatus};
use crate::remote::model_control;
use crate::remote::server::{RemoteModelControlSnapshot, RemoteModelControlUpdate, StatusResponse};
use crate::remote::settings::{ConnectionStatus, RemoteSettings, SavedConnection};
use crate::whisper::manager::WhisperManager;

/// Default port for remote transcription
pub const DEFAULT_PORT: u16 = 47842;
const SHARING_PASSWORD_KEY: &str = "remote_sharing_password";

fn remote_connection_password_key(server_id: &str) -> String {
    format!("remote_connection_password_{}", server_id)
}

fn secure_get_optional(app: &AppHandle, key: &str) -> Result<Option<String>, String> {
    crate::secure_store::secure_get(app, key)
}

fn secure_set_optional(
    app: &AppHandle,
    key: &str,
    value: Option<&str>,
) -> Result<Option<String>, String> {
    match value {
        Some(value) if !value.is_empty() => {
            crate::secure_store::secure_set(app, key, value)?;
            Ok(Some(value.to_string()))
        }
        _ => {
            crate::secure_store::secure_delete(app, key)?;
            Ok(None)
        }
    }
}

fn set_connection_password(
    app: &AppHandle,
    server_id: &str,
    password: Option<&str>,
) -> Result<Option<String>, String> {
    secure_set_optional(app, &remote_connection_password_key(server_id), password)
}

fn get_connection_password(app: &AppHandle, server_id: &str) -> Result<Option<String>, String> {
    secure_get_optional(app, &remote_connection_password_key(server_id))
}

fn delete_connection_password(app: &AppHandle, server_id: &str) -> Result<(), String> {
    crate::secure_store::secure_delete(app, &remote_connection_password_key(server_id))
}

fn ensure_sharing_engine_supported(engine: &str) -> Result<(), String> {
    if crate::cloud_stt::CloudProvider::from_id(engine).is_some() {
        return Err(
            "Network sharing is not available for cloud transcription. Please select a Whisper or Parakeet model to share."
                .to_string(),
        );
    }
    Ok(())
}

pub async fn resolve_shareable_model_config(
    app: &AppHandle,
    model_name: &str,
    engine: &str,
) -> Result<(PathBuf, String), String> {
    ensure_sharing_engine_supported(engine)?;

    match engine {
        "whisper" => {
            let whisper_state = app.state::<AsyncRwLock<WhisperManager>>();
            let guard = whisper_state.read().await;
            let model_path = guard
                .get_model_path(model_name)
                .ok_or_else(|| format!("Model '{}' not found or not downloaded", model_name))?;

            Ok((model_path, engine.to_string()))
        }
        "parakeet" => {
            let parakeet_manager = app.state::<crate::parakeet::ParakeetManager>();
            let downloaded = parakeet_manager
                .list_models()
                .into_iter()
                .find(|model| model.name == model_name)
                .map(|model| model.downloaded)
                .unwrap_or(false);

            if !downloaded {
                return Err(format!(
                    "Parakeet model '{}' not found or not downloaded",
                    model_name
                ));
            }

            Ok((PathBuf::new(), engine.to_string()))
        }
        other => Err(format!("Unsupported sharing engine '{}'", other)),
    }
}

fn is_self_connection(local_machine_id: Option<&str>, remote_machine_id: &str) -> bool {
    local_machine_id == Some(remote_machine_id)
}

fn remote_settings_has_server_password_marker(raw_value: Option<&serde_json::Value>) -> bool {
    raw_value
        .and_then(|value| value.get("server_config"))
        .and_then(|server_config| server_config.get("has_password"))
        .and_then(|has_password| has_password.as_bool())
        .unwrap_or(false)
}

fn remote_settings_connection_password_markers(
    raw_value: Option<&serde_json::Value>,
) -> std::collections::HashSet<String> {
    raw_value
        .and_then(|value| value.get("saved_connections"))
        .and_then(|connections| connections.as_array())
        .into_iter()
        .flatten()
        .filter(|connection| {
            connection
                .get("has_password")
                .and_then(|has_password| has_password.as_bool())
                .unwrap_or(false)
        })
        .filter_map(|connection| {
            connection
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_string)
        })
        .collect()
}

/// Sharing status exposed to the frontend, including persisted host opt-in settings.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SharingStatusView {
    pub enabled: bool,
    pub port: Option<u16>,
    pub model_name: Option<String>,
    pub server_name: Option<String>,
    pub active_connections: u32,
    pub password_configured: bool,
    pub binding_results: Vec<BindingResult>,
    pub allow_model_control: bool,
}

impl SharingStatusView {
    fn from_parts(status: SharingStatus, allow_model_control: bool) -> Self {
        Self {
            enabled: status.enabled,
            port: status.port,
            model_name: status.model_name,
            server_name: status.server_name,
            active_connections: status.active_connections,
            password_configured: status.password_configured,
            binding_results: status.binding_results,
            allow_model_control,
        }
    }
}

// ============================================================================
// Server Mode Commands
// ============================================================================

/// Start sharing the currently selected model with other Voicetypr instances
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn start_sharing(
    app: AppHandle,
    port: Option<u16>,
    password: Option<String>,
    preserve_password: Option<bool>,
    server_name: Option<String>,
    server_manager: State<'_, AsyncMutex<RemoteServerManager>>,
    whisper_manager: State<'_, AsyncRwLock<WhisperManager>>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<(), String> {
    let port = port.unwrap_or(DEFAULT_PORT);
    let preserve_password = preserve_password.unwrap_or(false);

    {
        let settings = remote_settings.lock().await;
        ensure_sharing_can_start(&settings)?;
        model_control::set_model_control_enabled(settings.server_config.allow_model_control);
    }

    let previous_password = secure_get_optional(&app, SHARING_PASSWORD_KEY)?;
    let password = if preserve_password {
        previous_password.clone()
    } else {
        password.filter(|password| !password.is_empty())
    };

    // Get server name from hostname if not provided
    let server_name = server_name.unwrap_or_else(|| {
        hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "Voicetypr Server".to_string())
    });

    // Get current model and engine from store
    // NOTE: Must use "settings" store to match save_settings command
    let store = app
        .store("settings")
        .map_err(|e| format!("Failed to access store: {}", e))?;

    let stored_model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    let stored_engine = store
        .get("current_model_engine")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "whisper".to_string());

    // Get model path based on engine
    let (current_model, model_path, engine) = {
        let manager = whisper_manager.read().await;

        // If no model explicitly selected, find first downloaded model (whisper only for auto-select)
        let model_name = if stored_model.is_empty() {
            manager
                .get_first_downloaded_model()
                .ok_or("No model downloaded. Please download a model first.")?
        } else {
            stored_model
        };

        let (path, engine) =
            resolve_shareable_model_config(&app, &model_name, &stored_engine).await?;

        (model_name, path, engine)
    };

    // Start the server (pass app handle for Parakeet support)
    let mut manager = server_manager.lock().await;
    manager
        .start(
            port,
            password.clone(),
            server_name,
            model_path,
            current_model,
            engine,
            Some(app.clone()),
        )
        .await?;

    if !preserve_password {
        if let Err(error) = secure_set_optional(&app, SHARING_PASSWORD_KEY, password.as_deref()) {
            manager.stop().await;
            return Err(error);
        }
    }

    // Persist the sharing enabled state so it auto-starts on next launch
    let save_result = {
        let mut settings = remote_settings.lock().await;
        // Snapshot prior values so the optimistic mutation below can be rolled
        // back symmetrically if persistence fails (matching the keychain rollback).
        let prev_enabled = settings.server_config.enabled;
        let prev_port = settings.server_config.port;
        let prev_password = settings.server_config.password.clone();
        settings.server_config.enabled = true;
        settings.server_config.port = port;
        settings.server_config.password = password.clone();
        let save_result = save_remote_settings(&app, &settings);
        if save_result.is_ok() {
            log::info!(
                "🌐 [SHARING] Saved sharing state: enabled=true, port={}",
                port
            );
        } else {
            // Restore the prior in-memory state; otherwise later guards/queries
            // would see sharing as enabled despite the failed save and server stop.
            settings.server_config.enabled = prev_enabled;
            settings.server_config.port = prev_port;
            settings.server_config.password = prev_password;
        }
        save_result
    };

    if let Err(error) = save_result {
        if !preserve_password {
            if let Err(rollback_error) =
                secure_set_optional(&app, SHARING_PASSWORD_KEY, previous_password.as_deref())
            {
                log::warn!(
                    "🌐 [SHARING] Failed to roll back sharing password after save failure: {}",
                    rollback_error
                );
            }
        }
        manager.stop().await;
        return Err(error);
    }

    log::info!("Sharing started on port {}", port);

    // Emit event to notify frontend of sharing status change
    let status = manager.get_status();
    let _ = app.emit(
        "sharing-status-changed",
        serde_json::json!({
            "enabled": status.enabled,
            "port": status.port,
            "model_name": status.model_name
        }),
    );
    log::info!("🔔 [SHARING] Emitted sharing-status-changed event (enabled=true)");

    Ok(())
}

/// Stop sharing models with other Voicetypr instances
#[tauri::command]
pub async fn stop_sharing(
    app: AppHandle,
    server_manager: State<'_, AsyncMutex<RemoteServerManager>>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<(), String> {
    let mut manager = server_manager.lock().await;
    manager.stop().await;

    // Persist the sharing disabled state
    {
        let mut settings = remote_settings.lock().await;
        settings.server_config.enabled = false;
        save_remote_settings(&app, &settings)?;
        log::info!("🌐 [SHARING] Saved sharing state: enabled=false");
    }

    log::info!("Sharing stopped");

    // Emit event to notify frontend of sharing status change
    let _ = app.emit(
        "sharing-status-changed",
        serde_json::json!({
            "enabled": false,
            "port": null,
            "model_name": null
        }),
    );
    log::info!("🔔 [SHARING] Emitted sharing-status-changed event (enabled=false)");

    Ok(())
}

/// Get the current sharing status
#[tauri::command]
pub async fn get_sharing_status(
    server_manager: State<'_, AsyncMutex<RemoteServerManager>>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<SharingStatusView, String> {
    let status = {
        let manager = server_manager.lock().await;
        manager.get_status()
    };
    let allow_model_control = {
        let settings = remote_settings.lock().await;
        settings.server_config.allow_model_control
    };
    Ok(SharingStatusView::from_parts(status, allow_model_control))
}

/// Persist host opt-in for remote model control without restarting sharing.
#[tauri::command]
pub async fn update_remote_model_control_enabled(
    app: AppHandle,
    enabled: bool,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<(), String> {
    {
        let mut settings = remote_settings.lock().await;
        settings.server_config.allow_model_control = enabled;
        save_remote_settings(&app, &settings)?;
    }
    model_control::set_model_control_enabled(enabled);
    Ok(())
}

/// Get local IP addresses for display in Network Sharing UI
#[tauri::command]
pub fn get_local_ips() -> Result<Vec<String>, String> {
    use local_ip_address::list_afinet_netifas;

    let network_interfaces =
        list_afinet_netifas().map_err(|e| format!("Failed to get network interfaces: {}", e))?;

    let ips: Vec<String> = network_interfaces
        .into_iter()
        .filter_map(|(name, ip)| {
            // Skip loopback addresses
            if ip.is_loopback() {
                return None;
            }
            // Only include IPv4 addresses for simplicity
            if let std::net::IpAddr::V4(ipv4) = ip {
                // Skip link-local addresses (169.254.x.x)
                if ipv4.is_link_local() {
                    return None;
                }
                Some(format!("{} ({})", ip, name))
            } else {
                None // Skip IPv6 for now
            }
        })
        .collect();

    Ok(ips)
}

// ============================================================================
// Client Mode Commands
// ============================================================================

/// Add a new remote server connection
#[tauri::command]
pub async fn add_remote_server(
    app: AppHandle,
    host: String,
    port: u16,
    password: Option<String>,
    name: Option<String>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<SavedConnection, String> {
    let local_machine_id = get_local_machine_id().ok();

    // Try to test connection to get server name and model, but don't require it
    let (display_name, model) = match test_connection(&host, port, password.as_deref()).await {
        Ok(status) => {
            if is_self_connection(local_machine_id.as_deref(), &status.machine_id) {
                return Err("Cannot add your own machine as a remote server".to_string());
            }

            // Use server name if no custom name provided
            let name_to_use = name.unwrap_or(status.name);
            (name_to_use, Some(status.model))
        }
        Err(_) => {
            // Connection failed, but still allow adding the server
            log::info!(
                "Connection test failed for {}:{}, adding server anyway",
                host,
                port
            );
            (name.unwrap_or_else(|| format!("{}:{}", host, port)), None)
        }
    };

    // Add to settings
    let mut settings = remote_settings.lock().await;
    let mut connection =
        settings.add_connection(host, port, None, Some(display_name), model.clone());
    let password_result = set_connection_password(&app, &connection.id, password.as_deref());
    connection.password = match password_result {
        Ok(password) => password,
        Err(error) => {
            let _ = settings.remove_connection(&connection.id);
            return Err(error);
        }
    };
    if let Some(stored) = settings
        .saved_connections
        .iter_mut()
        .find(|c| c.id == connection.id)
    {
        stored.password = connection.password.clone();
    }

    log::info!(
        "Added remote server: {} (model: {:?})",
        connection.display_name(),
        model
    );

    // Save settings
    save_remote_settings(&app, &settings)?;

    Ok(connection)
}

#[tauri::command]
pub async fn discover_remote_servers(
    timeout_ms: Option<u64>,
) -> Result<Vec<DiscoveredRemoteServer>, String> {
    let local_machine_id = get_local_machine_id().ok();
    crate::remote::discovery::discover_remote_servers(
        local_machine_id.as_deref(),
        std::time::Duration::from_millis(timeout_ms.unwrap_or(1_200)),
    )
    .await
}

/// Remove a remote server connection
#[tauri::command]
pub async fn remove_remote_server(
    app: AppHandle,
    server_id: String,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
    server_manager: State<'_, AsyncMutex<RemoteServerManager>>,
) -> Result<(), String> {
    let was_active = {
        let mut settings = remote_settings.lock().await;
        if settings.get_connection(&server_id).is_none() {
            return Err(format!("Connection '{}' not found", server_id));
        }

        let was_active = settings.active_connection_id.as_deref() == Some(server_id.as_str());
        settings.remove_connection(&server_id)?;

        log::info!("Removed remote server: {}", server_id);

        // Persist the connection removal FIRST. Only once that succeeds do we
        // drop the secret from the secure store, so a save failure can never
        // strand the connection without its password (unresolvable marker).
        save_remote_settings(&app, &settings)?;
        was_active
    };
    // Settings persisted — safe to forget the secret now. A failure here only
    // leaves an orphaned keychain entry (no connection references it), so warn
    // rather than fail the already-completed removal.
    if let Err(e) = delete_connection_password(&app, &server_id) {
        log::warn!(
            "Failed to delete remote connection password for '{}' from secure store: {}",
            server_id,
            e
        );
    }

    if was_active {
        return set_active_remote_server(app, None, remote_settings, server_manager).await;
    }

    Ok(())
}

fn apply_server_update(
    settings: &mut RemoteSettings,
    server_id: &str,
    host: String,
    port: u16,
    password: Option<String>,
    name: Option<String>,
    model: Option<String>,
) -> Result<SavedConnection, String> {
    let was_active = settings.active_connection_id.as_deref() == Some(server_id);

    settings.remove_connection(server_id)?;

    let display_name = name.unwrap_or_else(|| format!("{}:{}", host, port));
    let mut connection = settings.add_connection(host, port, password, Some(display_name), model);

    // Preserve the existing connection ID to keep external references stable
    connection.id = server_id.to_string();
    if let Some(last) = settings.saved_connections.last_mut() {
        last.id = server_id.to_string();
    }

    if was_active {
        settings.active_connection_id = Some(server_id.to_string());
    }

    Ok(connection)
}

fn ensure_sharing_can_start(settings: &RemoteSettings) -> Result<(), String> {
    if settings.active_connection_id.is_some() {
        return Err(
            "Network sharing is unavailable while using a remote Voicetypr instance as your model source."
                .to_string(),
        );
    }

    Ok(())
}

fn ensure_remote_selection_is_allowed(
    settings: &RemoteSettings,
    server_id: &str,
) -> Result<(), String> {
    let connection = settings
        .get_connection(server_id)
        .ok_or_else(|| format!("Server '{}' not found", server_id))?;

    if matches!(connection.status, ConnectionStatus::SelfConnection) {
        return Err("Cannot use this Voicetypr instance as its own remote server".to_string());
    }

    Ok(())
}

/// Update an existing remote server connection
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn update_remote_server(
    app: AppHandle,
    server_id: String,
    host: String,
    port: u16,
    password: Option<String>,
    preserve_password: Option<bool>,
    name: Option<String>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<SavedConnection, String> {
    let existing = {
        let settings = remote_settings.lock().await;
        settings
            .get_connection(&server_id)
            .ok_or_else(|| format!("Server '{}' not found", server_id))?
            .clone()
    };

    let preserve_password = preserve_password.unwrap_or(false);

    // Snapshot the current secure-store secret so we can restore it if persisting
    // the updated connection fails below (mirrors `start_sharing`'s rollback).
    // Without this, a save failure would leave the persisted connection pointing
    // at the old marker while the secret has already been replaced/removed.
    let previous_password = if preserve_password {
        None
    } else {
        get_connection_password(&app, &server_id)?
    };
    let resolved_password = if preserve_password {
        get_connection_password(&app, &server_id)?.or(existing.password.clone())
    } else {
        secure_set_optional(
            &app,
            &remote_connection_password_key(&server_id),
            password.as_deref(),
        )?
    };

    // Try to test connection to get model info, but don't require it
    let model = match test_connection(&host, port, resolved_password.as_deref()).await {
        Ok(status) => Some(status.model),
        Err(_) => existing.model.clone(), // Keep existing model if connection fails
    };

    let mut settings = remote_settings.lock().await;
    // Snapshot in-memory state so a failed save restores it symmetrically with
    // the keychain rollback below.
    let settings_snapshot = settings.clone();
    let connection = apply_server_update(
        &mut settings,
        &server_id,
        host,
        port,
        resolved_password,
        name,
        model,
    )?;

    log::info!("Updated remote server: {}", connection.display_name());

    match save_remote_settings(&app, &settings) {
        Ok(()) => Ok(connection),
        Err(error) => {
            // Roll back the in-memory mutation so later reads don't observe the
            // half-applied update.
            *settings = settings_snapshot;
            if !preserve_password {
                if let Err(rollback_error) = secure_set_optional(
                    &app,
                    &remote_connection_password_key(&server_id),
                    previous_password.as_deref(),
                ) {
                    log::warn!(
                        "Failed to roll back remote connection password after save failure for '{}': {}",
                        server_id,
                        rollback_error
                    );
                }
            }
            Err(error)
        }
    }
}

/// List all saved remote server connections
#[tauri::command]
pub async fn list_remote_servers(
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<Vec<SavedConnection>, String> {
    let settings = remote_settings.lock().await;
    Ok(settings.list_connections())
}

/// Test connection to a remote server by host/port/password (before saving)
/// Returns specific error types: "connection_failed", "auth_failed", or success
#[tauri::command]
pub async fn test_remote_connection(
    host: String,
    port: u16,
    password: Option<String>,
) -> Result<StatusResponse, String> {
    test_connection(&host, port, password.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Test connection to a saved remote server
/// Also updates the cached model if it changed and refreshes the tray menu
#[tauri::command]
pub async fn test_remote_server(
    app: AppHandle,
    server_id: String,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<StatusResponse, String> {
    // Get connection info and cached model
    let (connection, cached_model) = {
        let settings = remote_settings.lock().await;
        let conn = settings
            .get_connection(&server_id)
            .ok_or_else(|| format!("Server '{}' not found", server_id))?
            .clone();
        let cached = conn.model.clone();
        (conn, cached)
    };

    // Test the connection
    let status = match test_connection(
        &connection.host,
        connection.port,
        connection.password.as_deref(),
    )
    .await
    {
        Ok(status) => status,
        Err(error) => {
            {
                let mut settings = remote_settings.lock().await;
                settings.update_connection_status(
                    &server_id,
                    connection_status_for_remote_error(&error),
                    None,
                    None,
                );
                if let Err(save_error) = save_remote_settings(&app, &settings) {
                    log::warn!(
                        "Failed to save remote settings after test probe failure: {}",
                        save_error
                    );
                }
            }
            return Err(error.to_string());
        }
    };

    // Persist the freshly advertised capabilities (authoritative) and the model.
    let new_model = Some(status.model.clone());
    let model_changed = cached_model != new_model;
    {
        let mut settings = remote_settings.lock().await;
        if let Some(conn) = settings
            .saved_connections
            .iter_mut()
            .find(|c| c.id == server_id)
        {
            // A successful probe is authoritative for capabilities: this clears
            // stale caps when the host no longer advertises context support.
            conn.capabilities = status.capabilities.clone();
            if model_changed {
                conn.model = new_model;
            }
        }
        save_remote_settings(&app, &settings)?;
    }

    if model_changed {
        log::info!(
            "🔄 [REMOTE] Model changed for '{}': {:?} -> {}",
            connection.display_name(),
            cached_model,
            status.model
        );

        // Refresh tray menu to show new model
        if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
            log::warn!(
                "🔄 [REMOTE] Failed to update tray menu after model change: {}",
                e
            );
        } else {
            log::info!("🔄 [REMOTE] Tray menu updated with new model");
        }
    }

    Ok(status)
}

/// Check the status of a single remote server
/// Internal helper: probes a single connection and refreshes its cached status.
/// Not a Tauri command — invoked from other remote/settings command handlers.
pub(crate) async fn refresh_saved_connection_status(
    app: &AppHandle,
    server_id: &str,
) -> Result<SavedConnection, String> {
    let remote_settings = app.state::<AsyncMutex<RemoteSettings>>();
    let local_machine_id = get_local_machine_id().ok();

    let server = {
        let settings = remote_settings.lock().await;
        settings
            .get_connection(server_id)
            .cloned()
            .ok_or_else(|| format!("Server '{}' not found", server_id))?
    };

    let check_result = test_connection(&server.host, server.port, server.password.as_deref()).await;

    let (new_status, new_model, capabilities) = match check_result {
        Ok(status_response) => {
            let capabilities = status_response.capabilities.clone();
            if is_self_connection(local_machine_id.as_deref(), &status_response.machine_id) {
                (
                    ConnectionStatus::SelfConnection,
                    Some(status_response.model),
                    Some(capabilities),
                )
            } else {
                (
                    ConnectionStatus::Online,
                    Some(status_response.model),
                    Some(capabilities),
                )
            }
        }
        Err(error) => {
            log::warn!(
                "[Remote Client] Status probe failed for '{}': {}",
                server.display_name(),
                error
            );
            (connection_status_for_remote_error(&error), None, None)
        }
    };

    let (updated_server, was_active) = {
        let mut settings = remote_settings.lock().await;
        let was_active = settings.active_connection_id.as_deref() == Some(server_id);
        settings.update_connection_status(
            server_id,
            new_status.clone(),
            new_model.clone(),
            capabilities,
        );
        if let Err(e) = save_remote_settings(app, &settings) {
            log::warn!("Failed to save remote settings: {}", e);
        }

        let updated_server = settings
            .saved_connections
            .iter()
            .find(|s| s.id == server_id)
            .cloned()
            .ok_or_else(|| format!("Server '{}' not found after update", server_id))?;

        (updated_server, was_active)
    };

    let _ = app.emit(
        "remote-server-status-changed",
        serde_json::json!({
            "serverId": server_id,
            "status": format!("{:?}", new_status),
        }),
    );

    if was_active {
        let availability = crate::recognition::emit_recognition_availability(app).await;
        if let Err(error) =
            crate::recognition::auto_select_model_if_needed(app, &availability).await
        {
            log::warn!(
                "Failed to reconcile onboarding/model selection after active remote refresh: {}",
                error
            );
        }
    }

    log::debug!("🔄 [REMOTE] Server {} status: {:?}", server_id, new_status);
    Ok(updated_server)
}

pub(crate) async fn refresh_active_remote_server_status_impl(
    app: &AppHandle,
    remote_settings: &AsyncMutex<RemoteSettings>,
) -> Result<Option<SavedConnection>, String> {
    let active_remote_id = {
        let settings = remote_settings.lock().await;
        settings.active_connection_id.clone()
    };

    let Some(active_remote_id) = active_remote_id else {
        return Ok(None);
    };

    let has_active_connection = {
        let settings = remote_settings.lock().await;
        settings.get_connection(&active_remote_id).is_some()
    };

    if !has_active_connection {
        let mut settings = remote_settings.lock().await;
        settings.active_connection_id = None;
        save_remote_settings(app, &settings)?;
        let availability = crate::recognition::emit_recognition_availability(app).await;
        if let Err(error) =
            crate::recognition::auto_select_model_if_needed(app, &availability).await
        {
            log::warn!("Failed to reconcile onboarding/model selection after clearing orphaned active remote: {}", error);
        }
        return Ok(None);
    }

    let refreshed = refresh_saved_connection_status(app, &active_remote_id).await?;
    Ok(Some(refreshed))
}

/// Refresh only the active remote server status, if one is selected
#[tauri::command]
pub async fn refresh_active_remote_server_status(
    app: AppHandle,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<Option<SavedConnection>, String> {
    refresh_active_remote_server_status_impl(&app, &remote_settings).await
}

#[tauri::command]
pub async fn check_remote_server_status(
    app: AppHandle,
    server_id: String,
    _remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<SavedConnection, String> {
    log::debug!(
        "🔄 [REMOTE] check_remote_server_status called for {}",
        server_id
    );
    refresh_saved_connection_status(&app, &server_id).await
}

/// Refresh the status of all remote servers (legacy - returns list without checking)
/// For status checks, use check_remote_server_status for each server in parallel
#[tauri::command]
pub async fn refresh_remote_servers(
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<Vec<SavedConnection>, String> {
    log::info!("🔄 [REMOTE] refresh_remote_servers called (returning cached list)");
    let settings = remote_settings.lock().await;
    Ok(settings.saved_connections.clone())
}
fn connection_for_client(
    app: &AppHandle,
    connection: &SavedConnection,
) -> Result<RemoteServerConnection, String> {
    let password = get_connection_password(app, &connection.id)?.or(connection.password.clone());
    Ok(RemoteServerConnection::new(
        connection.host.clone(),
        connection.port,
        password,
    ))
}

fn remote_control_error_message(error: RemoteClientError) -> String {
    match &error {
        RemoteClientError::HttpStatus { status, body, .. }
            if *status == reqwest::StatusCode::FORBIDDEN
                && body
                    .as_deref()
                    .is_some_and(|body| body.contains("model_control_disabled")) =>
        {
            "Remote model control is disabled on this device.".to_string()
        }
        RemoteClientError::HttpStatus { status, body, .. }
            if *status == reqwest::StatusCode::FORBIDDEN
                && body
                    .as_deref()
                    .is_some_and(|body| body.contains("control_requires_password")) =>
        {
            "Remote model control requires a sharing password on the server.".to_string()
        }
        RemoteClientError::AuthFailed { .. } => {
            "Remote model control authentication failed.".to_string()
        }
        _ => error.to_string(),
    }
}

#[tauri::command]
pub async fn get_remote_transcription_control(
    app: AppHandle,
    server_id: String,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<RemoteModelControlSnapshot, String> {
    let connection = {
        let settings = remote_settings.lock().await;
        settings
            .get_connection(&server_id)
            .cloned()
            .ok_or_else(|| format!("Server '{}' not found", server_id))?
    };
    let connection = connection_for_client(&app, &connection)?;

    client::get_model_control(&connection)
        .await
        .map_err(remote_control_error_message)
}

#[tauri::command]
pub async fn update_remote_transcription_control(
    app: AppHandle,
    server_id: String,
    current_model: String,
    current_model_engine: String,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<RemoteModelControlSnapshot, String> {
    let connection = {
        let settings = remote_settings.lock().await;
        settings
            .get_connection(&server_id)
            .cloned()
            .ok_or_else(|| format!("Server '{}' not found", server_id))?
    };
    let client_connection = connection_for_client(&app, &connection)?;

    let snapshot = client::update_model_control(
        &client_connection,
        RemoteModelControlUpdate {
            model: current_model,
            engine: current_model_engine,
        },
    )
    .await
    .map_err(remote_control_error_message)?;

    {
        let mut settings = remote_settings.lock().await;
        settings.update_connection_status(
            &server_id,
            ConnectionStatus::Online,
            Some(snapshot.current.id.clone()),
            Some(None), // model/engine may have changed; clear caps to re-fetch (fail safe)
        );
        save_remote_settings(&app, &settings)?;
    }

    let _ = app.emit(
        "remote-server-settings-changed",
        serde_json::json!({ "serverId": server_id }),
    );
    Ok(snapshot)
}

/// Set the active remote server for transcription
#[tauri::command]
pub async fn set_active_remote_server(
    app: AppHandle,
    server_id: Option<String>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
    server_manager: State<'_, AsyncMutex<RemoteServerManager>>,
) -> Result<(), String> {
    use std::time::Instant;
    let cmd_start = Instant::now();

    log::info!(
        "🔧 [REMOTE] set_active_remote_server called with server_id={:?}",
        server_id
    );

    // Track if we need to restore sharing after clearing remote server
    let mut should_restore_sharing = false;
    let mut restore_port: Option<u16> = None;
    let mut restore_password: Option<String> = None;

    // If selecting a remote server, validate it first and refresh its status before side effects
    if let Some(id) = &server_id {
        {
            let settings = remote_settings.lock().await;
            ensure_remote_selection_is_allowed(&settings, id)?;
        }

        let refreshed_server = refresh_saved_connection_status(&app, id).await?;
        if matches!(refreshed_server.status, ConnectionStatus::SelfConnection) {
            return Err("Cannot use this Voicetypr instance as its own remote server".to_string());
        }

        log::info!(
            "⏱️ [TIMING] Acquiring server_manager lock... (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
        let manager = server_manager.lock().await;
        log::info!(
            "⏱️ [TIMING] server_manager lock acquired (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
        if manager.get_status().enabled {
            // Get current sharing settings before stopping
            let status = manager.get_status();
            restore_port = status.port;
            restore_password = secure_get_optional(&app, SHARING_PASSWORD_KEY)?;

            drop(manager); // Release lock before calling stop
            log::info!(
                "⏱️ [TIMING] Stopping network sharing... (+{}ms)",
                cmd_start.elapsed().as_millis()
            );
            let mut manager = server_manager.lock().await;
            manager.stop().await;
            log::info!(
                "⏱️ [TIMING] Network sharing stopped (+{}ms)",
                cmd_start.elapsed().as_millis()
            );

            // Set flag in remote settings to remember sharing was active
            log::info!(
                "⏱️ [TIMING] Acquiring remote_settings lock... (+{}ms)",
                cmd_start.elapsed().as_millis()
            );
            let mut settings = remote_settings.lock().await;
            log::info!(
                "⏱️ [TIMING] remote_settings lock acquired (+{}ms)",
                cmd_start.elapsed().as_millis()
            );
            settings.sharing_was_active = true;
            save_remote_settings(&app, &settings)?;
            log::info!(
                "⏱️ [TIMING] Settings saved (+{}ms)",
                cmd_start.elapsed().as_millis()
            );

            // Emit event to notify frontend of sharing status change
            let _ = app.emit(
                "sharing-status-changed",
                serde_json::json!({
                    "enabled": false,
                    "port": null,
                    "model_name": null
                }),
            );
            log::info!("🔔 [SHARING] Emitted sharing-status-changed event (auto-disabled for remote server)");
        }
    } else {
        // Clearing remote server - check if we should restore sharing
        log::info!(
            "⏱️ [TIMING] Acquiring remote_settings lock (clearing)... (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
        let settings = remote_settings.lock().await;
        log::info!(
            "⏱️ [TIMING] remote_settings lock acquired (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
        if settings.sharing_was_active {
            should_restore_sharing = true;
            restore_port = Some(settings.server_config.port);
            restore_password = settings.server_config.password.clone();
            log::info!(
                "⏱️ [TIMING] Will restore sharing (sharing_was_active=true) (+{}ms)",
                cmd_start.elapsed().as_millis()
            );
        }
    }

    // Update settings (scoped to release lock before tray update)
    {
        log::info!(
            "⏱️ [TIMING] Acquiring remote_settings lock (update)... (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
        let mut settings = remote_settings.lock().await;
        log::info!(
            "⏱️ [TIMING] remote_settings lock acquired (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
        log::info!(
            "🔧 [REMOTE] Before change: active_connection_id={:?}",
            settings.active_connection_id
        );

        if let Some(id) = &server_id {
            settings.set_active_connection(Some(id.clone()))?;
            log::info!("🔧 [REMOTE] Active remote server set to: {}", id);
        } else {
            settings.set_active_connection(None)?;
            log::info!("🔧 [REMOTE] Active remote server cleared");
        }

        log::info!(
            "🔧 [REMOTE] After change: active_connection_id={:?}",
            settings.active_connection_id
        );

        // Save settings
        log::info!(
            "⏱️ [TIMING] Saving remote settings... (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
        save_remote_settings(&app, &settings)?;
        log::info!(
            "⏱️ [TIMING] Remote settings saved (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
    }

    let availability = crate::recognition::emit_recognition_availability(&app).await;
    if let Err(error) = crate::recognition::auto_select_model_if_needed(&app, &availability).await {
        log::warn!(
            "Failed to reconcile onboarding/model selection after active remote change: {}",
            error
        );
    }

    // Restore sharing if we were using it before switching to remote
    if should_restore_sharing {
        let port = restore_port.unwrap_or(DEFAULT_PORT);
        log::info!(
            "⏱️ [TIMING] Auto-restoring network sharing on port {} (+{}ms)",
            port,
            cmd_start.elapsed().as_millis()
        );

        // Get current model and engine from store
        log::info!(
            "⏱️ [TIMING] Reading model/engine from store... (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
        let restore_config = app.store("settings").ok().and_then(|store| {
            let model = store
                .get("current_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))?;
            let engine = store
                .get("current_model_engine")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "whisper".to_string());
            Some((model, engine))
        });

        let Some((current_model, current_engine)) = restore_config else {
            log::warn!("⏱️ [TIMING] Skipping sharing restore: no valid local model stored");
            return Ok(());
        };

        // Normalize empty model to first available (matching start_sharing behavior)
        let Some(current_model) = (if current_model.is_empty() {
            let whisper_manager = app
                .state::<tauri::async_runtime::RwLock<crate::whisper::manager::WhisperManager>>();
            let manager = whisper_manager.read().await;
            manager.get_first_downloaded_model()
        } else {
            Some(current_model)
        }) else {
            log::warn!("⏱️ [TIMING] Skipping sharing restore: no downloaded models available");
            let manager = server_manager.lock().await;
            let status = manager.get_status();
            let _ = app.emit(
                "sharing-status-changed",
                serde_json::json!({
                    "enabled": status.enabled,
                    "port": status.port,
                    "model_name": status.model_name
                }),
            );
            let _ = crate::commands::settings::update_tray_menu(app.clone()).await;
            return Ok(());
        };

        log::info!(
            "⏱️ [TIMING] Got model={}, engine={} (+{}ms)",
            current_model,
            current_engine,
            cmd_start.elapsed().as_millis()
        );
        let server_name = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "Voicetypr Server".to_string());

        log::info!(
            "⏱️ [TIMING] Getting model path... (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
        let (model_path, current_engine) =
            match resolve_shareable_model_config(&app, &current_model, &current_engine).await {
                Ok(config) => config,
                Err(e) => {
                    log::warn!("⏱️ [TIMING] Skipping sharing restore: {}", e);
                    let manager = server_manager.lock().await;
                    let status = manager.get_status();
                    let _ = app.emit(
                        "sharing-status-changed",
                        serde_json::json!({
                            "enabled": status.enabled,
                            "port": status.port,
                            "model_name": status.model_name
                        }),
                    );
                    let _ = crate::commands::settings::update_tray_menu(app.clone()).await;
                    return Ok(());
                }
            };
        log::info!(
            "⏱️ [TIMING] Got model path (+{}ms)",
            cmd_start.elapsed().as_millis()
        );

        log::info!(
            "⏱️ [TIMING] Acquiring server_manager lock (restore)... (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
        let mut manager = server_manager.lock().await;
        log::info!(
            "⏱️ [TIMING] server_manager lock acquired (+{}ms)",
            cmd_start.elapsed().as_millis()
        );

        log::info!(
            "⏱️ [TIMING] Starting server... (+{}ms)",
            cmd_start.elapsed().as_millis()
        );
        if let Err(e) = manager
            .start(
                port,
                restore_password,
                server_name,
                model_path,
                current_model,
                current_engine,
                Some(app.clone()),
            )
            .await
        {
            log::warn!(
                "⏱️ [TIMING] Server start FAILED after {}ms: {}",
                cmd_start.elapsed().as_millis(),
                e
            );
        } else {
            log::info!(
                "⏱️ [TIMING] Server started successfully (+{}ms)",
                cmd_start.elapsed().as_millis()
            );

            // NOW clear the sharing_was_active flag
            {
                let mut settings = remote_settings.lock().await;
                settings.sharing_was_active = false;
                save_remote_settings(&app, &settings)?;
            }

            // Emit event to notify frontend of sharing status change
            let status = manager.get_status();
            let _ = app.emit(
                "sharing-status-changed",
                serde_json::json!({
                    "enabled": status.enabled,
                    "port": status.port,
                    "model_name": status.model_name
                }),
            );
            log::info!("🔔 [SHARING] Emitted sharing-status-changed event (auto-restored)");
        }
    }

    if !should_restore_sharing {
        let manager = server_manager.lock().await;
        let status = manager.get_status();
        let _ = app.emit(
            "sharing-status-changed",
            serde_json::json!({
                "enabled": status.enabled,
                "port": status.port,
                "model_name": status.model_name
            }),
        );
    }

    // Update tray menu in background - don't block the command
    // This is important because tray menu build checks remote server status which can timeout
    // Use generation counter to prevent stale updates from overwriting newer ones
    let my_generation = crate::commands::settings::next_tray_menu_generation();
    log::info!(
        "⏱️ [TIMING] Spawning tray menu update in background (gen={})... (+{}ms)",
        my_generation,
        cmd_start.elapsed().as_millis()
    );
    let app_for_tray = app.clone();
    tokio::spawn(async move {
        log::info!(
            "⏱️ [TIMING] Background tray menu update starting (gen={})...",
            my_generation
        );
        let tray_start = std::time::Instant::now();
        if let Err(e) = crate::commands::settings::update_tray_menu_with_generation(
            app_for_tray,
            Some(my_generation),
        )
        .await
        {
            log::warn!(
                "⏱️ [TIMING] Background tray menu update FAILED (gen={}) after {}ms: {}",
                my_generation,
                tray_start.elapsed().as_millis(),
                e
            );
        } else {
            log::info!(
                "⏱️ [TIMING] Background tray menu update completed (gen={}) in {}ms",
                my_generation,
                tray_start.elapsed().as_millis()
            );
        }
    });

    log::info!(
        "⏱️ [TIMING] set_active_remote_server COMPLETE - total: {}ms",
        cmd_start.elapsed().as_millis()
    );
    Ok(())
}

/// Get the currently active remote server ID
#[tauri::command]
pub async fn get_active_remote_server(
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<Option<String>, String> {
    let settings = remote_settings.lock().await;
    let active_id = settings.active_connection_id.clone();
    log::info!(
        "🔍 [REMOTE] get_active_remote_server returning: {:?}",
        active_id
    );
    Ok(active_id)
}

pub(crate) async fn resolve_remote_request_context(
    app: &AppHandle,
    target_server_id: &str,
    transcript_language: Option<&str>,
) -> Option<String> {
    let connection = refresh_saved_connection_status(app, target_server_id)
        .await
        .ok()?;

    if !matches!(connection.status, ConnectionStatus::Online) {
        return None;
    }

    let capabilities = connection.capabilities.as_ref()?;
    if capabilities.accepts_request_context
        && capabilities.supports_initial_prompt
        && capabilities.max_context_bytes > 0
    {
        crate::commands::audio::compile_remote_request_context(app, transcript_language)
    } else {
        None
    }
}

// ============================================================================
// Transcription Commands
// ============================================================================

/// Transcribe audio using a remote server
#[tauri::command]
pub async fn transcribe_remote(
    app: AppHandle,
    server_id: String,
    audio_path: String,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<String, String> {
    // Get the connection info
    let connection = {
        let settings = remote_settings.lock().await;
        settings
            .get_connection(&server_id)
            .cloned()
            .ok_or_else(|| format!("Server '{}' not found", server_id))?
    };

    let display_name = connection.display_name();
    log::info!(
        "[Remote Client] Starting remote transcription to '{}' ({}:{})",
        display_name,
        connection.host,
        connection.port
    );

    // Security: constrain audio_path to the app's recordings directory so a
    // compromised renderer cannot read and exfiltrate arbitrary files
    // (e.g. ~/.ssh/id_rsa) via the remote transcription upload.
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))?
        .join("recordings");
    let canonical_audio = std::fs::canonicalize(&audio_path)
        .map_err(|e| format!("Failed to resolve audio file path: {}", e))?;
    let canonical_dir = std::fs::canonicalize(&recordings_dir)
        .map_err(|e| format!("Failed to resolve recordings directory: {}", e))?;
    if !canonical_audio.starts_with(&canonical_dir) {
        return Err(format!(
            "Audio path is outside the recordings directory: {}",
            audio_path
        ));
    }
    let audio_path = canonical_audio
        .to_str()
        .ok_or_else(|| "Audio file path is not valid UTF-8".to_string())?
        .to_string();

    // Read the audio file
    let audio_data =
        std::fs::read(&audio_path).map_err(|e| format!("Failed to read audio file: {}", e))?;

    let audio_size_kb = audio_data.len() as f64 / 1024.0;
    log::info!(
        "[Remote Client] Sending {:.1} KB audio to '{}'",
        audio_size_kb,
        display_name
    );

    let timeout_ms = timeout_ms_for_wav_file(&audio_path, TranscriptionSource::Upload);

    let store = app.store("settings").map_err(|e| e.to_string())?;
    let legacy_speech_language = store
        .get("language")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let legacy_translate_to_english = store
        .get("translate_to_english")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let spoken_language = store
        .get("speech_language")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or(legacy_speech_language);
    let stored_transcription_task = store
        .get("transcription_task")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    drop(store);

    let uses_personal_dictation = crate::writing::effective_personal_dictation_mode(&app)?;
    let transcription_task = if uses_personal_dictation {
        crate::commands::settings::TRANSCRIPTION_TASK_TRANSCRIBE.to_string()
    } else {
        crate::commands::settings::normalize_transcription_task(
            stored_transcription_task.as_deref(),
            legacy_translate_to_english,
        )
    };

    // Create HTTP client connection
    let server_conn =
        RemoteServerConnection::new(connection.host, connection.port, connection.password);

    let translate_to_english =
        transcription_task == crate::commands::settings::TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH;
    let request_context =
        resolve_remote_request_context(&app, &server_id, spoken_language.as_deref()).await;
    let request = TranscriptionRequest::new(audio_data, TranscriptionSource::Upload)
        .with_language_and_task(spoken_language.clone(), Some(transcription_task))
        .with_context(request_context);
    let response = client::transcribe_audio(&server_conn, request, timeout_ms)
        .await
        .map_err(|e| e.to_string())?;

    log::info!(
        "[Remote Client] Transcription COMPLETED from '{}': {} chars received",
        display_name,
        response.text.len()
    );

    let job = crate::transcription::TranscriptionJob::from_legacy_settings(
        crate::transcription::TranscriptionSource::RemoteServer,
        "remote",
        response.model.clone(),
        spoken_language,
        translate_to_english,
    );
    let transcription = crate::transcription::TranscriptionResult::new(&job, response.text)
        .with_transcript_language(response.transcript_language)
        .with_processing_duration_ms(Some(response.duration_ms));
    let writing_result = crate::writing::process_transcription(app.clone(), transcription)
        .await
        .map_err(|error| error.user_message())?;
    Ok(writing_result.final_text)
}

/// Test connection to a remote server
/// Uses the Intel-Mac-safe blocking helper in `remote::client`
async fn test_connection(
    host: &str,
    port: u16,
    password: Option<&str>,
) -> Result<StatusResponse, RemoteClientError> {
    let conn = RemoteServerConnection::new(host.to_string(), port, password.map(String::from));

    log::info!("[Remote Client] Testing connection to {}:{}", host, port);

    client::test_connection(&conn).await
}

pub(crate) fn connection_status_for_remote_error(error: &RemoteClientError) -> ConnectionStatus {
    if error.is_auth_failure() {
        ConnectionStatus::AuthFailed
    } else {
        ConnectionStatus::Offline
    }
}

/// Save remote settings to the store
pub fn save_remote_settings(app: &AppHandle, settings: &RemoteSettings) -> Result<(), String> {
    let store = app
        .store("voicetypr-store.json")
        .map_err(|e| format!("Failed to access store: {}", e))?;

    let settings_json = serde_json::to_value(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    store.set("remote_settings", settings_json);
    store
        .save()
        .map_err(|e| format!("Failed to save store: {}", e))?;

    Ok(())
}

/// Load remote settings from the store
pub(crate) fn normalize_loaded_active_remote_status(settings: &mut RemoteSettings) {
    let Some(active_id) = settings.active_connection_id.clone() else {
        return;
    };

    if let Some(connection) = settings
        .saved_connections
        .iter_mut()
        .find(|c| c.id == active_id)
    {
        connection.status = ConnectionStatus::Unknown;
        return;
    }

    settings.active_connection_id = None;
}

pub fn load_remote_settings(app: &AppHandle) -> RemoteSettings {
    log::info!("🔧 [REMOTE] load_remote_settings called");

    let store = match app.store("voicetypr-store.json") {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "🔧 [REMOTE] Failed to open store: {:?}, returning default",
                e
            );
            return RemoteSettings::default();
        }
    };

    let raw_value = store.get("remote_settings");
    log::info!(
        "🔧 [REMOTE] Raw store value exists: {}",
        raw_value.is_some()
    );

    let server_had_password_marker = remote_settings_has_server_password_marker(raw_value.as_ref());
    let connection_password_markers =
        remote_settings_connection_password_markers(raw_value.as_ref());

    let mut settings: RemoteSettings = raw_value
        // Legacy records can still contain plaintext passwords before migration.
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let mut migrated_legacy_secret = false;

    match secure_get_optional(app, SHARING_PASSWORD_KEY) {
        Ok(Some(stored_password)) => {
            settings.server_config.password = Some(stored_password);
        }
        Ok(None)
            if settings
                .server_config
                .password
                .as_ref()
                .is_some_and(|p| !p.is_empty()) =>
        {
            match secure_set_optional(
                app,
                SHARING_PASSWORD_KEY,
                settings.server_config.password.as_deref(),
            ) {
                Ok(_) => migrated_legacy_secret = true,
                Err(error) => {
                    log::warn!(
                        "🔧 [REMOTE] Failed to migrate sharing password to secure store: {}",
                        error
                    );
                }
            }
        }
        Ok(None) if server_had_password_marker => {
            log::warn!(
                "🔧 [REMOTE] Sharing was saved with a password marker, but no secure password was available; disabling auto-start to fail closed"
            );
            settings.server_config.password = None;
            settings.server_config.enabled = false;
            settings.sharing_was_active = false;
            migrated_legacy_secret = true;
        }
        Ok(None) => {}
        Err(error) => {
            log::warn!(
                "🔧 [REMOTE] Failed to read sharing password from secure store: {}",
                error
            );
            if settings.server_config.password.is_none() && server_had_password_marker {
                settings.server_config.enabled = false;
                settings.sharing_was_active = false;
            }
        }
    }

    if settings.server_config.password.is_none() {
        if let Ok(settings_store) = app.store("settings") {
            if let Some(legacy_password) = settings_store
                .get("sharing_password")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
            {
                match secure_set_optional(app, SHARING_PASSWORD_KEY, Some(&legacy_password)) {
                    Ok(migrated) => {
                        settings.server_config.password = migrated;
                        settings_store.delete("sharing_password");
                        let _ = settings_store.save();
                        migrated_legacy_secret = true;
                    }
                    Err(error) => {
                        log::warn!(
                            "🔧 [REMOTE] Failed to migrate legacy sharing password to secure store: {}; disabling auto-start to fail closed",
                            error
                        );
                        settings.server_config.password = None;
                        settings.server_config.enabled = false;
                        settings.sharing_was_active = false;
                        migrated_legacy_secret = true;
                    }
                }
            }
        }
    }

    for connection in &mut settings.saved_connections {
        match get_connection_password(app, &connection.id) {
            Ok(Some(stored_password)) => {
                connection.password = Some(stored_password);
            }
            Ok(None) if connection.password.as_ref().is_some_and(|p| !p.is_empty()) => {
                match set_connection_password(app, &connection.id, connection.password.as_deref()) {
                    Ok(_) => migrated_legacy_secret = true,
                    Err(error) => {
                        log::warn!(
                            "🔧 [REMOTE] Failed to migrate password for saved connection '{}': {}",
                            connection.id,
                            error
                        );
                    }
                }
            }
            Ok(None) if connection_password_markers.contains(&connection.id) => {
                log::warn!(
                    "🔧 [REMOTE] Saved connection '{}' has a password marker, but no secure password was available",
                    connection.id
                );
                connection.password = None;
            }
            Ok(None) => {}
            Err(error) => {
                log::warn!(
                    "🔧 [REMOTE] Failed to read password for saved connection '{}': {}",
                    connection.id,
                    error
                );
            }
        }
    }

    normalize_loaded_active_remote_status(&mut settings);
    if migrated_legacy_secret {
        let _ = save_remote_settings(app, &settings);
    }

    log::info!(
        "🔧 [REMOTE] Loaded settings: {} connections, active_id={:?}",
        settings.saved_connections.len(),
        settings.active_connection_id
    );

    model_control::set_model_control_enabled(settings.server_config.allow_model_control);

    settings
}

/// Get the unique machine ID for this Voicetypr instance
/// Used to prevent adding self as a remote server
#[tauri::command]
pub fn get_local_machine_id() -> Result<String, String> {
    crate::license::device::get_device_hash()
}

// ============================================================================
// Firewall Detection (macOS and Windows)
// ============================================================================

/// Firewall status for network sharing
#[derive(Debug, Clone, serde::Serialize)]
pub struct FirewallStatus {
    /// Whether the system firewall is enabled
    pub firewall_enabled: bool,
    /// Whether Voicetypr is allowed through the firewall
    pub app_allowed: bool,
    /// Whether incoming connections may be blocked
    pub may_be_blocked: bool,
}

/// Check if the system firewall may be blocking incoming connections
/// Returns firewall status to help users troubleshoot connection issues
#[tauri::command]
pub fn get_firewall_status() -> FirewallStatus {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        // Check if firewall is enabled
        let firewall_enabled = Command::new("/usr/libexec/ApplicationFirewall/socketfilterfw")
            .arg("--getglobalstate")
            .output()
            .map(|output| {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.contains("enabled") || stdout.contains("State = 1")
            })
            .unwrap_or(false);

        if !firewall_enabled {
            return FirewallStatus {
                firewall_enabled: false,
                app_allowed: true, // No firewall means no blocking
                may_be_blocked: false,
            };
        }

        // Check if Voicetypr is in the allow list
        // The output format is:
        //   60 : /Applications/Voicetypr.app
        //              (Allow incoming connections)
        // We need to find a voicetypr entry followed by "Allow" on the next line
        let app_allowed = Command::new("/usr/libexec/ApplicationFirewall/socketfilterfw")
            .arg("--listapps")
            .output()
            .map(|output| {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = stdout.lines().collect();

                // Find any line containing voicetypr and check the next line for "Allow"
                for (i, line) in lines.iter().enumerate() {
                    if line.to_lowercase().contains("voicetypr") {
                        // Check if next line contains "Allow incoming connections"
                        if let Some(next_line) = lines.get(i + 1) {
                            if next_line
                                .to_lowercase()
                                .contains("allow incoming connections")
                            {
                                return true;
                            }
                        }
                        // Also check same line in case format varies
                        if line.to_lowercase().contains("allow incoming connections") {
                            return true;
                        }
                    }
                }
                false
            })
            .unwrap_or(false);

        FirewallStatus {
            firewall_enabled: true,
            app_allowed,
            may_be_blocked: !app_allowed,
        }
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, we don't show a proactive firewall warning because:
        // 1. Windows Firewall shows its own popup when an app starts listening on a port
        // 2. We can't reliably detect if traffic is actually blocked without testing
        // 3. Checking for a rule named "Voicetypr" is unreliable - user may have clicked
        //    "Allow" in the Windows popup, which creates a rule with a different name
        //
        // If users have connection issues, they'll troubleshoot from there.
        // Showing a warning when ports aren't actually blocked is confusing.
        FirewallStatus {
            firewall_enabled: false, // Don't claim we know firewall state
            app_allowed: true,
            may_be_blocked: false, // Don't show warning on Windows
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // On other platforms (Linux, etc.), assume no firewall issues
        // Linux firewall detection could be added later (iptables/ufw/firewalld)
        FirewallStatus {
            firewall_enabled: false,
            app_allowed: true,
            may_be_blocked: false,
        }
    }
}

/// Open the system firewall settings
#[tauri::command]
pub fn open_firewall_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        // Try macOS Ventura+ Network > Firewall URL first
        let result = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.Network-Settings.extension?Firewall")
            .spawn();

        if result.is_err() {
            // Fallback to older Security & Privacy > Firewall URL
            let result2 = Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Firewall")
                .spawn();

            if result2.is_err() {
                // Last resort: open System Settings directly
                let _ = Command::new("open")
                    .arg("-a")
                    .arg("System Settings")
                    .spawn();
            }
        }

        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        // Open Windows Firewall settings
        let _ = Command::new("control").arg("firewall.cpl").spawn();
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Firewall settings not supported on this platform".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_port() {
        assert_eq!(DEFAULT_PORT, 47842);
    }

    #[test]
    fn test_apply_server_update_preserves_active_connection() {
        let mut settings = RemoteSettings::default();
        let conn = settings.add_connection(
            "192.168.1.10".to_string(),
            47842,
            None,
            Some("Primary".to_string()),
            Some("whisper-base".to_string()),
        );
        let conn_id = conn.id.clone();

        settings
            .set_active_connection(Some(conn_id.clone()))
            .unwrap();

        let updated = apply_server_update(
            &mut settings,
            &conn_id,
            "192.168.1.11".to_string(),
            47843,
            Some("pw".to_string()),
            Some("Updated".to_string()),
            Some("whisper-large".to_string()),
        )
        .unwrap();

        assert_eq!(updated.id, conn_id);
        assert_eq!(settings.active_connection_id, Some(updated.id.clone()));

        let stored = settings.get_connection(&updated.id).unwrap();
        assert_eq!(stored.host, "192.168.1.11");
        assert_eq!(stored.port, 47843);
        assert_eq!(stored.password, Some("pw".to_string()));
        assert_eq!(stored.model, Some("whisper-large".to_string()));
    }

    #[test]
    fn test_apply_server_update_keeps_other_active_connection() {
        let mut settings = RemoteSettings::default();
        let conn_a = settings.add_connection(
            "192.168.1.20".to_string(),
            47842,
            None,
            Some("A".to_string()),
            None,
        );
        let conn_b = settings.add_connection(
            "192.168.1.21".to_string(),
            47842,
            None,
            Some("B".to_string()),
            None,
        );

        settings
            .set_active_connection(Some(conn_b.id.clone()))
            .unwrap();

        apply_server_update(
            &mut settings,
            &conn_a.id,
            "192.168.1.22".to_string(),
            47844,
            None,
            Some("A2".to_string()),
            None,
        )
        .unwrap();

        assert_eq!(settings.active_connection_id, Some(conn_b.id));
    }

    #[test]
    fn test_start_sharing_is_blocked_when_remote_server_is_active() {
        let mut settings = RemoteSettings::default();
        let conn = settings.add_connection(
            "192.168.1.30".to_string(),
            47842,
            None,
            Some("Remote".to_string()),
            None,
        );

        settings
            .set_active_connection(Some(conn.id))
            .expect("active remote server should be set");

        let result = ensure_sharing_can_start(&settings);

        assert_eq!(
            result,
            Err(
                "Network sharing is unavailable while using a remote Voicetypr instance as your model source."
                    .to_string()
            )
        );
    }

    #[test]
    fn test_start_sharing_is_allowed_without_active_remote_server() {
        let settings = RemoteSettings::default();
        assert!(ensure_sharing_can_start(&settings).is_ok());
    }

    #[test]
    fn test_start_sharing_rejects_cloud_transcription_engines() {
        let expected = Err(
            "Network sharing is not available for cloud transcription. Please select a Whisper or Parakeet model to share."
                .to_string(),
        );

        for engine in [
            "soniox", "openai", "groq", "deepgram", "cohere", "Soniox", " soniox ",
        ] {
            assert_eq!(ensure_sharing_engine_supported(engine), expected.clone());
        }
    }

    #[test]
    fn test_start_sharing_allows_whisper_parakeet_remote_and_unknown_engine() {
        assert!(ensure_sharing_engine_supported("whisper").is_ok());
        assert!(ensure_sharing_engine_supported("parakeet").is_ok());
        assert!(ensure_sharing_engine_supported("remote").is_ok());
        assert!(ensure_sharing_engine_supported("foo").is_ok());
    }

    #[test]
    fn test_remote_selection_rejects_self_connection() {
        let mut settings = RemoteSettings::default();
        let conn = settings.add_connection(
            "192.168.1.30".to_string(),
            47842,
            None,
            Some("Self".to_string()),
            None,
        );
        settings.update_connection_status(&conn.id, ConnectionStatus::SelfConnection, None, None);

        let result = ensure_remote_selection_is_allowed(&settings, &conn.id);

        assert_eq!(
            result,
            Err("Cannot use this Voicetypr instance as its own remote server".to_string())
        );
    }

    #[test]
    fn test_remote_selection_allows_normal_remote_server() {
        let mut settings = RemoteSettings::default();
        let conn = settings.add_connection(
            "192.168.1.31".to_string(),
            47842,
            None,
            Some("Remote".to_string()),
            None,
        );
        settings.update_connection_status(&conn.id, ConnectionStatus::Online, None, None);

        assert!(ensure_remote_selection_is_allowed(&settings, &conn.id).is_ok());
    }

    #[test]
    fn test_is_self_connection_matches_machine_ids() {
        assert!(is_self_connection(Some("machine-a"), "machine-a"));
        assert!(!is_self_connection(Some("machine-a"), "machine-b"));
        assert!(!is_self_connection(None, "machine-a"));
    }

    #[test]
    fn test_remote_settings_password_marker_helpers_detect_serialized_markers() {
        let raw = serde_json::json!({
            "server_config": {
                "enabled": true,
                "port": 47842,
                "has_password": true
            },
            "saved_connections": [
                {
                    "id": "with-password",
                    "host": "192.168.1.2",
                    "port": 47842,
                    "has_password": true
                },
                {
                    "id": "without-password",
                    "host": "192.168.1.3",
                    "port": 47842,
                    "has_password": false
                }
            ]
        });

        assert!(remote_settings_has_server_password_marker(Some(&raw)));

        let markers = remote_settings_connection_password_markers(Some(&raw));
        assert!(markers.contains("with-password"));
        assert!(!markers.contains("without-password"));
    }
}
