//! The only native ORT entry point. Selected startup preload is CPU-admitted;
//! availability checks and unselected engines never invoke it.
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const UNAVAILABLE: &str =
    "Parakeet runtime is unavailable or damaged. Reinstall Voicetypr to restore ONNX Runtime 1.28.0.";

fn runtime_pin() -> Result<(u64, String), &'static str> {
    // Compile-time embedded, never the mutable installed provenance file.
    let provenance: serde_json::Value =
        serde_json::from_str(include_str!("../../../models/onnxruntime/provenance.json"))
            .map_err(|_| UNAVAILABLE)?;
    Ok((
        provenance["dll_size_bytes"].as_u64().ok_or(UNAVAILABLE)?,
        provenance["dll_sha256"]["x86_64-pc-windows-msvc"]
            .as_str()
            .ok_or(UNAVAILABLE)?
            .to_owned(),
    ))
}

fn verified_library(
    executable: &Path,
    expected_bytes: u64,
    expected_sha256: &str,
) -> Result<PathBuf, &'static str> {
    // No relative paths, PATH lookup, working directory, user setting or System32 fallback.
    if !executable.is_absolute() {
        return Err(UNAVAILABLE);
    }
    let library = executable
        .parent()
        .ok_or(UNAVAILABLE)?
        .join("onnxruntime.dll");
    let mut file = std::fs::File::open(&library).map_err(|_| UNAVAILABLE)?;
    let metadata = file.metadata().map_err(|_| UNAVAILABLE)?;
    if !metadata.is_file() || metadata.len() != expected_bytes {
        return Err(UNAVAILABLE);
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut remaining = expected_bytes;
    while remaining > 0 {
        let count = file.read(&mut buffer).map_err(|_| UNAVAILABLE)?;
        if count == 0 || count as u64 > remaining {
            return Err(UNAVAILABLE);
        }
        remaining -= count as u64;
        hasher.update(&buffer[..count]);
    }
    if hex::encode(hasher.finalize()) != expected_sha256 {
        return Err(UNAVAILABLE);
    }
    Ok(library)
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub(super) fn initialize() -> Result<(), &'static str> {
    // Retain beta.8's conservative Parakeet CPU policy, independently of ggml VAD.
    crate::onnx_runtime::require_cpu()?;
    static INITIALIZED: std::sync::OnceLock<Result<(), &'static str>> = std::sync::OnceLock::new();
    *INITIALIZED.get_or_init(|| {
        let executable = std::env::current_exe().map_err(|_| UNAVAILABLE)?;
        let (bytes, sha256) = runtime_pin()?;
        let library = verified_library(&executable, bytes, &sha256)?;
        let builder = ort::init_from(&library).map_err(|_| UNAVAILABLE)?;
        if !builder
            .with_name("voicetypr-parakeet")
            .with_telemetry(false)
            // Native diagnostics can contain model paths; never forward their text.
            .with_logger(std::sync::Arc::new(|_, _, _, _, _| {}))
            .commit()
        {
            return Err(UNAVAILABLE);
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_pinned_app_local_library_is_accepted() {
        let install = tempfile::tempdir().unwrap();
        let exe = install.path().join("voicetypr.exe");
        let dll = install.path().join("onnxruntime.dll");
        let bytes = b"test-only library, never loaded";
        let digest = hex::encode(Sha256::digest(bytes));
        assert_eq!(
            verified_library(&exe, bytes.len() as u64, &digest),
            Err(UNAVAILABLE)
        );
        std::fs::write(&dll, bytes).unwrap();
        assert_eq!(
            verified_library(&exe, bytes.len() as u64, &digest),
            Ok(dll.clone())
        );
        assert_eq!(
            verified_library(Path::new("voicetypr.exe"), bytes.len() as u64, &digest),
            Err(UNAVAILABLE)
        );
        assert_eq!(
            verified_library(&exe, bytes.len() as u64 + 1, &digest),
            Err(UNAVAILABLE)
        );
        assert_eq!(
            verified_library(&exe, bytes.len() as u64, &runtime_pin().unwrap().1),
            Err(UNAVAILABLE)
        );
        std::fs::write(&dll, vec![b'x'; bytes.len()]).unwrap();
        assert_eq!(
            verified_library(&exe, bytes.len() as u64, &digest),
            Err(UNAVAILABLE)
        );
    }

    #[test]
    fn runtime_and_packaging_pin_the_same_official_dll() {
        let (bytes, sha256) = runtime_pin().unwrap();
        assert_eq!(bytes, 15_809_848);
        assert_eq!(
            sha256,
            "18370c375f07357fa5874344a9d9ac17e6b6fe1eb18b1dd209d79483b4470257"
        );
    }
}
