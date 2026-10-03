//! Bounded, ephemeral recovery ownership. Paths remain private to this module.
use super::island::RecoveryKind;
use crate::commands::audio;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

#[derive(Clone, Serialize)]
pub struct Recovery {
    generation: u64,
    id: String,
    kind: RecoveryKind,
    engine_short: String,
    alt_engine_short: Option<String>,
    expires_in_ms: u64,
}
fn emit_recovery<R: tauri::Runtime>(app: &AppHandle<R>, recovery: &Recovery) {
    crate::commands::pill_feedback::cancel_terminal_hide(recovery.generation);
    let _ = app.emit_to("pill", "dictation-recovery", recovery.clone());
}
struct Clip {
    recovery: Recovery,
    path: PathBuf,
    expires: Instant,
    busy: bool,
    target: PathBuf,
    retry_generation: Option<u64>,
}
#[derive(Default)]
struct Store {
    clips: VecDeque<Clip>,
    closed: bool,
    leases: Vec<PathBuf>,
    active_retry: Option<(String, u64)>,
}
static STORE: Lazy<Mutex<Store>> = Lazy::new(|| Mutex::new(Store::default()));
impl Store {
    fn retain(
        &mut self,
        dir: &Path,
        path: &Path,
        generation: u64,
        kind: RecoveryKind,
    ) -> Result<Recovery, String> {
        if self.closed {
            return Err("Recovery is shutting down".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let expires_in_ms = kind.ttl_ms();
        std::fs::create_dir_all(dir).map_err(|_| "Recovery storage unavailable")?;
        let target = dir.join(format!("{id}.wav"));
        // Finalized clips move to opaque names; pending writers retain their source
        // until the header is finalized, then the expiry task promotes them.
        let finalized = finalized_wav(path);
        let owned_path = if finalized {
            if move_finalized(path, &target) {
                target.clone()
            } else {
                path.to_owned()
            }
        } else {
            path.to_owned()
        };
        let recovery = Recovery {
            generation,
            id: id.clone(),
            kind,
            engine_short: "Whisper".into(),
            alt_engine_short: None,
            expires_in_ms,
        };
        self.insert(Clip {
            recovery: recovery.clone(),
            path: owned_path,
            expires: Instant::now() + Duration::from_millis(expires_in_ms),
            busy: false,
            target,
            retry_generation: None,
        })?;
        Ok(recovery)
    }
    fn discard(&mut self, id: &str) {
        if let Some(index) = self.clips.iter().position(|c| c.recovery.id == id) {
            if let Some(clip) = self.clips.remove(index) {
                let _ = std::fs::remove_file(clip.path);
            }
        }
    }
    fn discard_generation(&mut self, generation: u64) {
        let ids: Vec<_> = self
            .clips
            .iter()
            .filter(|c| {
                c.recovery.generation == generation || c.retry_generation == Some(generation)
            })
            .map(|c| c.recovery.id.clone())
            .collect();
        for id in ids {
            self.discard(&id);
        }
    }
    fn expire(&mut self, now: Instant) {
        let ids: Vec<_> = self
            .clips
            .iter()
            .filter(|c| c.expires <= now)
            .map(|c| c.recovery.id.clone())
            .collect();
        for id in ids {
            self.discard(&id);
        }
    }
    fn insert(&mut self, clip: Clip) -> Result<(), String> {
        self.expire(Instant::now());
        if self.closed {
            return Err("Recovery is shutting down".into());
        }
        while self.clips.len() >= 3 {
            let id = self.clips[0].recovery.id.clone();
            self.discard(&id);
        }
        self.clips.push_back(clip);
        Ok(())
    }
    fn complete_retry(&mut self, id: &str, succeeded: bool) {
        if self
            .active_retry
            .as_ref()
            .is_some_and(|(active, _)| active == id)
        {
            self.active_retry = None;
        }
        if succeeded {
            self.discard(id);
        } else if let Some(clip) = self.clips.iter_mut().find(|c| c.recovery.id == id) {
            clip.busy = false;
            clip.retry_generation = None;
        }
    }
    fn promote(&mut self, id: &str) -> bool {
        if let Some(clip) = self.clips.iter_mut().find(|c| c.recovery.id == id) {
            if clip.path != clip.target
                && finalized_wav(&clip.path)
                && move_finalized(&clip.path, &clip.target)
            {
                clip.path = clip.target.clone();
            }
            true
        } else {
            false
        }
    }
    fn clear(&mut self) {
        self.active_retry = None;
        for path in self.leases.drain(..) {
            let _ = std::fs::remove_file(path);
        }
        while let Some(clip) = self.clips.pop_front() {
            let _ = std::fs::remove_file(clip.path);
        }
    }
}
pub fn discard_generation(app: &AppHandle, generation: u64) {
    STORE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .discard_generation(generation);
    crate::menu::runtime::refresh(app);
}
pub fn has_generation(generation: u64) -> bool {
    STORE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clips
        .iter()
        .any(|c| c.recovery.generation == generation && c.expires > Instant::now())
}
/// Opaque recovery ID and display name only; never expose the private audio path.
pub fn tray_recovery() -> Option<(String, Option<String>, bool)> {
    STORE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clips
        .iter()
        .rev()
        .find(|c| c.expires > Instant::now())
        .map(|c| {
            (
                c.recovery.id.clone(),
                c.recovery.alt_engine_short.clone(),
                c.busy,
            )
        })
}
pub fn cleanup() {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.closed = true;
    store.clear();
}
async fn local_candidates(app: &AppHandle) -> Vec<(String, String)> {
    let mut models: Vec<_> = app
        .state::<crate::parakeet::ParakeetManager>()
        .list_models()
        .into_iter()
        .filter(|m| m.downloaded)
        .map(|m| (m.name, "parakeet".into()))
        .collect();
    let whisper = app.state::<tokio::sync::RwLock<crate::whisper::manager::WhisperManager>>();
    models.extend(
        whisper
            .read()
            .await
            .get_downloaded_model_names()
            .into_iter()
            .map(|m| (m, "whisper".into())),
    );
    models
}
pub async fn local_alternative(app: &AppHandle) -> Option<(String, String)> {
    local_candidates(app).await.into_iter().next()
}
/// Move ownership away from normal finalization before any await. A detached
/// writer retains its original path until its header is finalized; retry checks
/// that header and copies a lease, so discard/expiry never races decoding.
pub async fn keep(
    app: &AppHandle,
    generation: u64,
    path: &Path,
    kind: RecoveryKind,
) -> Result<(), String> {
    if audio::delivery_aborted(
        app.state::<crate::AppState>().is_cancellation_requested(),
        generation,
    ) {
        return Err("Dictation discarded".into());
    }
    let dir = app
        .path()
        .temp_dir()
        .map_err(|_| "Recovery storage unavailable")?
        .join("kept-dictation");
    let recovery = STORE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(&dir, path, generation, kind)?;
    let id = recovery.id.clone();
    let expires_in_ms = recovery.expires_in_ms;
    audio::clear_in_flight_transcription_audio_for_generation(generation);
    let expiry_id = id.clone();
    let expiry_app = app.clone();
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(expires_in_ms);
        loop {
            let ready = {
                let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
                if !store.promote(&expiry_id) {
                    return;
                }
                store
                    .clips
                    .iter()
                    .find(|c| c.recovery.id == expiry_id)
                    .is_some_and(|c| c.path == c.target)
            };
            if ready || tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep_until(
                deadline.min(tokio::time::Instant::now() + Duration::from_millis(250)),
            )
            .await;
        }
        tokio::time::sleep_until(deadline).await;
        STORE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .discard(&expiry_id);
        crate::menu::runtime::refresh(&expiry_app);
    });
    let alternative = local_alternative(app).await;
    let store_settings = app.store("settings").ok();
    let setting = |key: &str, default: &str| {
        store_settings
            .as_ref()
            .and_then(|s| s.get(key))
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| default.into())
    };
    let engine_short = crate::pill::context::engine_short_name(
        &setting("current_model", ""),
        &setting("current_model_engine", "whisper"),
    );
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    if audio::delivery_aborted(
        app.state::<crate::AppState>().is_cancellation_requested(),
        generation,
    ) {
        store.discard(&id);
        return Err("Dictation discarded".into());
    }
    if let Some(clip) = store.clips.iter_mut().find(|c| c.recovery.id == id) {
        clip.recovery.engine_short = if kind == RecoveryKind::RemoteOffline {
            "Network".into()
        } else {
            engine_short
        };
        clip.recovery.alt_engine_short =
            alternative.map(|(m, e)| crate::pill::context::engine_short_name(&m, &e));
        clip.recovery.expires_in_ms = clip
            .expires
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        if !audio::recording_generation_is_stale(generation) {
            emit_recovery(app, &clip.recovery);
        }
    }
    drop(store);
    crate::menu::runtime::refresh(app);
    Ok(())
}
fn move_finalized(source: &Path, target: &Path) -> bool {
    if std::fs::rename(source, target).is_ok() {
        return true;
    }
    // App data and the system temp directory may live on different volumes.
    if std::fs::copy(source, target).is_ok() && std::fs::remove_file(source).is_ok() {
        return true;
    }
    let _ = std::fs::remove_file(target);
    false
}
fn finalized_wav(path: &Path) -> bool {
    // hound writes the RIFF length at finalize, after samples. A live writer's
    // placeholder header must never be treated as a completed recording.
    use std::io::Read;
    let mut header = [0; 8];
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    if file.read_exact(&mut header).is_err() || &header[..4] != b"RIFF" {
        return false;
    }
    file.metadata()
        .map(|m| u64::from(u32::from_le_bytes(header[4..8].try_into().unwrap())) + 8 == m.len())
        .unwrap_or(false)
}
struct Lease {
    id: String,
    kind: RecoveryKind,
    file: tempfile::NamedTempFile,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        store.complete_retry(&self.id, false);
        store.leases.retain(|p| p != self.file.path());
    }
}
fn lease(id: &str, anyway: bool) -> Result<Lease, String> {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.expire(Instant::now());
    let clip = store
        .clips
        .iter_mut()
        .find(|c| c.recovery.id == id)
        .ok_or("Recording expired or discarded")?;
    if anyway && clip.recovery.kind != RecoveryKind::NoSpeech {
        return Err("This action requires a no-speech clip".into());
    }
    if clip.busy {
        return Err("Recording is already being retried".into());
    }
    if !finalized_wav(&clip.path) {
        return Err("Recording is still finalizing".into());
    }
    let file = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile_in(clip.path.parent().ok_or("Recovery unavailable")?)
        .map_err(|_| "Recovery unavailable")?;
    std::fs::copy(&clip.path, file.path()).map_err(|_| "Could not read kept recording")?;
    let kind = clip.recovery.kind;
    clip.busy = true;
    store.leases.push(file.path().to_owned());
    Ok(Lease {
        id: id.into(),
        kind,
        file,
    })
}
#[tauri::command]
pub fn discard_kept_dictation(app: AppHandle, id: String) {
    let generation = {
        let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        store
            .active_retry
            .as_ref()
            .filter(|(active, _)| active == &id)
            .map(|(_, g)| *g)
    };
    if generation.is_some_and(|g| !audio::recording_generation_is_stale(g)) {
        app.state::<crate::AppState>().request_cancellation();
    }
    STORE.lock().unwrap_or_else(|e| e.into_inner()).discard(&id);
    crate::menu::runtime::refresh(&app);
}
#[tauri::command]
pub async fn retry_kept_dictation(
    app: AppHandle,
    id: String,
    engine: Option<String>,
) -> Result<(), String> {
    retry(app, id, engine, false).await
}
#[tauri::command]
pub async fn transcribe_anyway(app: AppHandle, id: String) -> Result<(), String> {
    retry(app, id, None, true).await
}
async fn retry(
    app: AppHandle,
    id: String,
    engine: Option<String>,
    anyway: bool,
) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    if !matches!(
        crate::get_recording_state(&app),
        crate::RecordingState::Idle | crate::RecordingState::Error
    ) {
        return Err("Dictation is busy".into());
    }
    let _stop_guard = audio::StopInFlightGuard::try_acquire(state.stop_in_flight.clone())
        .ok_or("Dictation is busy")?;
    let lease = lease(&id, anyway)?;
    if crate::get_recording_state(&app) == crate::RecordingState::Error {
        crate::update_recording_state(&app, crate::RecordingState::Idle, None);
    }
    let generation = audio::begin_recording_generation();
    state.clear_cancellation();
    {
        let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        let clip = store
            .clips
            .iter_mut()
            .find(|c| c.recovery.id == id)
            .ok_or("Recording discarded")?;
        clip.retry_generation = Some(generation);
        store.active_retry = Some((id.clone(), generation));
    }
    if let Some(context) = crate::writing::capture_active_app_context() {
        state.set_recording_app_context(context);
    }
    crate::update_recording_state(&app, crate::RecordingState::Starting, None);
    crate::update_recording_state(&app, crate::RecordingState::Recording, None);
    crate::update_recording_state(&app, crate::RecordingState::Stopping, None);
    crate::update_recording_state(&app, crate::RecordingState::Transcribing, None);
    crate::menu::runtime::refresh(&app);
    let result = retry_inner(&app, &lease, generation, engine).await;
    if !audio::recording_generation_is_stale(generation) {
        crate::update_recording_state(&app, crate::RecordingState::Idle, None);
    }
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let discarded = audio::delivery_aborted(state.is_cancellation_requested(), generation);
    store.complete_retry(&id, result.is_ok() || discarded);
    if result.is_err() && !discarded {
        if let Some(clip) = store.clips.iter_mut().find(|c| c.recovery.id == id) {
            clip.recovery.generation = generation;
            clip.recovery.expires_in_ms = clip
                .expires
                .saturating_duration_since(Instant::now())
                .as_millis() as u64;
            emit_recovery(&app, &clip.recovery);
        }
    }
    drop(store);
    crate::menu::runtime::refresh(&app);
    result.map_err(|_| "Retry failed; recording remains available until expiry".into())
}
async fn retry_inner(
    app: &AppHandle,
    lease: &Lease,
    generation: u64,
    engine: Option<String>,
) -> Result<(), String> {
    let settings = crate::commands::settings::get_settings(app.clone()).await?;
    let (model, engine) = if let Some(engine) = engine {
        // Accept catalog IDs and short names, never paths or provider errors.
        let alternatives = local_candidates(app).await;
        if let Some((model, kind)) = alternatives.into_iter().find(|(m, e)| {
            engine == *e || engine == *m || engine == crate::pill::context::engine_short_name(m, e)
        }) {
            (model, Some(kind))
        } else {
            return Err("Requested local engine is not ready".into());
        }
    } else {
        let model = if settings.current_model.is_empty() {
            local_candidates(app)
                .await
                .into_iter()
                .find(|(_, e)| e == &settings.current_model_engine)
                .map(|(m, _)| m)
                .unwrap_or_default()
        } else {
            settings.current_model.clone()
        };
        (
            model,
            if lease.kind == RecoveryKind::RemoteOffline {
                None
            } else {
                Some(settings.current_model_engine.clone())
            },
        )
    };
    super::island::validate_recording_license(app).await?;
    let output = audio::transcribe_audio_file_impl(
        app.clone(),
        lease.file.path().to_string_lossy().into_owned(),
        model.clone(),
        engine,
        false,
        None,
        None,
        None,
        Some(generation),
    )
    .await?;
    if output.text.trim().is_empty() {
        return Err("No text produced; recording is still kept".into());
    }
    let state = app.state::<crate::AppState>();
    // An accepted retry owns its lease across the original clip's TTL/cap
    // eviction. Explicit Discard/Esc still cancels this generation.
    if audio::delivery_aborted(state.is_cancellation_requested(), generation) {
        return Err("Retry discarded".into());
    }
    audio::save_transcription_with_recording_if_current(
        app.clone(),
        generation,
        output.text.clone(),
        model,
        None,
        output.metadata,
    )
    .await
    .ok_or("Retry discarded")??;
    let future = audio::persist_if_current(&state, generation, || async {
        if audio::delivery_aborted(state.is_cancellation_requested(), generation) {
            return Err("Retry discarded".into());
        }
        if settings.auto_paste_transcription
            && crate::commands::text::insert_dictation_text(
                app.clone(),
                output.text.clone(),
                generation,
            )
            .await
            .is_ok()
        {
            return Ok(());
        }
        let result = crate::commands::text::copy_dictation_text_to_clipboard(
            app.clone(),
            output.text.clone(),
            generation,
        )
        .await;
        if result.is_ok() {
            crate::commands::text::emit_dictation_copy_outcome(
                app,
                Some(true),
                output.text.split_whitespace().count() as u32,
                generation,
            );
        }
        result
    })
    .ok_or("Retry discarded")?;
    future.await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn clip(dir: &Path, kind: RecoveryKind) -> Clip {
        let id = uuid::Uuid::new_v4().to_string();
        let path = dir.join(&id);
        std::fs::write(&path, b"clip").unwrap();
        Clip {
            recovery: Recovery {
                generation: 1,
                id,
                kind,
                engine_short: "Whisper".into(),
                alt_engine_short: None,
                expires_in_ms: kind.ttl_ms(),
            },
            target: path.clone(),
            path,
            retry_generation: None,
            expires: Instant::now() + Duration::from_millis(kind.ttl_ms()),
            busy: false,
        }
    }
    #[test]
    fn ttl_cap_discard_retry_and_exit_delete_owned_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let first = clip(dir.path(), RecoveryKind::NoSpeech);
        let path = first.path.clone();
        store.insert(first).unwrap();
        store.expire(Instant::now() + Duration::from_secs(31));
        assert!(!path.exists());
        assert_eq!(RecoveryKind::NoSpeech.ttl_ms(), 30_000);
        assert_eq!(RecoveryKind::CloudFailed.ttl_ms(), 600_000);
        let first = clip(dir.path(), RecoveryKind::CloudFailed);
        let path = first.path.clone();
        store.insert(first).unwrap();
        for _ in 0..3 {
            store
                .insert(clip(dir.path(), RecoveryKind::Integrity))
                .unwrap();
        }
        assert_eq!(store.clips.len(), 3);
        assert!(!path.exists());
        let id = store.clips[0].recovery.id.clone();
        let path = store.clips[0].path.clone();
        store.discard(&id);
        assert!(!path.exists());
        let paths: Vec<_> = store.clips.iter().map(|c| c.path.clone()).collect();
        store.clear();
        assert!(paths.iter().all(|p| !p.exists()));
    }
    #[test]
    fn failed_retry_retains_successful_retry_deletes_and_exit_cleans_leases() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let mut clip = clip(dir.path(), RecoveryKind::CloudFailed);
        clip.busy = true;
        let id = clip.recovery.id.clone();
        let path = clip.path.clone();
        store.insert(clip).unwrap();
        store.complete_retry(&id, false);
        assert!(path.exists());
        assert!(!store.clips[0].busy);
        store.complete_retry(&id, true);
        assert!(!path.exists());
        assert!(store.clips.is_empty());
        let lease = dir.path().join("retry-copy.wav");
        std::fs::write(&lease, b"clip").unwrap();
        store.leases.push(lease.clone());
        store.clear();
        assert!(!lease.exists());
    }
    #[test]
    fn every_recovery_kind_has_only_safe_metadata_and_the_correct_ttl() {
        let dir = tempfile::tempdir().unwrap();
        for kind in [
            RecoveryKind::CloudFailed,
            RecoveryKind::NetworkOffline,
            RecoveryKind::ModelMissing,
            RecoveryKind::RemoteOffline,
            RecoveryKind::NoSpeech,
            RecoveryKind::Integrity,
            RecoveryKind::MicDroppedEmpty,
        ] {
            let clip = clip(dir.path(), kind);
            let value = serde_json::to_value(&clip.recovery).unwrap();
            assert_eq!(value.as_object().unwrap().len(), 6);
            assert!(value.get("path").is_none());
            assert!(value.get("text").is_none());
            assert_eq!(
                value["expires_in_ms"],
                if kind == RecoveryKind::NoSpeech {
                    30_000
                } else {
                    600_000
                }
            );
            assert!(uuid::Uuid::parse_str(value["id"].as_str().unwrap()).is_ok());
        }
    }

    #[test]
    fn all_problem_paths_keep_the_wav_and_emit_only_recovery_metadata() {
        use tauri::Listener;
        let dir = tempfile::tempdir().unwrap();
        let app = tauri::test::mock_app();
        let received = std::sync::Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let listener = received.clone();
        app.listen_any("dictation-recovery", move |event| {
            listener
                .lock()
                .unwrap()
                .push(serde_json::from_str(event.payload()).unwrap())
        });
        let mut store = Store::default();
        for kind in [
            RecoveryKind::CloudFailed,
            RecoveryKind::NetworkOffline,
            RecoveryKind::ModelMissing,
            RecoveryKind::RemoteOffline,
            RecoveryKind::NoSpeech,
            RecoveryKind::Integrity,
            RecoveryKind::MicDroppedEmpty,
        ] {
            let source = dir.path().join("capture.wav");
            let mut writer = hound::WavWriter::create(
                &source,
                hound::WavSpec {
                    channels: 1,
                    sample_rate: 16000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            writer.write_sample(1_i16).unwrap();
            writer.finalize().unwrap();
            let recovery = store.retain(dir.path(), &source, 42, kind).unwrap();
            assert!(!source.exists());
            assert!(store.clips[0].path.exists());
            assert_eq!(
                store.clips[0].path.file_stem().unwrap().to_str(),
                Some(recovery.id.as_str())
            );
            emit_recovery(app.handle(), &recovery);
            assert_eq!(
                received.lock().unwrap().last(),
                Some(&serde_json::to_value(&recovery).unwrap())
            );
            store.clear();
        }
        assert_eq!(received.lock().unwrap().len(), 7);
    }

    #[test]
    fn escape_discards_only_its_own_generation_even_during_retry() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let mut older = clip(dir.path(), RecoveryKind::CloudFailed);
        older.recovery.generation = 1;
        let older_path = older.path.clone();
        store.insert(older).unwrap();
        let mut retried = clip(dir.path(), RecoveryKind::CloudFailed);
        retried.recovery.generation = 2;
        retried.retry_generation = Some(3);
        let retry_path = retried.path.clone();
        store.insert(retried).unwrap();
        store.discard_generation(3);
        assert!(!retry_path.exists());
        assert!(older_path.exists());
        store.clear();
    }

    #[test]
    fn live_header_is_not_retryable_until_finalized() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut writer = hound::WavWriter::create(
            file.path(),
            hound::WavSpec {
                channels: 1,
                sample_rate: 16000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        writer.write_sample(10_i16).unwrap();
        assert!(!finalized_wav(file.path()));
        writer.finalize().unwrap();
        assert!(finalized_wav(file.path()));
    }
}
