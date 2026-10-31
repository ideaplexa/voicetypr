//! Capture ownership and best-effort maintenance, never run on the audio callback.
//! Saved recordings stay in `recordings`; disposable takes live in `captures`.
use once_cell::sync::Lazy;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, SystemTime},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

const ORPHAN_AGE: Duration = Duration::from_secs(10 * 60);
static ACTIVE: Lazy<Mutex<HashMap<PathBuf, Weak<CaptureDirectory>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
pub(crate) struct CaptureFiles(Arc<CaptureDirectory>);
struct CaptureDirectory {
    path: PathBuf,
    writer_pending: AtomicBool,
}

impl CaptureFiles {
    pub fn create(app_data: &Path) -> Result<Self, String> {
        let path = app_data
            .join("captures")
            .join(uuid::Uuid::new_v4().to_string());
        // Register before creating any files. The sweep holds this same lock.
        let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        std::fs::create_dir_all(&path).map_err(|_| "Capture storage unavailable")?;
        let files = Self(Arc::new(CaptureDirectory {
            path: path.clone(),
            writer_pending: AtomicBool::new(true),
        }));
        active.insert(path, Arc::downgrade(&files.0));
        Ok(files)
    }
    pub fn audio_path(&self) -> PathBuf {
        self.0.path.join("recording.wav")
    }
    pub fn writer_stopped(&self, pending: bool) {
        self.0.writer_pending.store(pending, Ordering::Release);
    }
    pub fn for_path(path: &Path) -> Option<Self> {
        ACTIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(path.parent()?)
            .and_then(Weak::upgrade)
            .map(Self)
    }
}

pub fn writer_may_be_running(error: &str) -> bool {
    error.contains("within timeout")
        || error.contains("Recording thread panicked")
        || error.contains("Failed to acquire recorder lock")
}

impl Drop for CaptureDirectory {
    fn drop(&mut self) {
        let path = self.path.clone();
        let pending = self.writer_pending.load(Ordering::Acquire);
        // Aborted async tasks still release their take. A timed-out writer must
        // finish its RIFF header before deletion; after a crash the startup sweep
        // can remove the abandoned directory without guessing about liveness.
        tauri::async_runtime::spawn(async move {
            loop {
                let dir = path.clone();
                let done = tauri::async_runtime::spawn_blocking(move || {
                    let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
                    let raw = dir.join("recording.wav");
                    if pending && raw.exists() && !super::kept::finalized_wav(&raw) {
                        return false;
                    }
                    remove_capture_files(&dir);
                    active.remove(&dir);
                    true
                })
                .await
                .unwrap_or(true);
                if done {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }
}

pub fn remove_finished_audio(path: &Path) {
    if CaptureFiles::for_path(path).is_some_and(|files| {
        files.0.writer_pending.load(Ordering::Acquire) && !super::kept::finalized_wav(path)
    }) {
        return; // The capture owner's deferred finalizer will remove it.
    }
    let _ = std::fs::remove_file(path);
}

fn capture_name(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "wav")
        && path.file_stem().and_then(|n| n.to_str()).is_some_and(|n| {
            n == "recording"
                || n.strip_prefix("recording_")
                    .is_some_and(|t| t.parse::<u64>().is_ok())
                || n.strip_prefix("normalized_").is_some_and(|t| {
                    chrono::NaiveDateTime::parse_from_str(t, "%Y%m%d_%H%M%S").is_ok()
                })
        })
}
fn recovery_name(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "wav")
        && path
            .file_stem()
            .and_then(|n| n.to_str())
            .is_some_and(|n| uuid::Uuid::parse_str(n).is_ok() || n.starts_with(".tmp"))
}
fn saved_name(path: &Path) -> bool {
    path.file_stem()
        .and_then(|n| n.to_str())
        .and_then(|n| n.rsplit_once('_'))
        .is_some_and(|(timestamp, id)| {
            id.len() == 8
                && id.bytes().all(|c| c.is_ascii_hexdigit())
                && chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d_%H-%M-%S").is_ok()
        })
}
fn old_enough(path: &Path, now: SystemTime, age: Duration) -> bool {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|m| m.is_file())
        .and_then(|m| m.modified().ok())
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|elapsed| elapsed >= age)
}
fn entries(dir: &Path) -> impl Iterator<Item = PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
}
fn remove_capture_files(dir: &Path) -> usize {
    let removed = entries(dir)
        .filter(|p| capture_name(p) && p.is_file())
        .filter(|p| std::fs::remove_file(p).is_ok())
        .count();
    let _ = std::fs::remove_dir(dir); // Keep unrelated files/directories.
    removed
}

#[derive(Default)]
struct Counts {
    orphans: usize,
    expired: usize,
}

/// Only known disposable names are orphan candidates. Unreferenced saved
/// timestamp/UUID names remain intentional saves (History can be deleted).
fn sweep(
    data: &Path,
    legacy_recovery: Option<&Path>,
    history: Option<&HashSet<String>>,
    protected: &HashSet<PathBuf>,
    active: &HashSet<PathBuf>,
    retention: Option<u32>,
    now: SystemTime,
) -> Counts {
    let mut counts = Counts::default();
    for dir in entries(&data.join("captures")) {
        if active.contains(&dir)
            || dir
                .file_name()
                .and_then(|n| n.to_str())
                .is_none_or(|n| uuid::Uuid::parse_str(n).is_err())
            || std::fs::symlink_metadata(&dir).is_ok_and(|m| !m.is_dir())
        {
            continue;
        }
        for path in entries(&dir) {
            if capture_name(&path)
                && !protected.contains(&path)
                && old_enough(&path, now, ORPHAN_AGE)
            {
                counts.orphans += usize::from(std::fs::remove_file(&path).is_ok());
            }
        }
        let _ = std::fs::remove_dir(dir);
    }
    for dir in std::iter::once(data.join("recovery")).chain(legacy_recovery.map(Path::to_owned)) {
        for path in entries(&dir) {
            if recovery_name(&path)
                && !protected.contains(&path)
                && old_enough(&path, now, ORPHAN_AGE)
            {
                counts.orphans += usize::from(std::fs::remove_file(path).is_ok());
            }
        }
    }
    for path in entries(&data.join("recordings")) {
        if protected.contains(&path) || path.extension().is_none_or(|e| e != "wav") {
            continue;
        }
        let referenced = history.is_some_and(|h| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| h.contains(n))
        });
        if capture_name(&path) && !referenced {
            // Fail closed if History could not be read.
            if history.is_some() && old_enough(&path, now, ORPHAN_AGE) {
                counts.orphans += usize::from(std::fs::remove_file(&path).is_ok());
            }
        } else if (saved_name(&path) || referenced)
            && retention.is_some_and(|days| {
                old_enough(&path, now, Duration::from_secs(u64::from(days) * 86400))
            })
        {
            counts.expired += usize::from(std::fs::remove_file(&path).is_ok());
        }
    }
    counts
}

fn maintenance(app: &AppHandle) {
    let Ok(data) = app.path().app_data_dir() else {
        return;
    };
    let legacy = app.path().temp_dir().ok().map(|p| p.join("kept-dictation"));
    let retention = app
        .store("settings")
        .ok()
        .and_then(|s| crate::commands::settings::recording_retention_days_from_store(&s));
    let history = app.store("transcriptions").ok().map(|s| {
        s.keys()
            .into_iter()
            .filter_map(|k| s.get(k))
            .filter_map(|row| {
                row.get("recording_file")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            })
            .collect::<HashSet<_>>()
    });
    // Lock order matches recovery handoff: kept store, then capture registry.
    // Holding these across deletion closes the snapshot-to-delete race.
    super::kept::with_protected_paths(|protected| {
        let active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        let counts = sweep(
            &data,
            legacy.as_deref(),
            history.as_ref(),
            protected,
            &active.keys().cloned().collect(),
            retention,
            SystemTime::now(),
        );
        log::info!(
            "Recording cleanup: orphans={}, retention={}",
            counts.orphans,
            counts.expired
        );
    });
}

pub fn start_maintenance(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            maintain(app.clone()).await;
            tokio::time::sleep(Duration::from_secs(5 * 60)).await;
        }
    });
}
async fn maintain(app: AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(move || maintenance(&app)).await;
}
pub fn schedule_maintenance(app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || maintenance(&app));
}

/// Reset removes both opted-in saves and ephemeral audio. Clearing History
/// alone is not consent to delete intentionally saved recordings.
/// Detached writers remain protected until their finalizer releases ownership.
pub async fn clear_audio(app: &AppHandle) -> Result<(), String> {
    // Continue clearing even if stop timed out; its detached writer remains
    // owned by the capture finalizer and is removed once it is safe.
    let _ = crate::commands::audio::cancel_recording(app.clone()).await;
    super::kept::clear();
    let data = app
        .path()
        .app_data_dir()
        .map_err(|_| "Audio storage unavailable")?;
    let legacy = app.path().temp_dir().ok().map(|p| p.join("kept-dictation"));
    tauri::async_runtime::spawn_blocking(move || clear_storage(&data, legacy))
        .await
        .map_err(|_| "Could not clear audio storage")?
}

fn clear_storage(data: &Path, legacy: Option<PathBuf>) -> Result<(), String> {
    let active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    for dir in [
        Some(data.join("recordings")),
        Some(data.join("recovery")),
        legacy,
    ]
    .into_iter()
    .flatten()
    {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Could not clear audio storage".into()),
        }
    }
    for dir in entries(&data.join("captures")) {
        if !active.contains_key(&dir) {
            remove_capture_files(&dir);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
