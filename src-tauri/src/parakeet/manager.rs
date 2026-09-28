#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
use std::time::Duration;
use std::time::Instant;

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
use log::trace;
use log::warn;
use reqwest::Client;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::error::ParakeetError;
use super::messages::{
    ParakeetCommand, ParakeetResponse, ParakeetStreamConfig, ParakeetStreamEngine,
    ParakeetVocabularyTerm,
};
#[cfg(target_os = "macos")]
use super::models::get_available_models;
use super::models::{ParakeetModelDefinition, ParakeetModelKind};
use super::sidecar::{
    ParakeetClient, ParakeetStreamHandle, ParakeetStreamOpenRequest, ParakeetStreamPartial,
};
#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
use crate::utils::logger::log_performance;

#[derive(Debug, Clone, Serialize)]
pub struct ParakeetModelStatus {
    pub name: String,
    pub display_name: String,
    pub size: u64,
    pub url: String,
    pub sha256: String,
    pub downloaded: bool,
    pub speed_score: u8,
    pub accuracy_score: u8,
    pub recommended: bool,
    pub engine: String,
    pub supported_languages: Vec<String>,
    pub runtime: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParakeetEouModelStatus {
    pub chunk_ms: u16,
    pub downloaded: bool,
    pub path: Option<String>,
}

const EOU_MODEL_SIZE_BYTES: u64 = 250 * 1024 * 1024;

#[derive(Debug, Clone)]
// The Windows ONNX backend reads only the cancel flag (no language hint,
// translation or CTC vocabulary).
#[cfg_attr(all(target_os = "windows", target_arch = "x86_64"), allow(dead_code))]
pub struct ParakeetTranscriptionOptions {
    pub language: Option<String>,
    pub translate: bool,
    pub custom_vocabulary: Vec<ParakeetVocabularyTerm>,
    pub cancel_flag: Option<Arc<AtomicBool>>,
}

impl ParakeetTranscriptionOptions {
    pub fn new(
        language: Option<String>,
        translate: bool,
        cancel_flag: Option<Arc<AtomicBool>>,
    ) -> Self {
        Self {
            language,
            translate,
            custom_vocabulary: Vec::new(),
            cancel_flag,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ParakeetVocabularyStatus {
    pub supported: bool,
    pub ready: bool,
}

pub struct ParakeetManager {
    client: ParakeetClient,
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    onnx: super::onnx::session::OnnxSession,
    root_dir: PathBuf,
    last_model_load_ms: AtomicU64,
    last_inference_ms: AtomicU64,
    last_warmup_ms: AtomicU64,
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    real_transcription_active: AtomicBool,
    #[allow(dead_code)]
    http: Client,
}

#[cfg_attr(
    not(all(target_os = "windows", target_arch = "x86_64")),
    allow(dead_code)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreloadOutcome {
    Loaded,
    Superseded,
}

impl PreloadOutcome {
    #[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
    fn for_selection(engine: Option<&str>, model: Option<&str>, expected_model: &str) -> Self {
        if super::onnx::selected_model_matches(engine, model, "parakeet", expected_model) {
            Self::Loaded
        } else {
            Self::Superseded
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ParakeetTimingSnapshot {
    pub model_load_ms: u64,
    pub inference_ms: u64,
    pub warmup_ms: u64,
    pub total_ms: u64,
}

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
struct TranscriptionActiveGuard<'a>(&'a AtomicBool);

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
impl Drop for TranscriptionActiveGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

const PARAKEET_UNAVAILABLE_EVENT: &str = "parakeet-unavailable";

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
fn fluid_audio_model_dir(home: &Path, definition: &ParakeetModelDefinition) -> PathBuf {
    home.join("Library/Application Support/FluidAudio/Models")
        .join(definition.cache_subdir)
}

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
fn model_files_complete(model_dir: &Path, definition: &ParakeetModelDefinition) -> bool {
    definition.files.iter().all(|file| {
        let path = model_dir.join(file.filename);
        path.exists()
    })
}

pub struct ParakeetStreamRequest<'a> {
    pub app: AppHandle,
    pub model_name: &'a str,
    pub language: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub engine: ParakeetStreamEngine,
    pub chunk_ms: Option<u16>,
    pub config: Option<ParakeetStreamConfig>,
}

impl ParakeetManager {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            client: ParakeetClient::new("parakeet-sidecar"),
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            onnx: super::onnx::session::OnnxSession::new(),
            root_dir,
            last_model_load_ms: AtomicU64::new(0),
            last_inference_ms: AtomicU64::new(0),
            last_warmup_ms: AtomicU64::new(0),
            #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
            real_transcription_active: AtomicBool::new(false),
            http: Client::new(),
        }
    }

    pub fn latest_timing_snapshot(&self) -> ParakeetTimingSnapshot {
        let model_load_ms = self.last_model_load_ms.load(Ordering::Relaxed);
        let inference_ms = self.last_inference_ms.load(Ordering::Relaxed);
        let warmup_ms = self.last_warmup_ms.load(Ordering::Relaxed);
        ParakeetTimingSnapshot {
            model_load_ms,
            inference_ms,
            warmup_ms,
            total_ms: model_load_ms.saturating_add(inference_ms),
        }
    }

    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    fn real_transcription_busy(&self, app: &AppHandle) -> bool {
        if self.real_transcription_active.load(Ordering::SeqCst) {
            return true;
        }

        app.try_state::<crate::AppState>()
            .map(|state| {
                matches!(
                    state.get_current_state(),
                    crate::RecordingState::Starting
                        | crate::RecordingState::Recording
                        | crate::RecordingState::Stopping
                        | crate::RecordingState::Transcribing
                )
            })
            .unwrap_or(false)
    }

    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    fn mark_real_transcription_active(&self) -> TranscriptionActiveGuard<'_> {
        self.real_transcription_active.store(true, Ordering::SeqCst);
        TranscriptionActiveGuard(&self.real_transcription_active)
    }

    fn model_version_for(definition: &ParakeetModelDefinition) -> &'static str {
        match definition.kind {
            ParakeetModelKind::TdtV2 => "v2",
            ParakeetModelKind::TdtV3 => "v3",
            ParakeetModelKind::UnifiedEnglish640 => "unified_640",
            ParakeetModelKind::NemotronMultilingual1120 => "nemotron_multilingual_1120",
        }
    }

    /// Returns available Parakeet models for the current architecture.
    ///
    /// Platform-filtered catalog: CoreML on Apple Silicon, ONNX on Windows x64.
    ///
    /// # Platform Behavior
    /// - **macOS (Apple Silicon)**: Returns all Parakeet models
    /// - **macOS (Intel)**: Returns an empty list (Parakeet requires Apple Silicon)
    /// - **Windows x64**: Returns ONNX TDT v3 only
    /// - **Windows ARM64/Linux**: Returns empty vector
    #[allow(clippy::needless_return)]
    pub fn list_models(&self) -> Vec<ParakeetModelStatus> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            return super::models::catalog_for_platform("windows", "x86_64")
                .into_iter()
                .map(|definition| ParakeetModelStatus {
                    name: definition.id.to_string(),
                    display_name: definition.display_name.to_string(),
                    size: definition.estimated_size,
                    url: format!("https://huggingface.co/{}", definition.repo_id),
                    sha256: String::new(),
                    downloaded: self.is_onnx_downloaded(definition.id),
                    speed_score: definition.speed_score,
                    accuracy_score: definition.accuracy_score,
                    recommended: definition.recommended,
                    engine: "parakeet".to_string(),
                    supported_languages: definition
                        .languages
                        .iter()
                        .map(|s| (*s).to_string())
                        .collect(),
                    runtime: "onnx".to_string(),
                })
                .collect();
        }
        #[cfg(not(any(
            target_os = "macos",
            all(target_os = "windows", target_arch = "x86_64")
        )))]
        {
            return vec![];
        }

        #[cfg(target_os = "macos")]
        {
            // Use get_available_models() which filters out Apple Silicon-only models on Intel
            get_available_models()
                .into_iter()
                .map(|definition| ParakeetModelStatus {
                    name: definition.id.to_string(),
                    display_name: definition.display_name.to_string(),
                    size: definition.estimated_size,
                    url: format!("https://huggingface.co/{}", definition.repo_id),
                    sha256: String::new(),
                    downloaded: self.is_model_downloaded(definition),
                    speed_score: definition.speed_score,
                    accuracy_score: definition.accuracy_score,
                    recommended: definition.recommended,
                    engine: "parakeet".to_string(),
                    supported_languages: definition
                        .languages
                        .iter()
                        .map(|language| (*language).to_string())
                        .collect(),
                    runtime: "coreml".to_string(),
                })
                .collect()
        }
    }

    pub fn get_model_definition(
        &self,
        model_name: &str,
    ) -> Option<&'static ParakeetModelDefinition> {
        super::models::catalog_for_platform(std::env::consts::OS, std::env::consts::ARCH)
            .into_iter()
            .find(|model| model.id == model_name)
    }

    pub async fn open_stream(
        &self,
        request: ParakeetStreamRequest<'_>,
        partial_callback: impl FnMut(ParakeetStreamPartial) + Send + 'static,
    ) -> Result<ParakeetStreamHandle, String> {
        let ParakeetStreamRequest {
            app,
            model_name,
            language,
            sample_rate,
            channels,
            engine,
            chunk_ms,
            config,
        } = request;
        let Some(definition) = self.get_model_definition(model_name) else {
            return Err(format!("Unknown Parakeet model: {model_name}"));
        };
        if !self.is_model_downloaded(definition) {
            return Err(format!("Parakeet model is not downloaded: {model_name}"));
        }

        self.client
            .open_stream(
                ParakeetStreamOpenRequest {
                    app,
                    model_id: definition.id.to_string(),
                    model_version: Some(Self::model_version_for(definition).to_string()),
                    language,
                    sample_rate,
                    channels,
                    engine,
                    chunk_ms,
                    config,
                },
                partial_callback,
            )
            .await
            .map_err(|error| error.to_string())
    }

    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    pub fn model_dir(&self, model_name: &str) -> PathBuf {
        self.root_dir.join(model_name)
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub fn is_onnx_downloaded(&self, model_name: &str) -> bool {
        model_name == super::onnx::download::MODEL_ID
            && super::onnx::download::is_downloaded(
                &super::onnx::download::model_directory(
                    &self
                        .root_dir
                        .parent()
                        .unwrap_or(&self.root_dir)
                        .join("parakeet-onnx"),
                ),
                super::onnx::download::REVISION,
                &super::onnx::download::FILES,
            )
    }

    /// Check if a Parakeet model is available.
    /// FluidAudio stores models in ~/Library/Application Support/FluidAudio/Models/<repo-folder>/.
    #[cfg_attr(
        all(target_os = "windows", target_arch = "x86_64"),
        allow(clippy::needless_return)
    )]
    pub fn is_model_downloaded(&self, definition: &ParakeetModelDefinition) -> bool {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            return self.is_onnx_downloaded(definition.id);
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
        {
            let Some(home) = dirs::home_dir() else {
                return false;
            };

            let fluid_audio_model_path = fluid_audio_model_dir(&home, definition);
            let complete = model_files_complete(&fluid_audio_model_path, definition);

            if complete {
                trace!(
                    "Found complete FluidAudio model at: {:?}",
                    fluid_audio_model_path
                );
            }

            complete
        }
    }

    pub async fn download_model(
        &self,
        app: &AppHandle,
        model_name: &str,
        cancel_flag: Option<Arc<AtomicBool>>,
        mut progress_callback: impl FnMut(u64, u64, Option<String>) + Send + 'static,
    ) -> Result<(), String> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let _ = app;
            if model_name != super::onnx::download::MODEL_ID {
                return Err("This Parakeet model is not available on Windows.".into());
            }
            let cancel = cancel_flag.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
            let root = self
                .root_dir
                .parent()
                .unwrap_or(&self.root_dir)
                .join("parakeet-onnx");
            return super::onnx::download::download_pinned(
                &self.http,
                &root,
                &super::onnx::MODEL_OPERATION_LOCK,
                &super::onnx::LOCAL_MODEL_GATE,
                self.onnx.unload(),
                cancel,
                move |downloaded, total| progress_callback(downloaded, total, None),
            )
            .await;
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
        {
            let Some(definition) = self.get_model_definition(model_name) else {
                return Err(format!("Unknown Parakeet model: {model_name}"));
            };

            // For Swift sidecar, delegate download to FluidAudio
            // Send load_model command which triggers download in Swift
            let version = Self::model_version_for(definition);

            let command = ParakeetCommand::LoadModel {
                model_id: definition.id.to_string(),
                model_version: Some(version.to_string()),
                force_download: Some(true),
                local_path: None,
                cache_dir: None,
                precision: "bf16".to_string(),
                attention: "full".to_string(),
                local_attention_context: 256,
                chunk_duration: Some(120.0),
                overlap_duration: Some(15.0),
                eager_unload: Some(false),
            };

            // Send to sidecar and let it handle the download
            let estimated_size = definition.estimated_size;
            let mut last_downloaded = 0;
            match self
                .send_command_with_progress_and_cancel(
                    app,
                    &command,
                    cancel_flag.clone(),
                    |progress, phase| {
                        let progress = progress.clamp(0.0, 1.0) as f64;
                        let downloaded = (estimated_size as f64 * progress).round() as u64;
                        last_downloaded = downloaded;
                        progress_callback(downloaded, estimated_size, phase.map(str::to_string));
                    },
                )
                .await
            {
                Ok(ParakeetResponse::Status {
                    loaded_model: Some(id),
                    ..
                }) if id == definition.id => {
                    if cancel_flag
                        .as_ref()
                        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
                    {
                        let _ = self.delete_model(app, model_name).await;
                        return Err("Cancelled by user".to_string());
                    }

                    // Download/load completed for the requested version
                    if last_downloaded < estimated_size {
                        progress_callback(
                            estimated_size,
                            estimated_size,
                            Some("complete".to_string()),
                        );
                    }
                    Ok(())
                }
                Ok(ParakeetResponse::Status {
                    loaded_model: Some(other_id),
                    ..
                }) => Err(format!(
                    "Sidecar loaded '{}' but '{}' was requested",
                    other_id, definition.id
                )),
                Ok(ParakeetResponse::Ok { .. }) => {
                    Err("Unexpected OK without status payload".to_string())
                }
                Ok(ParakeetResponse::Error { code, message, .. }) => {
                    Err(format!("Failed to download model: {}: {}", code, message))
                }
                Err(e) => Err(format!("Failed to communicate with sidecar: {}", e)),
                _ => Err("Unexpected response from sidecar".to_string()),
            }
        }
    }

    pub async fn eou_model_status(
        &self,
        app: &AppHandle,
        chunk_ms: u16,
    ) -> Result<ParakeetEouModelStatus, ParakeetError> {
        match self
            .send_command(app, &ParakeetCommand::EouModelStatus { chunk_ms })
            .await?
        {
            ParakeetResponse::EouModelStatus {
                chunk_ms,
                downloaded,
                path,
            } => Ok(ParakeetEouModelStatus {
                chunk_ms,
                downloaded,
                path,
            }),
            ParakeetResponse::Error { code, message, .. } => {
                Err(ParakeetError::SidecarError { code, message })
            }
            other => Err(ParakeetError::SidecarError {
                code: "unexpected_response".to_string(),
                message: format!("Unexpected EOU status response: {:?}", other),
            }),
        }
    }

    pub async fn download_eou_model(
        &self,
        app: &AppHandle,
        chunk_ms: u16,
        mut progress_callback: impl FnMut(u64, u64, Option<String>) + Send + 'static,
    ) -> Result<(), String> {
        let mut last_downloaded = 0;
        match self
            .send_command_with_progress_and_cancel(
                app,
                &ParakeetCommand::DownloadEouModel { chunk_ms },
                None,
                |progress, phase| {
                    let progress = progress.clamp(0.0, 1.0) as f64;
                    let downloaded = (EOU_MODEL_SIZE_BYTES as f64 * progress).round() as u64;
                    last_downloaded = downloaded;
                    progress_callback(downloaded, EOU_MODEL_SIZE_BYTES, phase.map(str::to_string));
                },
            )
            .await
        {
            Ok(ParakeetResponse::Ok { .. }) => {
                if last_downloaded < EOU_MODEL_SIZE_BYTES {
                    progress_callback(
                        EOU_MODEL_SIZE_BYTES,
                        EOU_MODEL_SIZE_BYTES,
                        Some("complete".to_string()),
                    );
                }
                Ok(())
            }
            Ok(ParakeetResponse::Error { code, message, .. }) => {
                Err(format!("Failed to download EOU model: {code}: {message}"))
            }
            Ok(other) => Err(format!("Unexpected EOU download response: {other:?}")),
            Err(error) => Err(format!("Failed to communicate with sidecar: {error}")),
        }
    }

    // Unused since decode-ahead replaced EOU for live preview (plan 051); returns
    // with the EOU activation path when upstream FluidAudio fixes empty transcripts.
    #[allow(dead_code)]
    pub async fn warmup_eou(&self, app: &AppHandle, chunk_ms: u16) -> Result<(), ParakeetError> {
        match self
            .send_command(app, &ParakeetCommand::WarmupEou { chunk_ms })
            .await?
        {
            ParakeetResponse::Warmed { warmed: true, .. } => Ok(()),
            ParakeetResponse::Warmed { error, .. } => Err(ParakeetError::SidecarError {
                code: "eou_warmup_failed".to_string(),
                message: error.unwrap_or_else(|| "EOU warmup failed".to_string()),
            }),
            ParakeetResponse::Error { code, message, .. } => {
                Err(ParakeetError::SidecarError { code, message })
            }
            other => Err(ParakeetError::SidecarError {
                code: "unexpected_response".to_string(),
                message: format!("Unexpected EOU warmup response: {:?}", other),
            }),
        }
    }

    pub async fn delete_model(&self, app: &AppHandle, model_name: &str) -> Result<(), String> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let _ = app;
            if model_name != super::onnx::download::MODEL_ID {
                return Err("This Parakeet model is not available on Windows.".into());
            }
            let directory = super::onnx::download::model_directory(
                &self
                    .root_dir
                    .parent()
                    .unwrap_or(&self.root_dir)
                    .join("parakeet-onnx"),
            );
            return super::onnx::download::delete(
                &super::onnx::MODEL_OPERATION_LOCK,
                &super::onnx::LOCAL_MODEL_GATE,
                &directory,
                self.onnx.unload(),
            )
            .await;
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
        {
            let Some(definition) = self.get_model_definition(model_name) else {
                return Err(format!("Unknown Parakeet model: {model_name}"));
            };

            let version = Self::model_version_for(definition);

            // Send delete_model command to Swift sidecar to remove FluidAudio cached files
            let command = ParakeetCommand::DeleteModel {
                model_id: Some(definition.id.to_string()),
                model_version: Some(version.to_string()),
            };

            match self.send_command(app, &command).await {
                Ok(ParakeetResponse::Ok { .. }) | Ok(ParakeetResponse::Status { .. }) => {
                    // Successfully deleted
                }
                Ok(ParakeetResponse::Error { code, message, .. }) => {
                    return Err(format!("Failed to delete model: {}: {}", code, message));
                }
                Err(e) => {
                    return Err(format!("Failed to communicate with sidecar: {}", e));
                }
                _ => {
                    // Unexpected response but continue
                }
            }

            // Remove our tracking directory if it exists (from old Python implementation)
            let model_dir = self.model_dir(definition.id);
            if model_dir.exists() {
                std::fs::remove_dir_all(&model_dir).map_err(|e| {
                    format!(
                        "Failed to delete old model directory {}: {}",
                        model_dir.display(),
                        e
                    )
                })?;
            }

            Ok(())
        }
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub async fn unload_onnx(&self) {
        self.onnx.unload().await;
    }

    /// Call only from a blocking thread, never from an async task.
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub fn unload_onnx_blocking(&self) {
        self.onnx.unload_blocking();
    }

    #[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
    async fn lock_local_model_gate_with_cancel<'a>(
        gate: &'a tokio::sync::Mutex<()>,
        cancel: &AtomicBool,
    ) -> Result<tokio::sync::MutexGuard<'a, ()>, ParakeetError> {
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(ParakeetError::Unavailable("Transcription cancelled".into()));
            }
            tokio::select! {
                guard = gate.lock() => {
                    if cancel.load(Ordering::SeqCst) {
                        return Err(ParakeetError::Unavailable("Transcription cancelled".into()));
                    }
                    return Ok(guard);
                }
                _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            }
        }
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    fn is_selected_onnx(app: &AppHandle, model_name: &str) -> bool {
        use tauri_plugin_store::StoreExt;
        app.store("settings").ok().is_some_and(|store| {
            let engine = store
                .get("current_model_engine")
                .and_then(|v| v.as_str().map(str::to_owned));
            let model = store
                .get("current_model")
                .and_then(|v| v.as_str().map(str::to_owned));
            PreloadOutcome::for_selection(engine.as_deref(), model.as_deref(), model_name)
                == PreloadOutcome::Loaded
        })
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    async fn load_onnx_locked(
        &self,
        app: &AppHandle,
        directory: PathBuf,
        cancel: Arc<AtomicBool>,
        remote_cache: Option<Arc<std::sync::Mutex<crate::whisper::cache::TranscriberCache>>>,
    ) -> Result<(), ParakeetError> {
        if cancel.load(Ordering::SeqCst) {
            return Err(ParakeetError::Unavailable("Transcription cancelled".into()));
        }
        if let Some(cache) =
            app.try_state::<tauri::async_runtime::Mutex<crate::whisper::cache::TranscriberCache>>()
        {
            cache.lock().await.clear();
        }
        if let Some(cache) = remote_cache {
            let _ = tokio::task::spawn_blocking(move || {
                cache
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .clear();
            })
            .await;
        }
        if let Some(gpu) = app.try_state::<crate::whisper::gpu_sidecar::GpuSidecarClient>() {
            gpu.abort_active_process().await;
        }
        let start = Instant::now();
        self.onnx
            .load(directory, cancel)
            .await
            .map_err(ParakeetError::Unavailable)?;
        self.last_model_load_ms
            .store(start.elapsed().as_millis() as u64, Ordering::Relaxed);
        Ok(())
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub async fn preload_selected_onnx(
        &self,
        app: &AppHandle,
        model_name: &str,
    ) -> Result<PreloadOutcome, ParakeetError> {
        let _gate = super::onnx::LOCAL_MODEL_GATE.lock().await;
        let remote_cache = crate::remote::lifecycle::remote_whisper_cache_handle();
        if !Self::is_selected_onnx(app, model_name) {
            return Ok(PreloadOutcome::Superseded);
        }
        let directory = match self.onnx_model_dir(model_name) {
            Ok(directory) => directory,
            Err(_) if !Self::is_selected_onnx(app, model_name) => {
                return Ok(PreloadOutcome::Superseded);
            }
            Err(error) => return Err(error),
        };
        let load_result = self
            .load_onnx_locked(
                app,
                directory,
                Arc::new(AtomicBool::new(false)),
                remote_cache,
            )
            .await;
        if !Self::is_selected_onnx(app, model_name) {
            self.onnx.unload().await;
            return Ok(PreloadOutcome::Superseded);
        }
        load_result?;
        Ok(PreloadOutcome::Loaded)
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    fn onnx_model_dir(&self, model_name: &str) -> Result<PathBuf, ParakeetError> {
        if model_name != "parakeet-tdt-0.6b-v3" {
            return Err(ParakeetError::Unavailable(
                "This Parakeet model is not available on Windows.".into(),
            ));
        }
        let directory = self
            .root_dir
            .parent()
            .unwrap_or(&self.root_dir)
            .join("parakeet-onnx")
            .join(model_name);
        if !super::onnx::download::is_downloaded(
            &directory,
            super::onnx::download::REVISION,
            &super::onnx::download::FILES,
        ) {
            return Err(ParakeetError::Unavailable(
                "Parakeet model files are missing or incomplete. Download the model again.".into(),
            ));
        }
        Ok(directory)
    }

    pub async fn load_model(&self, app: &AppHandle, model_name: &str) -> Result<(), ParakeetError> {
        self.load_model_with_cancel(app, model_name, None).await
    }

    // Windows returns early from a cfg block; other platforms fall through.
    #[cfg_attr(
        all(target_os = "windows", target_arch = "x86_64"),
        allow(clippy::needless_return)
    )]
    pub async fn load_model_with_cancel(
        &self,
        app: &AppHandle,
        model_name: &str,
        cancel_flag: Option<Arc<AtomicBool>>,
    ) -> Result<(), ParakeetError> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let directory = self.onnx_model_dir(model_name)?;
            let cancel = cancel_flag.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
            let _gate =
                Self::lock_local_model_gate_with_cancel(&super::onnx::LOCAL_MODEL_GATE, &cancel)
                    .await?;
            let remote_cache = crate::remote::lifecycle::remote_whisper_cache_handle();
            self.load_onnx_locked(app, directory, cancel, remote_cache)
                .await?;
            return Ok(());
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
        {
            let load_start = Instant::now();
            let Some(definition) = self.get_model_definition(model_name) else {
                return Err(ParakeetError::SpawnError(format!(
                    "Unknown Parakeet model: {model_name}"
                )));
            };
            if !self.is_model_downloaded(definition) {
                return Err(ParakeetError::SidecarError {
                    code: "model_not_downloaded".to_string(),
                    message: format!("Parakeet model is not downloaded: {model_name}"),
                });
            }

            let version = Self::model_version_for(definition);
            let command = ParakeetCommand::LoadModel {
                model_id: definition.id.to_string(),
                model_version: Some(version.to_string()),
                force_download: Some(false),
                local_path: None,
                cache_dir: None,
                precision: "bf16".into(),
                attention: "local".into(),
                local_attention_context: 256,
                chunk_duration: Some(120.0),
                overlap_duration: Some(15.0),
                eager_unload: Some(false),
            };

            let result = match self
                .send_command_with_progress_and_cancel(app, &command, cancel_flag, |_, _| {})
                .await?
            {
                ParakeetResponse::Ok { .. } => Ok(()),
                ParakeetResponse::Status {
                    loaded_model,
                    model_version,
                    ..
                } => {
                    let expected_v = version.to_string();
                    let ok = match (loaded_model.as_deref(), model_version.as_deref()) {
                        (Some(id), mv) if mv == Some(expected_v.as_str()) => {
                            // Accept exact match ("parakeet-...-v2") or prefix match for id variants
                            id == definition.id
                                || id == format!("{}-{}", definition.id, expected_v)
                                || id.starts_with(definition.id)
                        }
                        _ => false,
                    };
                    if ok {
                        Ok(())
                    } else {
                        Err(ParakeetError::SidecarError {
                        code: "load_mismatch".to_string(),
                        message: format!(
                            "Sidecar loaded '{:?}' (version {:?}) but '{}' (version {}) was requested",
                            loaded_model, model_version, definition.id, expected_v
                        ),
                    })
                    }
                }
                ParakeetResponse::Error { code, message, .. } => {
                    Err(ParakeetError::SidecarError { code, message })
                }
                other => Err(ParakeetError::SidecarError {
                    code: "unexpected_response".to_string(),
                    message: format!("Unexpected response: {:?}", other),
                }),
            };
            if result.is_ok() {
                let elapsed_ms = load_start.elapsed().as_millis() as u64;
                self.last_model_load_ms.store(elapsed_ms, Ordering::Relaxed);
                log_performance(
                    "PARAKEET_MODEL_LOAD",
                    elapsed_ms,
                    Some(&format!("model={model_name}")),
                );
            }
            result
        }
    }

    // Windows returns early from a cfg block; other platforms fall through.
    #[cfg_attr(
        all(target_os = "windows", target_arch = "x86_64"),
        allow(clippy::needless_return)
    )]
    pub async fn warmup(&self, app: &AppHandle) -> Result<Option<u64>, ParakeetError> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let _ = app;
            return Ok(None);
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
        {
            if self.real_transcription_busy(app) {
                log::info!("Skipping Parakeet warmup because recording/transcription is active");
                return Ok(None);
            }

            let warmup_start = Instant::now();
            match self.send_command(app, &ParakeetCommand::Warmup {}).await? {
                ParakeetResponse::Warmed { warmed, ms, error } => {
                    let elapsed_ms = if ms == 0 {
                        warmup_start.elapsed().as_millis() as u64
                    } else {
                        ms
                    };
                    log_performance(
                        "PARAKEET_WARMUP",
                        elapsed_ms,
                        Some(&format!("warmed={warmed}")),
                    );
                    if warmed {
                        self.last_warmup_ms.store(elapsed_ms, Ordering::Relaxed);
                        Ok(Some(elapsed_ms))
                    } else {
                        log::warn!(
                            "Parakeet warmup did not complete: {}",
                            error.unwrap_or_else(|| "unknown warmup error".to_string())
                        );
                        Ok(None)
                    }
                }
                ParakeetResponse::Error { code, message, .. } => {
                    log::warn!("Parakeet warmup failed: {code}: {message}");
                    Ok(None)
                }
                other => Err(ParakeetError::SidecarError {
                    code: "unexpected_response".to_string(),
                    message: format!("Unexpected warmup response: {:?}", other),
                }),
            }
        }
    }

    pub fn vocabulary_status_from_response(
        response: &ParakeetResponse,
    ) -> Option<ParakeetVocabularyStatus> {
        match response {
            ParakeetResponse::Status {
                custom_vocabulary_supported,
                custom_vocabulary_ready,
                ..
            } => Some(ParakeetVocabularyStatus {
                supported: *custom_vocabulary_supported,
                ready: *custom_vocabulary_ready,
            }),
            _ => None,
        }
    }

    pub async fn status(&self, app: &AppHandle) -> Result<ParakeetResponse, ParakeetError> {
        self.send_command(app, &ParakeetCommand::Status {}).await
    }

    // Windows returns early from a cfg block; other platforms fall through.
    #[cfg_attr(
        all(target_os = "windows", target_arch = "x86_64"),
        allow(clippy::needless_return)
    )]
    pub async fn download_ctc_models(
        &self,
        app: &AppHandle,
    ) -> Result<ParakeetResponse, ParakeetError> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let _ = app;
            return Err(ParakeetError::Unavailable(
                "Parakeet CTC vocabulary is unavailable on Windows.".into(),
            ));
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
        self.send_command(app, &ParakeetCommand::DownloadCtcModels {})
            .await
    }

    pub async fn transcribe(
        &self,
        app: &AppHandle,
        model_name: &str,
        audio_path: PathBuf,
        language: Option<String>,
        translate: bool,
        cancel_flag: Option<Arc<AtomicBool>>,
    ) -> Result<ParakeetResponse, ParakeetError> {
        self.transcribe_with_custom_vocabulary(
            app,
            model_name,
            audio_path,
            ParakeetTranscriptionOptions::new(language, translate, cancel_flag),
        )
        .await
    }

    // Windows returns early from a cfg block; other platforms fall through.
    #[cfg_attr(
        all(target_os = "windows", target_arch = "x86_64"),
        allow(clippy::needless_return)
    )]
    pub async fn transcribe_with_custom_vocabulary(
        &self,
        app: &AppHandle,
        model_name: &str,
        audio_path: PathBuf,
        options: ParakeetTranscriptionOptions,
    ) -> Result<ParakeetResponse, ParakeetError> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let _ = app;
            self.onnx_model_dir(model_name)?;
            let cancel = options
                .cancel_flag
                .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
            let started = Instant::now();
            let (output, duration) = self
                .onnx
                .transcribe_wav(audio_path, cancel)
                .await
                .map_err(ParakeetError::Unavailable)?;
            self.last_inference_ms
                .store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
            return Ok(ParakeetResponse::Transcription {
                text: output.text,
                segments: output
                    .words
                    .into_iter()
                    .map(|word| super::messages::ParakeetSegment {
                        text: word.text,
                        start: Some(word.start),
                        end: Some(word.end),
                        tokens: None,
                    })
                    .collect(),
                language: None,
                duration: Some(duration),
            });
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
        {
            let _active_guard = self.mark_real_transcription_active();
            let inference_start = Instant::now();
            let command = ParakeetCommand::Transcribe {
                audio_path: audio_path.to_string_lossy().to_string(),
                language: options.language,
                translate_to_english: options.translate,
                prompt: None,
                use_word_timestamps: Some(true),
                chunk_duration: None,
                overlap_duration: None,
                attention: None,
                local_attention_context: None,
                custom_vocabulary: (!options.custom_vocabulary.is_empty())
                    .then_some(options.custom_vocabulary),
            };

            let result = self
                .send_command_with_progress_and_cancel(
                    app,
                    &command,
                    options.cancel_flag,
                    |_, _| {},
                )
                .await;
            if matches!(result, Ok(ParakeetResponse::Transcription { .. })) {
                let elapsed_ms = inference_start.elapsed().as_millis() as u64;
                self.last_inference_ms.store(elapsed_ms, Ordering::Relaxed);
                log_performance(
                    "PARAKEET_INFERENCE",
                    elapsed_ms,
                    Some(&format!(
                        "model={model_name}, audio_path={}",
                        audio_path.display()
                    )),
                );
            }
            result
        }
    }

    pub async fn diarize(
        &self,
        app: &AppHandle,
        audio_path: PathBuf,
    ) -> Result<ParakeetResponse, ParakeetError> {
        let command = ParakeetCommand::Diarize {
            audio_path: audio_path.to_string_lossy().to_string(),
        };

        self.send_command(app, &command).await
    }

    /// Check if the Parakeet sidecar is healthy and can respond to commands
    #[allow(dead_code)]
    pub async fn health_check(&self, app: &AppHandle) -> Result<bool, ParakeetError> {
        match self.send_command(app, &ParakeetCommand::Status {}).await {
            Ok(ParakeetResponse::Status { .. }) => Ok(true),
            Ok(_) => Ok(true), // Any successful response is good
            Err(e) => {
                warn!("Parakeet sidecar health check failed: {:?}", e);
                Err(e)
            }
        }
    }

    #[allow(dead_code)]
    pub async fn shutdown(&self) {
        self.client.shutdown().await;
    }

    #[cfg_attr(
        all(target_os = "windows", target_arch = "x86_64"),
        allow(clippy::needless_return)
    )]
    fn friendly_spawn_message(details: &str) -> String {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let _ = details;
            return "Parakeet is unavailable on this Windows system. Try downloading the model again or choose another engine.".to_string();
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
        {
            format!(
                "Parakeet is unavailable. Please reinstall Voicetypr or remove the quarantine flag by running `xattr -dr com.apple.quarantine /Applications/Voicetypr.app`. Details: {}",
                details
            )
        }
    }

    fn emit_unavailable(app: &AppHandle, message: &str) {
        if let Err(err) = app.emit(PARAKEET_UNAVAILABLE_EVENT, message.to_string()) {
            warn!("Failed to emit Parakeet unavailable event: {err:?}");
        }
    }

    async fn send_command(
        &self,
        app: &AppHandle,
        command: &ParakeetCommand,
    ) -> Result<ParakeetResponse, ParakeetError> {
        match self.client.send(app, command).await {
            Ok(response) => Ok(response),
            Err(ParakeetError::SpawnError(details)) => {
                let message = Self::friendly_spawn_message(&details);
                Self::emit_unavailable(app, &message);
                Err(ParakeetError::Unavailable(message))
            }
            Err(err) => Err(err),
        }
    }

    async fn send_command_with_progress_and_cancel<F>(
        &self,
        app: &AppHandle,
        command: &ParakeetCommand,
        cancel_flag: Option<Arc<AtomicBool>>,
        progress_callback: F,
    ) -> Result<ParakeetResponse, ParakeetError>
    where
        F: FnMut(f32, Option<&str>),
    {
        match self
            .client
            .send_with_progress_and_cancel(app, command, cancel_flag, progress_callback)
            .await
        {
            Ok(response) => Ok(response),
            Err(ParakeetError::SpawnError(details)) => {
                let message = Self::friendly_spawn_message(&details);
                Self::emit_unavailable(app, &message);
                Err(ParakeetError::Unavailable(message))
            }
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    use super::{fluid_audio_model_dir, model_files_complete};
    use super::{ParakeetManager, PreloadOutcome};
    use crate::parakeet::models::AVAILABLE_MODELS;
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    use std::fs;
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    use tempfile::TempDir;

    #[test]
    fn switched_selection_skips_preload_without_a_failure() {
        let model = "parakeet-tdt-0.6b-v3";
        assert_eq!(
            PreloadOutcome::for_selection(Some("parakeet"), Some(model), model),
            PreloadOutcome::Loaded
        );
        assert_eq!(
            PreloadOutcome::for_selection(Some("whisper"), Some("base"), model),
            PreloadOutcome::Superseded
        );
        assert_eq!(
            PreloadOutcome::for_selection(Some("parakeet"), Some("other"), model),
            PreloadOutcome::Superseded
        );
    }

    #[tokio::test]
    async fn cancelled_wait_for_local_model_gate_returns_promptly() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        let gate = Arc::new(tokio::sync::Mutex::new(()));
        let held = gate.lock().await;
        let cancel = Arc::new(AtomicBool::new(false));
        let waiting_gate = Arc::clone(&gate);
        let waiting_cancel = Arc::clone(&cancel);
        let waiter = tokio::spawn(async move {
            ParakeetManager::lock_local_model_gate_with_cancel(&waiting_gate, &waiting_cancel)
                .await
                .map(|_| ())
                .map_err(|error| error.to_string())
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel.store(true, Ordering::SeqCst);
        let result = tokio::time::timeout(Duration::from_millis(250), waiter)
            .await
            .expect("cancelled waiter must not wait for the held gate")
            .expect("waiter task must complete");
        assert_eq!(result.unwrap_err(), "Transcription cancelled");
        drop(held);
    }

    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    #[test]
    fn fluid_audio_model_dir_matches_fluidaudio_cache_shape() {
        let temp = TempDir::new().expect("temp dir");

        for definition in AVAILABLE_MODELS.iter() {
            assert_eq!(
                fluid_audio_model_dir(temp.path(), definition),
                temp.path()
                    .join("Library/Application Support/FluidAudio/Models")
                    .join(definition.cache_subdir)
            );
        }
    }

    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    #[test]
    fn model_files_complete_rejects_partial_cache() {
        let temp = TempDir::new().expect("temp dir");
        let definition = &AVAILABLE_MODELS[0];
        let model_dir = temp.path().join(definition.id);
        fs::create_dir_all(&model_dir).expect("model dir");
        fs::create_dir_all(model_dir.join(definition.files[0].filename)).expect("one model file");

        assert!(!model_files_complete(&model_dir, definition));
    }

    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    #[test]
    fn model_files_complete_accepts_required_files() {
        let temp = TempDir::new().expect("temp dir");
        let definition = &AVAILABLE_MODELS[0];
        let model_dir = temp.path().join(definition.id);
        fs::create_dir_all(&model_dir).expect("model dir");

        for file in definition.files {
            let path = model_dir.join(file.filename);
            if file.filename.ends_with(".mlmodelc") {
                fs::create_dir_all(path).expect("model package");
            } else {
                fs::write(path, b"{}").expect("model asset");
            }
        }

        assert!(model_files_complete(&model_dir, definition));
    }

    #[test]
    fn native_models_use_distinct_sidecar_versions() {
        let unified = AVAILABLE_MODELS
            .iter()
            .find(|model| model.id == "parakeet-unified-640ms")
            .expect("Unified model");
        assert_eq!(ParakeetManager::model_version_for(unified), "unified_640");

        let multilingual = AVAILABLE_MODELS
            .iter()
            .find(|model| model.id == "nemotron-multilingual-1120ms")
            .expect("Nemotron model");
        assert_eq!(
            ParakeetManager::model_version_for(multilingual),
            "nemotron_multilingual_1120"
        );
    }
}
