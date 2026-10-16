#![cfg_attr(not(target_os = "windows"), allow(dead_code, unused_imports))]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use super::transcriber::{WhisperTranscriptionOutput, WhisperTranscriptionTimings};
use crate::transcription::TranscriptionSegment;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const CONTROL_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
pub(crate) const SIDECAR_ABORT_ERROR: &str = "Vulkan sidecar request aborted";
const DEFAULT_TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(180);
const MIN_TRANSCRIPTION_TIMEOUT_SECS: u64 = 180;
const MAX_TRANSCRIPTION_TIMEOUT_SECS: u64 = 30 * 60;

#[cfg(target_os = "windows")]
const SIDECAR_CANDIDATES: &[&str] = &[
    "whisper-vulkan-sidecar.exe",
    "whisper-vulkan-sidecar-x86_64-pc-windows-msvc.exe",
];

#[cfg(not(target_os = "windows"))]
const SIDECAR_CANDIDATES: &[&str] = &["whisper-vulkan-sidecar"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GpuDiagnostic {
    code: &'static str,
    action: &'static str,
    message: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct AccelerationRuntimeStatus {
    pub mode: String,
    pub effective_backend: String,
    pub gpu_available: Option<bool>,
    pub message: String,
    pub last_error: Option<String>,
    pub diagnostic_code: String,
    pub recommended_action: String,
}

impl Default for AccelerationRuntimeStatus {
    fn default() -> Self {
        Self {
            mode: "auto".to_string(),
            effective_backend: "unknown".to_string(),
            gpu_available: None,
            message: "GPU acceleration has not been tested yet.".to_string(),
            last_error: None,
            diagnostic_code: "not_tested".to_string(),
            recommended_action: "none".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SidecarRequest<'a> {
    Probe {
        id: u64,
        model_path: &'a str,
    },
    Transcribe {
        id: u64,
        model_path: &'a str,
        audio_path: &'a str,
        language: Option<&'a str>,
        translate: bool,
        initial_prompt: Option<&'a str>,
    },
}

impl SidecarRequest<'_> {
    fn id(&self) -> u64 {
        match self {
            Self::Probe { id, .. } | Self::Transcribe { id, .. } => *id,
        }
    }

    fn response_timeout(&self) -> Duration {
        match self {
            Self::Probe { .. } => CONTROL_REQUEST_TIMEOUT,
            Self::Transcribe { audio_path, .. } => transcription_timeout(Path::new(audio_path)),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SidecarSegment {
    start: f64,
    end: f64,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SidecarResponse {
    Health {
        id: u64,
        ok: bool,
        #[allow(dead_code)]
        backend: String,
    },
    Probe {
        id: u64,
        ok: bool,
        backend: String,
        load_time_ms: u64,
    },
    Transcription {
        id: u64,
        ok: bool,
        backend: String,
        text: String,
        transcript_language: Option<String>,
        segments: Vec<SidecarSegment>,
        audio_duration_ms: u64,
        processing_duration_ms: u64,
    },
    Shutdown {
        id: u64,
        ok: bool,
        #[allow(dead_code)]
        backend: String,
    },
    Error {
        id: u64,
        ok: bool,
        code: String,
        message: String,
    },
}

impl SidecarResponse {
    fn id(&self) -> u64 {
        match self {
            Self::Health { id, .. }
            | Self::Probe { id, .. }
            | Self::Transcription { id, .. }
            | Self::Shutdown { id, .. }
            | Self::Error { id, .. } => *id,
        }
    }

    fn ok(&self) -> bool {
        match self {
            Self::Health { ok, .. }
            | Self::Probe { ok, .. }
            | Self::Transcription { ok, .. }
            | Self::Shutdown { ok, .. }
            | Self::Error { ok, .. } => *ok,
        }
    }
}

struct GpuSidecarProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
}

impl GpuSidecarProcess {
    async fn spawn(app: &AppHandle) -> Result<Self, String> {
        let path = resolve_sidecar_binary(app)?;
        ensure_vulkan_loader_available()?;
        log::info!("Spawning Whisper Vulkan sidecar: {}", path.display());

        let mut command = Command::new(&path);
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        #[cfg(target_os = "windows")]
        {
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|err| format!("failed to spawn Vulkan sidecar: {err}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Vulkan sidecar stdin was not captured".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Vulkan sidecar stdout was not captured".to_string())?;

        if let Some(stderr) = child.stderr.take() {
            tauri::async_runtime::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::warn!("Whisper Vulkan sidecar stderr: {}", line);
                }
            });
        }

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
        })
    }

    async fn request(&mut self, request: &SidecarRequest<'_>) -> Result<SidecarResponse, String> {
        let mut payload = serde_json::to_vec(request)
            .map_err(|err| format!("failed to encode Vulkan sidecar request: {err}"))?;
        payload.push(b'\n');

        self.stdin
            .write_all(&payload)
            .await
            .map_err(|err| format!("failed to write Vulkan sidecar request: {err}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|err| format!("failed to flush Vulkan sidecar request: {err}"))?;

        let line = tokio::time::timeout(request.response_timeout(), self.stdout.next_line())
            .await
            .map_err(|_| "Vulkan sidecar request timed out".to_string())?
            .map_err(|err| format!("failed to read Vulkan sidecar response: {err}"))?
            .ok_or_else(|| "Vulkan sidecar exited before responding".to_string())?;
        serde_json::from_str::<SidecarResponse>(&line).map_err(|err| {
            format!(
                "failed to parse Vulkan sidecar response: {err}; response_bytes={}",
                line.len()
            )
        })
    }

    async fn kill_and_wait(&mut self) {
        if let Err(err) = self.child.start_kill() {
            log::debug!("Failed to kill Whisper Vulkan sidecar: {err}");
            return;
        }

        match tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await {
            Ok(Ok(status)) => log::debug!("Whisper Vulkan sidecar exited after kill: {status}"),
            Ok(Err(err)) => log::debug!("Failed waiting for Whisper Vulkan sidecar exit: {err}"),
            Err(_) => log::warn!("Timed out waiting for Whisper Vulkan sidecar to exit after kill"),
        }
    }
}

impl Drop for GpuSidecarProcess {
    fn drop(&mut self) {
        if let Err(err) = self.child.start_kill() {
            log::debug!("Failed to kill Whisper Vulkan sidecar on drop: {err}");
        }
    }
}

pub struct GpuTranscribeRequest<'a> {
    pub model_path: &'a Path,
    pub audio_path: &'a Path,
    pub language: Option<&'a str>,
    pub translate: bool,
    pub initial_prompt: Option<&'a str>,
    pub mode: &'a str,
}

pub struct GpuSidecarClient {
    next_id: AtomicU64,
    process: AsyncMutex<Option<GpuSidecarProcess>>,
    status: AsyncRwLock<AccelerationRuntimeStatus>,
    abort_requested: AtomicBool,
    abort_notify: tokio::sync::Notify,
}

impl Default for GpuSidecarClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuSidecarClient {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            process: AsyncMutex::new(None),
            status: AsyncRwLock::new(AccelerationRuntimeStatus::default()),
            abort_requested: AtomicBool::new(false),
            abort_notify: tokio::sync::Notify::new(),
        }
    }

    pub async fn status(&self) -> AccelerationRuntimeStatus {
        self.status.read().await.clone()
    }

    /// Release cached weights for another local engine without interrupting an
    /// in-flight transcription: requests own this lock for their full lifetime.
    pub(crate) async fn unload_if_idle(&self) {
        if let Ok(mut guard) = self.process.try_lock() {
            if let Some(mut process) = guard.take() {
                let _ = tokio::time::timeout(Duration::from_secs(2), process.kill_and_wait()).await;
            }
        }
    }

    pub async fn abort_active_process(&self) {
        self.abort_requested.store(true, Ordering::SeqCst);
        self.abort_notify.notify_waiters();

        if let Ok(mut guard) =
            tokio::time::timeout(Duration::from_millis(250), self.process.lock()).await
        {
            if let Some(process) = guard.as_mut() {
                log::info!("Aborting active Whisper Vulkan sidecar process");
                process.kill_and_wait().await;
            }
            guard.take();
            self.abort_requested.store(false, Ordering::SeqCst);
        } else {
            log::warn!(
                "abort_active_process: timed out acquiring process lock after 250ms; \
                 GPU sidecar may remain resident (CPU fallback could double-load the model)"
            );
        }
    }

    pub async fn set_cpu_status(&self, mode: &str, message: impl Into<String>) {
        *self.status.write().await = AccelerationRuntimeStatus {
            mode: mode.to_string(),
            effective_backend: "cpu".to_string(),
            gpu_available: None,
            message: message.into(),
            last_error: None,
            diagnostic_code: "cpu_selected".to_string(),
            recommended_action: "none".to_string(),
        };
    }

    /// Best-effort warm of the Vulkan sidecar and model during Whisper preload.
    /// Returns true only when the sidecar accepted the model and kept it warm.
    pub async fn warm_on_preload(
        &self,
        app: &AppHandle,
        model_path: &Path,
        mode: &str,
        gpu_available: Option<bool>,
    ) -> bool {
        if !should_attempt_vulkan_warm_on_preload(mode, gpu_available) {
            log::debug!(
                "Skipping Vulkan sidecar warm on preload (mode={mode}, gpu_available={gpu_available:?})"
            );
            return false;
        }

        log::info!(
            "Warming Whisper Vulkan sidecar for preloaded model: {}",
            model_path.display()
        );
        match self.probe(app, model_path, mode).await {
            Ok(()) => {
                log::info!("Whisper Vulkan sidecar warmed successfully on preload");
                true
            }
            Err(error) => {
                log::warn!(
                    "Whisper Vulkan sidecar warm on preload failed; unloading sidecar before CPU preload: {error}"
                );
                self.abort_active_process().await;
                false
            }
        }
    }

    pub async fn probe(
        &self,
        app: &AppHandle,
        model_path: &Path,
        mode: &str,
    ) -> Result<(), String> {
        let model_path = model_path
            .to_str()
            .ok_or_else(|| format!("Model path contains invalid UTF-8: {:?}", model_path))?;
        let id = self.next_request_id();
        match self
            .send(app, &SidecarRequest::Probe { id, model_path })
            .await
        {
            Ok(SidecarResponse::Probe {
                ok: true,
                backend,
                load_time_ms,
                ..
            }) => {
                *self.status.write().await = AccelerationRuntimeStatus {
                    mode: mode.to_string(),
                    effective_backend: backend,
                    gpu_available: Some(true),
                    message: format!(
                        "GPU acceleration is available (model loaded in {load_time_ms}ms)."
                    ),
                    last_error: None,
                    diagnostic_code: "ready".to_string(),
                    recommended_action: "none".to_string(),
                };
                Ok(())
            }
            Ok(SidecarResponse::Error { code, message, .. }) => {
                let error = format!("{code}: {message}");
                self.record_gpu_error(mode, &error).await;
                Err(error)
            }
            Ok(other) => {
                let error = format!("unexpected Vulkan probe response: {other:?}");
                self.record_gpu_error(mode, &error).await;
                Err(error)
            }
            Err(error) => {
                self.record_gpu_error(mode, &error).await;
                Err(error)
            }
        }
    }

    pub async fn transcribe(
        &self,
        app: &AppHandle,
        request: GpuTranscribeRequest<'_>,
    ) -> Result<WhisperTranscriptionOutput, String> {
        let model_path = request.model_path.to_str().ok_or_else(|| {
            format!(
                "Model path contains invalid UTF-8: {:?}",
                request.model_path
            )
        })?;
        let audio_path = request.audio_path.to_str().ok_or_else(|| {
            format!(
                "Audio path contains invalid UTF-8: {:?}",
                request.audio_path
            )
        })?;
        let id = self.next_request_id();
        let mode = request.mode;

        match self
            .send(
                app,
                &SidecarRequest::Transcribe {
                    id,
                    model_path,
                    audio_path,
                    language: request.language,
                    translate: request.translate,
                    initial_prompt: request.initial_prompt,
                },
            )
            .await
        {
            Ok(SidecarResponse::Transcription {
                ok: true,
                backend,
                text,
                transcript_language,
                segments,
                audio_duration_ms,
                processing_duration_ms,
                ..
            }) => {
                *self.status.write().await = AccelerationRuntimeStatus {
                    mode: mode.to_string(),
                    effective_backend: backend,
                    gpu_available: Some(true),
                    message: format!(
                        "Last transcription used GPU acceleration ({processing_duration_ms}ms)."
                    ),
                    last_error: None,
                    diagnostic_code: "ready".to_string(),
                    recommended_action: "none".to_string(),
                };
                Ok(WhisperTranscriptionOutput {
                    raw_text: text,
                    transcript_language,
                    segments: sidecar_segments_to_transcription_segments(segments),
                    audio_duration_ms,
                    processing_duration_ms,
                    timings: WhisperTranscriptionTimings {
                        preprocessing_ms: 0,
                        inference_ms: processing_duration_ms,
                        extraction_ms: 0,
                        total_ms: processing_duration_ms,
                    },
                })
            }
            Ok(SidecarResponse::Error { code, message, .. }) => {
                let error = format!("{code}: {message}");
                if code == "context_failed" {
                    self.record_gpu_error(mode, &error).await;
                }
                Err(error)
            }
            Ok(other) => {
                let error = format!("unexpected Vulkan transcription response: {other:?}");
                self.record_gpu_error(mode, &error).await;
                Err(error)
            }
            Err(error) => {
                // An intentional abort (user cancel / watchdog) is not a GPU fault —
                // recording it would wrongly mark GPU unavailable for later recordings.
                if error != SIDECAR_ABORT_ERROR {
                    self.record_gpu_error(mode, &error).await;
                }
                Err(error)
            }
        }
    }

    async fn send(
        &self,
        app: &AppHandle,
        request: &SidecarRequest<'_>,
    ) -> Result<SidecarResponse, String> {
        let mut guard = self.process.lock().await;
        self.abort_requested.store(false, Ordering::SeqCst);
        if guard.is_none() {
            guard.replace(GpuSidecarProcess::spawn(app).await?);
        }

        let result = match guard.as_mut() {
            Some(process) => {
                let abort_notified = self.abort_notify.notified();
                tokio::pin!(abort_notified);
                if self.abort_requested.load(Ordering::SeqCst) {
                    Err(SIDECAR_ABORT_ERROR.to_string())
                } else {
                    tokio::select! {
                        biased;
                        _ = &mut abort_notified => Err(SIDECAR_ABORT_ERROR.to_string()),
                        res = process.request(request) => res,
                    }
                }
            }
            None => Err("Vulkan sidecar was not started".to_string()),
        };

        let response = match result {
            Ok(response) => {
                if self.abort_requested.swap(false, Ordering::SeqCst) {
                    if let Some(process) = guard.as_mut() {
                        process.kill_and_wait().await;
                    }
                    guard.take();
                    return Err(SIDECAR_ABORT_ERROR.to_string());
                }
                response
            }
            Err(error) => {
                if let Some(process) = guard.as_mut() {
                    process.kill_and_wait().await;
                }
                guard.take();
                if error == SIDECAR_ABORT_ERROR {
                    self.abort_requested.store(false, Ordering::SeqCst);
                }
                return Err(error);
            }
        };

        if response.id() != request.id() {
            if let Some(process) = guard.as_mut() {
                process.kill_and_wait().await;
            }
            guard.take();
            return Err(format!(
                "Vulkan sidecar response id mismatch: expected {}, got {}",
                request.id(),
                response.id()
            ));
        }

        if !response.ok() {
            return Ok(response);
        }
        Ok(response)
    }

    async fn record_gpu_error(&self, mode: &str, error: &str) {
        let diagnostic = classify_gpu_error(error);
        *self.status.write().await = AccelerationRuntimeStatus {
            mode: mode.to_string(),
            effective_backend: "cpu".to_string(),
            gpu_available: Some(false),
            message: diagnostic.message.to_string(),
            last_error: Some(error.to_string()),
            diagnostic_code: diagnostic.code.to_string(),
            recommended_action: diagnostic.action.to_string(),
        };
    }

    fn next_request_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    #[cfg(test)]
    fn abort_pending(&self) -> bool {
        self.abort_requested.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    fn abort_notified_for_test(&self) -> impl std::future::Future<Output = ()> + '_ {
        self.abort_notify.notified()
    }
}

fn sidecar_segments_to_transcription_segments(
    segments: Vec<SidecarSegment>,
) -> Vec<TranscriptionSegment> {
    segments
        .into_iter()
        .map(|segment| TranscriptionSegment {
            text: segment.text,
            start_ms: seconds_to_duration_ms(segment.start),
            end_ms: seconds_to_duration_ms(segment.end),
            speaker_id: None,
        })
        .collect()
}

fn seconds_to_duration_ms(seconds: f64) -> Option<u64> {
    if !seconds.is_finite() || seconds < 0.0 {
        None
    } else {
        Some((seconds * 1000.0) as u64)
    }
}

/// Whether preload should spawn/probe the Vulkan sidecar for the given acceleration mode.
pub(crate) fn should_attempt_vulkan_warm_on_preload(
    mode: &str,
    gpu_available: Option<bool>,
) -> bool {
    if mode == "cpu" {
        return false;
    }
    if mode == "auto" && gpu_available == Some(false) {
        return false;
    }
    true
}

fn classify_gpu_error(error: &str) -> GpuDiagnostic {
    let normalized = error.to_ascii_lowercase();
    let mentions_vulkan = normalized.contains("vulkan")
        || normalized.contains("vkcreateinstance")
        || normalized.contains("context_failed")
        || normalized.contains("probe_failed");

    if normalized.contains("select or download")
        || normalized.contains("download a local whisper model")
        || normalized.contains("model path contains invalid")
    {
        return GpuDiagnostic {
            code: "model_missing",
            action: "download_model",
            message: "Select or download a local Whisper model before testing GPU acceleration.",
        };
    }

    if normalized.contains("vulkan loader missing")
        || (normalized.contains("vulkan-1.dll") && normalized.contains("missing"))
    {
        return GpuDiagnostic {
            code: "vulkan_loader_missing",
            action: "update_graphics_driver",
            message: "Vulkan is not installed. Install or update your NVIDIA, AMD, or Intel graphics driver, then try GPU acceleration again.",
        };
    }

    if normalized.contains("whisper vulkan sidecar not found")
        || normalized.contains("sidecar was not started")
        || normalized.contains("stdin was not captured")
        || normalized.contains("stdout was not captured")
    {
        return GpuDiagnostic {
            code: "sidecar_missing",
            action: "report_bug",
            message: "The bundled Vulkan helper is missing or broken. Reinstall Voicetypr, then report a bug if this continues.",
        };
    }

    if normalized.contains("failed to parse")
        || normalized.contains("response id mismatch")
        || normalized.contains("unexpected vulkan")
        || normalized.contains("failed to encode")
    {
        return GpuDiagnostic {
            code: "sidecar_protocol_error",
            action: "report_bug",
            message: "The Vulkan helper returned an unexpected response. Voicetypr is using CPU mode; please report this bug.",
        };
    }

    if normalized.contains("timed out") {
        return GpuDiagnostic {
            code: "sidecar_timeout",
            action: "use_cpu",
            message: "The Vulkan helper did not respond in time. Voicetypr is using CPU mode.",
        };
    }

    if normalized.contains("no vulkan device")
        || normalized.contains("no suitable vulkan device")
        || (mentions_vulkan && normalized.contains("device init"))
    {
        return GpuDiagnostic {
            code: "vulkan_device_missing",
            action: "update_graphics_driver",
            message: "No compatible Vulkan GPU was found. Install or update your NVIDIA, AMD, or Intel graphics driver, then try GPU acceleration again.",
        };
    }

    if normalized.contains("vkcreateinstance")
        || normalized.contains("context_failed")
        || normalized.contains("probe_failed")
        || (mentions_vulkan
            && (normalized.contains("loader")
                || normalized.contains("vulkan-1")
                || normalized.contains("backend init")
                || normalized.contains("failed to spawn")
                || normalized.contains("failed to read")
                || normalized.contains("failed to write")
                || normalized.contains("failed to flush")
                || normalized.contains("exited before responding")))
    {
        return GpuDiagnostic {
            code: "driver_or_runtime_failed",
            action: "update_graphics_driver",
            message: "Vulkan GPU acceleration failed to start. Install or update your NVIDIA, AMD, or Intel graphics driver. Voicetypr is using CPU mode.",
        };
    }

    GpuDiagnostic {
        code: "gpu_failed",
        action: "use_cpu",
        message: "GPU acceleration failed. Voicetypr is using CPU mode. Retry GPU after updating your graphics driver, or report this with logs if it continues.",
    }
}

#[cfg(target_os = "windows")]
fn ensure_vulkan_loader_available() -> Result<(), String> {
    use windows::core::w;
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::LoadLibraryW;

    let module = unsafe { LoadLibraryW(w!("vulkan-1.dll")) };
    let Ok(module) = module else {
        return Err("Vulkan loader missing: install or update your NVIDIA, AMD, or Intel graphics driver so vulkan-1.dll is available.".to_string());
    };

    unsafe {
        let _ = FreeLibrary(module);
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn ensure_vulkan_loader_available() -> Result<(), String> {
    Ok(())
}

fn transcription_timeout(audio_path: &Path) -> Duration {
    wav_duration_seconds(audio_path)
        .map(transcription_timeout_for_duration)
        .unwrap_or(DEFAULT_TRANSCRIPTION_TIMEOUT)
}

fn transcription_timeout_for_duration(duration_secs: f64) -> Duration {
    if !duration_secs.is_finite() || duration_secs <= 0.0 {
        return DEFAULT_TRANSCRIPTION_TIMEOUT;
    }

    let timeout_secs = (duration_secs.ceil() as u64)
        .saturating_mul(4)
        .saturating_add(60)
        .clamp(
            MIN_TRANSCRIPTION_TIMEOUT_SECS,
            MAX_TRANSCRIPTION_TIMEOUT_SECS,
        );
    Duration::from_secs(timeout_secs)
}

fn wav_duration_seconds(audio_path: &Path) -> Option<f64> {
    let reader = hound::WavReader::open(audio_path).ok()?;
    let spec = reader.spec();
    if spec.sample_rate == 0 || spec.channels == 0 {
        return None;
    }

    let frames = reader.duration() as f64;
    Some(frames / f64::from(spec.sample_rate))
}

fn resolve_sidecar_binary(app: &AppHandle) -> Result<PathBuf, String> {
    let search_dirs = sidecar_search_dirs(
        app.path().resource_dir().ok(),
        std::env::current_exe().ok(),
        dev_sidecar_cwd(),
    );
    let mut tried = Vec::new();

    for dir in &search_dirs {
        for name in SIDECAR_CANDIDATES {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
            tried.push(candidate);
        }
    }

    let searched = tried
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "Whisper Vulkan sidecar not found. Searched: {searched}"
    ))
}

fn sidecar_search_dirs(
    resource_dir: Option<PathBuf>,
    exe_path: Option<PathBuf>,
    dev_cwd: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(resource_dir) = resource_dir {
        push_sidecar_dir(&mut dirs, resource_dir);
    }

    if let Some(exe_dir) = exe_path.and_then(|path| path.parent().map(Path::to_path_buf)) {
        push_unique_dir(&mut dirs, exe_dir);
    }

    if let Some(cwd) = dev_cwd {
        push_sidecar_dir(&mut dirs, cwd.clone());
        push_sidecar_dir(&mut dirs, cwd.join(".."));
    }

    dirs
}

fn push_sidecar_dir(dirs: &mut Vec<PathBuf>, base: PathBuf) {
    push_unique_dir(dirs, base.clone());
    push_unique_dir(
        dirs,
        base.join("sidecar").join("whisper-vulkan").join("dist"),
    );
}

fn push_unique_dir(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dirs.iter().any(|existing| existing == &dir) {
        dirs.push(dir);
    }
}

#[cfg(debug_assertions)]
fn dev_sidecar_cwd() -> Option<PathBuf> {
    std::env::current_dir().ok()
}

#[cfg(not(debug_assertions))]
fn dev_sidecar_cwd() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::{
        classify_gpu_error, seconds_to_duration_ms, should_attempt_vulkan_warm_on_preload,
        sidecar_search_dirs, sidecar_segments_to_transcription_segments,
        transcription_timeout_for_duration, GpuSidecarClient, SidecarRequest, SidecarResponse,
        SidecarSegment, CONTROL_REQUEST_TIMEOUT, DEFAULT_TRANSCRIPTION_TIMEOUT,
        MAX_TRANSCRIPTION_TIMEOUT_SECS, MIN_TRANSCRIPTION_TIMEOUT_SECS,
    };
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn abort_sets_flag_and_is_consumed() {
        let client = GpuSidecarClient::new();

        tokio::time::timeout(Duration::from_secs(1), client.abort_active_process())
            .await
            .expect("idle abort should not wait for the full timeout");

        assert!(!client.abort_pending());
    }

    #[tokio::test]
    async fn notify_wakes_waiter() {
        let client = Arc::new(GpuSidecarClient::new());
        let waiter_client = Arc::clone(&client);
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let waiter = tokio::spawn(async move {
            let notified = waiter_client.abort_notified_for_test();
            ready_tx
                .send(())
                .expect("waiter readiness signal should send");
            notified.await;
        });

        ready_rx
            .await
            .expect("waiter readiness signal should be received");
        client.abort_active_process().await;

        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("abort notify should wake waiter")
            .expect("waiter task should complete");
    }

    #[test]
    fn sidecar_search_dirs_do_not_traverse_parent_directories() {
        let dirs = sidecar_search_dirs(
            Some(PathBuf::from("/opt/Voicetypr/resources")),
            Some(PathBuf::from("/opt/Voicetypr/voicetypr.exe")),
            None,
        );

        assert!(dirs.contains(&PathBuf::from("/opt/Voicetypr/resources")));
        assert!(dirs.contains(&PathBuf::from(
            "/opt/Voicetypr/resources/sidecar/whisper-vulkan/dist"
        )));
        assert!(dirs.contains(&PathBuf::from("/opt/Voicetypr")));
        assert!(!dirs.contains(&PathBuf::from("/opt")));
        assert!(!dirs.contains(&PathBuf::from("/")));
    }

    #[test]
    fn transcribe_requests_use_duration_based_timeout() {
        let probe = SidecarRequest::Probe {
            id: 1,
            model_path: "model.bin",
        };
        let transcribe = SidecarRequest::Transcribe {
            id: 2,
            model_path: "model.bin",
            audio_path: "missing-audio.wav",
            language: None,
            translate: false,
            initial_prompt: None,
        };

        assert_eq!(probe.response_timeout(), CONTROL_REQUEST_TIMEOUT);
        assert_eq!(transcribe.response_timeout(), DEFAULT_TRANSCRIPTION_TIMEOUT);
    }

    #[test]
    fn transcription_timeout_scales_with_audio_duration_and_stays_bounded() {
        assert_eq!(
            transcription_timeout_for_duration(1.0),
            std::time::Duration::from_secs(MIN_TRANSCRIPTION_TIMEOUT_SECS)
        );
        assert_eq!(
            transcription_timeout_for_duration(600.0),
            std::time::Duration::from_secs(MAX_TRANSCRIPTION_TIMEOUT_SECS)
        );
    }

    #[test]
    fn should_attempt_vulkan_warm_on_preload_respects_mode_and_prior_failure() {
        assert!(!should_attempt_vulkan_warm_on_preload("cpu", None));
        assert!(!should_attempt_vulkan_warm_on_preload("cpu", Some(true)));
        assert!(should_attempt_vulkan_warm_on_preload("gpu", None));
        assert!(should_attempt_vulkan_warm_on_preload("gpu", Some(false)));
        assert!(should_attempt_vulkan_warm_on_preload("auto", None));
        assert!(should_attempt_vulkan_warm_on_preload("auto", Some(true)));
        assert!(!should_attempt_vulkan_warm_on_preload("auto", Some(false)));
    }

    #[test]
    fn gpu_error_classifier_reports_missing_vulkan_loader() {
        let diagnostic = classify_gpu_error(
            "Vulkan loader missing: install or update your NVIDIA, AMD, or Intel graphics driver so vulkan-1.dll is available.",
        );

        assert_eq!(diagnostic.code, "vulkan_loader_missing");
        assert_eq!(diagnostic.action, "update_graphics_driver");
        assert!(diagnostic.message.contains("NVIDIA, AMD, or Intel"));
    }

    #[test]
    fn gpu_error_classifier_separates_packaging_from_protocol_errors() {
        let missing_sidecar =
            classify_gpu_error("Whisper Vulkan sidecar not found. Searched: C:\\app\\missing.exe");
        assert_eq!(missing_sidecar.code, "sidecar_missing");
        assert_eq!(missing_sidecar.action, "report_bug");

        let protocol = classify_gpu_error(
            "failed to parse Vulkan sidecar response: expected value; response_bytes=4",
        );
        assert_eq!(protocol.code, "sidecar_protocol_error");
        assert_eq!(protocol.action, "report_bug");
    }

    #[test]
    fn gpu_error_classifier_maps_runtime_failures_to_driver_guidance() {
        let no_device = classify_gpu_error("probe_failed: no Vulkan device available");
        assert_eq!(no_device.code, "vulkan_device_missing");
        assert_eq!(no_device.action, "update_graphics_driver");

        let init_failed = classify_gpu_error("context_failed: vkCreateInstance returned -9");
        assert_eq!(init_failed.code, "driver_or_runtime_failed");
        assert_eq!(init_failed.action, "update_graphics_driver");

        let timeout = classify_gpu_error("Vulkan sidecar request timed out");

        let unknown_io = classify_gpu_error("failed to read model header");
        assert_eq!(unknown_io.code, "gpu_failed");
        assert_eq!(unknown_io.action, "use_cpu");
        assert_eq!(timeout.code, "sidecar_timeout");
        assert_eq!(timeout.action, "use_cpu");
    }

    #[test]
    fn sidecar_timing_responses_deserialize_from_json_numbers() {
        let probe = serde_json::from_str::<SidecarResponse>(
            r#"{"type":"probe","id":7,"ok":true,"backend":"vulkan","load_time_ms":123}"#,
        )
        .expect("probe response should parse");
        assert!(matches!(
            probe,
            SidecarResponse::Probe {
                id: 7,
                load_time_ms: 123,
                ..
            }
        ));

        let transcription = serde_json::from_str::<SidecarResponse>(
            r#"{"type":"transcription","id":8,"ok":true,"backend":"vulkan","text":"hello","transcript_language":"en","segments":[{"start":0.0,"end":1.5,"text":"hello"}],"audio_duration_ms":1500,"processing_duration_ms":456}"#,
        )
        .expect("transcription response should parse");
        assert!(matches!(
            transcription,
            SidecarResponse::Transcription {
                id: 8,
                processing_duration_ms: 456,
                audio_duration_ms: 1500,
                ..
            }
        ));
    }

    #[test]
    fn sidecar_segments_map_to_transcription_segments_without_speaker_id() {
        let segments = sidecar_segments_to_transcription_segments(vec![SidecarSegment {
            start: 0.5,
            end: 2.25,
            text: "hello".to_string(),
        }]);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "hello");
        assert_eq!(segments[0].start_ms, Some(500));
        assert_eq!(segments[0].end_ms, Some(2250));
        assert_eq!(segments[0].speaker_id, None);
    }

    #[test]
    fn seconds_to_duration_ms_rejects_invalid_values() {
        assert_eq!(seconds_to_duration_ms(-1.0), None);
        assert_eq!(seconds_to_duration_ms(f64::NAN), None);
        assert_eq!(seconds_to_duration_ms(0.0), Some(0));
    }
}
