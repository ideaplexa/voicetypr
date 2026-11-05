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

    pub(super) fn delete(&self) {
        let _ = std::fs::remove_file(&self.index);
    }

    pub(super) fn retire(&self, clip: &Clip) {
        // Commit deletion intent before unlinking. A crash or sharing violation
        // must not turn deliberately retired audio into a recovered orphan.
        if self.write_deadline(clip, 0).is_err() {
            log::warn!("Could not persist recording deletion intent");
        }
    }

    fn write(&self, clip: &Clip) -> std::io::Result<()> {
        self.write_deadline(clip, self.expires_at_ms)
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
        };
        let mut file = tempfile::Builder::new()
            .prefix(".index-")
            .tempfile_in(self.index.parent().unwrap())?;
        serde_json::to_writer(&mut file, &entry)?;
        file.flush()?;
        file.as_file().sync_all()?;
        file.persist(&self.index).map_err(|e| e.error)?;
        Ok(())
    }
}

impl Store {
    pub(super) fn persist(&self, id: &str) {
        let Some(clip) = self.clips.iter().find(|c| c.recovery.id == id) else {
            return;
        };
        if let Some(disk) = &clip.disk {
            // Never index an unfinished writer or a source outside our directory.
            if clip.path == clip.target && clip.recovery.retry_ready && disk.write(clip).is_err() {
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

    pub(super) fn preserve_on_exit(&mut self) {
        // Persist before detaching ownership. Explicit Reset still calls clear().
        self.clips.retain(|clip| {
            !clip.disk.as_ref().is_some_and(|disk| {
                clip.path == clip.target && clip.recovery.retry_ready && {
                    // A failed index save leaves a recoverable UUID WAV.
                    // Never turn a temporary disk error into deletion on quit.
                    let _ = disk.write(clip);
                    true
                }
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
            let wav = dir.join(format!("{id}.wav"));
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
                if std::fs::remove_file(&wav).is_ok()
                    || std::fs::symlink_metadata(&wav)
                        .err()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                {
                    let _ = std::fs::remove_file(&path);
                }
                continue;
            }
            let Some(mut item) = item.filter(|item| item.version == 1 && item.recovery.id == id)
            else {
                let _ = std::fs::remove_file(&path);
                continue; // Recover finalized audio separately, never delete it here.
            };
            match std::fs::symlink_metadata(&wav) {
                Ok(m) if m.is_file() && !alias(&m) => {
                    // A transient open/read failure is not corrupt metadata.
                    if !finalized_wav(&wav) {
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
            item.recovery.retry_ready = true;
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
                target: path.clone(),
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
