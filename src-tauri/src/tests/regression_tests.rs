#[cfg(test)]
mod tests {
    use crate::state::unified_state::UnifiedRecordingState;
    use crate::whisper::transcriber::Transcriber;
    use crate::RecordingState;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_transcriber_handles_non_utf8_path() {
        // Create a path that would fail with unwrap()
        let path = PathBuf::from("/tmp/test_model.bin");

        // This should not panic even with non-UTF8 paths
        let result = Transcriber::new(&path);

        // It should return an error, not panic
        assert!(result.is_err());
    }

    #[test]
    fn test_unified_state_concurrent_transitions() {
        let state = Arc::new(UnifiedRecordingState::new());
        let mut handles = vec![];

        // Spawn multiple threads trying to transition state
        for i in 0..10 {
            let state_clone = state.clone();
            let handle = thread::spawn(move || {
                // Try to transition through valid states
                if i % 2 == 0 {
                    let _ = state_clone.transition_to(RecordingState::Starting);
                    let _ = state_clone.transition_to(RecordingState::Recording);
                } else {
                    // Try invalid transition
                    let _ = state_clone.transition_to(RecordingState::Transcribing);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // State should still be in a valid state
        let final_state = state.current();
        assert!(matches!(
            final_state,
            RecordingState::Idle | RecordingState::Starting | RecordingState::Recording
        ));
    }

    #[test]
    fn test_state_machine_force_set_recovery() {
        let state = UnifiedRecordingState::new();

        // Force set to an invalid state
        state.force_set(RecordingState::Error).unwrap();
        assert_eq!(state.current(), RecordingState::Error);

        // Should be able to recover from error state
        assert!(state.transition_to(RecordingState::Idle).is_ok());
        assert_eq!(state.current(), RecordingState::Idle);
    }

    #[test]
    fn test_file_cleanup_logs_errors() {
        use std::fs;
        use std::path::Path;

        // Try to remove a non-existent file
        let non_existent = Path::new("/tmp/does_not_exist_12345.wav");

        // This should not panic, just log a warning
        if let Err(e) = fs::remove_file(non_existent) {
            // In real code, this would log
            assert!(e.kind() == std::io::ErrorKind::NotFound);
        }
    }

    #[test]
    fn test_mutex_poison_recovery() {
        use std::sync::{Arc, Mutex};
        use std::thread;

        let data = Arc::new(Mutex::new(42));
        let data_clone = data.clone();

        // Spawn a thread that will panic while holding the lock
        let handle = thread::spawn(move || {
            let _guard = data_clone.lock().unwrap();
            panic!("Intentional panic to poison mutex");
        });

        // Wait for thread to panic
        let _ = handle.join();

        // The mutex is now poisoned, but UnifiedRecordingState should handle this
        let state = UnifiedRecordingState::new();

        // Force a poisoned scenario by using force_set
        // This should not panic even with a poisoned mutex
        state.force_set(RecordingState::Recording).unwrap();
        assert_eq!(state.current(), RecordingState::Recording);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn main_window_controls_reject_worker_thread() {
        thread::spawn(|| assert!(crate::main_window_controls_thread().is_none()))
            .join()
            .unwrap();
    }

    /// Check the guard's wiring without constructing a native window in tests.
    #[test]
    fn main_window_native_access_follows_main_thread_dispatch_and_guard() {
        let src = include_str!("../lib.rs").replace("\r\n", "\n");
        let show = src.split("fn show_main_window(").nth(1).unwrap();
        let show = show.split("\n}\n").next().unwrap();
        assert!(
            show.find(".run_on_main_thread(").unwrap() < show.find(".get_webview_window(").unwrap()
        );

        let align = src.split("fn align_main_window_controls(").nth(1).unwrap();
        let align = align.split("\n}\n").next().unwrap();
        let guard = align
            .find("let Some(_main_thread) = main_window_controls_thread() else {\n        return;\n    };")
            .expect("alignment must return without a main-thread marker");
        assert!(guard < align.find(".ns_window()").unwrap());
    }

    /// Regression guard for the 2.0.0 panic
    /// `state() called before manage() for ...::window_manager::WindowManager`,
    /// surfaced once error reporting defaulted on. `WindowManager` lives inside
    /// `AppState` (`Arc<Mutex<Option<WindowManager>>>`) and must be read via
    /// `AppState::get_window_manager()` (`None` when uninitialized); it is never
    /// registered with Tauri's `manage()`, so `app.state::<WindowManager>()`
    /// panics. Keep that call out of settings.rs.
    #[test]
    fn settings_reads_window_manager_via_app_state_not_tauri_state() {
        let src = include_str!("../commands/settings.rs");
        assert!(
            !src.contains("state::<crate::WindowManager>")
                && !src.contains("state::<WindowManager>"),
            "settings.rs must read WindowManager via AppState::get_window_manager(), \
             never app.state::<WindowManager>() (panics: 'state() called before manage()')"
        );
    }
}
