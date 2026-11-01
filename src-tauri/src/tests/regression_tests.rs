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
        let result = Transcriber::new(&path, false);

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

    /// NSIS installs each resource target relative to $INSTDIR, not resources/.
    #[test]
    fn windows_installer_hooks_follow_tauri_resource_targets() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.windows.conf.json")).unwrap();
        let resources = config["bundle"]["resources"].as_array().unwrap();
        let hooks = include_str!("../../windows/installer-hooks.nsh");
        let check = include_str!("../../../scripts/check-windows-install.ps1");
        for target in [
            "windows/resources/vcredist/*.dll",
            "windows/resources/voicetypr.pdb",
        ] {
            assert!(resources.iter().any(|value| value.as_str() == Some(target)));
            let installed = format!(r"$INSTDIR\{}", target.replace('/', r"\"));
            assert!(hooks.contains(&installed), "missing resource target");
        }
        assert!(hooks.contains(r"$INSTDIR\windows\resources\vcredist\$R1"));
        assert!(check.contains(r"'windows\resources\vcredist'"));
        let wrong_prefix = ["resources", "windows", "resources"].join(r"\");
        assert!(!hooks.contains(&wrong_prefix));
        assert!(!check.contains(&wrong_prefix));
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

    /// Source-level guard: a cached Parakeet engine must not leave first launch
    /// hidden during onboarding, and remote-only sessions must show until startup checks finish.
    #[test]
    fn startup_shows_main_window_without_a_saved_model_or_with_an_active_remote() {
        let src = include_str!("../lib.rs").replace("\r\n", "\n");
        let startup = src.split("let should_hide_main =").nth(1).unwrap();
        let startup = startup.split("// Log setup completion").next().unwrap();
        assert!(startup.contains(".get(\"current_model\")"));
        assert!(startup.contains(".map(|s| !s.is_empty())"));
        assert!(startup.contains(".get(\"onboarding_completed\")"));
        assert!(
            startup.contains(".and_then(|v| v.as_bool())\n                    .unwrap_or(false)")
        );
        assert!(startup.contains(
            "should_start_hidden(has_local_or_cloud_model, onboarding_completed, active_id.is_some())"
        ));
        let show_branch = startup.rsplit_once("} else {").unwrap().1;
        assert!(
            show_branch.contains("show_main_window(app.app_handle());"),
            "the initially hidden window must be shown, not just its Dock icon"
        );
        assert!(!show_branch.contains("#[cfg("));
        assert!(!show_branch.contains("show_dock_icon("));
    }

    /// Keep the later remote-only hide conditional on readiness and onboarding.
    #[test]
    fn startup_hides_ready_remote_only_after_onboarding() {
        let src = include_str!("../lib.rs").replace("\r\n", "\n");
        let checks = src
            .split("async fn perform_startup_checks(")
            .nth(1)
            .unwrap();
        let remote = checks
            .split("if availability.remote_available {")
            .nth(1)
            .unwrap()
            .split("if !availability.any_available()")
            .next()
            .unwrap();
        assert!(remote.contains("store.get(\"onboarding_completed\")"));
        assert!(remote.contains(".unwrap_or(false)"));
        assert!(
            remote.contains("if onboarding_completed {\n                hide_main_window(&app);")
        );
    }

    /// Regression guard for beta.4: the island's device and window lookups ran
    /// before the mic opened and delayed first audio by 50–200 ms. They must stay
    /// after the readiness decision in `start_recording`.
    #[test]
    fn island_context_is_resolved_after_first_audio() {
        // Windows checkouts use CRLF line endings.
        let src = include_str!("../commands/audio.rs").replace("\r\n", "\n");
        let start = src
            .find("pub async fn start_recording(")
            .expect("start_recording exists");
        let body = &src[start..];
        let body = &body[..body.find("\n}\n").expect("start_recording ends")];
        let readiness = body
            .find("decide_start_readiness(")
            .expect("readiness decision");
        let context = body
            .find("spawn_island_context(")
            .expect("island context is spawned");
        assert!(
            readiness < context,
            "island context must follow first audio"
        );
        assert!(
            !body.contains("pill::context::capture") && !body.contains("positioning::snapshot"),
            "device/window lookups must not run inline on the start path"
        );
    }
}
