//! Soniox realtime STT over WebSocket — plan 043 (C-WS + C-BRIDGE).
//!
//! A synchronous [`SonioxStreamHandle`] (used by the recorder tap on its worker
//! thread) bridged to an async `wss` task via an unbounded mpsc channel, mirroring
//! `parakeet::sidecar::ParakeetStreamHandle`. `send_chunk` NEVER blocks (it only
//! enqueues), so it cannot stall the recorder mutex; the unbounded channel also
//! buffers audio produced before the socket finishes connecting. The task connects,
//! sends the JSON config frame (with the API key in the BODY — never logged),
//! streams raw `pcm_s16le` binary frames, folds token responses into
//! committed/tentative preview text, and on finalize drains late finals until the
//! server reports `finished` or a hard timeout fires (then REST-on-WAV is the
//! FALLBACK path; the WS final is authoritative when it succeeds with non-empty text).

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;

use crate::cloud_stt::common::SttError;
use crate::cloud_stt::soniox_rt::{
    rt_error_to_stt, SonioxRtFolder, SonioxRtPartial, SonioxRtResponse,
};

/// Soniox realtime WebSocket endpoint — a DIFFERENT origin than the REST API
/// (`api.soniox.com`), so warm-up must target this host separately.
const RT_ENDPOINT: &str = "wss://stt-rt.soniox.com/transcribe-websocket";
/// Realtime model, pairs with the async REST `stt-async-v5`.
const RT_MODEL: &str = "stt-rt-v5";
/// Hard cap on draining late finals after an empty-frame finalize; on expiry the
/// caller falls back to REST-on-WAV (the WS final is authoritative on success).
const FINALIZE_TIMEOUT: Duration = Duration::from_secs(3);
/// Soniox drops idle sockets after ~20s; keepalive well under that.
const KEEPALIVE_EVERY: Duration = Duration::from_secs(10);

enum Control {
    Chunk(Vec<i16>),
    Finalize,
    Cancel,
}

/// Connection parameters. `api_key` is written only into the config frame body and
/// is never logged.
pub(crate) struct SonioxStreamConfig {
    pub api_key: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub language_hints: Vec<String>,
    pub context: Option<crate::writing::SonioxContext>,
}

/// Sync handle over the async WS task. Mirrors `ParakeetStreamHandle`.
pub(crate) struct SonioxStreamHandle {
    tx: mpsc::UnboundedSender<Control>,
    final_rx: Mutex<Option<oneshot::Receiver<Result<String, SttError>>>>,
}

impl SonioxStreamHandle {
    /// Enqueue a PCM chunk. Non-blocking: never waits on the socket.
    pub(crate) fn send_chunk(&self, samples: &[i16]) -> Result<(), SttError> {
        self.tx
            .send(Control::Chunk(samples.to_vec()))
            .map_err(|_| SttError::Network)
    }

    /// Request end-of-stream and await the final committed text, bounded by
    /// [`FINALIZE_TIMEOUT`]. On timeout/transport error returns `Err` so the caller
    /// falls back to REST-on-WAV.
    pub(crate) async fn finalize(&self) -> Result<String, SttError> {
        self.tx
            .send(Control::Finalize)
            .map_err(|_| SttError::Network)?;
        let Some(rx) = self.final_rx.lock().await.take() else {
            return Err(SttError::BadResponse);
        };
        match tokio::time::timeout(FINALIZE_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(SttError::Network),
            Err(_) => Err(SttError::Timeout),
        }
    }

    pub(crate) fn cancel(&self) {
        let _ = self.tx.send(Control::Cancel);
    }
}

impl Drop for SonioxStreamHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Control::Cancel);
    }
}

/// Spawn the WS task and return the handle immediately. The connect happens inside
/// the task; chunks produced before it completes buffer in the unbounded channel,
/// so this never blocks the caller (recorder start).
pub(crate) fn open<F>(config: SonioxStreamConfig, on_partial: F) -> SonioxStreamHandle
where
    F: Fn(SonioxRtPartial) + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let (final_tx, final_rx) = oneshot::channel();
    tauri::async_runtime::spawn(run_task(config, rx, final_tx, on_partial));
    SonioxStreamHandle {
        tx,
        final_rx: Mutex::new(Some(final_rx)),
    }
}

/// Build the first-frame JSON config. The API key lives in the body per the Soniox
/// RT protocol; this string is NEVER logged.
fn build_config_frame(config: &SonioxStreamConfig) -> String {
    let mut payload = serde_json::json!({
        "api_key": config.api_key,
        "model": RT_MODEL,
        "audio_format": "pcm_s16le",
        "sample_rate": config.sample_rate,
        "num_channels": config.channels,
        "enable_endpoint_detection": true,
        "enable_language_identification": true,
    });
    if !config.language_hints.is_empty() {
        payload["language_hints"] = serde_json::json!(config.language_hints);
    }
    if let Some(context) = &config.context {
        if let Ok(value) = serde_json::to_value(context) {
            if value.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
                payload["context"] = value;
            }
        }
    }
    payload.to_string()
}

/// Resolve the oneshot exactly once.
fn finish(
    slot: &mut Option<oneshot::Sender<Result<String, SttError>>>,
    result: Result<String, SttError>,
) {
    if let Some(tx) = slot.take() {
        let _ = tx.send(result);
    }
}

async fn run_task<F>(
    config: SonioxStreamConfig,
    mut control_rx: mpsc::UnboundedReceiver<Control>,
    final_tx: oneshot::Sender<Result<String, SttError>>,
    on_partial: F,
) where
    F: Fn(SonioxRtPartial) + Send + 'static,
{
    let mut final_slot = Some(final_tx);

    let stream = match tokio_tungstenite::connect_async(RT_ENDPOINT).await {
        Ok((stream, _response)) => stream,
        Err(error) => {
            log::warn!("Soniox RT connect failed: {error}");
            finish(&mut final_slot, Err(SttError::Network));
            return;
        }
    };
    let (mut write, mut read) = stream.split();

    // Config frame (api_key in body — never logged).
    if write
        .send(Message::text(build_config_frame(&config)))
        .await
        .is_err()
    {
        finish(&mut final_slot, Err(SttError::Network));
        return;
    }

    let mut folder = SonioxRtFolder::new();
    let mut finalizing = false;
    let mut keepalive = tokio::time::interval(KEEPALIVE_EVERY);
    keepalive.tick().await; // drop the immediate first tick

    loop {
        tokio::select! {
            control = control_rx.recv() => match control {
                Some(Control::Chunk(samples)) => {
                    if write
                        .send(Message::binary(crate::cloud_stt::common::samples_to_le_bytes(&samples)))
                        .await
                        .is_err()
                    {
                        finish(&mut final_slot, Err(SttError::Network));
                        return;
                    }
                }
                Some(Control::Finalize) => {
                    finalizing = true;
                    // An empty binary frame tells Soniox end-of-audio; it flushes
                    // pending finals then sends `finished:true`.
                    let _ = write.send(Message::binary(Vec::<u8>::new())).await;
                }
                Some(Control::Cancel) | None => {
                    // Cancelled: the result must never read as a completed transcript
                    // (authority would paste a truncated prefix), so resolve Err even
                    // though no caller normally awaits it after cancel.
                    let _ = write.send(Message::Close(None)).await;
                    finish(&mut final_slot, Err(SttError::Network));
                    return;
                }
            },
            incoming = read.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let response: SonioxRtResponse =
                        serde_json::from_str(text.as_str()).unwrap_or_default();
                    if let Some(code) = response.error_code {
                        log::warn!("Soniox RT in-band error {code}");
                        finish(&mut final_slot, Err(rt_error_to_stt(code)));
                        return;
                    }
                    if !response.tokens.is_empty() {
                        on_partial(folder.ingest(&response));
                    }
                    if response.finished {
                        finish(&mut final_slot, Ok(folder.committed().to_string()));
                        return;
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    // Server closed WITHOUT `finished:true` (that path returns above):
                    // the stream is incomplete — committed may be a truncated prefix.
                    // Err(Network) so REST-on-WAV owns the authoritative result
                    // (Codex 043b finding).
                    log::warn!(
                        "Soniox RT socket closed before `finished`; treating stream as incomplete"
                    );
                    finish(&mut final_slot, Err(SttError::Network));
                    return;
                }
                // Ping/Pong/Binary/Frame from the server: ignore.
                Some(Ok(_)) => {}
                Some(Err(error)) => {
                    log::warn!("Soniox RT read error: {error}");
                    finish(&mut final_slot, Err(SttError::Network));
                    return;
                }
            },
            _ = keepalive.tick(), if !finalizing => {
                let _ = write.send(Message::text("{\"type\":\"keepalive\"}")).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_frame_includes_required_fields_and_hides_nothing_extra() {
        let config = SonioxStreamConfig {
            api_key: "secret-key".to_string(),
            sample_rate: 16_000,
            channels: 1,
            language_hints: vec!["en".to_string()],
            context: None,
        };
        let frame = build_config_frame(&config);
        let parsed: serde_json::Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(parsed["model"], "stt-rt-v5");
        assert_eq!(parsed["audio_format"], "pcm_s16le");
        assert_eq!(parsed["sample_rate"], 16_000);
        assert_eq!(parsed["num_channels"], 1);
        assert_eq!(parsed["api_key"], "secret-key");
        assert_eq!(parsed["enable_endpoint_detection"], true);
        assert_eq!(parsed["language_hints"][0], "en");
        // No context key when none supplied.
        assert!(parsed.get("context").is_none());
    }

    #[test]
    fn empty_language_hints_omits_the_key() {
        let config = SonioxStreamConfig {
            api_key: "k".to_string(),
            sample_rate: 48_000,
            channels: 2,
            language_hints: vec![],
            context: None,
        };
        let parsed: serde_json::Value = serde_json::from_str(&build_config_frame(&config)).unwrap();
        assert!(parsed.get("language_hints").is_none());
        assert_eq!(parsed["num_channels"], 2);
    }

    #[test]
    fn finish_resolves_exactly_once() {
        let (tx, mut rx) = oneshot::channel::<Result<String, SttError>>();
        let mut slot = Some(tx);
        finish(&mut slot, Ok("done".to_string()));
        // Second call is a no-op (slot already taken) — must not panic.
        finish(&mut slot, Ok("again".to_string()));
        assert_eq!(rx.try_recv().unwrap().unwrap(), "done");
    }
}
