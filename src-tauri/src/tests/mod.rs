#[cfg(test)]
mod audio_commands;

#[cfg(test)]
mod model_commands;

#[cfg(test)]
mod settings_commands;

#[cfg(test)]
mod transcription_history;

#[cfg(test)]
mod regression_tests;

#[cfg(test)]
mod language_tests;

#[cfg(test)]
mod panic_prevention_tests;

#[cfg(test)]
mod error_event_tests;

#[cfg(test)]
mod test_data_helpers;

#[cfg(test)]
mod logging_performance_tests;

#[cfg(test)]
mod log_commands_tests;

#[cfg(test)]
mod remote_server_tests;

#[cfg(test)]
mod remote_client_tests;

#[cfg(test)]
mod remote_settings_tests;

#[cfg(test)]
mod remote_http_tests;

#[cfg(test)]
mod remote_lifecycle_tests;

#[cfg(test)]
mod remote_transcription_tests;

#[cfg(test)]
mod remote_commands_tests;

#[cfg(test)]
mod audio_recording_tests;

#[cfg(test)]
mod recording_state_characterization;

#[cfg(test)]
mod tray_menu_tests;

#[cfg(test)]
mod shortcut_bindings;

/// Serializes every unit test that touches the process-global recording-lifecycle
/// statics (`RECORDING_GENERATION`, `IN_FLIGHT_TRANSCRIPTION_AUDIO` in `commands::audio`).
/// These statics are shared across the whole test binary; without one common lock,
/// per-file schemes only serialize within a file and races leak across files. Every
/// such test must take this guard as its FIRST line.
pub(crate) static RECORDING_LIFECYCLE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod integration_tests {
    use crate::whisper::manager::{ModelSize, WhisperManager};
    use tempfile::TempDir;

    #[test]
    fn test_model_size_validation() {
        // Test minimum size validation (10MB)
        let too_small = ModelSize::new(5 * 1024 * 1024); // 5MB
        assert!(too_small.is_err());

        // Test maximum size validation (3.5GB)
        let too_large = ModelSize::new(4 * 1024 * 1024 * 1024); // 4GB
        assert!(too_large.is_err());

        // Test valid sizes
        let valid_small = ModelSize::new(50 * 1024 * 1024); // 50MB
        assert!(valid_small.is_ok());
        assert_eq!(valid_small.unwrap().as_bytes(), 50 * 1024 * 1024);

        let valid_large = ModelSize::new(3 * 1024 * 1024 * 1024); // 3GB
        assert!(valid_large.is_ok());
    }

    #[test]
    fn test_whisper_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let _manager = WhisperManager::new(temp_dir.path().to_path_buf());

        // Manager should be created successfully
        // (We can't access private fields, but creation shouldn't panic)
    }

    #[test]
    fn test_cache_basic_operations() {
        use crate::whisper::cache::TranscriberCache;

        let mut cache = TranscriberCache::new();
        assert_eq!(cache.size(), 0);

        // Test with custom capacity
        let cache_large = TranscriberCache::with_capacity(5);
        assert_eq!(cache_large.capacity(), 5);

        // Clear should work on empty cache
        cache.clear();
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn test_recording_size_check() {
        use crate::audio::recorder::RecordingSize;

        // Test valid size
        let result = RecordingSize::check(100 * 1024 * 1024); // 100MB
        assert!(result.is_ok());

        // Test over limit (500MB max)
        let result = RecordingSize::check(600 * 1024 * 1024); // 600MB
        assert!(result.is_err());
    }
}
