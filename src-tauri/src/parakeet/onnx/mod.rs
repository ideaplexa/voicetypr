//! Hidden Windows x64 ONNX backend. Catalog exposure follows in slice 4.

#[cfg_attr(
    not(all(target_os = "windows", target_arch = "x86_64")),
    allow(dead_code)
)]
pub mod chunking;
#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
pub mod download;
#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
pub mod slot;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub static LOCAL_MODEL_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Serializes ONNX download and delete without blocking Whisper transcription.
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub static MODEL_OPERATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub mod session;

#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
pub(crate) fn selected_model_matches(
    engine: Option<&str>,
    model: Option<&str>,
    expected_engine: &str,
    expected_model: &str,
) -> bool {
    engine == Some(expected_engine) && model == Some(expected_model)
}

#[cfg(test)]
mod selection_tests {
    #[test]
    fn only_current_model_can_preload() {
        use super::selected_model_matches as matches;
        assert!(matches(Some("whisper"), Some("base"), "whisper", "base"));
        assert!(!matches(Some("parakeet"), Some("base"), "whisper", "base"));
        assert!(!matches(Some("whisper"), Some("large"), "whisper", "base"));
        assert!(!matches(None, None, "whisper", "base"));
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub(crate) fn can_initialize_runtime() -> bool {
    parakeet_rs::ExecutionConfig::default()
        .build_session(std::path::Path::new(
            "__voicetypr_onnx_link_probe_missing__.onnx",
        ))
        .is_err()
}

#[cfg(all(test, target_os = "windows", target_arch = "x86_64"))]
mod tests {
    #[test]
    fn links_static_ort_without_a_model() {
        assert!(super::can_initialize_runtime());
    }
}
