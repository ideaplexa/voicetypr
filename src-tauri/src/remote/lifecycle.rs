//! Server lifecycle management for remote transcription
//!
//! Handles starting and stopping the HTTP server when sharing is enabled/disabled.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tokio::sync::{oneshot, RwLock, Semaphore};

use super::discovery::{
    start_discovery_responder, DiscoveryResponderConfig, DiscoveryResponderHandle,
};
use super::http::{count_recent_clients, create_routes, ClientActivityMap, RECENT_CLIENT_WINDOW};
use super::transcription::{
    RealTranscriptionContext, SharedServerState, TranscriptionServerConfig,
};

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Result of attempting to bind to an IP address
#[derive(Debug, Clone, serde::Serialize)]
pub struct BindingResult {
    /// The IP address we attempted to bind to
    pub ip: String,
    /// Whether the binding was successful
    pub success: bool,
    /// Error message if binding failed
    pub error: Option<String>,
}

/// Handle to a running server, used to stop it
pub struct ServerHandle {
    /// Channels to signal server shutdown (one per bound IP)
    shutdown_txs: Vec<oneshot::Sender<()>>,
    /// Handles to spawned server tasks for awaiting completion
    task_handles: Vec<tokio::task::JoinHandle<()>>,
    /// The port the server is listening on
    pub port: u16,
    /// Results of binding attempts (for UI display)
    pub binding_results: Vec<BindingResult>,
    /// UDP discovery responder advertised while the server is running.
    discovery_handle: Option<DiscoveryResponderHandle>,
}

/// Generous upper bound while waiting for in-flight HTTP work during disable/restart.
const GRACEFUL_SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(60);

// The ONNX loader needs to evict the remote Whisper model even while the
// manager is awaiting a draining request. Never hold this registry lock while
// waiting for the cache or doing model work.
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
static REMOTE_WHISPER_CACHE: Mutex<Option<Arc<Mutex<crate::whisper::cache::TranscriberCache>>>> =
    Mutex::new(None);

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub(crate) fn remote_whisper_cache_handle(
) -> Option<Arc<Mutex<crate::whisper::cache::TranscriberCache>>> {
    let handle = REMOTE_WHISPER_CACHE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    handle
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn set_remote_whisper_cache_handle(
    handle: Option<Arc<Mutex<crate::whisper::cache::TranscriberCache>>>,
) {
    let previous = std::mem::replace(
        &mut *REMOTE_WHISPER_CACHE
            .lock()
            .unwrap_or_else(|error| error.into_inner()),
        handle,
    );
    drop(previous);
}

impl ServerHandle {
    /// Stop the server gracefully
    pub async fn stop(&mut self) {
        if let Some(handle) = self.discovery_handle.take() {
            handle.stop().await;
        }
        for tx in self.shutdown_txs.drain(..) {
            let _ = tx.send(());
        }
        log::info!(
            "[Remote Server] Shutdown signal sent for port {}",
            self.port
        );

        let mut still_draining = false;
        for handle in self.task_handles.drain(..) {
            match tokio::time::timeout(GRACEFUL_SHUTDOWN_DRAIN_TIMEOUT, handle).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => log::warn!("[Remote Server] Server task panicked: {}", e),
                Err(_) => {
                    still_draining = true;
                    log::warn!(
                        "[Remote Server] Graceful shutdown still draining after {}s on port {}; proceeding",
                        GRACEFUL_SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
                        self.port
                    );
                }
            }
        }
        if still_draining {
            log::info!(
                "[Remote Server] Shutdown proceeded while work may still be in flight on port {}",
                self.port
            );
        }
        log::info!(
            "[Remote Server] All server tasks stopped for port {}",
            self.port
        );
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.discovery_handle.take() {
            drop(handle);
        }

        for tx in self.shutdown_txs.drain(..) {
            let _ = tx.send(());
        }
        for handle in self.task_handles.drain(..) {
            handle.abort();
        }
    }
}

/// Server lifecycle manager
pub struct RemoteServerManager {
    /// Handle to the currently running server (if any)
    handle: Option<ServerHandle>,
    /// Server configuration
    config: Option<TranscriptionServerConfig>,
    /// Shared state for dynamic model updates (only valid while server is running)
    shared_state: Option<SharedServerState>,
    /// Tracks distinct client IPs for recent-connection counting (cleared on server stop)
    client_activity: Option<ClientActivityMap>,
}

impl Default for RemoteServerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteServerManager {
    /// Create a new server manager
    pub fn new() -> Self {
        Self {
            handle: None,
            config: None,
            shared_state: None,
            client_activity: None,
        }
    }

    /// Check if the server is currently running
    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }

    /// Get the port the server is listening on (if running)
    #[cfg(test)]
    pub fn get_port(&self) -> Option<u16> {
        self.handle.as_ref().map(|h| h.port)
    }

    /// Start the remote transcription server
    ///
    /// # Arguments
    /// * `port` - Port to listen on
    /// * `password` - Optional password for authentication
    /// * `server_name` - Display name for this server
    /// * `model_path` - Path to the currently selected model
    /// * `model_name` - Name of the current model
    /// * `engine` - Transcription engine (whisper, parakeet, etc.)
    /// * `app_handle` - Optional AppHandle for Parakeet support
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        &mut self,
        port: u16,
        password: Option<String>,
        server_name: String,
        model_path: PathBuf,
        model_name: String,
        engine: String,
        app_handle: Option<AppHandle>,
    ) -> Result<(), String> {
        let start_time = Instant::now();
        log::info!("⏱️ [SERVER TIMING] start() called");

        // Stop existing server if running
        if self.handle.is_some() {
            log::info!(
                "⏱️ [SERVER TIMING] Stopping existing server... (+{}ms)",
                start_time.elapsed().as_millis()
            );
            self.stop().await;
            log::info!(
                "⏱️ [SERVER TIMING] Existing server stopped (+{}ms)",
                start_time.elapsed().as_millis()
            );
        }

        let config = TranscriptionServerConfig {
            server_name: server_name.clone(),
            password: password.clone(),
            model_path: model_path.clone(),
            model_name: model_name.clone(),
        };

        self.config = Some(config.clone());

        // Create shared state for dynamic model updates
        let shared_state = SharedServerState::new(model_name, model_path, engine.clone());
        self.shared_state = Some(shared_state.clone());

        // Create the transcription context with shared state and app handle
        // App handle is needed for Parakeet engine support
        let context = RealTranscriptionContext::new_with_shared_state(
            server_name.clone(),
            password,
            shared_state.clone(),
            app_handle.clone(),
        );
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        let whisper_cache = context.whisper_cache_handle();
        let ctx = Arc::new(RwLock::new(context));
        log::info!(
            "⏱️ [SERVER TIMING] Context created (+{}ms)",
            start_time.elapsed().as_millis()
        );

        // Get all local IPs to bind to
        // On Intel Macs, binding to 0.0.0.0 doesn't work properly for non-localhost connections,
        // so we bind to each specific IP address instead
        let mut bind_ips: Vec<IpAddr> = Vec::new();

        // Always include localhost
        bind_ips.push(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

        // Add all network interface IPs (only IPv4 for now - IPv6 link-local addresses cause binding issues)
        log::info!(
            "⏱️ [SERVER TIMING] Listing network interfaces... (+{}ms)",
            start_time.elapsed().as_millis()
        );
        if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
            for (name, ip) in interfaces {
                // Skip loopback and IPv6 addresses (IPv6 link-local addresses like fe80:: can't be bound without scope ID)
                if !ip.is_loopback() && ip.is_ipv4() {
                    log::info!("[Remote Server] Found interface {}: {}", name, ip);
                    bind_ips.push(ip);
                }
            }
        }
        log::info!(
            "⏱️ [SERVER TIMING] Found {} IPs to bind (+{}ms)",
            bind_ips.len(),
            start_time.elapsed().as_millis()
        );

        log::info!(
            "[Remote Server] Starting server on {} IPs as '{}': {:?}",
            bind_ips.len(),
            server_name,
            bind_ips
        );

        let mut shutdown_txs = Vec::new();
        let mut task_handles = Vec::new();
        let mut bound_ips = Vec::new();
        let mut binding_results = Vec::new();
        let transcription_guard = Arc::new(Semaphore::new(1));
        let client_activity: ClientActivityMap = Arc::new(Mutex::new(HashMap::new()));
        self.client_activity = Some(client_activity.clone());

        for ip in bind_ips {
            let bind_start = Instant::now();
            let addr: SocketAddr = SocketAddr::new(ip, port);
            let ip_str = ip.to_string();

            // Clone routes for each server instance; all bound IPs share one transcription guard
            // and one client-activity map.
            let routes = create_routes(
                ctx.clone(),
                transcription_guard.clone(),
                client_activity.clone(),
            );

            // Create shutdown channel for this instance
            let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

            let shutdown_ip = ip_str.clone();
            let server_ip = ip_str.clone();

            // Try to bind to this address using try_bind
            log::info!(
                "⏱️ [SERVER TIMING] Binding to {}... (+{}ms)",
                addr,
                start_time.elapsed().as_millis()
            );
            match warp::serve(routes).try_bind_with_graceful_shutdown(addr, async move {
                shutdown_rx.await.ok();
                log::info!(
                    "[Remote Server] Received shutdown signal for {}; draining in-flight requests",
                    shutdown_ip
                );
            }) {
                Ok((bound_addr, server_future)) => {
                    shutdown_txs.push(shutdown_tx);

                    let handle = tokio::spawn(async move {
                        server_future.await;
                        log::info!("[Remote Server] Server task completed for {}", server_ip);
                    });
                    task_handles.push(handle);

                    bound_ips.push(ip);
                    binding_results.push(BindingResult {
                        ip: ip_str.clone(),
                        success: true,
                        error: None,
                    });
                    log::info!(
                        "⏱️ [SERVER TIMING] Bound to {} in {}ms (+{}ms total)",
                        bound_addr,
                        bind_start.elapsed().as_millis(),
                        start_time.elapsed().as_millis()
                    );
                }
                Err(e) => {
                    // Log the error but continue with other IPs
                    let error_msg = format!("{}", e);
                    log::warn!(
                        "⏱️ [SERVER TIMING] Failed to bind to {} in {}ms: {}",
                        addr,
                        bind_start.elapsed().as_millis(),
                        error_msg
                    );
                    binding_results.push(BindingResult {
                        ip: ip_str,
                        success: false,
                        error: Some(error_msg),
                    });
                }
            }
        }

        // Check if we successfully bound to at least one address
        if bound_ips.is_empty() {
            return Err("Failed to bind to any IP address".to_string());
        }

        let discovery_handle = match app_handle.as_ref() {
            Some(_) => {
                let machine_id = crate::license::device::get_device_hash()
                    .unwrap_or_else(|_| "unknown".to_string());
                match start_discovery_responder(DiscoveryResponderConfig {
                    server_name: server_name.clone(),
                    model_name: {
                        let state = shared_state.clone();
                        Arc::new(move || state.get_model_name())
                    },
                    port,
                    auth_required: self
                        .config
                        .as_ref()
                        .is_some_and(|config| config.password.is_some()),
                    machine_id,
                })
                .await
                {
                    Ok(handle) => Some(handle),
                    Err(error) => {
                        log::warn!("[Remote Discovery] disabled: {}", error);
                        None
                    }
                }
            }
            None => None,
        };

        self.handle = Some(ServerHandle {
            shutdown_txs,
            task_handles,
            port,
            binding_results,
            discovery_handle,
        });
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            set_remote_whisper_cache_handle(Some(whisper_cache));
        }

        log::info!(
            "⏱️ [SERVER TIMING] Server STARTED - total: {}ms (port={}, model='{}')",
            start_time.elapsed().as_millis(),
            port,
            self.config
                .as_ref()
                .map(|c| c.model_name.as_str())
                .unwrap_or("unknown")
        );

        Ok(())
    }

    /// Stop the remote transcription server
    pub async fn stop(&mut self) {
        if let Some(mut handle) = self.handle.take() {
            let port = handle.port;
            handle.stop().await;
            log::info!("[Remote Server] Server STOPPED (was on port {})", port);
        }
        self.config = None;
        self.shared_state = None;
        self.client_activity = None;
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            set_remote_whisper_cache_handle(None);
        }
    }

    /// Update the model being served (without restarting server)
    ///
    /// This updates the shared state that the running server reads from,
    /// so the change takes effect immediately for new requests.
    pub fn update_model(&mut self, model_path: PathBuf, model_name: String, engine: String) {
        // Update config for tracking
        if let Some(config) = &mut self.config {
            config.model_path = model_path.clone();
            config.model_name = model_name.clone();
        }

        // Update shared state - this is what the running server actually reads
        if let Some(shared_state) = &self.shared_state {
            shared_state.update_model(model_name.clone(), model_path, engine.clone());
            log::info!(
                "[Remote Server] Model dynamically updated to '{}' (engine: {})",
                model_name,
                engine
            );
        }
    }

    /// Get the current server configuration
    #[cfg(test)]
    pub fn get_config(&self) -> Option<&TranscriptionServerConfig> {
        self.config.as_ref()
    }
}

/// Information about the sharing status
#[derive(Debug, Clone, serde::Serialize)]
pub struct SharingStatus {
    /// Whether sharing is currently enabled
    pub enabled: bool,
    /// Port the server is listening on (if enabled)
    pub port: Option<u16>,
    /// Name of the model being shared (if enabled)
    pub model_name: Option<String>,
    /// Server display name (if enabled)
    pub server_name: Option<String>,
    /// Number of active connections (placeholder for future)
    pub active_connections: u32,
    /// Whether authentication is required.
    pub password_configured: bool,
    /// Results of IP binding attempts (shows which addresses are active)
    pub binding_results: Vec<BindingResult>,
}

impl RemoteServerManager {
    /// Get the current sharing status
    pub fn get_status(&self) -> SharingStatus {
        if let Some(handle) = &self.handle {
            let config = self.config.as_ref();
            let active_connections = self
                .client_activity
                .as_ref()
                .and_then(|map| map.lock().ok())
                .map(|mut map| count_recent_clients(&mut map, Instant::now(), RECENT_CLIENT_WINDOW))
                .unwrap_or(0);
            SharingStatus {
                enabled: true,
                port: Some(handle.port),
                model_name: config.map(|c| c.model_name.clone()),
                server_name: config.map(|c| c.server_name.clone()),
                active_connections,
                password_configured: config.is_some_and(|c| c.password.is_some()),
                binding_results: handle.binding_results.clone(),
            }
        } else {
            SharingStatus {
                enabled: false,
                port: None,
                model_name: None,
                server_name: None,
                active_connections: 0,
                password_configured: false,
                binding_results: Vec::new(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::http::prune_stale_clients;
    use super::*;

    #[test]
    fn test_server_manager_new() {
        let manager = RemoteServerManager::new();
        assert!(!manager.is_running());
        assert!(manager.get_port().is_none());
    }

    #[test]
    fn test_sharing_status_disabled() {
        let manager = RemoteServerManager::new();
        let status = manager.get_status();

        assert!(!status.enabled);
        assert!(status.port.is_none());
        assert!(status.model_name.is_none());
        assert!(status.server_name.is_none());
        assert_eq!(status.active_connections, 0);
    }

    #[tokio::test]
    async fn test_server_start_stop() {
        let mut manager = RemoteServerManager::new();

        // Start server (no app handle needed for whisper-only test)
        let result = manager
            .start(
                47843, // Use non-default port for test
                None,
                "Test Server".to_string(),
                PathBuf::from("/fake/model.bin"),
                "test-model".to_string(),
                "whisper".to_string(),
                None,
            )
            .await;

        assert!(result.is_ok());
        assert!(manager.is_running());
        assert_eq!(manager.get_port(), Some(47843));

        let status = manager.get_status();
        assert!(status.enabled);
        assert_eq!(status.port, Some(47843));
        assert_eq!(status.model_name, Some("test-model".to_string()));
        assert_eq!(status.server_name, Some("Test Server".to_string()));

        // Stop server
        manager.stop().await;
        assert!(!manager.is_running());
        assert!(manager.get_port().is_none());

        let status = manager.get_status();
        assert!(!status.enabled);
    }

    #[tokio::test]
    async fn test_server_restart() {
        let mut manager = RemoteServerManager::new();

        // Start first server
        manager
            .start(
                47844,
                None,
                "Server 1".to_string(),
                PathBuf::from("/model1.bin"),
                "model1".to_string(),
                "whisper".to_string(),
                None,
            )
            .await
            .unwrap();

        assert_eq!(manager.get_status().model_name, Some("model1".to_string()));

        // Start second server (should stop first)
        manager
            .start(
                47845,
                Some("password".to_string()),
                "Server 2".to_string(),
                PathBuf::from("/model2.bin"),
                "model2".to_string(),
                "whisper".to_string(),
                None,
            )
            .await
            .unwrap();

        assert_eq!(manager.get_port(), Some(47845));
        assert_eq!(manager.get_status().model_name, Some("model2".to_string()));
        assert_eq!(
            manager.get_status().server_name,
            Some("Server 2".to_string())
        );

        manager.stop().await;
    }

    // ── count_recent_clients unit tests ───────────────────────────────────────

    #[test]
    fn test_count_recent_clients_empty() {
        let mut map: HashMap<IpAddr, Instant> = HashMap::new();
        let now = Instant::now();
        assert_eq!(count_recent_clients(&mut map, now, RECENT_CLIENT_WINDOW), 0);
    }

    #[test]
    fn test_count_recent_clients_recent_entries_counted() {
        let mut map: HashMap<IpAddr, Instant> = HashMap::new();
        let now = Instant::now();
        // Two distinct IPs seen just now — both within window
        let ip1: IpAddr = "10.0.0.1".parse().unwrap();
        let ip2: IpAddr = "10.0.0.2".parse().unwrap();
        map.insert(ip1, now);
        map.insert(ip2, now);
        assert_eq!(count_recent_clients(&mut map, now, RECENT_CLIENT_WINDOW), 2);
        // Map still holds both entries (not pruned — they're recent)
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_count_recent_clients_stale_entries_pruned() {
        let mut map: HashMap<IpAddr, Instant> = HashMap::new();
        let window = Duration::from_secs(60);
        let now = Instant::now();
        let stale = now.checked_sub(Duration::from_secs(120)).unwrap();

        let ip_stale: IpAddr = "192.168.1.1".parse().unwrap();
        let ip_recent: IpAddr = "192.168.1.2".parse().unwrap();
        map.insert(ip_stale, stale);
        map.insert(ip_recent, now);

        // Only the recent entry is counted; the stale one is pruned
        assert_eq!(count_recent_clients(&mut map, now, window), 1);
        // The stale IP has been removed in-place
        assert!(!map.contains_key(&ip_stale));
        assert!(map.contains_key(&ip_recent));
    }

    #[test]
    fn test_count_recent_clients_all_stale_returns_zero() {
        let mut map: HashMap<IpAddr, Instant> = HashMap::new();
        let window = Duration::from_secs(30);
        let now = Instant::now();
        let stale = now.checked_sub(Duration::from_secs(60)).unwrap();

        map.insert("1.2.3.4".parse().unwrap(), stale);
        map.insert("5.6.7.8".parse().unwrap(), stale);

        assert_eq!(count_recent_clients(&mut map, now, window), 0);
        assert!(map.is_empty(), "all stale entries must be pruned");
    }

    #[test]
    fn test_count_recent_clients_boundary_exactly_at_window() {
        let mut map: HashMap<IpAddr, Instant> = HashMap::new();
        let window = Duration::from_secs(300);
        let now = Instant::now();
        // Entry seen exactly `window` ago — still within boundary (elapsed == window)
        let boundary = now.checked_sub(window).unwrap_or(now);
        map.insert("10.10.10.10".parse().unwrap(), boundary);
        assert_eq!(count_recent_clients(&mut map, now, window), 1);
    }

    // ── prune_stale_clients unit tests ────────────────────────────────────────

    #[test]
    fn test_prune_stale_clients_removes_stale_keeps_recent() {
        let mut map: HashMap<IpAddr, Instant> = HashMap::new();
        let window = Duration::from_secs(60);
        let now = Instant::now();
        let stale = now.checked_sub(Duration::from_secs(120)).unwrap();

        map.insert("10.0.0.1".parse().unwrap(), stale);
        map.insert("10.0.0.2".parse().unwrap(), now);

        prune_stale_clients(&mut map, now, window);

        assert_eq!(map.len(), 1);
        assert!(map.contains_key(&"10.0.0.2".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn test_prune_on_write_bounds_map_size() {
        // Simulate what the transcribe handler does: prune then insert.
        // Verifies that stale entries are removed before the new IP is added,
        // so the map stays bounded to distinct-recent IPs.
        let window = Duration::from_secs(60);
        let now = Instant::now();
        let stale = now.checked_sub(Duration::from_secs(120)).unwrap();

        let mut map: HashMap<IpAddr, Instant> = HashMap::new();
        // Pre-populate with stale entries from many distinct IPs
        for i in 1u8..=10 {
            map.insert(IpAddr::from([10, 0, 0, i]), stale);
        }
        assert_eq!(map.len(), 10);

        // Handler logic: prune then insert new IP
        prune_stale_clients(&mut map, now, window);
        map.insert("192.168.1.1".parse().unwrap(), now);

        // All 10 stale entries gone; only the new one remains
        assert_eq!(map.len(), 1, "map must be bounded after prune-on-write");
        assert!(map.contains_key(&"192.168.1.1".parse::<IpAddr>().unwrap()));
    }
}
