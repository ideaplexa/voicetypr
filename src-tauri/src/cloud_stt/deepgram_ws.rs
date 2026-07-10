//! Deepgram realtime STT over WebSocket — plan 044.
//!
//! A synchronous [`DeepgramStreamHandle`] bridged to an async `wss` task via an
//! unbounded mpsc channel, mirroring `soniox_ws.rs`. `send_chunk` NEVER blocks, so
//! it cannot stall the recorder mutex; the unbounded channel also buffers audio
//! produced before the socket finishes connecting. The task connects with a `Token`
//! auth header on the WS upgrade request (NEVER logged, marked sensitive), streams
//! raw `linear16` binary frames, folds Results messages into committed/tentative
//! preview text, and on finalize sends `CloseStream` then drains remaining Results
//! until the server cleanly closes or a hard timeout fires (then REST-on-WAV is the
//! FALLBACK path; the WS final is authoritative when it succeeds).
//!
//! Completion signal: Deepgram has NO `finished:true` flag. A close resolves
//! `Ok(committed)` ONLY when `CloseStream` was successfully sent AND the server's
//! post-CloseStream `Metadata` summary arrived ([`close_is_complete`]); any other
//! close is an incomplete stream → `Err(Network)` so REST fallback owns the result
//! (Soniox Codex 043b lesson + Codex 044 findings).

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::header::{HeaderValue, AUTHORIZATION};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use crate::cloud_stt::common::SttError;
use crate::cloud_stt::deepgram::{is_nova3, MODEL};
use crate::cloud_stt::deepgram_rt::{DeepgramRtFolder, DeepgramRtPartial, DeepgramRtResponse};

/// Deepgram realtime WebSocket endpoint — SAME origin as the REST API
/// (`api.deepgram.com`), so warm-up already covers it.
const RT_ENDPOINT: &str = "wss://api.deepgram.com/v1/listen";
/// Hard cap on draining remaining Results after a `CloseStream`; on expiry the
/// caller falls back to REST-on-WAV (the WS final is authoritative on success).
const FINALIZE_TIMEOUT: Duration = Duration::from_secs(3);
/// Deepgram drops idle sockets after ~10-12s; keepalive well under that.
const KEEPALIVE_EVERY: Duration = Duration::from_secs(5);

enum Control {
    Chunk(Vec<i16>),
    Finalize,
    Cancel,
}

/// Connection parameters. `api_key` is written only into the WS upgrade header and
/// is never logged.
pub(crate) struct DeepgramStreamConfig {
    pub api_key: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub language: Option<String>,
    pub keyterms: Vec<String>,
}

/// Sync handle over the async WS task. Mirrors `SonioxStreamHandle`.
pub(crate) struct DeepgramStreamHandle {
    tx: mpsc::UnboundedSender<Control>,
    final_rx: Mutex<Option<oneshot::Receiver<Result<String, SttError>>>>,
}

impl DeepgramStreamHandle {
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

impl Drop for DeepgramStreamHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Control::Cancel);
    }
}

/// Spawn the WS task and return the handle immediately. The connect happens inside
/// the task; chunks produced before it completes buffer in the unbounded channel,
/// so this never blocks the caller (recorder start).
pub(crate) fn open<F>(config: DeepgramStreamConfig, on_partial: F) -> DeepgramStreamHandle
where
    F: Fn(DeepgramRtPartial) + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let (final_tx, final_rx) = oneshot::channel();
    tauri::async_runtime::spawn(run_task(config, rx, final_tx, on_partial));
    DeepgramStreamHandle {
        tx,
        final_rx: Mutex::new(Some(final_rx)),
    }
}

/// Build the WS upgrade URL with all streaming query params. The API key is NOT in
/// the URL — it goes in the `Authorization` header on the upgrade request.
/// Keyterms are appended (and percent-encoded) only for Nova-3.
fn build_listen_url(config: &DeepgramStreamConfig) -> Result<reqwest::Url, SttError> {
    let mut url = reqwest::Url::parse(RT_ENDPOINT).map_err(|_| SttError::BadResponse)?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("model", MODEL);
        if let Some(lang) = config
            .language
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            q.append_pair("language", lang);
        }
        q.append_pair("encoding", "linear16");
        q.append_pair("sample_rate", &config.sample_rate.to_string());
        q.append_pair("channels", &config.channels.to_string());
        q.append_pair("interim_results", "true");
        q.append_pair("smart_format", "true");
        if is_nova3(MODEL) {
            for term in &config.keyterms {
                q.append_pair("keyterm", term);
            }
        }
    }
    Ok(url)
}

/// The close-authority state machine (Codex 044 findings): a server close/EOF may
/// resolve `Ok(committed)` ONLY when we successfully sent `CloseStream` (`finalizing`;
/// a failed send bails to Err before ever reaching a close) AND the server confirmed
/// the flush with its post-CloseStream `Metadata` summary. Every other close is an
/// incomplete stream → the caller's REST-on-WAV fallback owns the result.
fn close_is_complete(finalizing: bool, saw_final_metadata: bool) -> bool {
    finalizing && saw_final_metadata
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
    config: DeepgramStreamConfig,
    mut control_rx: mpsc::UnboundedReceiver<Control>,
    final_tx: oneshot::Sender<Result<String, SttError>>,
    on_partial: F,
) where
    F: Fn(DeepgramRtPartial) + Send + 'static,
{
    let mut final_slot = Some(final_tx);

    let url = match build_listen_url(&config) {
        Ok(url) => url,
        Err(error) => {
            log::warn!("Deepgram RT URL build failed: {error:?}");
            finish(&mut final_slot, Err(error));
            return;
        }
    };

    // Auth: `Token <key>` header on the WS upgrade request — same AuthScheme as
    // the REST path, but as a WS handshake header (NOT a body frame like Soniox).
    // Marked sensitive so no tracing/interceptor can leak it; NEVER logged.
    let mut request = match url.as_str().into_client_request() {
        Ok(request) => request,
        Err(error) => {
            log::warn!("Deepgram RT request build failed: {error}");
            finish(&mut final_slot, Err(SttError::Network));
            return;
        }
    };
    match HeaderValue::from_str(&format!("Token {}", config.api_key)) {
        Ok(mut value) => {
            value.set_sensitive(true);
            request.headers_mut().insert(AUTHORIZATION, value);
        }
        Err(error) => {
            log::warn!("Deepgram RT auth header build failed: {error}");
            finish(&mut final_slot, Err(SttError::Auth));
            return;
        }
    }

    let stream = match tokio_tungstenite::connect_async(request).await {
        Ok((stream, _response)) => stream,
        Err(error) => {
            log::warn!("Deepgram RT connect failed: {error}");
            finish(&mut final_slot, Err(SttError::Network));
            return;
        }
    };
    let (mut write, mut read) = stream.split();

    let mut folder = DeepgramRtFolder::new();
    let mut last_committed = String::new();
    let mut last_tentative = String::new();
    let mut finalizing = false;
    // Completion requires BOTH a successfully-sent CloseStream AND the server's
    // post-CloseStream Metadata summary (Deepgram's documented shutdown: flush
    // remaining Results → Metadata → close). Anything less is an incomplete
    // stream and must NOT become the authoritative pasted text (Codex 044
    // finding: a failed CloseStream write, or an abnormal close after it, would
    // otherwise return a truncated prefix as Ok).
    let mut saw_final_metadata = false;
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
                    // CloseStream tells Deepgram end-of-audio; it flushes remaining
                    // Results, sends Metadata, then closes cleanly. A FAILED send
                    // means the server never learned we finished — any later close
                    // would be for a truncated stream, so bail to REST immediately.
                    if write
                        .send(Message::text("{\"type\":\"CloseStream\"}"))
                        .await
                        .is_err()
                    {
                        log::warn!("Deepgram RT CloseStream send failed; treating stream as incomplete");
                        finish(&mut final_slot, Err(SttError::Network));
                        return;
                    }
                }
                Some(Control::Cancel) | None => {
                    // Cancelled: the result must never read as a completed transcript
                    // (authority would paste a truncated prefix), so resolve Err.
                    let _ = write.send(Message::Close(None)).await;
                    finish(&mut final_slot, Err(SttError::Network));
                    return;
                }
            },
            incoming = read.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let response: DeepgramRtResponse =
                        serde_json::from_str(text.as_str()).unwrap_or_default();
                    // Only Results mutate state; call on_partial only when committed
                    // or tentative actually changed (skip Metadata noise).
                    if response.r#type == "Results" {
                        let partial = folder.ingest(&response);
                        if partial.committed != last_committed || partial.tentative != last_tentative {
                            last_committed = partial.committed.clone();
                            last_tentative = partial.tentative.clone();
                            on_partial(partial);
                        }
                    } else if finalizing && response.r#type == "Metadata" {
                        // The post-CloseStream summary: the server has flushed every
                        // remaining Result. The stream is now provably complete.
                        saw_final_metadata = true;
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    if close_is_complete(finalizing, saw_final_metadata) {
                        finish(&mut final_slot, Ok(folder.committed().to_string()));
                    } else {
                        // Close before CloseStream, or after it but WITHOUT the
                        // flush-complete Metadata (server error/policy close mid-
                        // drain): committed may be a truncated prefix. Err(Network)
                        // so REST-on-WAV owns the authoritative result.
                        log::warn!(
                            "Deepgram RT socket closed without a completed shutdown \
                             (finalizing={finalizing}, metadata={saw_final_metadata}); \
                             treating stream as incomplete"
                        );
                        finish(&mut final_slot, Err(SttError::Network));
                    }
                    return;
                }
                // Ping/Pong/Binary/Frame from the server: ignore.
                Some(Ok(_)) => {}
                Some(Err(error)) => {
                    log::warn!("Deepgram RT read error: {error}");
                    finish(&mut final_slot, Err(SttError::Network));
                    return;
                }
            },
            _ = keepalive.tick(), if !finalizing => {
                let _ = write.send(Message::text("{\"type\":\"KeepAlive\"}")).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_listen_url_has_required_params_omits_empty_language_and_hides_key() {
        let config = DeepgramStreamConfig {
            api_key: "secret-key".to_string(),
            sample_rate: 16_000,
            channels: 1,
            language: None,
            keyterms: vec![],
        };
        let url = build_listen_url(&config).unwrap();
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let key_of = |k: &str| {
            pairs
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        };

        // Required params present.
        assert_eq!(key_of("model").as_deref(), Some("nova-3"));
        assert_eq!(key_of("encoding").as_deref(), Some("linear16"));
        assert_eq!(key_of("sample_rate").as_deref(), Some("16000"));
        assert_eq!(key_of("channels").as_deref(), Some("1"));
        assert_eq!(key_of("interim_results").as_deref(), Some("true"));
        assert_eq!(key_of("smart_format").as_deref(), Some("true"));

        // Language omitted when empty/None.
        assert!(key_of("language").is_none());

        // API key is NOT in the URL.
        assert!(!url.as_str().contains("secret-key"));
    }

    #[test]
    fn build_listen_url_includes_language_and_percent_encodes_keyterms() {
        let config = DeepgramStreamConfig {
            api_key: "k".to_string(),
            sample_rate: 48_000,
            channels: 2,
            language: Some("en".to_string()),
            keyterms: vec!["Foo Bar".to_string(), "baz&qux".to_string()],
        };
        let url = build_listen_url(&config).unwrap();
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let keyterm_vals: Vec<&str> = pairs
            .iter()
            .filter(|(k, _)| k == "keyterm")
            .map(|(_, v)| v.as_str())
            .collect();

        // Language present.
        assert_eq!(
            pairs
                .iter()
                .find(|(k, _)| k == "language")
                .map(|(_, v)| v.as_str()),
            Some("en")
        );
        assert_eq!(
            pairs
                .iter()
                .find(|(k, _)| k == "channels")
                .map(|(_, v)| v.as_str()),
            Some("2")
        );
        assert_eq!(
            pairs
                .iter()
                .find(|(k, _)| k == "sample_rate")
                .map(|(_, v)| v.as_str()),
            Some("48000")
        );

        // Keyterms appended in order, decoded back to their original values (proves
        // percent-encoding round-trips).
        assert_eq!(keyterm_vals, vec!["Foo Bar", "baz&qux"]);

        // API key is NOT in the URL.
        assert!(!url.as_str().contains("&api_key"));
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

    #[test]
    fn close_is_complete_requires_both_closestream_and_metadata() {
        // Codex 044 findings pinned: a close may only be authoritative when
        // CloseStream was successfully sent AND the flush-complete Metadata
        // arrived. (A failed CloseStream send never reaches a close — the task
        // bails to Err immediately — so `finalizing` here means "sent OK".)
        assert!(!close_is_complete(false, false)); // server closed mid-stream
        assert!(!close_is_complete(false, true)); // stray Metadata pre-finalize never set anyway
        assert!(!close_is_complete(true, false)); // abnormal close mid-drain (no flush confirm)
        assert!(close_is_complete(true, true)); // documented shutdown: CloseStream → Metadata → close
    }
}
