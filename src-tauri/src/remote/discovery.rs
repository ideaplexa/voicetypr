//! LAN discovery for remote transcription servers.
//!
//! This is deliberately small and dependency-free: clients broadcast a UDP
//! discovery request on the local network, and running Voicetypr servers reply
//! with the HTTP host/port clients should add manually or automatically.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::net::UdpSocket;
use tokio::sync::oneshot;

/// A stable identifier for sharing, distinct from the licence device hash.
pub(crate) fn sharing_machine_id() -> Result<String, String> {
    let device_hash = crate::license::device::get_device_hash()?;
    let mut hasher = Sha256::new();
    hasher.update(b"voicetypr.remote-sharing.machine-id.v1\0");
    hasher.update(device_hash.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

pub const DISCOVERY_PORT: u16 = 47_842;
const DISCOVERY_PROTOCOL: &str = "voicetypr.remote-discovery.v1";
const MAX_PACKET_BYTES: usize = 2048;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DiscoveryRequest {
    protocol: String,
    #[serde(default)]
    machine_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveredRemoteServer {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub model: String,
    pub auth_required: bool,
    pub machine_id: String,
}

#[derive(Clone)]
pub struct DiscoveryResponderConfig {
    pub server_name: String,
    pub model_name: Arc<dyn Fn() -> String + Send + Sync>,
    pub port: u16,
    pub auth_required: bool,
    pub machine_id: String,
}

pub struct DiscoveryResponderHandle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl DiscoveryResponderHandle {
    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        if let Some(task_handle) = self.task_handle.take() {
            match tokio::time::timeout(Duration::from_secs(2), task_handle).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    log::warn!("[Remote Discovery] responder task panicked: {}", error)
                }
                Err(_) => log::warn!("[Remote Discovery] responder did not stop within timeout"),
            }
        }
    }
}

impl Drop for DiscoveryResponderHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
    }
}

pub async fn start_discovery_responder(
    config: DiscoveryResponderConfig,
) -> Result<DiscoveryResponderHandle, String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT))
        .await
        .map_err(|error| format!("Failed to bind discovery responder: {}", error))?;
    socket
        .set_broadcast(true)
        .map_err(|error| format!("Failed to enable discovery broadcast: {}", error))?;

    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task_handle = tokio::spawn(async move {
        let mut buffer = [0u8; MAX_PACKET_BYTES];

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                received = socket.recv_from(&mut buffer) => {
                    let Ok((len, peer)) = received else {
                        continue;
                    };

                    let Ok(request) = serde_json::from_slice::<DiscoveryRequest>(&buffer[..len]) else {
                        continue;
                    };

                    if request.protocol != DISCOVERY_PROTOCOL {
                        continue;
                    }

                    if request.machine_id.as_deref() == Some(config.machine_id.as_str()) {
                        continue;
                    }

                    let model_name = (config.model_name)();

                    let response = DiscoveredRemoteServer {
                        name: if config.auth_required {
                            "Voicetypr Server".to_string()
                        } else {
                            config.server_name.clone()
                        },
                        host: String::new(),
                        port: config.port,
                        model: model_name,
                        auth_required: config.auth_required,
                        machine_id: config.machine_id.clone(),
                    };

                    let Ok(payload) = serde_json::to_vec(&response) else {
                        continue;
                    };

                    if let Err(error) = socket.send_to(&payload, peer).await {
                        log::debug!("[Remote Discovery] failed to reply to {}: {}", peer, error);
                    }
                }
            }
        }
    });

    Ok(DiscoveryResponderHandle {
        shutdown_tx: Some(shutdown_tx),
        task_handle: Some(task_handle),
    })
}

pub async fn discover_remote_servers(
    local_machine_id: Option<&str>,
    timeout: Duration,
) -> Result<Vec<DiscoveredRemoteServer>, String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .await
        .map_err(|error| format!("Failed to bind discovery socket: {}", error))?;
    socket
        .set_broadcast(true)
        .map_err(|error| format!("Failed to enable discovery broadcast: {}", error))?;

    let request = DiscoveryRequest {
        protocol: DISCOVERY_PROTOCOL.to_string(),
        machine_id: local_machine_id.map(str::to_string),
    };
    let payload = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    let broadcast = SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), DISCOVERY_PORT);
    socket
        .send_to(&payload, broadcast)
        .await
        .map_err(|error| format!("Failed to send discovery broadcast: {}", error))?;

    let deadline = Instant::now() + timeout;
    let mut buffer = [0u8; MAX_PACKET_BYTES];
    let mut discovered: HashMap<(String, u16), DiscoveredRemoteServer> = HashMap::new();

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let wait = remaining.min(Duration::from_millis(150));

        let received = tokio::time::timeout(wait, socket.recv_from(&mut buffer)).await;
        let Ok(Ok((len, peer))) = received else {
            continue;
        };

        let Ok(mut server) = serde_json::from_slice::<DiscoveredRemoteServer>(&buffer[..len])
        else {
            continue;
        };

        if local_machine_id == Some(server.machine_id.as_str()) {
            continue;
        }

        server.host = peer.ip().to_string();
        discovered.insert((server.machine_id.clone(), server.port), server);
    }

    let mut servers: Vec<_> = discovered.into_values().collect();
    servers.sort_by(|a, b| a.name.cmp(&b.name).then(a.host.cmp(&b.host)));
    Ok(servers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_response_round_trips_without_password() {
        let response = DiscoveredRemoteServer {
            name: "Studio Mac".to_string(),
            host: "".to_string(),
            port: 47842,
            model: "Parakeet V3".to_string(),
            auth_required: true,
            machine_id: "machine-a".to_string(),
        };

        let encoded = serde_json::to_string(&response).unwrap();
        assert!(!encoded.contains("password"));

        let decoded: DiscoveredRemoteServer = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, response);
    }
}
