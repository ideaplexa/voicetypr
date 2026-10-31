//! Caller-owned upload WAVs. Blocking jobs retain ownership even if their
//! awaiting future is dropped; an abandoned job result cleans itself up.
use std::path::{Path, PathBuf};

use crate::recording::kept::{protect_path, ProtectedPath};

pub(super) struct NormalizedTempFile {
    path: PathBuf,
    _lease: ProtectedPath,
}

impl NormalizedTempFile {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            _lease: protect_path(&path),
            path,
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for NormalizedTempFile {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!("Failed to remove normalized temporary audio");
            }
        }
    }
}

pub(super) async fn normalize_upload_audio(
    recordings_dir: &Path,
    input: &Path,
) -> Result<NormalizedTempFile, String> {
    let output = recordings_dir.join(format!("upload-normalized-{}.wav", uuid::Uuid::new_v4()));
    normalize_owned(
        input.to_owned(),
        output,
        crate::audio::decode::normalize_to_wav,
    )
    .await
}

async fn normalize_owned(
    input: PathBuf,
    output: PathBuf,
    decode: impl FnOnce(&Path, &Path) -> Result<(), String> + Send + 'static,
) -> Result<NormalizedTempFile, String> {
    // Own output before queueing work, not after awaiting it. Both leases travel
    // into the worker; cancellation cannot release them while it is still writing.
    let output = NormalizedTempFile::new(output);
    let source_lease = protect_path(&input);
    tokio::task::spawn_blocking(move || {
        let _source_lease = source_lease;
        decode(&input, output.path())
            .map_err(|e| format!("Audio normalization (decode) failed: {e}"))?;
        Ok(output)
    })
    .await
    .map_err(|_| "Audio normalization task failed".to_string())?
}

#[cfg(test)]
#[path = "normalized_audio_tests.rs"]
mod tests;
