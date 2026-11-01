//! CLI retry captures and file-job audio: owned while running, bounded after crashes.
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use super::kept::{protect_path, ProtectedPath};

const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const TEMP_PREFIX: &str = "voicetypr-audio-";

pub(crate) struct TempAudio {
    path: tempfile::TempPath,
    _lease: ProtectedPath,
}

impl TempAudio {
    pub fn new() -> std::io::Result<Self> {
        let dir = std::env::temp_dir();
        let name = format!("{TEMP_PREFIX}{}", uuid::Uuid::new_v4());
        // Register before creation so reset cannot race this new file's lease.
        let lease = protect_path(&dir.join(format!("{name}.wav")));
        let path = tempfile::Builder::new()
            .prefix(&name)
            .rand_bytes(0)
            .suffix(".wav")
            .tempfile_in(dir)?
            .into_temp_path();
        Ok(Self {
            _lease: lease,
            path,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub async fn normalize(input: PathBuf) -> Result<TempAudio, String> {
    normalize_owned(input, crate::audio::decode::normalize_to_wav).await
}

async fn normalize_owned(
    input: PathBuf,
    decode: impl FnOnce(&Path, &Path) -> Result<(), String> + Send + 'static,
) -> Result<TempAudio, String> {
    let output = TempAudio::new().map_err(|_| "Temporary audio storage unavailable")?;
    let input_lease = protect_path(&input);
    // Output ownership travels into AND back out of the worker. Cancellation
    // drops its eventual result, not a guard while the decoder is still writing.
    tauri::async_runtime::spawn_blocking(move || {
        let _input_lease = input_lease;
        decode(&input, output.path())?;
        Ok(output)
    })
    .await
    .map_err(|_| "Audio normalization task failed".to_string())?
}

pub fn cli_directory(data: &Path) -> std::io::Result<PathBuf> {
    let dir = data.join("cli-recordings");
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&dir)?;
    Ok(dir)
}

fn cli_name(path: &Path, prefix: &str) -> bool {
    path.extension().is_some_and(|e| e == "wav")
        && path.file_stem().and_then(|n| n.to_str()).is_some_and(|n| {
            n.strip_prefix(prefix)
                .and_then(|s| s.split_once('_'))
                .is_some_and(|(stamp, id)| {
                    stamp.len() == 17
                        && stamp.bytes().all(|b| b.is_ascii_digit())
                        && chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d%H%M%S%3f").is_ok()
                        && id.len() == 8
                        && id.bytes().all(|b| b.is_ascii_hexdigit())
                })
        })
}

fn temp_name(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "wav")
        && path
            .file_stem()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix(TEMP_PREFIX))
            .is_some_and(|n| uuid::Uuid::parse_str(n).is_ok_and(|id| id.to_string() == n))
}

/// Exact app-created names only; anonymous legacy `.tmp*` files cannot safely
/// be attributed to us. `reset` ignores age, but never a live in-process lease.
pub fn sweep(
    data: &Path,
    temp: &Path,
    protected: &HashSet<PathBuf>,
    now: SystemTime,
    reset: bool,
) -> usize {
    let mut removed = 0;
    for (dir, os_temp) in [
        (data.join("cli-recordings"), false),
        (data.join("recordings"), false),
        (temp.to_owned(), true),
    ] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let known = if os_temp {
                temp_name(&path) || crate::audio::decode::is_temp_path(&path)
            } else {
                cli_name(&path, "cli-recording-")
                    || cli_name(&path, "cli-remote-normalized-")
                    || (dir == data.join("cli-recordings")
                        && crate::audio::decode::is_temp_path(&path))
            };
            if !known || protected.contains(&path) {
                continue;
            }
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_file()
                && (reset
                    || metadata
                        .modified()
                        .ok()
                        .and_then(|time| now.duration_since(time).ok())
                        .is_some_and(|age| age >= MAX_AGE))
            {
                removed += usize::from(std::fs::remove_file(path).is_ok());
            }
        }
    }
    removed
}

pub async fn startup(data: PathBuf) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        super::kept::with_protected_paths(|protected| {
            sweep(
                &data,
                &std::env::temp_dir(),
                protected,
                SystemTime::now(),
                false,
            )
        });
    })
    .await;
}

#[cfg(test)]
#[path = "file_jobs_tests.rs"]
mod tests;
