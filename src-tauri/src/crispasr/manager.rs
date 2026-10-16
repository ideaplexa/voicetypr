use super::final_result::FinalResults;
use super::messages::{encode_audio, Command, NativeTranscript, Response};
use super::sidecar::SidecarProcess;
use super::{download, models, pcm, sidecar};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tokio::sync::Mutex;

#[derive(Default)]
pub(crate) struct RuntimeState {
    pub process: Option<SidecarProcess>,
    gpu_failed: HashSet<String>,
}

impl RuntimeState {
    pub(crate) fn record_success(&mut self, model: &str) {
        if self.process.as_ref().is_some_and(|process| process.gpu) {
            self.gpu_failed.remove(model);
        }
    }
}

pub struct CrispasrManager {
    root: PathBuf,
    pub(crate) runtime: Mutex<RuntimeState>,
    pub(crate) finals: FinalResults,
    shutting_down: AtomicBool,
    status: tokio::sync::RwLock<crate::whisper::gpu_sidecar::AccelerationRuntimeStatus>,
}

impl CrispasrManager {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            runtime: Mutex::new(RuntimeState::default()),
            finals: FinalResults::default(),
            shutting_down: AtomicBool::new(false),
            status: tokio::sync::RwLock::new(Default::default()),
        }
    }
    pub(crate) fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub async fn acceleration_status(
        &self,
        mode: &str,
    ) -> crate::whisper::gpu_sidecar::AccelerationRuntimeStatus {
        let mut status = self.status.read().await.clone();
        status.mode = mode.to_string();
        status
    }

    #[cfg(target_os = "windows")]
    pub async fn test_acceleration(
        &self,
        app: &AppHandle,
        model: &str,
        mode: &str,
    ) -> Result<crate::whisper::gpu_sidecar::AccelerationRuntimeStatus, String> {
        let mut runtime = self
            .runtime
            .try_lock()
            .map_err(|_| "Stop transcription before testing acceleration")?;
        runtime.gpu_failed.remove(model);
        self.ensure_loaded(app, &mut runtime, model, &|| self.is_shutting_down(), false)
            .await?;
        Ok(self.acceleration_status(mode).await)
    }
    pub fn model_path(&self, name: &str) -> Option<PathBuf> {
        models::get(name).map(|model| self.root.join(model.id).join(model.filename))
    }
    pub fn is_downloaded(&self, name: &str) -> bool {
        let Some(model) = models::get(name) else {
            return false;
        };
        self.model_path(name)
            .and_then(|path| path.metadata().ok())
            .is_some_and(|metadata| metadata.len() == model.size)
    }
    pub async fn download(
        &self,
        name: &str,
        cancelled: Arc<AtomicBool>,
        progress: impl Fn(u64, u64, Option<String>),
    ) -> Result<(), String> {
        let model = models::get(name).ok_or("Unknown CrispASR model")?;
        let directory = self.root.join(model.id);
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|_| "Unable to create model directory")?;
        // Keep the upstream agreement and derivative notice with every R2T2
        // installation, including when both quantizations are downloaded.
        if model.family == models::ModelFamily::R2t2 {
            for (filename, size, checksum) in [
                (
                    "MODEL_LICENSE",
                    11071,
                    "4d9321cdad58182faa878b015de7d60069881614ddd7571de70f751a9b8e3811",
                ),
                (
                    "README.md",
                    3826,
                    "fca3827297b9746f01de00dafabdfaedca444d010a904b668eb6f2e7eadd045b",
                ),
            ] {
                download::download_file(
                    &model.url_for(filename),
                    &directory.join(filename),
                    size,
                    checksum,
                    &cancelled,
                    |_, _| {},
                )
                .await?;
            }
        } else {
            tokio::fs::write(directory.join("NOTICE.txt"), "Parakeet Ultra by Moondream; GGUF conversion by CrispStrobe.\nCC BY 4.0: https://creativecommons.org/licenses/by/4.0/\nSource: https://huggingface.co/moondream/parakeet-ultra\n").await.map_err(|_| "Unable to save model attribution")?;
        }
        download::download_file(
            &model.url_for(model.filename),
            &directory.join(model.filename),
            model.size,
            model.sha256,
            &cancelled,
            |downloaded, total| progress(downloaded, total, None),
        )
        .await
    }
    pub async fn delete(&self, name: &str) -> Result<(), String> {
        let model = models::get(name).ok_or("Unknown CrispASR model")?;
        let mut runtime = self
            .runtime
            .try_lock()
            .map_err(|_| "Stop transcription before removing this model")?;
        if let Some(mut process) = runtime.process.take() {
            process.abort().await;
        }
        let directory = self.root.join(model.id);
        if directory.exists() {
            tokio::fs::remove_dir_all(directory)
                .await
                .map_err(|_| "Unable to remove CrispASR model")?;
        }
        Ok(())
    }
    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        if let Ok(mut runtime) =
            tokio::time::timeout(Duration::from_secs(3), self.runtime.lock()).await
        {
            if let Some(mut process) = runtime.process.take() {
                process.abort().await;
            }
        }
    }
    pub(crate) async fn unload_when(&self, should_unload: impl Fn() -> bool) {
        // Selection cleanup is background work: wait for warmup/inference, then
        // recheck selection so a quick switch back cannot unload the new choice.
        let mut runtime = self.runtime.lock().await;
        if should_unload() {
            if let Some(mut process) = runtime.process.take() {
                process.abort().await;
            }
            runtime.gpu_failed.clear();
        }
    }
    pub async fn preload(&self, app: &AppHandle, model: &str) -> Result<(), String> {
        // Never queue speculative warmup ahead of a recording or batch request.
        use tauri::Manager;
        if app
            .try_state::<crate::state::AppState>()
            .is_some_and(|state| {
                !matches!(
                    state.get_current_state(),
                    crate::RecordingState::Idle | crate::RecordingState::Error
                )
            })
        {
            return Ok(());
        }
        let Ok(mut runtime) = self.runtime.try_lock() else {
            return Ok(());
        };
        self.ensure_loaded(app, &mut runtime, model, &|| self.is_shutting_down(), false)
            .await
    }
    pub(crate) async fn ensure_loaded(
        &self,
        app: &AppHandle,
        state: &mut RuntimeState,
        name: &str,
        cancelled: &(impl Fn() -> bool + Sync),
        force_cpu: bool,
    ) -> Result<(), String> {
        let model = models::get(name).ok_or("Unknown CrispASR model")?;
        if !self.is_downloaded(name) {
            return Err("CrispASR model is not downloaded. Download it first.".into());
        }
        let path = self.model_path(name).ok_or("Unknown CrispASR model")?;
        let path = path.to_str().ok_or("Unsupported model path")?;
        if state
            .process
            .as_ref()
            .is_some_and(|process| !process.is_reusable())
        {
            if let Some(mut process) = state.process.take() {
                process.abort().await;
            }
        }
        if state.process.is_none() {
            super::release_idle_whisper(app).await;
        }
        let mode = app
            .store("settings")
            .ok()
            .and_then(|store| store.get("transcription_acceleration"))
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "auto".into());
        let gpu = !force_cpu
            && mode != "cpu"
            && (mode == "gpu" || !state.gpu_failed.contains(name))
            && sidecar::runtime_path(app, true).is_some();
        for use_gpu in if gpu { vec![true, false] } else { vec![false] } {
            if cancelled() {
                return Err("Transcription cancelled".into());
            }
            if state
                .process
                .as_ref()
                .is_some_and(|process| process.gpu != use_gpu)
            {
                if let Some(mut process) = state.process.take() {
                    process.abort().await;
                }
            }
            let attempt = async {
                if state.process.is_none() {
                    let executable = sidecar::runtime_path(app, use_gpu)
                        .ok_or("CrispASR runtime is missing. Repair or reinstall the app.")?;
                    state.process = Some(SidecarProcess::spawn(&executable, use_gpu)?);
                    let status = state
                        .process
                        .as_mut()
                        .unwrap()
                        .request(
                            Command::Status,
                            Duration::from_secs(10),
                            cancelled,
                            &mut |_, _, _| {},
                        )
                        .await?;
                    if !matches!(status, Response::Status { protocol: 1, .. }) {
                        return Err("Incompatible CrispASR runtime. Reinstall the app.".into());
                    }
                }
                let response = state
                    .process
                    .as_mut()
                    .unwrap()
                    .request(
                        Command::LoadModel {
                            model: name,
                            backend: model.backend(),
                            model_path: path,
                            gpu: use_gpu,
                            threads: std::thread::available_parallelism()
                                .map(|count| count.get().saturating_sub(1).clamp(1, 4))
                                .unwrap_or(4),
                        },
                        Duration::from_secs(60),
                        cancelled,
                        &mut |_, _, _| {},
                    )
                    .await?;
                match response {
                    Response::Loaded { .. } => Ok(()),
                    _ => Err("Invalid CrispASR model response".into()),
                }
            }
            .await;
            if attempt.is_ok() {
                let gpu_failed = state.gpu_failed.contains(name);
                *self.status.write().await =
                    crate::whisper::gpu_sidecar::AccelerationRuntimeStatus {
                        mode: mode.clone(),
                        effective_backend: if use_gpu {
                            if cfg!(target_os = "macos") {
                                "metal"
                            } else {
                                "vulkan"
                            }
                        } else {
                            "cpu"
                        }
                        .into(),
                        gpu_available: if use_gpu {
                            Some(true)
                        } else if gpu_failed {
                            Some(false)
                        } else {
                            None
                        },
                        message: if use_gpu {
                            "CrispASR GPU acceleration is ready."
                        } else if gpu_failed {
                            "CrispASR is using CPU fallback after a GPU failure."
                        } else {
                            "CrispASR CPU runtime is ready."
                        }
                        .into(),
                        last_error: None,
                        diagnostic_code: if use_gpu {
                            "ready"
                        } else if gpu_failed {
                            "gpu_unavailable"
                        } else {
                            "cpu_selected"
                        }
                        .into(),
                        recommended_action: "none".into(),
                    };
                return attempt;
            }
            if let Some(mut process) = state.process.take() {
                process.abort().await;
            }
            if cancelled() {
                return Err("Transcription cancelled".into());
            }
            if use_gpu {
                state.gpu_failed.insert(name.to_string());
                log::warn!("CrispASR GPU initialization failed; using CPU fallback");
            } else {
                return attempt;
            }
        }
        Err("CrispASR model could not be loaded".into())
    }
    pub async fn transcribe(
        &self,
        app: &AppHandle,
        model: &str,
        audio_path: &Path,
        language: Option<&str>,
        cancel: Arc<AtomicBool>,
    ) -> Result<NativeTranscript, String> {
        // Capture already supplies PCM16 WAV: retain its native sample rate and
        // use exactly the stream converter. Other upload formats use the existing
        // decoder once, without Whisper-specific peak gain or tail trimming.
        let original = audio_path.to_path_buf();
        let probe = original.clone();
        let pcm16 = tokio::task::spawn_blocking(move || {
            hound::WavReader::open(probe).ok().is_some_and(|reader| {
                reader.spec().bits_per_sample == 16
                    && reader.spec().sample_format == hound::SampleFormat::Int
            })
        })
        .await
        .map_err(|_| "Audio preparation failed")?;
        let prepared = if pcm16 {
            None
        } else {
            let temp = tempfile::NamedTempFile::new()
                .map_err(|_| "Unable to prepare audio")?
                .into_temp_path();
            crate::audio::decode::normalize_to_wav_async(original.clone(), temp.to_path_buf())
                .await
                .map_err(|_| "Audio preparation failed")?;
            Some(temp)
        };
        let path = prepared
            .as_ref()
            .map(|path| path.to_path_buf())
            .unwrap_or(original);
        let read_cancel = cancel.clone();
        let audio = tokio::task::spawn_blocking(move || pcm::read_wav(&path, &read_cancel))
            .await
            .map_err(|_| "Audio preparation failed")??;
        let cancelled = || cancel.load(Ordering::SeqCst) || self.is_shutting_down();
        let mut runtime = self.runtime.lock().await;
        for force_cpu in [false, true] {
            self.ensure_loaded(app, &mut runtime, model, &cancelled, force_cpu)
                .await?;
            let result = async {
            let process = runtime.process.as_mut().ok_or("CrispASR runtime unavailable")?;
            let session_id = 0; // This lock serializes offline jobs with recording sessions.
            process.request(Command::StartStream { session_id, language: language.unwrap_or("auto"), mode: "batch" }, Duration::from_secs(10), &cancelled, &mut |_, _, _| {}).await?;
            let mut sent = 0;
            for samples in audio.chunks(16_000) {
                let pcm = encode_audio(samples);
                let response = process.request(Command::AudioChunk { session_id, pcm: &pcm }, Duration::from_secs(30), &cancelled, &mut |_, _, _| {}).await?;
                sent += samples.len() as u64;
                if !matches!(response, Response::Ok { samples: Some(count), .. } if count == sent) { return Err("CrispASR audio coverage mismatch".into()); }
            }
            let budget = crate::transcription::engines::transcription_watchdog_budget(Some(sent * 1000 / 16_000));
            match process.request(Command::FinalizeStream { session_id }, budget, &cancelled, &mut |_, _, _| {}).await? {
                Response::Final { session_id: 0, result, .. } if result.samples == sent => Ok(result),
                _ => Err("CrispASR final audio coverage mismatch".into()),
            }
        }.await;
            if result.is_ok() {
                runtime.record_success(model);
                return result;
            }
            // A GPU can load successfully and still fail on a later graph/shape.
            // Mirror Whisper's complete-audio CPU retry; cancellation never retries.
            let retry_cpu = !force_cpu
                && !cancelled()
                && runtime.process.as_ref().is_some_and(|process| process.gpu);
            if let Some(mut process) = runtime.process.take() {
                process.abort().await;
            }
            if !retry_cpu {
                return result;
            }
            runtime.gpu_failed.insert(model.to_string());
            log::warn!("CrispASR GPU decode failed; retrying complete audio on CPU");
        }
        Err("CrispASR transcription failed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unselection_waits_for_the_active_model_lease() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CrispasrManager::new(temp.path().to_path_buf());
        let active = manager.runtime.lock().await;
        let unload = manager.unload_when(|| true);
        tokio::pin!(unload);
        assert!(matches!(
            futures_util::poll!(unload.as_mut()),
            std::task::Poll::Pending
        ));
        drop(active);
        unload.await;
    }

    #[test]
    fn missing_partial_and_truncated_weights_are_not_downloaded() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CrispasrManager::new(temp.path().to_path_buf());
        let name = models::MODELS[0].id;
        assert!(!manager.is_downloaded(name));
        assert!(!manager.is_downloaded("../unknown"));
        let path = manager.model_path(name).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path.with_extension("partial"), b"unfinished").unwrap();
        assert!(!manager.is_downloaded(name));
        std::fs::write(path, b"truncated").unwrap();
        assert!(!manager.is_downloaded(name));
    }
}
