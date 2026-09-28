//! Windows x64 ONNX link probe. Model loading and app integration follow in later slices.

use std::path::Path;

// The app retains a function pointer but never calls it. The test calls through
// parakeet-rs into ORT without needing a model file.
pub(crate) fn can_initialize_runtime() -> bool {
    parakeet_rs::ExecutionConfig::default()
        .build_session(Path::new("__voicetypr_onnx_link_probe_missing__.onnx"))
        .is_err()
}

#[cfg(test)]
mod tests {
    #[test]
    fn links_static_ort_without_a_model() {
        assert_eq!(
            parakeet_rs::ExecutionConfig::default().execution_provider,
            parakeet_rs::ExecutionProvider::Cpu
        );
        assert!(super::can_initialize_runtime());
    }
}
