//! Private, versioned per-take index. No user content or source paths.
use super::*;
use std::io::Write;

pub(super) const DIRECTORY: &str = "failed-recordings";
pub(super) const TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
pub(super) const CAP: usize = 20;

pub(super) fn alias(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.is_symlink()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(super) struct DiskClip {
    index: PathBuf,
    expires_at_ms: u64,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    version: u8,
    expires_at_ms: u64,
    recovery: Recovery,
    #[serde(default)]
    selection: Option<RetrySelection>,
    // Opaque app-owned capture UUID only, never an arbitrary source path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture_id: Option<String>,
}

fn capture_source(dir: &Path, id: &str) -> Option<PathBuf> {
    uuid::Uuid::parse_str(id).ok()?;
    let audio = super::super::storage::capture_audio_path(dir.parent()?, id);
    let capture = audio.parent()?;
    for path in [capture.parent()?, capture] {
        if !std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !alias(&m)) {
            return None;
        }
    }
    Some(audio)
}

fn capture_id(clip: &Clip, dir: &Path) -> Option<String> {
    let id = clip.path.parent()?.file_name()?.to_str()?;
    (capture_source(dir, id)? == clip.path).then(|| id.into())
}

impl DiskClip {
    pub(super) fn new(dir: &Path, id: &str) -> Self {
        Self {
            index: dir.join(format!("{id}.json")),
            expires_at_ms: now_ms() + TTL_MS,
        }
    }

    pub(super) fn remaining_ms(&self) -> u64 {
        self.expires_at_ms.saturating_sub(now_ms())
    }

    pub(super) fn delete(&self, audio: &Path) {
        // Commit the unlink before removing its tombstone, not the reverse.
        if durable::sync_parent(audio).is_ok() && std::fs::remove_file(&self.index).is_ok() {
            let _ = durable::sync_parent(&self.index);
        }
    }

    pub(super) fn retire(&self, clip: &Clip) -> std::io::Result<()> {
        // Commit deletion intent before unlinking. A crash or sharing violation
        // must not turn deliberately retired audio into a recovered orphan.
        self.write_deadline(clip, 0)
    }

    fn write(&self, clip: &Clip) -> std::io::Result<()> {
        // Audio bytes/header, then WAV name, then synced JSON replacement.
        // No committed index may point at audio not yet flushed to storage.
        durable::sync_file(&clip.path)?;
        durable::sync_parent(&clip.path)?;
        let dir = self.index.parent().unwrap();
        durable::sync_parent(dir)?;
        if capture_id(clip, dir).is_some() {
            let capture_dir = clip.path.parent().unwrap();
            durable::sync_parent(capture_dir)?;
            durable::sync_parent(capture_dir.parent().unwrap())?;
        }
        self.write_deadline(
            clip,
            if clip.retire_after_retry {
                0
            } else {
                self.expires_at_ms
            },
        )
    }

    fn write_deadline(&self, clip: &Clip, expires_at_ms: u64) -> std::io::Result<()> {
        if !std::fs::symlink_metadata(self.index.parent().unwrap())
            .is_ok_and(|m| m.is_dir() && !alias(&m))
        {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        let entry = Entry {
            version: 1,
            expires_at_ms,
            recovery: clip.recovery.clone(),
            selection: clip.retry_selection.clone(),
            capture_id: capture_id(clip, self.index.parent().unwrap()),
        };
        let mut file = tempfile::Builder::new()
            .prefix(".index-")
            .tempfile_in(self.index.parent().unwrap())?;
        serde_json::to_writer(&mut file, &entry)?;
        file.flush()?;
        durable::replace_index(file, &self.index)
    }
}

impl Store {
    pub(super) fn persist(&self, id: &str) {
        let Some(clip) = self.clips.iter().find(|c| c.recovery.id == id) else {
            return;
        };
        if let Some(disk) = &clip.disk {
            // Pending writers keep an opaque capture reference until promotion.
            if (clip.path == clip.target
                || capture_id(clip, disk.index.parent().unwrap()).is_some())
                && disk.write(clip).is_err()
            {
                log::warn!("Could not persist failed recording recovery");
                if let Some(app) = &self.app {
                    super::super::island::note(
                        app,
                        clip.recovery.generation,
                        super::super::island::Note::StorageFailed,
                    );
                }
            }
        }
    }

    pub(super) fn preserve_on_exit_until(&mut self, deadline: Instant) {
        // Disarm capture cleanup before the first possibly blocking file call.
        // A timed-out exit worker must never drop an unpromoted only copy.
        for clip in &self.clips {
            if clip.disk.is_some() {
                if let Some(files) = &clip._capture_files {
                    files.preserve_on_exit();
                }
            }
        }
        // The last availability poll is not a shutdown barrier. Probe again.
        let ids: Vec<_> = self
            .clips
            .iter()
            .filter(|c| c.disk.is_some())
            .map(|c| c.recovery.id.clone())
            .collect();
        for id in ids {
            if Instant::now() >= deadline {
                break;
            }
            if let Some(clip) = self.clips.iter_mut().find(|c| c.recovery.id == id) {
                clip.recovery.retry_ready = finalized_wav(&clip.path);
            }
            self.promote(&id);
        }
        let preserved: std::collections::HashSet<_> = self
            .clips
            .iter()
            .filter(|c| c.disk.is_some())
            .map(|c| c.path.clone())
            .chain(
                self.deleting
                    .values()
                    .filter(|d| d.durable)
                    .map(|d| d.path.clone()),
            )
            .collect();
        self.fallback.retain(|(path, files)| {
            if !preserved.contains(path) {
                return true;
            }
            if let Some(files) = files {
                files.preserve_on_exit();
            }
            false // An in-flight handoff must not unlink the preserved original.
        });
        // Every durable owner is excluded from clear(), even if promotion or
        // index refresh failed. Never delete the only copy as a quit side effect.
        self.clips.retain(|clip| {
            !clip.disk.as_ref().is_some_and(|disk| {
                if Instant::now() < deadline {
                    let _ = disk.write(clip);
                }
                if clip.path != clip.target {
                    if let Some(files) = &clip._capture_files {
                        files.preserve_on_exit();
                    }
                }
                true
            })
        });
    }

    fn load_disk(&mut self, dir: &Path, now: u64) {
        // Never follow a substituted storage directory.
        if !std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && !alias(&m)) {
            return;
        }
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if uuid::Uuid::parse_str(id).is_err() {
                continue;
            }
            let mut wav = dir.join(format!("{id}.wav"));
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let item = if metadata.is_file() && !alias(&metadata) && metadata.len() <= 16_384 {
                // A temporary read/permission failure is not consent to delete.
                let Ok(bytes) = std::fs::read(&path) else {
                    continue;
                };
                serde_json::from_slice::<Entry>(&bytes).ok()
            } else {
                None
            };
            let expired = item.as_ref().is_some_and(|item| {
                item.version == 1 && item.recovery.id == id && item.expires_at_ms <= now
            });
            if expired {
                // Keep the expired index if unlinking fails, otherwise the WAV
                // could be resurrected as an orphan on the following launch.
                let capture = item
                    .as_ref()
                    .and_then(|i| i.capture_id.as_deref())
                    .and_then(|id| capture_source(dir, id));
                let capture_removed = capture
                    .as_ref()
                    .is_none_or(|p| durable::unlink_committed(p));
                if capture_removed && durable::unlink_committed(&wav) {
                    let _ = std::fs::remove_file(&path);
                }
                continue;
            }
            let Some(mut item) = item.filter(|item| item.version == 1 && item.recovery.id == id)
            else {
                let _ = std::fs::remove_file(&path);
                continue; // Recover finalized audio separately, never delete it here.
            };
            if !std::fs::symlink_metadata(&wav).is_ok_and(|m| m.is_file() && !alias(&m))
                || !finalized_wav(&wav)
            {
                if let Some(source) = item
                    .capture_id
                    .as_deref()
                    .and_then(|id| capture_source(dir, id))
                    .filter(|p| {
                        std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file() && !alias(&m))
                    })
                {
                    wav = source;
                }
            }
            match std::fs::symlink_metadata(&wav) {
                Ok(m) if m.is_file() && !alias(&m) => {
                    if wav != path.with_extension("wav")
                        && item.capture_id.is_some()
                        && !finalized_wav(&wav)
                        && super::super::storage::CaptureFiles::for_path(&wav).is_none()
                    {
                        let target = path.with_extension("wav");
                        if durable::recover_interrupted_capture(&wav, &target).is_ok() {
                            wav = target;
                        }
                    }
                    // A transient open/read failure is not corrupt metadata.
                    if !finalized_wav(&wav) && item.capture_id.is_none() {
                        continue;
                    }
                }
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => continue,
                _ => {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
            }
            // Restored takes belong to no live capture. Idle Cancel must not discard them.
            item.recovery.generation = u64::MAX;
            item.recovery.expires_in_ms = item.expires_at_ms - now;
            item.recovery.retry_ready = finalized_wav(&wav);
            if let Some(selection) = &item.selection {
                item.recovery.engine_short = selection.short_name();
            }
            // Alternatives are installed models, not durable promises.
            item.recovery.set_alternative(None);
            entries.push((item, path, wav));
        }
        // A crash can happen after moving the only WAV and before committing its
        // index. Recover finalized UUID files using their original file time.
        // Never follow aliases or infer ownership from arbitrary filenames.
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if path.extension().is_some_and(|e| e == "wav")
                && uuid::Uuid::parse_str(stem).is_ok()
                && !entries.iter().any(|(_, _, wav)| *wav == path)
                && std::fs::symlink_metadata(path.with_extension("json"))
                    .err()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            {
                if let Some(item) = recover_orphan(&path, stem, now) {
                    entries.push((item, path.with_extension("json"), path));
                }
            } else if stem.starts_with(".tmp")
                || path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(".index-"))
            {
                // Disposable retry copies and incomplete index writes, not originals.
                let _ = std::fs::remove_file(path);
            }
        }
        entries.sort_by_key(|(item, _, _)| item.expires_at_ms);
        for (item, index, path) in entries {
            let expires = Instant::now() + Duration::from_millis(item.expires_at_ms - now);
            let clip = Clip {
                _capture_files: None,
                trace: None,
                _trace_lease: None,
                _retry_trace_lease: None,
                retry_selection: item.selection,
                recovery: item.recovery,
                target: index.with_extension("wav"),
                path,
                expires,
                busy: false,
                retire_after_retry: false,
                announced: false,
                terminal: Default::default(),
                retry_generation: None,
                disk: Some(DiskClip {
                    index,
                    expires_at_ms: item.expires_at_ms,
                }),
            };
            let id = clip.recovery.id.clone();
            if self.insert(clip).is_ok() {
                self.promote(&id);
                self.persist(&id);
            }
        }
    }
}

fn recover_orphan(path: &Path, id: &str, now: u64) -> Option<Entry> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || alias(&metadata) {
        return None;
    }
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    let expires_at_ms = modified
        .saturating_add(TTL_MS)
        .min(now.saturating_add(TTL_MS));
    if expires_at_ms <= now {
        let _ = std::fs::remove_file(path);
        return None;
    }
    if !finalized_wav(path) {
        return None;
    }
    Some(Entry {
        version: 1,
        expires_at_ms,
        selection: None,
        capture_id: None,
        recovery: Recovery {
            generation: u64::MAX,
            id: id.into(),
            kind: RecoveryKind::Unknown,
            engine_short: "Unknown".into(),
            alt_engine_short: None,
            alt_engine_id: None,
            expires_in_ms: expires_at_ms - now,
            retry_ready: true,
            retry_failed: false,
        },
    })
}

pub(super) fn initialize(app: &AppHandle) {
    let Ok(data) = app.path().app_data_dir() else {
        return;
    };
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.app.get_or_insert_with(|| app.clone());
    store.load_disk(&data.join(DIRECTORY), now_ms());
}

pub(super) fn expire() {
    STORE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .expire(Instant::now());
}

#[cfg(test)]
#[path = "kept_persistent_tests.rs"]
mod tests;
