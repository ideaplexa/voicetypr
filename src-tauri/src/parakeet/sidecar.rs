#![allow(dead_code)]

use super::error::ParakeetError;
use super::messages::{
    ParakeetCommand, ParakeetResponse, ParakeetStreamConfig, ParakeetStreamEngine,
};
use base64::{engine::general_purpose, Engine as _};
use log::{debug, error, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::async_runtime::{Receiver, RwLock};
use tauri::AppHandle;
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};
use tokio::sync::RwLockWriteGuard;

use crate::utils::logger::log_performance;

const STREAM_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(30);
const STREAM_FINALIZE_TIMEOUT: Duration = Duration::from_secs(30);
const STREAM_OPEN_TIMEOUT: Duration = Duration::from_secs(30);
/// Bounded queue for cosmetic preview PCM only. Chunks typically arrive every
/// ~100–320 ms (see `chunk_ms` / preview cadence); capacity 64 keeps ~6–20 s of
/// preview backlog without unbounded RAM growth while the sidecar drains stdin.
const STREAM_PREVIEW_CHUNK_CHANNEL_CAPACITY: usize = 64;
/// Terminal finalize/cancel signals use a separate channel so they are not stuck
/// behind a full preview chunk queue.
const STREAM_TERMINAL_CONTROL_CHANNEL_CAPACITY: usize = 8;

#[derive(Debug, Clone)]
pub struct ParakeetStreamPartial {
    pub text: String,
    pub is_confirmed: bool,
    pub confidence: Option<f32>,
}

pub struct ParakeetStreamHandle {
    chunk_tx: tokio::sync::mpsc::Sender<Vec<i16>>,
    terminal_tx: tokio::sync::mpsc::Sender<StreamTerminalControl>,
    stream_finalizing: Arc<AtomicBool>,
    final_rx:
        tokio::sync::Mutex<Option<tokio::sync::oneshot::Receiver<Result<String, ParakeetError>>>>,
}

#[derive(Debug, PartialEq, Eq)]
enum StreamTerminalControl {
    Finalize,
    Cancel,
}

pub struct ParakeetStreamOpenRequest {
    pub app: AppHandle,
    pub model_id: String,
    pub model_version: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub engine: ParakeetStreamEngine,
    pub chunk_ms: Option<u16>,
    pub config: Option<ParakeetStreamConfig>,
}

impl ParakeetStreamHandle {
    fn try_send_preview_chunk(
        chunk_tx: &tokio::sync::mpsc::Sender<Vec<i16>>,
        stream_finalizing: &AtomicBool,
        samples: &[i16],
    ) -> Result<(), ParakeetError> {
        if stream_finalizing.load(Ordering::SeqCst) {
            return Err(ParakeetError::Terminated);
        }
        match chunk_tx.try_send(samples.to_vec()) {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                Err(ParakeetError::Terminated)
            }
        }
    }

    pub fn send_chunk(&self, samples: &[i16]) -> Result<(), ParakeetError> {
        Self::try_send_preview_chunk(&self.chunk_tx, &self.stream_finalizing, samples)
    }

    pub async fn finalize(&self) -> Result<String, ParakeetError> {
        let Some(rx) = self.final_rx.lock().await.take() else {
            return Err(ParakeetError::SidecarError {
                code: "stream_already_finalized".to_string(),
                message: "Stream finalization was already requested".to_string(),
            });
        };
        self.stream_finalizing.store(true, Ordering::SeqCst);
        self.terminal_tx
            .send(StreamTerminalControl::Finalize)
            .await
            .map_err(|_| ParakeetError::Terminated)?;
        match tokio::time::timeout(STREAM_FINALIZE_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(ParakeetError::Terminated),
            Err(_) => Err(ParakeetError::Timeout {
                operation: "finalize_stream".to_string(),
                timeout_secs: STREAM_FINALIZE_TIMEOUT.as_secs(),
            }),
        }
    }

    pub fn cancel(&self) {
        let _ = self.terminal_tx.try_send(StreamTerminalControl::Cancel);
    }
}

impl Drop for ParakeetStreamHandle {
    fn drop(&mut self) {
        let _ = self.terminal_tx.try_send(StreamTerminalControl::Cancel);
    }
}

/// Every preview chunk already accepted into the bounded queue, in FIFO order,
/// before `FinalizeStream` is written to the sidecar.
fn drain_buffered_preview_chunks(
    chunk_rx: &mut tokio::sync::mpsc::Receiver<Vec<i16>>,
) -> Vec<Vec<i16>> {
    let mut drained = Vec::new();
    while let Ok(samples) = chunk_rx.try_recv() {
        drained.push(samples);
    }
    drained
}

/// Sidecar writes for a finalize terminal signal: all buffered preview chunks, then finalize.
fn finalize_terminal_command_sequence(
    chunk_rx: &mut tokio::sync::mpsc::Receiver<Vec<i16>>,
) -> Vec<ParakeetCommand> {
    drain_buffered_preview_chunks(chunk_rx)
        .into_iter()
        .map(|samples| ParakeetCommand::AudioChunk {
            pcm_b64: encode_i16_le_base64(&samples),
        })
        .chain(std::iter::once(ParakeetCommand::FinalizeStream {}))
        .collect()
}

fn extract_json_payload(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    (start < end).then_some(&raw[start..=end])
}

fn parse_response_line(raw: &str) -> Result<ParakeetResponse, ParakeetError> {
    match serde_json::from_str::<ParakeetResponse>(raw) {
        Ok(response) => Ok(response),
        Err(_) => {
            let Some(payload) = extract_json_payload(raw) else {
                return Err(ParakeetError::InvalidResponse);
            };
            match serde_json::from_str::<ParakeetResponse>(payload) {
                Ok(response) => {
                    debug!("Recovered Parakeet response from a noisy sidecar line");
                    Ok(response)
                }
                Err(_) => Err(ParakeetError::InvalidResponse),
            }
        }
    }
}

fn log_parakeet_stderr(line: &str) {
    let lower = line.to_ascii_lowercase();
    let looks_like_error = lower.contains("error")
        || lower.contains("fail")
        || lower.contains("rate limit")
        || lower.contains("huggingface")
        || line.contains('❌');
    if is_benign_coreml_shape_probe(line) {
        log::info!(
            "Parakeet sidecar: Core ML shape probe emitted a benign diagnostic; model loading continued"
        );
    } else if looks_like_error {
        warn!("Parakeet sidecar: {}", line);
    } else {
        log::info!("Parakeet sidecar: {}", line);
    }
}

fn write_command_to_child(
    child: &mut Option<CommandChild>,
    command: &ParakeetCommand,
) -> Result<(), ParakeetError> {
    let mut payload = serde_json::to_string(command)?;
    payload.push('\n');
    child
        .as_mut()
        .ok_or(ParakeetError::Terminated)?
        .write(payload.as_bytes())
        .map_err(|e| ParakeetError::SpawnError(e.to_string()))
}

fn response_from_command_event(
    event: CommandEvent,
) -> Result<Option<ParakeetResponse>, ParakeetError> {
    let (line_bytes, from_stdout) = match event {
        CommandEvent::Stdout(line) => (line, true),
        CommandEvent::Stderr(line) => (line, false),
        CommandEvent::Terminated(payload) => {
            error!(
                "Parakeet sidecar terminated unexpectedly code={:?}",
                payload.code
            );
            return Err(ParakeetError::Terminated);
        }
        CommandEvent::Error(err) => {
            error!("Error from Parakeet sidecar pipe: {err}");
            return Err(ParakeetError::SpawnError(err));
        }
        _ => return Ok(None),
    };

    let text = String::from_utf8_lossy(&line_bytes);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    match parse_response_line(trimmed) {
        Ok(response) => Ok(Some(response)),
        Err(err) => {
            if from_stdout {
                error!(
                    "Failed to parse Parakeet sidecar stdout protocol line ({} bytes)",
                    trimmed.len()
                );
                Err(err)
            } else {
                log_parakeet_stderr(trimmed);
                Ok(None)
            }
        }
    }
}

fn encode_i16_le_base64(samples: &[i16]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    general_purpose::STANDARD.encode(bytes)
}

async fn request_with_timeout<F>(
    operation: String,
    timeout_secs: u64,
    timeout: Duration,
    request: F,
) -> Result<ParakeetResponse, ParakeetError>
where
    F: std::future::Future<Output = Result<ParakeetResponse, ParakeetError>>,
{
    match tokio::time::timeout(timeout, request).await {
        Ok(result) => result,
        Err(_) => Err(ParakeetError::Timeout {
            operation,
            timeout_secs,
        }),
    }
}

async fn timed_request<F>(
    command: &ParakeetCommand,
    request: F,
) -> Result<ParakeetResponse, ParakeetError>
where
    F: std::future::Future<Output = Result<ParakeetResponse, ParakeetError>>,
{
    let operation = command.operation_name().to_string();
    let timeout_secs = command.request_timeout_secs();
    request_with_timeout(
        operation,
        timeout_secs,
        Duration::from_secs(timeout_secs),
        request,
    )
    .await
}

/// Cancellable command dispatch shared by
/// [`ParakeetClient::send_with_progress_and_cancel`]: runs the first attempt,
/// and on `Terminated` (when the user did NOT cancel) respawns and retries once.
/// EVERY attempt is wrapped in [`timed_request`] — including the cancel-flag
/// path — so a sidecar that stays alive but stops responding still hits the
/// command deadline instead of hanging forever.
///
/// `make_request(is_retry, cancel)` produces the deadline-bounded sidecar
/// future (or a stand-in under test); the caller owns sidecar lifecycle around
/// it. Extracted so the deadline-on-cancel-path invariant is unit-testable
/// without spawning a real process.
async fn dispatch_cancellable<F, Fut>(
    command: &ParakeetCommand,
    cancel_flag: Option<Arc<AtomicBool>>,
    mut make_request: F,
) -> Result<ParakeetResponse, ParakeetError>
where
    F: FnMut(bool, Option<Arc<AtomicBool>>) -> Fut,
    Fut: std::future::Future<Output = Result<ParakeetResponse, ParakeetError>>,
{
    // First attempt — always deadline-bounded, even with a cancel flag. This was
    // the bug: the cancel path used to call the sidecar directly, so a live but
    // unresponsive sidecar could hang indefinitely while still polling cancel.
    let response = timed_request(command, make_request(false, cancel_flag.clone())).await;
    match response {
        Err(ParakeetError::Terminated)
            if !cancel_flag
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed)) =>
        {
            // The sidecar died but the user did not cancel: respawn once and
            // retry. The retry shared the same bug class, so it is bounded too.
            timed_request(command, make_request(true, cancel_flag.clone())).await
        }
        other => other,
    }
}

pub struct ParakeetSidecar {
    rx: Receiver<CommandEvent>,
    child: Option<CommandChild>,
}

impl ParakeetSidecar {
    pub async fn spawn(app: &AppHandle, binary_name: &str) -> Result<Self, ParakeetError> {
        let spawn_start = Instant::now();
        // In Tauri v2, use the shell plugin and pass just the filename.
        // The externalBin entry in tauri.conf.json must include this binary.
        let (rx, child) = app
            .shell()
            .sidecar(binary_name)
            .map_err(|e| ParakeetError::SpawnError(e.to_string()))?
            .spawn()
            .map_err(|e| ParakeetError::SpawnError(e.to_string()))?;
        log_performance(
            "PARAKEET_SPAWN",
            spawn_start.elapsed().as_millis() as u64,
            Some(&format!("binary={binary_name}")),
        );

        log::info!(
            "Spawned Parakeet sidecar pid={} name={}",
            child.pid(),
            binary_name
        );
        Ok(Self {
            rx,
            child: Some(child),
        })
    }

    pub async fn request(
        &mut self,
        command: &ParakeetCommand,
    ) -> Result<ParakeetResponse, ParakeetError> {
        self.request_with_progress_and_cancel(command, None::<&mut fn(f32, Option<&str>)>, None)
            .await
    }

    fn write_command(&mut self, command: &ParakeetCommand) -> Result<(), ParakeetError> {
        let mut payload = serde_json::to_string(command)?;
        payload.push('\n');
        self.child
            .as_mut()
            .ok_or(ParakeetError::Terminated)?
            .write(payload.as_bytes())
            .map_err(|e| ParakeetError::SpawnError(e.to_string()))
    }

    async fn next_protocol_response(&mut self) -> Result<ParakeetResponse, ParakeetError> {
        loop {
            let Some(event) = self.rx.recv().await else {
                return Err(ParakeetError::Terminated);
            };

            let (line_bytes, from_stdout) = match event {
                CommandEvent::Stdout(line) => (line, true),
                CommandEvent::Stderr(line) => (line, false),
                CommandEvent::Terminated(payload) => {
                    error!(
                        "Parakeet sidecar terminated unexpectedly code={:?}",
                        payload.code
                    );
                    return Err(ParakeetError::Terminated);
                }
                CommandEvent::Error(err) => {
                    error!("Error from Parakeet sidecar pipe: {err}");
                    return Err(ParakeetError::SpawnError(err));
                }
                _ => continue,
            };

            let text = String::from_utf8_lossy(&line_bytes);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }

            match parse_response_line(trimmed) {
                Ok(response) => return Ok(response),
                Err(err) => {
                    if from_stdout {
                        error!(
                            "Failed to parse Parakeet sidecar stdout protocol line ({} bytes)",
                            trimmed.len()
                        );
                        return Err(err);
                    }
                    log_parakeet_stderr(trimmed);
                }
            }
        }
    }

    pub async fn request_with_progress_and_cancel<F>(
        &mut self,
        command: &ParakeetCommand,
        mut progress_callback: Option<&mut F>,
        cancel_flag: Option<Arc<AtomicBool>>,
    ) -> Result<ParakeetResponse, ParakeetError>
    where
        F: FnMut(f32, Option<&str>),
    {
        let mut payload = serde_json::to_string(command)?;
        payload.push('\n');
        self.child
            .as_mut()
            .ok_or(ParakeetError::Terminated)?
            .write(payload.as_bytes())
            .map_err(|e| ParakeetError::SpawnError(e.to_string()))?;

        loop {
            if cancel_flag
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed))
            {
                if let Some(child) = self.child.take() {
                    if let Err(err) = child.kill() {
                        warn!("Failed to kill Parakeet sidecar during cancellation: {err:?}");
                    }
                }
                return Err(ParakeetError::SidecarError {
                    code: "cancelled".to_string(),
                    message: "Cancelled by user".to_string(),
                });
            }

            let event = if cancel_flag.is_some() {
                tokio::select! {
                    event = self.rx.recv() => event,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => continue,
                }
            } else {
                self.rx.recv().await
            };

            let Some(event) = event else {
                break;
            };

            let (line_bytes, from_stdout) = match event {
                CommandEvent::Stdout(line) => (line, true),
                CommandEvent::Stderr(line) => (line, false),
                CommandEvent::Terminated(payload) => {
                    error!(
                        "Parakeet sidecar terminated unexpectedly code={:?}",
                        payload.code
                    );
                    return Err(ParakeetError::Terminated);
                }
                CommandEvent::Error(err) => {
                    error!("Error from Parakeet sidecar pipe: {err}");
                    return Err(ParakeetError::SpawnError(err));
                }
                _ => continue,
            };

            let text = String::from_utf8_lossy(&line_bytes);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }

            // The sidecar redirects stdout->stderr around native CoreML calls, so a
            // protocol response (load/transcribe result or status) can surface on
            // stderr instead of stdout. Parse responses from EITHER stream; a
            // non-protocol stderr line is only a diagnostic log.
            match parse_response_line(trimmed) {
                Ok(ParakeetResponse::Error { code, message, .. }) => {
                    return Err(ParakeetError::SidecarError { code, message });
                }
                Ok(ParakeetResponse::Progress { progress, phase }) => {
                    if let Some(callback) = progress_callback.as_deref_mut() {
                        callback(progress, phase.as_deref());
                    }
                }
                Ok(response) => return Ok(response),
                Err(err) => {
                    if from_stdout {
                        error!(
                            "Failed to parse Parakeet sidecar stdout protocol line ({} bytes)",
                            trimmed.len()
                        );
                        return Err(err);
                    }
                    // FluidAudio writes its REAL download/runtime diagnostics
                    // to stderr (HuggingFace rate-limits, `downloadFailed`,
                    // CoreML errors, …) as plain text, not JSON protocol.
                    // Surface error-looking lines at `warn!`; floor everything
                    // else at `info!` (NOT `trace!`) — the shipped app logs at
                    // info level, so trace lines are invisible in field logs,
                    // which would hide the last thing FluidAudio prints before a
                    // stall. `info` keeps that stall context visible by default.
                    log_parakeet_stderr(trimmed);
                }
            }
        }

        Err(ParakeetError::Terminated)
    }

    pub fn kill(self) {
        if let Some(child) = self.child {
            if let Err(err) = child.kill() {
                warn!("Failed to kill Parakeet sidecar: {err:?}");
            }
        }
    }
}

pub struct ParakeetClient {
    binary_name: String,
    inner: Arc<RwLock<Option<ParakeetSidecar>>>,
}

impl ParakeetClient {
    pub fn new(binary_name: impl Into<String>) -> Self {
        Self {
            binary_name: binary_name.into(),
            inner: Arc::new(RwLock::new(None)),
        }
    }

    async fn ensure(
        &self,
        app: &AppHandle,
    ) -> Result<RwLockWriteGuard<'_, Option<ParakeetSidecar>>, ParakeetError> {
        let mut guard = self.inner.write().await;
        if guard.is_none() {
            let sidecar = ParakeetSidecar::spawn(app, &self.binary_name).await?;
            guard.replace(sidecar);
        }
        Ok(guard)
    }

    fn clear_sidecar(guard: &mut RwLockWriteGuard<'_, Option<ParakeetSidecar>>) {
        if let Some(sidecar) = guard.take() {
            sidecar.kill();
        }
    }

    pub async fn send(
        &self,
        app: &AppHandle,
        command: &ParakeetCommand,
    ) -> Result<ParakeetResponse, ParakeetError> {
        let mut guard = self.ensure(app).await?;
        let response = match guard.as_mut() {
            Some(sidecar) => timed_request(command, sidecar.request(command)).await,
            None => return Err(ParakeetError::Terminated),
        };

        match response {
            Err(ParakeetError::Timeout { .. }) => {
                Self::clear_sidecar(&mut guard);
                response
            }
            Err(ParakeetError::Terminated) => {
                Self::clear_sidecar(&mut guard);
                drop(guard);
                let mut guard = self.ensure(app).await?;
                let response = match guard.as_mut() {
                    Some(sidecar) => timed_request(command, sidecar.request(command)).await,
                    None => Err(ParakeetError::Terminated),
                };
                if matches!(response, Err(ParakeetError::Timeout { .. })) {
                    Self::clear_sidecar(&mut guard);
                }
                response
            }
            other => other,
        }
    }

    pub async fn send_with_progress_and_cancel<F>(
        &self,
        app: &AppHandle,
        command: &ParakeetCommand,
        cancel_flag: Option<Arc<AtomicBool>>,
        progress_callback: F,
    ) -> Result<ParakeetResponse, ParakeetError>
    where
        F: FnMut(f32, Option<&str>),
    {
        // The progress callback is shared across the (at most two) sequential
        // attempts. Wrap it so each attempt future can borrow it mutably without
        // holding one long-lived `&mut` across both `make_request` invocations —
        // the two attempts never overlap, so the lock is uncontended.
        let progress = Arc::new(tokio::sync::Mutex::new(progress_callback));
        let make_request = move |is_retry: bool, cancel: Option<Arc<AtomicBool>>| {
            let progress = progress.clone();
            async move {
                let mut guard = self.ensure(app).await?;
                if is_retry {
                    // Previous attempt's sidecar died (Terminated): kill it so
                    // `ensure` respawns a fresh process.
                    Self::clear_sidecar(&mut guard);
                    drop(guard);
                    guard = self.ensure(app).await?;
                }
                let response = match guard.as_mut() {
                    Some(sidecar) => {
                        let mut cb = progress.lock().await;
                        sidecar
                            .request_with_progress_and_cancel(command, Some(&mut *cb), cancel)
                            .await
                    }
                    None => Err(ParakeetError::Terminated),
                };
                response
            }
        };

        // Delegate the dispatch: it wraps BOTH attempts in `timed_request`
        // (deadline enforced even on the cancel path) and retries once on
        // Terminated-while-not-cancelled. This centralises the deadline-on-cancel
        // invariant so it is unit-testable without spawning a real process.
        let response = dispatch_cancellable(command, cancel_flag, make_request).await;

        // Lifecycle: kill the sidecar when it is left unusable — deadline
        // exceeded (the process may be wedged) or the user cancelled (the
        // sidecar's cancel loop already tried to kill it; clear for a clean slate
        // so the next command spawns fresh).
        let clear_after = match &response {
            Err(ParakeetError::Timeout { .. }) => true,
            Err(ParakeetError::SidecarError { code, .. }) => code.as_str() == "cancelled",
            _ => false,
        };
        if clear_after {
            let mut guard = self.inner.write().await;
            Self::clear_sidecar(&mut guard);
        }

        response
    }

    pub async fn open_stream<F>(
        &self,
        request: ParakeetStreamOpenRequest,
        mut partial_callback: F,
    ) -> Result<ParakeetStreamHandle, ParakeetError>
    where
        F: FnMut(ParakeetStreamPartial) + Send + 'static,
    {
        let inner = self.inner.clone();
        let binary_name = self.binary_name.clone();
        let ParakeetStreamOpenRequest {
            app,
            model_id,
            model_version,
            sample_rate,
            channels,
            engine,
            chunk_ms,
            config,
        } = request;
        let (chunk_tx, mut chunk_rx) =
            tokio::sync::mpsc::channel::<Vec<i16>>(STREAM_PREVIEW_CHUNK_CHANNEL_CAPACITY);
        let (terminal_tx, mut terminal_rx) =
            tokio::sync::mpsc::channel(STREAM_TERMINAL_CONTROL_CHANNEL_CAPACITY);
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<Result<(), ParakeetError>>();
        let (final_tx, final_rx) = tokio::sync::oneshot::channel::<Result<String, ParakeetError>>();
        let open_aborted = Arc::new(AtomicBool::new(false));
        let stream_finalizing = Arc::new(AtomicBool::new(false));
        let task_open_aborted = open_aborted.clone();

        tauri::async_runtime::spawn(async move {
            let mut ready_tx = Some(ready_tx);
            let mut final_tx = Some(final_tx);
            let mut guard = inner.write().await;
            if task_open_aborted.load(Ordering::Relaxed) {
                return;
            }
            if guard.is_none() {
                match ParakeetSidecar::spawn(&app, &binary_name).await {
                    Ok(sidecar) => {
                        guard.replace(sidecar);
                    }
                    Err(error) => {
                        if let Some(tx) = ready_tx.take() {
                            let _ = tx.send(Err(error));
                        }
                        if let Some(tx) = final_tx.take() {
                            let _ = tx.send(Err(ParakeetError::Terminated));
                        }
                        return;
                    }
                }
            }

            let Some(sidecar) = guard.as_mut() else {
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(Err(ParakeetError::Terminated));
                }
                if let Some(tx) = final_tx.take() {
                    let _ = tx.send(Err(ParakeetError::Terminated));
                }
                return;
            };

            let start_command = ParakeetCommand::StartStream {
                model_id,
                model_version,
                sample_rate,
                channels,
                engine,
                chunk_ms,
                config,
            };
            if let Err(error) = write_command_to_child(&mut sidecar.child, &start_command) {
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(Err(error));
                }
                if let Some(tx) = final_tx.take() {
                    let _ = tx.send(Err(ParakeetError::Terminated));
                }
                return;
            }

            let open_deadline = tokio::time::sleep(STREAM_OPEN_TIMEOUT);
            tokio::pin!(open_deadline);
            loop {
                tokio::select! {
                    response = sidecar.next_protocol_response() => {
                        match response {
                            Ok(ParakeetResponse::StreamStarted {}) => {
                                if let Some(tx) = ready_tx.take() {
                                    let _ = tx.send(Ok(()));
                                }
                                break;
                            }
                            Ok(ParakeetResponse::StreamPartial {
                                text,
                                is_confirmed,
                                confidence,
                            }) => {
                                partial_callback(ParakeetStreamPartial {
                                    text,
                                    is_confirmed,
                                    confidence,
                                });
                            }
                            Ok(ParakeetResponse::Error { code, message, .. }) => {
                                if let Some(tx) = ready_tx.take() {
                                    let _ = tx.send(Err(ParakeetError::SidecarError {
                                        code: code.clone(),
                                        message: message.clone(),
                                    }));
                                }
                                if let Some(tx) = final_tx.take() {
                                    let _ = tx.send(Err(ParakeetError::SidecarError { code, message }));
                                }
                                let _ = sidecar;
                                Self::clear_sidecar(&mut guard);
                                return;
                            }
                            Ok(_) => {}
                            Err(error) => {
                                if let Some(tx) = ready_tx.take() {
                                    let _ = tx.send(Err(error));
                                }
                                if let Some(tx) = final_tx.take() {
                                    let _ = tx.send(Err(ParakeetError::Terminated));
                                }
                                let _ = sidecar;
                                Self::clear_sidecar(&mut guard);
                                return;
                            }
                        }
                    }
                    _ = &mut open_deadline => {
                        let operation = "start_stream".to_string();
                        if let Some(tx) = ready_tx.take() {
                            let _ = tx.send(Err(ParakeetError::Timeout {
                                operation: operation.clone(),
                                timeout_secs: STREAM_OPEN_TIMEOUT.as_secs(),
                            }));
                        }
                        if let Some(tx) = final_tx.take() {
                            let _ = tx.send(Err(ParakeetError::Timeout {
                                operation,
                                timeout_secs: STREAM_OPEN_TIMEOUT.as_secs(),
                            }));
                        }
                        let _ = write_command_to_child(&mut sidecar.child, &ParakeetCommand::CancelStream {});
                        let _ = sidecar;
                        Self::clear_sidecar(&mut guard);
                        return;
                    }
                }
            }

            let mut accept_preview_chunks = true;
            let mut clear_after_exit = false;
            'stream: loop {
                tokio::select! {
                    biased;

                    terminal = terminal_rx.recv() => {
                        match terminal {
                            Some(StreamTerminalControl::Finalize) => {
                                accept_preview_chunks = false;
                                chunk_rx.close();
                                for command in finalize_terminal_command_sequence(&mut chunk_rx) {
                                    if let Err(error) =
                                        write_command_to_child(&mut sidecar.child, &command)
                                    {
                                        if let Some(tx) = final_tx.take() {
                                            let _ = tx.send(Err(error));
                                        }
                                        clear_after_exit = true;
                                        break 'stream;
                                    }
                                }
                            }
                            Some(StreamTerminalControl::Cancel) => {
                                chunk_rx.close();
                                let _ = write_command_to_child(
                                    &mut sidecar.child,
                                    &ParakeetCommand::CancelStream {},
                                );
                                if let Some(tx) = final_tx.take() {
                                    let _ = tx.send(Err(ParakeetError::SidecarError {
                                        code: "cancelled".to_string(),
                                        message: "Stream cancelled".to_string(),
                                    }));
                                }
                                clear_after_exit = true;
                                break;
                            }
                            None => {
                                chunk_rx.close();
                                let _ = write_command_to_child(
                                    &mut sidecar.child,
                                    &ParakeetCommand::CancelStream {},
                                );
                                clear_after_exit = true;
                                break;
                            }
                        }
                    }
                    chunk = chunk_rx.recv(), if accept_preview_chunks => {
                        let Some(samples) = chunk else {
                            let _ = write_command_to_child(
                                &mut sidecar.child,
                                &ParakeetCommand::CancelStream {},
                            );
                            clear_after_exit = true;
                            break;
                        };
                        let command = ParakeetCommand::AudioChunk {
                            pcm_b64: encode_i16_le_base64(&samples),
                        };
                        if let Err(error) = write_command_to_child(&mut sidecar.child, &command) {
                            if let Some(tx) = final_tx.take() {
                                let _ = tx.send(Err(error));
                            }
                            clear_after_exit = true;
                            break;
                        }
                    }
                    event = sidecar.rx.recv() => {
                        let Some(event) = event else {
                            if let Some(tx) = final_tx.take() {
                                let _ = tx.send(Err(ParakeetError::Terminated));
                            }
                            clear_after_exit = true;
                            break;
                        };
                        match response_from_command_event(event) {
                            Ok(Some(ParakeetResponse::StreamPartial { text, is_confirmed, confidence })) => {
                                partial_callback(ParakeetStreamPartial { text, is_confirmed, confidence });
                            }
                            Ok(Some(ParakeetResponse::StreamFinal { text })) => {
                                if let Some(tx) = final_tx.take() {
                                    let _ = tx.send(Ok(text));
                                }
                                break;
                            }
                            Ok(Some(ParakeetResponse::StreamCancelled {})) => {
                                if let Some(tx) = final_tx.take() {
                                    let _ = tx.send(Err(ParakeetError::SidecarError {
                                        code: "cancelled".to_string(),
                                        message: "Stream cancelled".to_string(),
                                    }));
                                }
                                break;
                            }
                            Ok(Some(ParakeetResponse::Error { code, message, .. })) => {
                                if let Some(tx) = final_tx.take() {
                                    let _ = tx.send(Err(ParakeetError::SidecarError { code, message }));
                                }
                                let _ = write_command_to_child(
                                    &mut sidecar.child,
                                    &ParakeetCommand::CancelStream {},
                                );
                                clear_after_exit = true;
                                break;
                            }
                            Ok(Some(_)) | Ok(None) => {}
                            Err(error) => {
                                if let Some(tx) = final_tx.take() {
                                    let _ = tx.send(Err(error));
                                }
                                let _ = write_command_to_child(
                                    &mut sidecar.child,
                                    &ParakeetCommand::CancelStream {},
                                );
                                clear_after_exit = true;
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(STREAM_INACTIVITY_TIMEOUT) => {
                        if let Some(tx) = final_tx.take() {
                            let _ = tx.send(Err(ParakeetError::Timeout {
                                operation: "stream_session".to_string(),
                                timeout_secs: STREAM_INACTIVITY_TIMEOUT.as_secs(),
                            }));
                        }
                        let _ = write_command_to_child(
                            &mut sidecar.child,
                            &ParakeetCommand::CancelStream {},
                        );
                        clear_after_exit = true;
                        break;
                    }
                }
            }
            if clear_after_exit {
                let _ = sidecar;
                Self::clear_sidecar(&mut guard);
            }
        });

        match tokio::time::timeout(STREAM_OPEN_TIMEOUT, ready_rx).await {
            Err(_) => {
                open_aborted.store(true, Ordering::Relaxed);
                Err(ParakeetError::Timeout {
                    operation: "start_stream".to_string(),
                    timeout_secs: STREAM_OPEN_TIMEOUT.as_secs(),
                })
            }
            Ok(ready) => match ready {
                Ok(Ok(())) => Ok(ParakeetStreamHandle {
                    chunk_tx,
                    terminal_tx,
                    stream_finalizing,
                    final_rx: tokio::sync::Mutex::new(Some(final_rx)),
                }),
                Ok(Err(error)) => Err(error),
                Err(_) => Err(ParakeetError::Terminated),
            },
        }
    }

    pub async fn shutdown(&self) {
        if let Some(sidecar) = self.inner.write().await.take() {
            sidecar.kill();
        }
    }
}

fn is_benign_coreml_shape_probe(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("e5rt encountered an stl exception")
        && lower.contains("failed to propagateinputtensorshapes")
        && lower.contains("zero shape error")
}

#[cfg(test)]
mod tests {
    use super::{
        dispatch_cancellable, drain_buffered_preview_chunks, extract_json_payload,
        finalize_terminal_command_sequence, is_benign_coreml_shape_probe, parse_response_line,
        request_with_timeout, ParakeetStreamHandle, StreamTerminalControl,
    };
    use crate::parakeet::error::ParakeetError;
    use crate::parakeet::messages::{
        ParakeetCommand, ParakeetResponse, SHORT_REQUEST_TIMEOUT_SECS,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn test_stream_finalizing() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn extract_json_payload_returns_object_slice() {
        let raw = r#"noise before {"type":"status","loadedModel":"parakeet-tdt-0.6b-v2","modelVersion":"v2"}"#;
        assert_eq!(
            extract_json_payload(raw),
            Some(r#"{"type":"status","loadedModel":"parakeet-tdt-0.6b-v2","modelVersion":"v2"}"#)
        );
    }

    #[test]
    fn parse_response_line_accepts_clean_json() {
        let raw = r#"{"type":"status","loadedModel":"parakeet-tdt-0.6b-v2","modelVersion":"v2"}"#;
        let response = parse_response_line(raw).expect("expected valid response");

        match response {
            ParakeetResponse::Status {
                loaded_model,
                model_version,
                ..
            } => {
                assert_eq!(loaded_model.as_deref(), Some("parakeet-tdt-0.6b-v2"));
                assert_eq!(model_version.as_deref(), Some("v2"));
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[test]
    fn parse_response_line_recovers_json_after_noisy_prefix() {
        let raw = r#"E5RT encountered an STL exception. msg = Failed to PropagateInputTensorShapes: std::runtime_error during type inference for ios17.slice_by_index: zero shape error. {"type":"status","loadedModel":"parakeet-tdt-0.6b-v2","modelVersion":"v2"}"#;
        assert!(is_benign_coreml_shape_probe(raw));
        assert!(!is_benign_coreml_shape_probe(
            "downloadFailed: unauthorized"
        ));
        let response = parse_response_line(raw).expect("expected recovered response");

        match response {
            ParakeetResponse::Status {
                loaded_model,
                model_version,
                ..
            } => {
                assert_eq!(loaded_model.as_deref(), Some("parakeet-tdt-0.6b-v2"));
                assert_eq!(model_version.as_deref(), Some("v2"));
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[test]
    fn parse_response_line_accepts_progress_events() {
        let raw = r#"{"type":"progress","progress":0.42,"phase":"downloading 1/3"}"#;
        let response = parse_response_line(raw).expect("expected valid response");

        match response {
            ParakeetResponse::Progress { progress, phase } => {
                assert!((progress - 0.42).abs() < f32::EPSILON);
                assert_eq!(phase.as_deref(), Some("downloading 1/3"));
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[tokio::test]
    async fn request_with_timeout_returns_typed_timeout_for_pending_receive() {
        let started = tokio::time::Instant::now();
        let err = request_with_timeout(
            "status".to_string(),
            SHORT_REQUEST_TIMEOUT_SECS,
            Duration::from_millis(10),
            std::future::pending::<Result<ParakeetResponse, ParakeetError>>(),
        )
        .await
        .expect_err("expected timeout");

        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(matches!(
            err,
            ParakeetError::Timeout {
                operation,
                timeout_secs: SHORT_REQUEST_TIMEOUT_SECS,
            } if operation == "status"
        ));
    }
    #[tokio::test(start_paused = true)]
    async fn cancellable_dispatch_enforces_deadline_on_first_attempt_with_cancel_flag() {
        // Reproduces the bug: send_with_progress_and_cancel used to call the
        // sidecar directly (no timed_request) when a cancel_flag was present, so a
        // sidecar that stayed alive but never responded hung forever while still
        // polling cancel. The dispatch must wrap the FIRST attempt in a deadline
        // even with a cancel flag.
        //
        // Fails on the pre-fix code: without the wrap, `make_request`'s pending
        // future never resolves, the outer guard elapses, and `.expect` panics.
        let command = ParakeetCommand::Status {};
        let cancel = Some(Arc::new(AtomicBool::new(false)));
        let make_request = |_is_retry: bool, _cancel: Option<Arc<AtomicBool>>| async move {
            // A sidecar that stays alive but never sends a protocol line.
            std::future::pending::<Result<ParakeetResponse, ParakeetError>>().await
        };

        let result = tokio::time::timeout(
            Duration::from_secs(command.request_timeout_secs() * 2),
            dispatch_cancellable(&command, cancel, make_request),
        )
        .await;

        let err = result
            .expect("dispatch hung — the cancel path's first attempt is not deadline-bounded")
            .expect_err("expected a typed Timeout, not a response");
        let expected = command.request_timeout_secs();
        assert!(matches!(
            err,
            ParakeetError::Timeout { operation, timeout_secs }
                if operation == "status" && timeout_secs == expected
        ));
        assert_eq!(expected, SHORT_REQUEST_TIMEOUT_SECS);
    }

    #[tokio::test(start_paused = true)]
    async fn cancellable_dispatch_enforces_deadline_on_retry_with_cancel_flag() {
        // The retry branch shared the same bug: when the first attempt returned
        // Terminated (sidecar died) the retry also skipped timed_request. With a
        // cancel flag present, a retry that never responds must still hit the
        // deadline rather than hang. Fails on the pre-fix code via the same
        // outer-guard panic as the first-attempt case.
        let command = ParakeetCommand::Status {};
        let cancel = Some(Arc::new(AtomicBool::new(false)));
        let make_request = |is_retry: bool, _cancel: Option<Arc<AtomicBool>>| async move {
            if is_retry {
                std::future::pending::<Result<ParakeetResponse, ParakeetError>>().await
            } else {
                Err(ParakeetError::Terminated)
            }
        };

        let result = tokio::time::timeout(
            Duration::from_secs(command.request_timeout_secs() * 2),
            dispatch_cancellable(&command, cancel, make_request),
        )
        .await;

        let err = result
            .expect("dispatch hung on retry — the cancel path's retry is not deadline-bounded")
            .expect_err("expected a typed Timeout, not a response");
        assert!(matches!(
            err,
            ParakeetError::Timeout { operation, .. } if operation == "status"
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn cancellable_dispatch_preserves_cancel_polling() {
        // The added deadline must NOT mask cancellation: when the (simulated)
        // sidecar honors the cancel flag and aborts, the dispatch surfaces the
        // cancellation promptly instead of waiting the full deadline. Proves the
        // wrap preserves cancel polling rather than always timing out.
        let command = ParakeetCommand::Status {};
        let cancel = Some(Arc::new(AtomicBool::new(true))); // already cancelled
        let make_request = |_is_retry: bool, cancel: Option<Arc<AtomicBool>>| async move {
            if cancel.as_ref().is_some_and(|f| f.load(Ordering::Relaxed)) {
                Err(ParakeetError::SidecarError {
                    code: "cancelled".to_string(),
                    message: "Cancelled by user".to_string(),
                })
            } else {
                std::future::pending::<Result<ParakeetResponse, ParakeetError>>().await
            }
        };

        let started = tokio::time::Instant::now();
        let err = dispatch_cancellable(&command, cancel, make_request)
            .await
            .expect_err("expected cancellation, not a response");
        // Returned promptly (well before the deadline) — cancel polling wins.
        assert!(started.elapsed() < Duration::from_secs(command.request_timeout_secs()));
        assert!(matches!(
            err,
            ParakeetError::SidecarError { code, .. } if code == "cancelled"
        ));
    }

    #[tokio::test]
    async fn dropping_stream_handle_sends_cancel_on_terminal_channel() {
        let (chunk_tx, _chunk_rx) = tokio::sync::mpsc::channel(4);
        let (terminal_tx, mut terminal_rx) = tokio::sync::mpsc::channel(8);
        let handle = ParakeetStreamHandle {
            chunk_tx,
            terminal_tx,
            stream_finalizing: test_stream_finalizing(),
            final_rx: tokio::sync::Mutex::new(None),
        };

        drop(handle);

        assert_eq!(
            terminal_rx.recv().await,
            Some(StreamTerminalControl::Cancel)
        );
    }

    #[test]
    fn send_chunk_delivers_samples_when_capacity_available() {
        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel(4);
        let (terminal_tx, _terminal_rx) = tokio::sync::mpsc::channel(8);
        let handle = ParakeetStreamHandle {
            chunk_tx,
            terminal_tx,
            stream_finalizing: test_stream_finalizing(),
            final_rx: tokio::sync::Mutex::new(None),
        };

        handle.send_chunk(&[10, 20, 30]).expect("chunk send");
        assert_eq!(chunk_rx.try_recv().unwrap(), vec![10, 20, 30]);
    }

    #[test]
    fn finalize_sequence_drains_buffered_chunks_before_finalize_command() {
        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel(8);
        chunk_tx.try_send(vec![1, 2]).unwrap();
        chunk_tx.try_send(vec![3]).unwrap();

        let sequence = finalize_terminal_command_sequence(&mut chunk_rx);

        assert_eq!(sequence.len(), 3);
        assert!(matches!(sequence[0], ParakeetCommand::AudioChunk { .. }));
        assert!(matches!(sequence[1], ParakeetCommand::AudioChunk { .. }));
        assert!(matches!(
            sequence[2],
            ParakeetCommand::FinalizeStream { .. }
        ));
        assert!(chunk_rx.try_recv().is_err());
        assert!(drain_buffered_preview_chunks(&mut chunk_rx).is_empty());
    }

    #[test]
    fn send_chunk_returns_terminated_after_finalize_requested() {
        let (chunk_tx, _chunk_rx) = tokio::sync::mpsc::channel(4);
        let (terminal_tx, _terminal_rx) = tokio::sync::mpsc::channel(8);
        let stream_finalizing = Arc::new(AtomicBool::new(true));
        let handle = ParakeetStreamHandle {
            chunk_tx,
            terminal_tx,
            stream_finalizing,
            final_rx: tokio::sync::Mutex::new(None),
        };

        let err = handle.send_chunk(&[7]).expect_err("after finalize flag");
        assert!(matches!(err, ParakeetError::Terminated));
    }

    #[test]
    fn send_chunk_returns_terminated_when_chunk_channel_closed() {
        let (chunk_tx, chunk_rx) = tokio::sync::mpsc::channel(4);
        drop(chunk_rx);
        let (terminal_tx, _terminal_rx) = tokio::sync::mpsc::channel(8);
        let handle = ParakeetStreamHandle {
            chunk_tx,
            terminal_tx,
            stream_finalizing: test_stream_finalizing(),
            final_rx: tokio::sync::Mutex::new(None),
        };

        let err = handle.send_chunk(&[1]).expect_err("closed chunk channel");
        assert!(matches!(err, ParakeetError::Terminated));
    }

    #[test]
    fn send_chunk_drops_preview_when_bounded_queue_full() {
        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel(1);
        let (terminal_tx, _terminal_rx) = tokio::sync::mpsc::channel(8);
        let handle = ParakeetStreamHandle {
            chunk_tx,
            terminal_tx,
            stream_finalizing: test_stream_finalizing(),
            final_rx: tokio::sync::Mutex::new(None),
        };

        handle.send_chunk(&[1]).expect("first chunk queued");
        handle
            .send_chunk(&[2])
            .expect("overflow drops without error");

        assert_eq!(chunk_rx.try_recv().unwrap(), vec![1]);
        assert!(chunk_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn terminal_cancel_reaches_worker_when_chunk_queue_saturated() {
        let (chunk_tx, _chunk_rx) = tokio::sync::mpsc::channel(1);
        let (terminal_tx, mut terminal_rx) = tokio::sync::mpsc::channel(8);
        let handle = ParakeetStreamHandle {
            chunk_tx,
            terminal_tx,
            stream_finalizing: test_stream_finalizing(),
            final_rx: tokio::sync::Mutex::new(None),
        };

        handle.send_chunk(&[99]).expect("saturate preview queue");
        handle.cancel();

        assert_eq!(
            terminal_rx.recv().await,
            Some(StreamTerminalControl::Cancel)
        );
    }

    #[tokio::test]
    async fn terminal_finalize_not_blocked_by_full_chunk_queue() {
        let (chunk_tx, _chunk_rx) = tokio::sync::mpsc::channel(1);
        let (terminal_tx, mut terminal_rx) = tokio::sync::mpsc::channel(8);
        let (final_tx, final_rx) = tokio::sync::oneshot::channel::<Result<String, ParakeetError>>();
        let handle = ParakeetStreamHandle {
            chunk_tx,
            terminal_tx,
            stream_finalizing: test_stream_finalizing(),
            final_rx: tokio::sync::Mutex::new(Some(final_rx)),
        };

        handle.send_chunk(&[1]).expect("fill preview queue");

        let finalize_task = tokio::spawn(async move { handle.finalize().await });
        assert_eq!(
            terminal_rx.recv().await,
            Some(StreamTerminalControl::Finalize)
        );
        let _ = final_tx.send(Ok("final".to_string()));
        assert_eq!(finalize_task.await.unwrap().unwrap(), "final");
    }

    #[tokio::test]
    async fn finalize_consumes_final_rx_once() {
        let (chunk_tx, _) = tokio::sync::mpsc::channel(4);
        let (terminal_tx, mut terminal_rx) = tokio::sync::mpsc::channel(8);
        let (final_tx, final_rx) = tokio::sync::oneshot::channel::<Result<String, ParakeetError>>();
        let handle = Arc::new(ParakeetStreamHandle {
            chunk_tx,
            terminal_tx,
            stream_finalizing: test_stream_finalizing(),
            final_rx: tokio::sync::Mutex::new(Some(final_rx)),
        });

        let drain_terminal = tokio::spawn(async move {
            terminal_rx.recv().await;
        });

        let worker = handle.clone();
        let finalize_task = tokio::spawn(async move { worker.finalize().await });
        let _ = final_tx.send(Ok("once".to_string()));
        assert_eq!(finalize_task.await.unwrap().unwrap(), "once");
        let _ = drain_terminal.await;

        assert!(matches!(
            handle.finalize().await,
            Err(ParakeetError::SidecarError { code, .. }) if code == "stream_already_finalized"
        ));
    }

    #[test]
    fn parse_response_line_accepts_diarization_events() {
        let raw = r#"{"type":"diarization","segments":[{"speakerId":"speaker_1","start":0.5,"end":2.25}]}"#;
        let response = parse_response_line(raw).expect("expected valid response");

        match response {
            ParakeetResponse::Diarization { segments } => {
                assert_eq!(segments.len(), 1);
                assert_eq!(segments[0].speaker_id, "speaker_1");
                assert!((segments[0].start - 0.5).abs() < f32::EPSILON);
                assert!((segments[0].end - 2.25).abs() < f32::EPSILON);
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[test]
    fn parse_response_line_rejects_non_json_output() {
        let err = parse_response_line("definitely not json").expect_err("expected parse failure");
        assert!(matches!(err, ParakeetError::InvalidResponse));
    }

    #[test]
    fn parse_response_line_accepts_transcription() {
        let raw = r#"{"type":"transcription","text":"Hello world","segments":[],"language":"en","duration":1.25}"#;
        let response = parse_response_line(raw).expect("expected valid transcription response");
        match response {
            ParakeetResponse::Transcription { text, duration, .. } => {
                assert_eq!(text, "Hello world");
                assert!((duration.unwrap() - 1.25_f32).abs() < 1e-4);
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[test]
    fn parse_response_line_rejects_banner_line() {
        let err = parse_response_line("🔄 LOAD MODEL REQUEST")
            .expect_err("expected parse failure for banner");
        assert!(matches!(err, ParakeetError::InvalidResponse));
    }

    #[test]
    fn parse_response_line_recovers_transcription_from_noisy_line() {
        let raw = r#"🔄 LOAD MODEL REQUEST {"type":"transcription","text":"Noisy","segments":[]}"#;
        let response =
            parse_response_line(raw).expect("expected recovery via extract_json_payload");
        match response {
            ParakeetResponse::Transcription { text, .. } => {
                assert_eq!(text, "Noisy");
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }
}
