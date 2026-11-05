//! Bounded recovery ownership, durable for failed-only saving. Paths remain private to this module.
use super::island::RecoveryKind;
use crate::commands::audio;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
#[path = "kept_persistent.rs"]
mod persistent;
#[path = "kept_selection.rs"]
mod selection;
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

#[derive(Clone, Deserialize, Serialize)]
pub struct Recovery {
    generation: u64,
    pub(crate) id: String,
    kind: RecoveryKind,
    engine_short: String,
    alt_engine_short: Option<String>,
    alt_engine_id: Option<String>,
    expires_in_ms: u64,
    retry_ready: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    retry_failed: bool,
}
impl Recovery {
    fn set_alternative(&mut self, alternative: Option<(String, String)>) {
        self.alt_engine_short = alternative
            .as_ref()
            .map(|(m, e)| crate::pill::context::engine_short_name(m, e));
        self.alt_engine_id = alternative.map(|(m, _)| m);
    }
}
fn emit_recovery<R: tauri::Runtime>(app: &AppHandle<R>, recovery: &Recovery) {
    crate::observability::emit(
        "island_state",
        vec![("state", "recovery".into())],
        Some(recovery.generation),
    );
    crate::observability::update(
        recovery.generation,
        "recovery_kind",
        serde_json::to_value(recovery.kind).unwrap(),
    );
    crate::commands::pill_feedback::cancel_terminal_hide(recovery.generation);
    crate::commands::pill_feedback::send(
        app,
        "dictation-recovery",
        serde_json::to_value(recovery).unwrap(),
    );
}
#[derive(Clone, Serialize, Deserialize)]
struct RetrySelection {
    model: String,
    engine: Option<String>,
}
struct Clip {
    _capture_files: Option<super::storage::CaptureFiles>,
    trace: Option<crate::observability::Trace>,
    _trace_lease: Option<crate::observability::TraceLease>,
    _retry_trace_lease: Option<crate::observability::TraceLease>,
    recovery: Recovery,
    path: PathBuf,
    expires: Instant,
    busy: bool,
    retire_after_retry: bool,
    announced: bool,
    terminal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    target: PathBuf,
    retry_generation: Option<u64>,
    disk: Option<persistent::DiskClip>,
    retry_selection: Option<RetrySelection>,
}
impl Clip {
    fn remaining_ms(&self, now: Instant) -> u64 {
        self.disk.as_ref().map_or_else(
            || self.expires.saturating_duration_since(now).as_millis() as u64,
            persistent::DiskClip::remaining_ms,
        )
    }
}
fn current_retry_model(app: &AppHandle, candidates: &[(String, String)]) -> Option<String> {
    let store = app.store("settings").ok()?;
    let model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    if !model.is_empty() {
        return Some(model);
    }
    let engine = store
        .get("current_model_engine")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "whisper".into());
    candidates
        .iter()
        .find(|(_, e)| e == &engine)
        .map(|(m, _)| m.clone())
}
fn alternative_for(
    clip: &Clip,
    candidates: &[(String, String)],
    current: Option<&str>,
) -> Option<(String, String)> {
    candidates
        .iter()
        .find(|(model, engine)| {
            if let Some(selection) = &clip.retry_selection {
                selection.model != *model || selection.engine.as_ref() != Some(engine)
            } else {
                (clip.recovery.kind == RecoveryKind::RemoteOffline
                    || Some(model.as_str()) != current)
                    && crate::pill::context::engine_short_name(model, engine)
                        != clip.recovery.engine_short
            }
        })
        .cloned()
}
#[derive(Default)]
struct Store {
    // Installed before the first clip; every permanent deletion retires its UI id.
    app: Option<AppHandle>,
    clips: VecDeque<Clip>,
    closed: bool,
    leases: Vec<PathBuf>,
    borrowed_paths: HashMap<PathBuf, usize>,
    active_retry: Option<(String, u64, tokio::sync::watch::Sender<bool>)>,
    fallback: Vec<(PathBuf, Option<super::storage::CaptureFiles>)>,
}
static STORE: Lazy<Mutex<Store>> = Lazy::new(|| Mutex::new(Store::default()));

/// Probe files without the global store lock, then reconcile and snapshot in
/// Store -> status -> outbox order. The callback must not emit or re-enter.
pub(crate) fn with_live_recoveries<T>(work: impl FnOnce(&[serde_json::Value]) -> T) -> T {
    let probes = probe_store_availability(None);
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.apply_availability(probes);
    let now = Instant::now();
    // Expired leases are excluded here; their deadline task owns file deletion.
    // Readiness does no file work while holding STORE.
    work(&store.live_recoveries(now))
}
struct Availability {
    id: String,
    path: PathBuf,
    missing: bool,
    ready: bool,
    was_ready: bool,
    generation: u64,
    terminal: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Availability {
    fn snapshot(clip: &Clip) -> Self {
        Self {
            id: clip.recovery.id.clone(),
            path: clip.path.clone(),
            was_ready: clip.recovery.retry_ready,
            generation: clip.recovery.generation,
            terminal: clip.terminal.clone(),
            missing: false,
            ready: false,
        }
    }
    fn matches(&self, clip: &Clip) -> bool {
        clip.recovery.id == self.id
            && clip.path == self.path
            && clip.recovery.generation == self.generation
            && std::sync::Arc::ptr_eq(&clip.terminal, &self.terminal)
            && clip.recovery.retry_ready == self.was_ready
    }
}
fn probe_store_availability(include_busy: Option<&str>) -> Vec<Availability> {
    let paths = STORE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .availability_paths(include_busy);
    probe_availability(paths) // No STORE guard survives the snapshot statement.
}
fn probe_availability(mut paths: Vec<Availability>) -> Vec<Availability> {
    for probe in &mut paths {
        probe.missing = match std::fs::metadata(&probe.path) {
            Ok(metadata) => !metadata.is_file(),
            Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        };
        probe.ready = !probe.missing && finalized_wav(&probe.path);
    }
    paths
}

/// Borrow an existing file without taking ownership of its deletion. Registration
/// and maintenance deletion use the same store lock; overlapping jobs are counted.
pub(crate) struct ProtectedPath(PathBuf);

pub(crate) fn protect_path(path: &Path) -> ProtectedPath {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    super::file_jobs::refresh_retry(path);
    *store.borrowed_paths.entry(path.to_owned()).or_default() += 1;
    ProtectedPath(path.to_owned())
}

impl Drop for ProtectedPath {
    fn drop(&mut self) {
        let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(count) = store.borrowed_paths.get_mut(&self.0) {
            *count -= 1;
            if *count == 0 {
                store.borrowed_paths.remove(&self.0);
            }
        }
    }
}

impl Store {
    fn live_recoveries(&self, now: Instant) -> Vec<serde_json::Value> {
        self.clips
            .iter()
            .filter(|clip| !self.closed && !clip.busy && clip.remaining_ms(now) > 0)
            .filter_map(|clip| {
                let mut recovery = clip.recovery.clone();
                recovery.expires_in_ms = clip.remaining_ms(now);
                (recovery.expires_in_ms > 0).then(|| serde_json::to_value(recovery).unwrap())
            })
            .collect()
    }

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
        // Ownership setup remains serialized with discard/shutdown: splitting
        // validation, move and insertion needs a separate reservation protocol.
        // A missing source is not a pending writer. Never advertise a phantom retry.
        if !std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
            return Err("Recording expired or discarded".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let durable = dir.file_name().is_some_and(|n| n == persistent::DIRECTORY);
        let expires_in_ms = if durable {
            persistent::TTL_MS
        } else {
            kind.ttl_ms()
        };
        std::fs::create_dir_all(dir).map_err(|_| "Recovery storage unavailable")?;
        let target = dir.join(format!("{id}.wav"));
        let capture_files = super::storage::CaptureFiles::for_path(path);
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
            alt_engine_id: None,
            expires_in_ms,
            retry_ready: finalized,
            retry_failed: false,
        };
        let disk = durable.then(|| persistent::DiskClip::new(dir, &id));
        self.insert(Clip {
            _capture_files: capture_files,
            trace: crate::observability::snapshot(generation),
            _trace_lease: crate::observability::pin(generation),
            _retry_trace_lease: None,
            recovery: recovery.clone(),
            path: owned_path,
            expires: Instant::now() + Duration::from_millis(expires_in_ms),
            busy: false,
            retire_after_retry: false,
            announced: false,
            terminal: Default::default(),
            target,
            retry_generation: None,
            disk,
            retry_selection: None,
        })?;
        self.persist(&id);
        Ok(recovery)
    }
    fn cancel_retry(&mut self) {
        if let Some((_, _, cancel)) = self.active_retry.take() {
            let _ = cancel.send(true);
        }
    }
    fn discard(&mut self, id: &str) {
        if let Some(clip) = self.take(id) {
            super::storage::remove_finished_audio(&clip.path);
            if let Some(disk) = &clip.disk {
                if std::fs::symlink_metadata(&clip.path)
                    .err()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                {
                    disk.delete();
                }
            }
        }
    }
    /// Retire ownership/UI and commit deletion intent before removing audio.
    /// Missing-file reconciliation already probed the WAV outside STORE.
    fn take(&mut self, id: &str) -> Option<Clip> {
        if self
            .active_retry
            .as_ref()
            .is_some_and(|(active, _, _)| active == id)
        {
            self.cancel_retry();
        }
        let index = self.clips.iter().position(|c| c.recovery.id == id)?;
        let clip = self.clips.remove(index)?;
        if let Some(disk) = &clip.disk {
            disk.retire(&clip);
        }
        if let Some(app) = &self.app {
            crate::commands::pill_feedback::expire_kept_id(app, id);
        } else {
            crate::commands::pill_feedback::retire_kept_id(id);
        }
        Some(clip)
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
            if let Some(clip) = self.clips.iter().find(|c| c.recovery.id == id) {
                resolve_clip(clip, "discarded");
            }
            self.discard(&id);
        }
    }
    /// Expiry/cap pressure never cancels an accepted retry or deletes its
    /// original audio. Explicit Discard/Esc still uses discard() immediately.
    fn retire_automatically(&mut self, id: &str, resolution: &'static str) {
        if let Some(clip) = self.clips.iter_mut().find(|c| c.recovery.id == id) {
            if clip.busy {
                clip.retire_after_retry = true;
                return;
            }
            resolve_clip(clip, resolution);
        }
        self.discard(id);
    }
    fn expire(&mut self, now: Instant) {
        let ids: Vec<_> = self
            .clips
            .iter()
            .filter(|c| c.remaining_ms(now) == 0)
            .map(|c| c.recovery.id.clone())
            .collect();
        for id in ids {
            self.retire_automatically(&id, "expired");
        }
    }
    fn availability_paths(&self, include_busy: Option<&str>) -> Vec<Availability> {
        self.clips
            .iter()
            .filter(|c| !self.closed && (!c.busy || include_busy == Some(c.recovery.id.as_str())))
            .map(Availability::snapshot)
            .collect()
    }
    fn apply_availability(&mut self, probes: Vec<Availability>) {
        for probe in probes {
            let Some(clip) = self.clips.iter_mut().find(|c| !c.busy && probe.matches(c)) else {
                continue;
            };
            // Do not overwrite a finalizer publication that won while this
            // unlocked probe was reading the old placeholder header.
            clip.recovery.retry_ready = probe.ready;
            if probe.missing {
                drop(self.take(&probe.id));
            }
        }
    }
    fn insert(&mut self, clip: Clip) -> Result<(), String> {
        self.expire(Instant::now());
        if self.closed {
            return Err("Recovery is shutting down".into());
        }
        let durable = clip.disk.is_some();
        let cap = if durable { persistent::CAP } else { 3 };
        while self
            .clips
            .iter()
            .filter(|c| c.disk.is_some() == durable)
            .count()
            >= cap
        {
            // Evict only the oldest idle original. Once this satisfies the
            // cap, a busy original needs no deferred retirement.
            let id = self
                .clips
                .iter()
                .find(|old| old.disk.is_some() == durable && !old.busy)
                .map(|old| old.recovery.id.clone())
                .ok_or("Recovery is busy")?;
            self.retire_automatically(&id, "discarded");
        }
        self.clips.push_back(clip);
        Ok(())
    }
    fn complete_retry(&mut self, id: &str, succeeded: bool) {
        if self
            .active_retry
            .as_ref()
            .is_some_and(|(active, _, _)| active == id)
        {
            self.active_retry = None;
        }
        if succeeded
            || self.clips.iter().any(|c| {
                c.recovery.id == id && (c.retire_after_retry || c.remaining_ms(Instant::now()) == 0)
            })
        {
            self.discard(id);
        } else if let Some(clip) = self.clips.iter_mut().find(|c| c.recovery.id == id) {
            clip.busy = false;
            clip.retry_generation = None;
            clip.terminal = Default::default();
        }
    }
    fn promote(&mut self, id: &str) -> bool {
        if let Some(clip) = self.clips.iter_mut().find(|c| c.recovery.id == id) {
            if clip.path != clip.target
                && clip.recovery.retry_ready
                && move_finalized(&clip.path, &clip.target)
            {
                clip.path = clip.target.clone();
            }
            true
        } else {
            false
        }
    }
    fn poll_finalization(&mut self, id: &str, now: Instant) -> Option<(bool, Option<Recovery>)> {
        if !self.promote(id) {
            return None;
        }
        self.persist(id);
        let clip = self.clips.iter_mut().find(|c| c.recovery.id == id)?;
        let ready = clip.recovery.retry_ready && (clip.disk.is_none() || clip.path == clip.target);
        // A hydration/Home probe may have refreshed the cache first. That
        // must not suppress the notification to an already-rendered card.
        let publication = if ready && clip.announced && !clip.busy && clip.remaining_ms(now) > 0 {
            clip.recovery.expires_in_ms = clip.remaining_ms(now);
            Some(clip.recovery.clone())
        } else {
            None
        };
        Some((ready, publication))
    }
    fn clear(&mut self) {
        self.cancel_retry();
        for (path, _files) in self.fallback.drain(..) {
            super::storage::remove_finished_audio(&path);
        }
        for path in self.leases.drain(..) {
            let _ = std::fs::remove_file(path);
        }
        while let Some(clip) = self.clips.front() {
            let id = clip.recovery.id.clone();
            self.discard(&id);
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
        .any(|c| c.recovery.generation == generation && c.remaining_ms(Instant::now()) > 0)
}
/// Opaque recovery ID and display name only; never expose the private audio path.
pub fn tray_recovery() -> Option<(String, Option<String>, Option<String>, bool)> {
    STORE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clips
        .iter()
        .rev()
        .find(|c| c.remaining_ms(Instant::now()) > 0)
        .map(|c| {
            (
                c.recovery.id.clone(),
                c.recovery.alt_engine_short.clone(),
                c.recovery.alt_engine_id.clone(),
                c.busy,
            )
        })
}
/// What Home shows for a kept recording: opaque ID and engine names only.
#[derive(Serialize)]
pub struct KeptSummary {
    id: String,
    engine_short: String,
    alt_engine_short: Option<String>,
    alt_engine_id: Option<String>,
    remaining_ms: u64,
    busy: bool,
    retry_ready: bool,
    kind: RecoveryKind,
    retry_failed: bool,
}
/// Failed dictations still waiting for a retry, even after the island card is closed.
#[tauri::command]
pub async fn list_kept_dictations(app: AppHandle) -> Vec<KeptSummary> {
    let candidates = local_candidates(&app).await;
    let current = current_retry_model(&app, &candidates);
    let probes = probe_store_availability(None);
    let now = Instant::now();
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.apply_availability(probes);
    store.expire(now);
    for clip in &mut store.clips {
        let alternative = alternative_for(clip, &candidates, current.as_deref());
        clip.recovery.set_alternative(alternative);
    }
    store
        .clips
        .iter()
        .filter(|c| {
            c.remaining_ms(now) > 0 && !matches!(c.recovery.kind, RecoveryKind::MicDroppedEmpty)
        })
        .map(|c| KeptSummary {
            id: c.recovery.id.clone(),
            engine_short: c.recovery.engine_short.clone(),
            alt_engine_short: c.recovery.alt_engine_short.clone(),
            alt_engine_id: c.recovery.alt_engine_id.clone(),
            remaining_ms: c.remaining_ms(now),
            busy: c.busy,
            retry_ready: c.recovery.retry_ready,
            kind: c.recovery.kind,
            retry_failed: c.recovery.retry_failed,
        })
        .collect()
}
pub fn cancel_active_retry(app: &AppHandle) {
    let generation = {
        let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        let generation = store.active_retry.as_ref().map(|(_, g, _)| *g);
        store.cancel_retry();
        generation
    };
    finish_cancelled_retry(app, generation);
}
fn finish_cancelled_retry(app: &AppHandle, generation: Option<u64>) {
    if generation.is_some_and(|g| !audio::recording_generation_is_stale(g)) {
        app.state::<crate::AppState>().request_cancellation();
        // Retry has no capture guard or recorder to stop. Publish Idle now so
        // the caller may start a new take before the cancelled future unwinds.
        if crate::get_recording_state(app) == crate::RecordingState::Transcribing {
            crate::update_recording_state(app, crate::RecordingState::Idle, None);
        }
    }
}
pub fn alternative_id(id: &str) -> Option<String> {
    STORE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clips
        .iter()
        .find(|c| c.recovery.id == id && c.remaining_ms(Instant::now()) > 0)
        .and_then(|c| c.recovery.alt_engine_id.clone())
}
/// A fallback owner is installed before any fallible storage operation or await.
/// Failed handoffs expire with the same bounded lifetime as ordinary recovery.
pub async fn handoff(app: &AppHandle, generation: u64, path: &Path, kind: RecoveryKind) {
    STORE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .fallback
        .push((
            path.to_owned(),
            super::storage::CaptureFiles::for_path(path),
        ));
    match keep(app, generation, path, kind).await {
        Ok(()) => STORE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .fallback
            .retain(|(p, _)| p != path),
        Err(_) => {
            // Normal finalization must not delete the fallback owner's file.
            audio::clear_in_flight_transcription_audio_for_generation(generation);
            if audio::delivery_aborted(
                app.state::<crate::AppState>().is_cancellation_requested(),
                generation,
            ) {
                let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
                super::storage::remove_finished_audio(path);
                store.fallback.retain(|(p, _)| p != path);
                return;
            }
            let path = path.to_owned();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(kind.ttl_ms())).await;
                let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
                store.fallback.retain(|(p, _)| {
                    if p == &path {
                        super::storage::remove_finished_audio(p);
                        false
                    } else {
                        true
                    }
                });
            });
            if !audio::delivery_aborted(
                app.state::<crate::AppState>().is_cancellation_requested(),
                generation,
            ) {
                crate::telemetry::capture_error(
                    "kept_storage_failed",
                    crate::telemetry::ErrorContext {
                        generation: Some(generation),
                        ..Default::default()
                    },
                );
                super::island::note(app, generation, super::island::Note::StorageFailed);
            }
        }
    }
}
pub fn initialize(app: &AppHandle) {
    persistent::initialize(app);
}
pub fn expire() {
    persistent::expire();
}
pub fn cleanup() {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.closed = true;
    store.preserve_on_exit();
    store.clear();
    store.app = None;
}
pub fn clear() {
    STORE.lock().unwrap_or_else(|e| e.into_inner()).clear();
}
pub fn with_protected_paths<T>(work: impl FnOnce(&std::collections::HashSet<PathBuf>) -> T) -> T {
    let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let paths = store
        .clips
        .iter()
        .map(|c| c.path.clone())
        .chain(store.leases.iter().cloned())
        .chain(store.borrowed_paths.keys().cloned())
        .chain(store.fallback.iter().map(|(p, _)| p.clone()))
        .collect();
    work(&paths)
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
    let selected = app
        .store("settings")
        .ok()
        .and_then(|s| s.get("current_model"))
        .and_then(|v| v.as_str().map(str::to_owned));
    local_candidates(app)
        .await
        .into_iter()
        .find(|(m, _)| Some(m) != selected.as_ref())
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
    let settings = crate::commands::settings::get_settings(app.clone()).await?;
    let durable = !settings.save_recordings
        && settings.save_failed_recordings
        && kind != RecoveryKind::MicDroppedEmpty;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "Recovery storage unavailable")?
        .join(if durable {
            persistent::DIRECTORY
        } else {
            "recovery"
        });
    if std::fs::symlink_metadata(&dir).is_ok_and(|m| !m.is_dir() || persistent::alias(&m)) {
        return Err("Recovery storage unavailable".into());
    }
    let recovery = {
        let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        store.app.get_or_insert_with(|| app.clone());
        store.retain(&dir, path, generation, kind)?
    };
    let id = recovery.id.clone();
    let expires_in_ms = recovery.expires_in_ms;
    audio::clear_in_flight_transcription_audio_for_generation(generation);
    let expiry_id = id.clone();
    let expiry_app = app.clone();
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(expires_in_ms);
        loop {
            let probes = probe_store_availability(None);
            let finalized = {
                let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
                store.apply_availability(probes);
                store.poll_finalization(&expiry_id, Instant::now())
            };
            let Some((ready, publication)) = finalized else {
                return;
            };
            if let Some(recovery) = publication {
                emit_recovery(&expiry_app, &recovery);
            }
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
            .retire_automatically(&expiry_id, "expired");
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
    let probes = probe_store_availability(None);
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    if audio::delivery_aborted(
        app.state::<crate::AppState>().is_cancellation_requested(),
        generation,
    ) {
        store.discard(&id);
        return Err("Dictation discarded".into());
    }
    store.apply_availability(probes);
    let mut publication = None;
    if let Some(clip) = store.clips.iter_mut().find(|c| c.recovery.id == id) {
        clip.recovery.engine_short = if kind == RecoveryKind::RemoteOffline {
            "Network".into()
        } else {
            engine_short
        };
        clip.recovery.set_alternative(alternative);
        clip.announced = true;
        clip.recovery.expires_in_ms = clip.remaining_ms(Instant::now());
        if !audio::recording_generation_is_stale(generation) {
            publication = Some(clip.recovery.clone());
        }
    }
    store.persist(&id);
    drop(store);
    if let Some(recovery) = publication {
        emit_recovery(app, &recovery);
    }
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
pub(super) fn finalized_wav(path: &Path) -> bool {
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
    selection: Option<RetrySelection>,
    kind: RecoveryKind,
    file: tempfile::NamedTempFile,
    terminal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    trace: Option<crate::observability::Trace>,
    original_generation: u64,
    generation: u64,
    cancelled: tokio::sync::watch::Receiver<bool>,
}
struct RetryRegistration {
    generation: u64,
    cancel: tokio::sync::watch::Sender<bool>,
    cancelled: tokio::sync::watch::Receiver<bool>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        // An older lease must not release a newer retry of the same clip.
        if store.clips.iter().any(|clip| {
            clip.recovery.id == self.id && std::sync::Arc::ptr_eq(&clip.terminal, &self.terminal)
        }) {
            store.complete_retry(&self.id, false);
        }
        store.leases.retain(|p| p != self.file.path());
    }
}
impl Store {
    fn lease(
        &mut self,
        id: &str,
        anyway: bool,
        register: impl FnOnce() -> RetryRegistration,
    ) -> Result<Lease, String> {
        self.expire(Instant::now());
        if self.clips.iter().any(|c| c.busy && c.recovery.id != id) {
            return Err("Dictation is busy".into());
        }
        let clip = self
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
        // The caller refreshed this identity-checked cache before taking STORE.
        // Copy and active registration still share one critical section.
        if !clip.recovery.retry_ready {
            return Err("Recording is still finalizing".into());
        }
        let file = tempfile::Builder::new()
            .suffix(".wav")
            .tempfile_in(clip.path.parent().ok_or("Recovery unavailable")?)
            .map_err(|_| "Recovery unavailable")?;
        std::fs::copy(&clip.path, file.path()).map_err(|_| "Could not read kept recording")?;
        let kind = clip.recovery.kind;
        clip.terminal = Default::default();
        let terminal = clip.terminal.clone();
        let trace = clip.trace.clone();
        clip.busy = true;
        // Neither expiry nor explicit Discard can enter between lease acceptance
        // and cancellation registration. Generation advancement uses Store -> hide.
        let registered = register();
        let generation = registered.generation;
        clip.retry_generation = Some(generation);
        let original_generation = clip.recovery.generation;
        self.active_retry = Some((id.into(), generation, registered.cancel));
        self.leases.push(file.path().to_owned());
        Ok(Lease {
            id: id.into(),
            selection: clip.retry_selection.clone(),
            kind,
            file,
            terminal,
            trace,
            original_generation,
            generation,
            cancelled: registered.cancelled,
        })
    }
}
#[tauri::command]
pub fn discard_kept_dictation(app: AppHandle, id: String) {
    let generation = {
        let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        store
            .active_retry
            .as_ref()
            .filter(|(active, _, _)| active == &id)
            .map(|(_, g, _)| *g)
    };
    if generation.is_some_and(|g| !audio::recording_generation_is_stale(g)) {
        app.state::<crate::AppState>().request_cancellation();
    }
    resolve_id(&id, "discarded");
    STORE.lock().unwrap_or_else(|e| e.into_inner()).discard(&id);
    finish_cancelled_retry(&app, generation);
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
    let lease = retry_setup(state.stop_in_flight.clone(), || {
        let probes = probe_store_availability(None);
        let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        store.apply_availability(probes);
        store.lease(&id, anyway, || {
            state.clear_cancellation();
            let generation = audio::begin_recording_generation();
            let (cancel, cancelled) = tokio::sync::watch::channel(false);
            RetryRegistration {
                generation,
                cancel,
                cancelled,
            }
        })
    })?;
    let generation = lease.generation;
    crate::observability::inherit(lease.original_generation, generation, lease.trace.clone());
    let _trace_lease = crate::observability::pin(generation);
    let retry_started = Instant::now();
    if let Some(context) = crate::writing::capture_active_app_context() {
        state.set_recording_app_context(context);
    }
    // Retry never owns the capture-stop lock while awaiting decoding/network I/O.
    if crate::get_recording_state(&app) == crate::RecordingState::Error {
        crate::update_recording_state(&app, crate::RecordingState::Idle, None);
    }
    crate::update_recording_state(&app, crate::RecordingState::Starting, None);
    crate::update_recording_state(&app, crate::RecordingState::Recording, None);
    crate::update_recording_state(&app, crate::RecordingState::Stopping, None);
    crate::update_recording_state(&app, crate::RecordingState::Transcribing, None);
    crate::menu::runtime::refresh(&app);
    let alt_engine_kind = engine
        .as_deref()
        .map(|id| {
            if id.starts_with("parakeet") || id.starts_with("nemotron") {
                "parakeet"
            } else {
                "whisper"
            }
        })
        .unwrap_or("none");
    let mut attempted = None;
    let result = cancellable_retry(
        lease.cancelled.clone(),
        retry_inner(&app, &lease, generation, engine, &mut attempted),
    )
    .await;
    let cancelled = matches!(&result, Err(RetryError::Cancelled))
        || audio::delivery_aborted(state.is_cancellation_requested(), generation);
    let result = if cancelled {
        Err(RetryError::Cancelled)
    } else {
        result
    };
    let resolution = retry_resolution(result.is_ok(), cancelled, anyway);
    if claim_terminal(&lease.terminal) {
        resolved(
            lease.kind,
            generation,
            resolution,
            alt_engine_kind,
            retry_started.elapsed().as_millis() as u64,
            lease.trace.as_ref(),
        );
        if resolution == "retried_failed" {
            crate::telemetry::capture_error(
                "retry_failed",
                crate::telemetry::ErrorContext {
                    generation: Some(generation),
                    ..Default::default()
                },
            );
        }
    }
    if !audio::recording_generation_is_stale(generation) {
        crate::update_recording_state(&app, crate::RecordingState::Idle, None);
    }
    // A model may have been removed or the selected engine changed since this card's offer.
    let candidates = if result.is_err() && !cancelled {
        local_candidates(&app).await
    } else {
        Vec::new()
    };
    let current = current_retry_model(&app, &candidates);
    let probes = probe_store_availability(Some(&id));
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let discarded = audio::delivery_aborted(state.is_cancellation_requested(), generation);
    let mut missing = false;
    if let Some(clip) = store.clips.iter_mut().find(|c| c.recovery.id == id) {
        if let Some(probe) = probes.iter().find(|probe| probe.matches(clip)) {
            clip.recovery.retry_ready = probe.ready;
            missing = probe.missing;
        }
    }
    store.complete_retry(&id, result.is_ok());
    store.apply_availability(probes);
    if missing {
        drop(store.take(&id));
    }
    let retained = store.clips.iter().any(|c| c.recovery.id == id);
    let mut publication = None;
    if result.is_err() && !cancelled && !discarded {
        if let Some(clip) = store.clips.iter_mut().find(|c| c.recovery.id == id) {
            if matches!(&result, Err(RetryError::Failed)) {
                // Only a completed failure owns the failed-model label. A
                // cancelled/new-take retry keeps the preceding recovery intact.
                if let Some(selection) = attempted {
                    clip.recovery.engine_short = selection.short_name();
                    clip.retry_selection = Some(selection);
                }
                clip.recovery.retry_failed = true;
                clip._retry_trace_lease = crate::observability::pin(generation);
                clip.recovery.generation = generation;
                clip.recovery.expires_in_ms = clip.remaining_ms(Instant::now());
            }
            let alternative = alternative_for(clip, &candidates, current.as_deref());
            clip.recovery.set_alternative(alternative);
            if matches!(&result, Err(RetryError::Failed)) {
                publication = Some(clip.recovery.clone());
            }
        }
    }
    store.persist(&id);
    drop(store);
    if let Some(recovery) = publication {
        emit_recovery(&app, &recovery);
    }
    crate::menu::runtime::refresh(&app);
    result.map_err(|error| retry_error_message(error, retained).to_owned())
}
fn retry_setup<T>(
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    setup: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _guard = audio::StopInFlightGuard::try_acquire(flag).ok_or("Dictation is busy")?;
    setup()
}
#[derive(Debug, PartialEq, Eq)]
enum RetryError {
    Cancelled,
    EngineUnavailable,
    Failed,
}
fn retry_error_message(error: RetryError, retained: bool) -> &'static str {
    match error {
        RetryError::Cancelled => "Retry discarded",
        _ if !retained => "Retry failed; recording is no longer available",
        RetryError::EngineUnavailable => "Requested local engine is not ready",
        RetryError::Failed => "Retry failed; recording remains available until expiry",
    }
}
fn claim_terminal(terminal: &std::sync::atomic::AtomicBool) -> bool {
    !terminal.swap(true, std::sync::atomic::Ordering::SeqCst)
}
fn retry_resolution(succeeded: bool, cancelled: bool, anyway: bool) -> &'static str {
    if cancelled {
        "discarded"
    } else if succeeded {
        if anyway {
            "transcribed_anyway"
        } else {
            "retried_ok"
        }
    } else {
        "retried_failed"
    }
}
async fn cancellable_retry<T>(
    mut cancelled: tokio::sync::watch::Receiver<bool>,
    work: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, RetryError> {
    if *cancelled.borrow() {
        return Err(RetryError::Cancelled);
    }
    tokio::select! {
        biased;
        _ = cancelled.changed() => Err(RetryError::Cancelled),
        result = work => result.map_err(|error| {
            if error == "Requested local engine is not ready" {
                RetryError::EngineUnavailable
            } else {
                RetryError::Failed
            }
        }),
    }
}
async fn retry_inner(
    app: &AppHandle,
    lease: &Lease,
    generation: u64,
    engine: Option<String>,
    attempted: &mut Option<RetrySelection>,
) -> Result<(), String> {
    let settings = crate::commands::settings::get_settings(app.clone()).await?;
    let selected = RetrySelection::resolve(
        &settings,
        lease.kind,
        lease.selection.as_ref(),
        engine.as_deref(),
        &local_candidates(app).await,
    )?;
    let model = selected.model.clone();
    let engine = selected.engine.clone();
    *attempted = Some(selected);
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
    // Polish changed the words when the pipeline kept the pre-Polish text.
    let polished = output
        .metadata
        .as_ref()
        .is_some_and(|m| m.get("original_text").is_some());
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
                polished,
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
                polished,
            );
        }
        result
    })
    .ok_or("Retry discarded")?;
    future.await
}

fn resolved(
    kind: RecoveryKind,
    generation: u64,
    resolution: &str,
    alt: &str,
    latency_ms: u64,
    trace: Option<&crate::observability::Trace>,
) {
    crate::observability::update(
        generation,
        "recovery_kind",
        serde_json::to_value(kind).unwrap(),
    );
    crate::observability::update(generation, "recovery_resolution", resolution.into());
    crate::observability::emit_with_trace(
        "recovery_resolved",
        vec![
            ("kind", serde_json::to_value(kind).unwrap()),
            ("resolution", resolution.into()),
            ("alt_engine_kind", alt.into()),
            ("latency_ms", latency_ms.min(86_400_000).into()),
        ],
        Some(generation),
        trace,
    );
}
fn resolve_id(id: &str, resolution: &str) {
    let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(clip) = store.clips.iter().find(|c| c.recovery.id == id) {
        resolve_clip(clip, resolution);
    }
}
fn resolve_clip(clip: &Clip, resolution: &str) {
    if !claim_terminal(&clip.terminal) {
        return;
    }
    let ttl = if clip.disk.is_some() {
        persistent::TTL_MS
    } else {
        clip.recovery.kind.ttl_ms()
    };
    let latency = ttl.saturating_sub(clip.remaining_ms(Instant::now()));
    resolved(
        clip.recovery.kind,
        clip.retry_generation.unwrap_or(clip.recovery.generation),
        resolution,
        "none",
        latency,
        clip.trace.as_ref(),
    );
}

#[cfg(test)]
#[path = "kept_availability_tests.rs"]
mod availability_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_snapshot_checks_expiry_busy_retry_deletion_and_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let clip = clip(dir.path(), RecoveryKind::CloudFailed);
        let id = clip.recovery.id.clone();
        let expires = clip.expires;
        store.insert(clip).unwrap();
        let now = expires - Duration::from_millis(1234);
        let live = store.live_recoveries(now);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0]["id"], id);
        assert_eq!(live[0]["expires_in_ms"], 1234);
        // Expired, even before the expiry task removes it from the store.
        assert!(store.live_recoveries(expires).is_empty());
        store.clips[0].busy = true;
        assert!(store.live_recoveries(now).is_empty());
        store.complete_retry(&id, false);
        assert_eq!(store.live_recoveries(now).len(), 1);
        store.closed = true;
        assert!(store.live_recoveries(now).is_empty());
        store.closed = false;
        store.complete_retry(&id, true);
        assert!(store.live_recoveries(now).is_empty());
        assert!(store.clips.is_empty());
    }

    #[tokio::test]
    async fn capture_handoff_preserves_no_speech_and_failed_retry_audio_until_bounded_expiry() {
        for kind in [RecoveryKind::NoSpeech, RecoveryKind::CloudFailed] {
            let data = tempfile::tempdir().unwrap();
            let files = super::super::storage::CaptureFiles::create(data.path()).unwrap();
            let source = files.audio_path();
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
            files.writer_stopped(false);
            let mut store = Store::default();
            let recovery = store
                .retain(&data.path().join("recovery"), &source, u64::MAX, kind)
                .unwrap();
            let kept = store.clips[0].path.clone();
            let expires = store.clips[0].expires;
            drop(files);
            assert!(
                !source.exists(),
                "handoff moves, rather than duplicates, capture audio"
            );
            assert!(kept.exists());
            store.expire(expires - Duration::from_millis(1));
            assert!(kept.exists());
            assert_eq!(
                recovery.expires_in_ms,
                if kind == RecoveryKind::NoSpeech {
                    30_000
                } else {
                    600_000
                }
            );
            store.expire(expires);
            assert!(!kept.exists());
            tokio::time::timeout(Duration::from_secs(15), async {
                while source.parent().unwrap().exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
        }
    }
    fn clip(dir: &Path, kind: RecoveryKind) -> Clip {
        let id = uuid::Uuid::new_v4().to_string();
        let path = dir.join(&id);
        super::availability_tests::writer(&path).finalize().unwrap();
        Clip {
            _capture_files: None,
            trace: None,
            _trace_lease: None,
            _retry_trace_lease: None,
            recovery: Recovery {
                generation: 1,
                id,
                kind,
                engine_short: "Whisper".into(),
                alt_engine_short: None,
                alt_engine_id: None,
                expires_in_ms: kind.ttl_ms(),
                retry_ready: true,
                retry_failed: false,
            },
            target: path.clone(),
            path,
            retry_generation: None,
            disk: None,
            retry_selection: None,
            expires: Instant::now() + Duration::from_millis(kind.ttl_ms()),
            busy: false,
            retire_after_retry: false,
            announced: false,
            terminal: Default::default(),
        }
    }
    #[test]
    fn recovery_resolution_uses_saved_uuid_after_mapping_eviction() {
        let dir = tempfile::tempdir().unwrap();
        let mut clip = clip(dir.path(), RecoveryKind::CloudFailed);
        clip.recovery.generation = u64::MAX;
        let id = uuid::Uuid::new_v4().to_string();
        clip.trace = Some(crate::observability::Trace {
            generation: u64::MAX,
            id: id.clone(),
            properties: vec![],
        });
        assert!(crate::observability::trace_id(u64::MAX).is_none());
        crate::product_analytics::take_test_captures();
        resolve_clip(&clip, "expired");
        resolve_clip(&clip, "discarded");
        let events = crate::product_analytics::take_test_captures();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "recovery_resolved");
        assert_eq!(events[0].2.as_deref(), Some(id.as_str()));
    }
    #[tokio::test]
    async fn explicitly_discarded_retry_has_one_terminal_and_no_failure() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let mut clip = clip(dir.path(), RecoveryKind::CloudFailed);
        clip.retry_generation = Some(2);
        clip.busy = true;
        let id = clip.recovery.id.clone();
        let terminal = clip.terminal.clone();
        store.insert(clip).unwrap();
        let (cancel, cancelled) = tokio::sync::watch::channel(false);
        store.active_retry = Some((id, 2, cancel));
        store.discard_generation(2);
        assert_eq!(
            cancellable_retry(cancelled, std::future::pending::<Result<(), String>>()).await,
            Err(RetryError::Cancelled)
        );
        assert!(
            !claim_terminal(&terminal),
            "a second terminal would be emitted"
        );
        assert_eq!(retry_resolution(false, true, false), "discarded");
        assert_eq!(retry_resolution(false, false, false), "retried_failed");
        assert_eq!(retry_resolution(true, false, true), "transcribed_anyway");
    }
    fn registration() -> RetryRegistration {
        let (cancel, cancelled) = tokio::sync::watch::channel(false);
        RetryRegistration {
            generation: 77,
            cancel,
            cancelled,
        }
    }
    #[tokio::test]
    async fn no_speech_retry_at_29s_survives_30s_then_success_or_failure_retires_audio() {
        for succeeds in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let mut store = Store::default();
            let mut clip = clip(dir.path(), RecoveryKind::NoSpeech);
            // The 30-second lease has one second left when Retry is accepted.
            clip.expires = Instant::now() + Duration::from_secs(1);
            let deadline = clip.expires;
            let id = clip.recovery.id.clone();
            let path = clip.path.clone();
            store.insert(clip).unwrap();
            let lease = store.lease(&id, true, registration).unwrap();
            assert!(store.clips[0].busy);
            assert_eq!(store.clips[0].retry_generation, Some(77));
            assert_eq!(store.active_retry.as_ref().unwrap().1, 77);
            let (finish, finished) = tokio::sync::oneshot::channel();
            let retry =
                cancellable_retry(lease.cancelled.clone(), async { finished.await.unwrap() });
            tokio::pin!(retry);
            assert!(tokio::time::timeout(Duration::from_millis(1), &mut retry)
                .await
                .is_err());
            store.expire(deadline);
            assert!(
                path.exists(),
                "expiry removed the accepted retry's original"
            );
            assert!(lease.file.path().exists());
            assert!(!*lease.cancelled.borrow(), "expiry cancelled accepted work");
            assert!(store.live_recoveries(deadline).is_empty());
            assert!(tokio::time::timeout(Duration::from_millis(1), &mut retry)
                .await
                .is_err());
            finish
                .send(if succeeds {
                    Ok(())
                } else {
                    Err("provider failure".into())
                })
                .unwrap();
            let result = retry.await;
            assert_eq!(
                result,
                if succeeds {
                    Ok(())
                } else {
                    Err(RetryError::Failed)
                }
            );
            assert!(claim_terminal(&lease.terminal));
            store.complete_retry(&id, succeeds);
            assert!(!path.exists());
            assert!(store.clips.is_empty());
            assert!(store.active_retry.is_none());
            assert!(!claim_terminal(&lease.terminal));
            store.clear();
        }
    }
    #[test]
    fn cap_eviction_preserves_busy_original_unless_retry_succeeds() {
        for succeeds in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let mut store = Store::default();
            let first = clip(dir.path(), RecoveryKind::CloudFailed);
            let id = first.recovery.id.clone();
            let path = first.path.clone();
            store.insert(first).unwrap();
            let lease = store.lease(&id, false, registration).unwrap();
            for _ in 0..3 {
                store
                    .insert(clip(dir.path(), RecoveryKind::Integrity))
                    .unwrap();
            }
            assert_eq!(store.clips.len(), 3);
            assert!(path.exists());
            assert!(!*lease.cancelled.borrow());
            assert!(!store.clips[0].retire_after_retry);
            store.complete_retry(&id, succeeds);
            assert_eq!(path.exists(), !succeeds);
            assert_eq!(store.clips.len(), if succeeds { 2 } else { 3 });
            store.clear();
        }
    }
    #[test]
    fn retired_retry_failure_never_claims_audio_is_still_kept() {
        assert_eq!(
            retry_error_message(RetryError::Failed, false),
            "Retry failed; recording is no longer available"
        );
        assert_eq!(
            retry_error_message(RetryError::Cancelled, false),
            "Retry discarded"
        );
        assert_eq!(
            retry_error_message(RetryError::Failed, true),
            "Retry failed; recording remains available until expiry"
        );
    }
    #[test]
    fn another_retry_cannot_replace_the_accepted_cancellation_registration() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        store
            .insert(clip(dir.path(), RecoveryKind::CloudFailed))
            .unwrap();
        store
            .insert(clip(dir.path(), RecoveryKind::CloudFailed))
            .unwrap();
        let first = store.clips[0].recovery.id.clone();
        let second = store.clips[1].recovery.id.clone();
        let lease = store.lease(&first, false, registration).unwrap();
        assert_eq!(
            store.lease(&second, false, registration).err().as_deref(),
            Some("Dictation is busy")
        );
        assert_eq!(store.active_retry.as_ref().unwrap().0, first);
        assert!(!*lease.cancelled.borrow());
        store.clear();
    }
    #[tokio::test]
    async fn cancel_pending_retry_then_new_recording_can_stop() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let flag = Arc::new(AtomicBool::new(false));
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let clip = clip(dir.path(), RecoveryKind::CloudFailed);
        let id = clip.recovery.id.clone();
        store.insert(clip).unwrap();
        retry_setup(flag.clone(), || Ok(())).unwrap();
        let (cancel, cancelled) = tokio::sync::watch::channel(false);
        store.active_retry = Some((id, 2, cancel));
        store.clips[0].retry_generation = Some(2);
        let retry = cancellable_retry(cancelled, std::future::pending::<Result<(), String>>());
        tokio::pin!(retry);
        // Poll the pending decode before Escape/Discard cancels its generation.
        assert!(tokio::time::timeout(Duration::from_millis(1), &mut retry)
            .await
            .is_err());
        assert!(
            !flag.load(Ordering::SeqCst),
            "retry must not own capture Stop"
        );
        store.discard_generation(2);
        assert!(tokio::time::timeout(Duration::from_secs(1), retry)
            .await
            .unwrap()
            .is_err());
        // The next take's stop path can acquire the actual StopInFlightGuard.
        let recording = AtomicBool::new(true);
        let stop =
            audio::StopInFlightGuard::try_acquire(flag.clone()).expect("next Stop was disabled");
        recording.store(false, Ordering::SeqCst);
        drop(stop);
        assert!(!recording.load(Ordering::SeqCst));
        assert!(!flag.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn new_take_cancels_retry_and_preserves_the_kept_clip() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let mut clip = clip(dir.path(), RecoveryKind::CloudFailed);
        clip.busy = true;
        let id = clip.recovery.id.clone();
        let path = clip.path.clone();
        store.insert(clip).unwrap();
        let (cancel, cancelled) = tokio::sync::watch::channel(false);
        store.active_retry = Some((id.clone(), 2, cancel));
        store.cancel_retry();
        assert!(
            cancellable_retry(cancelled, std::future::pending::<Result<(), String>>())
                .await
                .is_err()
        );
        store.complete_retry(&id, false);
        assert!(path.exists());
        assert!(!store.clips[0].busy);
        store.clear();
    }
    #[test]
    fn failed_storage_handoff_keeps_fallback_until_exit() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("capture.wav");
        std::fs::write(&source, b"capture").unwrap();
        let unavailable = dir.path().join("not-a-directory");
        std::fs::write(&unavailable, b"blocked").unwrap();
        let mut store = Store::default();
        store.fallback.push((source.clone(), None));
        assert!(store
            .retain(&unavailable, &source, 1, RecoveryKind::Integrity)
            .is_err());
        assert!(source.exists());
        assert!(store.clips.is_empty());
        store.clear();
        assert!(!source.exists());
    }
    #[test]
    fn alternative_keeps_model_identifier_separate_from_label() {
        let dir = tempfile::tempdir().unwrap();
        let mut clip = clip(dir.path(), RecoveryKind::RemoteOffline);
        clip.recovery
            .set_alternative(Some(("small.en".into(), "whisper".into())));
        let value = serde_json::to_value(clip.recovery).unwrap();
        assert_eq!(value["alt_engine_short"], "Whisper Small");
        assert_eq!(value["alt_engine_id"], "small.en");
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
    fn discard_retires_only_its_queued_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let first = clip(dir.path(), RecoveryKind::NoSpeech);
        let id = first.recovery.id.clone();
        let other = clip(dir.path(), RecoveryKind::CloudFailed);
        let other_id = other.recovery.id.clone();
        store.insert(first).unwrap();
        store.insert(other).unwrap();
        queue_recoveries(&store);
        store.discard(&id);
        assert!(!recovery_is_queued(&id));
        assert!(recovery_is_queued(&other_id));
        store.clear();
        assert!(!recovery_is_queued(&other_id));
    }
    #[test]
    fn successful_retry_retires_queued_recovery_but_failure_preserves_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let clip = clip(dir.path(), RecoveryKind::CloudFailed);
        let id = clip.recovery.id.clone();
        store.insert(clip).unwrap();
        queue_recoveries(&store);
        store.complete_retry(&id, false);
        assert!(recovery_is_queued(&id));
        store.complete_retry(&id, true);
        assert!(!recovery_is_queued(&id));
    }
    #[test]
    fn generation_expiry_eviction_and_reset_also_retire_queued_recoveries() {
        for deletion in ["generation", "expiry", "eviction", "reset"] {
            let dir = tempfile::tempdir().unwrap();
            let mut store = Store::default();
            let first = clip(dir.path(), RecoveryKind::NoSpeech);
            let id = first.recovery.id.clone();
            store.insert(first).unwrap();
            queue_recoveries(&store);
            match deletion {
                "generation" => store.discard_generation(1),
                "expiry" => store.expire(Instant::now() + Duration::from_secs(31)),
                "eviction" => {
                    for _ in 0..3 {
                        store
                            .insert(clip(dir.path(), RecoveryKind::Integrity))
                            .unwrap();
                    }
                }
                "reset" => store.clear(),
                _ => unreachable!(),
            }
            assert!(!recovery_is_queued(&id), "{deletion}");
            store.clear();
        }
    }
    fn queue_recoveries(store: &Store) {
        crate::commands::pill_feedback::with_queue_for_test(|queued| {
            for clip in &store.clips {
                queued.push((
                    "dictation-recovery",
                    serde_json::to_value(&clip.recovery).unwrap(),
                ));
            }
        });
    }
    fn recovery_is_queued(id: &str) -> bool {
        crate::commands::pill_feedback::with_queue_for_test(|queued| {
            queued.iter().any(|(event, payload)| {
                *event == "dictation-recovery" && payload["id"].as_str() == Some(id)
            })
        })
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
            assert_eq!(value.as_object().unwrap().len(), 8);
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
            let mut expected = serde_json::to_value(&recovery).unwrap();
            expected["renderer_epoch"] = 0.into();
            assert_eq!(received.lock().unwrap().last(), Some(&expected));
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
