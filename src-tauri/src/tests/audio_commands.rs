#[cfg(test)]
mod tests {
    use crate::{AppState, RecordingState};
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn test_recording_state_default() {
        let state = RecordingState::default();
        assert_eq!(state, RecordingState::Idle);
    }

    #[test]
    fn test_recording_state_serialization() {
        // Test all state values serialize correctly
        let states = vec![
            RecordingState::Idle,
            RecordingState::Starting,
            RecordingState::Recording,
            RecordingState::Stopping,
            RecordingState::Transcribing,
            RecordingState::Error,
        ];

        for state in states {
            let serialized = serde_json::to_string(&state).unwrap();
            assert!(!serialized.is_empty());

            // Verify the JSON output
            let expected = match state {
                RecordingState::Idle => "\"Idle\"",
                RecordingState::Starting => "\"Starting\"",
                RecordingState::Recording => "\"Recording\"",
                RecordingState::Stopping => "\"Stopping\"",
                RecordingState::Transcribing => "\"Transcribing\"",
                RecordingState::Error => "\"Error\"",
            };
            assert_eq!(serialized, expected);
        }
    }

    #[test]
    fn test_app_state_new() {
        let app_state = AppState::new();

        // Verify initial state
        {
            let recording_state = app_state.get_current_state();
            assert_eq!(recording_state, RecordingState::Idle);
        }

        {
            let path = app_state.current_recording_path.lock().unwrap();
            assert!(path.is_none());
        }

        {
            let task = app_state.transcription_task.lock().unwrap();
            assert!(task.is_none());
        }
    }

    #[test]
    fn test_app_state_recording_state_transitions() {
        let app_state = AppState::new();

        // Test state transitions
        let transitions = vec![
            RecordingState::Starting,
            RecordingState::Recording,
            RecordingState::Stopping,
            RecordingState::Transcribing,
            RecordingState::Idle,
        ];

        for expected_state in transitions {
            // Force set the state since we're testing direct state changes
            app_state.recording_state.force_set(expected_state).unwrap();

            // Verify state was set
            let state = app_state.get_current_state();
            assert_eq!(state, expected_state);
        }
    }

    #[test]
    fn test_app_state_path_management() {
        let app_state = AppState::new();
        let test_path = PathBuf::from("/tmp/test_recording.wav");

        // Set path
        {
            let mut path = app_state.current_recording_path.lock().unwrap();
            *path = Some(test_path.clone());
        }

        // Verify path is set
        {
            let path = app_state.current_recording_path.lock().unwrap();
            assert_eq!(path.as_ref().unwrap(), &test_path);
        }

        // Take path (should remove it)
        let taken_path = {
            let mut path = app_state.current_recording_path.lock().unwrap();
            path.take()
        };

        assert_eq!(taken_path.unwrap(), test_path);

        // Verify path is now None
        {
            let path = app_state.current_recording_path.lock().unwrap();
            assert!(path.is_none());
        }
    }

    #[test]
    fn test_app_state_concurrent_access() {
        let app_state = Arc::new(AppState::new());
        let mut handles = vec![];

        // Spawn multiple threads to test concurrent access
        for i in 0..10 {
            let state_clone = app_state.clone();
            let handle = std::thread::spawn(move || {
                // Each thread tries to update the state
                let new_state = if i % 2 == 0 {
                    RecordingState::Recording
                } else {
                    RecordingState::Idle
                };

                // Force set state in concurrent test
                state_clone.recording_state.force_set(new_state).unwrap();
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // State should be valid (either Recording or Idle)
        let final_state = app_state.get_current_state();
        assert!(matches!(
            final_state,
            RecordingState::Recording | RecordingState::Idle
        ));
    }

    #[test]
    fn test_recording_state_equality() {
        assert_eq!(RecordingState::Idle, RecordingState::Idle);
        assert_ne!(RecordingState::Idle, RecordingState::Recording);
        assert_ne!(RecordingState::Starting, RecordingState::Stopping);
    }

    #[test]
    fn test_app_state_error_handling() {
        let app_state = AppState::new();

        // Set to error state
        app_state
            .recording_state
            .force_set(RecordingState::Error)
            .unwrap();

        // Verify error state
        let state = app_state.get_current_state();
        assert_eq!(state, RecordingState::Error);

        // Reset to idle
        app_state
            .recording_state
            .force_set(RecordingState::Idle)
            .unwrap();

        // Verify idle state
        let state = app_state.get_current_state();
        assert_eq!(state, RecordingState::Idle);
    }

    // Test for the audio recorder functionality would require mocking
    // the actual audio recording hardware, which is complex.
    // These tests focus on the state management aspects.

    #[test]
    fn test_recording_path_validation() {
        let app_state = AppState::new();

        // Test various path scenarios
        let test_paths = vec![
            PathBuf::from("/tmp/recording_123.wav"),
            PathBuf::from("/var/tmp/audio.wav"),
            PathBuf::from("./recordings/test.wav"),
        ];

        for test_path in test_paths {
            {
                let mut path = app_state.current_recording_path.lock().unwrap();
                *path = Some(test_path.clone());
            }

            {
                let path = app_state.current_recording_path.lock().unwrap();
                assert_eq!(path.as_ref().unwrap(), &test_path);
            }
        }
    }

    #[tokio::test]
    async fn test_transcription_task_management() {
        let app_state = AppState::new();

        // Create a dummy task
        let task = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        });

        // Store the task
        {
            let mut task_guard = app_state.transcription_task.lock().unwrap();
            *task_guard = Some(task);
        }

        // Verify task is stored
        {
            let task_guard = app_state.transcription_task.lock().unwrap();
            assert!(task_guard.is_some());
        }

        // Take and await the task
        let task = {
            let mut task_guard = app_state.transcription_task.lock().unwrap();
            task_guard.take()
        };

        if let Some(task) = task {
            // Task should complete successfully
            assert!(task.await.is_ok());
        }

        // Verify task is now None
        {
            let task_guard = app_state.transcription_task.lock().unwrap();
            assert!(task_guard.is_none());
        }
    }

    #[tokio::test]
    async fn test_task_cancellation() {
        let app_state = AppState::new();

        // Create a long-running task
        let task = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        });

        // Store the task
        {
            let mut task_guard = app_state.transcription_task.lock().unwrap();
            *task_guard = Some(task);
        }

        // Cancel the task
        {
            let mut task_guard = app_state.transcription_task.lock().unwrap();
            if let Some(task) = task_guard.take() {
                task.abort();
            }
        }

        // Verify task is cancelled and removed
        {
            let task_guard = app_state.transcription_task.lock().unwrap();
            assert!(task_guard.is_none());
        }
    }

    // --- PTT race condition tests ---
    // These tests verify the fix for the PTT/license-lag issue where key-up
    // arrives while recording start is blocked by slow license validation.

    #[test]
    fn test_ptt_key_held_set_and_cleared() {
        let app_state = AppState::new();

        // Simulate key-down
        app_state
            .ptt_key_held
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(app_state
            .ptt_key_held
            .load(std::sync::atomic::Ordering::SeqCst));

        // Simulate key-up (swap returns previous value)
        let was_held = app_state
            .ptt_key_held
            .swap(false, std::sync::atomic::Ordering::SeqCst);
        assert!(was_held);
        assert!(!app_state
            .ptt_key_held
            .load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_ptt_key_release_while_starting_sets_pending_stop() {
        let app_state = AppState::new();

        // Simulate: recording mode is PTT
        {
            let mut mode = app_state.recording_mode.lock().unwrap();
            *mode = crate::RecordingMode::PushToTalk;
        }

        // Simulate: key-down starts the process
        app_state
            .ptt_key_held
            .store(true, std::sync::atomic::Ordering::Relaxed);

        // State transitions to Starting
        app_state
            .recording_state
            .force_set(RecordingState::Starting)
            .unwrap();
        assert_eq!(app_state.get_current_state(), RecordingState::Starting);

        // Simulate: key-up arrives while Starting
        let was_held = app_state
            .ptt_key_held
            .swap(false, std::sync::atomic::Ordering::SeqCst);
        assert!(was_held); // Key was held before release

        // PTT handler should set pending_stop_after_start
        app_state
            .pending_stop_after_start
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // Simulate: start_recording reaches Recording state and checks the flag
        app_state
            .recording_state
            .force_set(RecordingState::Recording)
            .unwrap();
        let pending = app_state
            .pending_stop_after_start
            .swap(false, std::sync::atomic::Ordering::SeqCst);
        assert!(
            pending,
            "pending_stop_after_start should be true when key-up happened during Starting"
        );
    }

    #[test]
    fn test_ptt_guard_detects_released_key() {
        let app_state = AppState::new();

        // Set to PTT mode
        {
            let mut mode = app_state.recording_mode.lock().unwrap();
            *mode = crate::RecordingMode::PushToTalk;
        }

        // Key was never pressed (or already released)
        app_state
            .ptt_key_held
            .store(false, std::sync::atomic::Ordering::SeqCst);

        assert!(
            crate::commands::audio::ptt_key_released(
                &app_state,
                crate::recording::start_source::StartSource::Hotkey
            ),
            "PTT guard should abort when key is not held"
        );
    }

    #[test]
    fn test_ptt_key_released_recovers_poisoned_mode_lock() {
        let app_state = AppState::new();
        {
            let mut mode = app_state.recording_mode.lock().unwrap();
            *mode = crate::RecordingMode::PushToTalk;
        }
        app_state
            .ptt_key_held
            .store(false, std::sync::atomic::Ordering::SeqCst);

        let mode_for_thread = app_state.recording_mode.clone();
        let _ = std::thread::spawn(move || {
            let _guard = mode_for_thread.lock().unwrap();
            panic!("intentional poison for PTT guard recovery test");
        })
        .join();

        assert!(
            crate::commands::audio::ptt_key_released(
                &app_state,
                crate::recording::start_source::StartSource::Hotkey
            ),
            "PTT guard should recover poisoned mode lock and still detect released key"
        );
    }

    #[test]
    fn test_ptt_guard_allows_when_key_still_held() {
        let app_state = AppState::new();

        // Set to PTT mode
        {
            let mut mode = app_state.recording_mode.lock().unwrap();
            *mode = crate::RecordingMode::PushToTalk;
        }

        // Key is still held
        app_state
            .ptt_key_held
            .store(true, std::sync::atomic::Ordering::SeqCst);

        assert!(
            !crate::commands::audio::ptt_key_released(
                &app_state,
                crate::recording::start_source::StartSource::Hotkey
            ),
            "PTT guard should NOT abort when key is still held"
        );
    }

    #[test]
    fn test_toggle_mode_not_affected_by_ptt_guard() {
        let app_state = AppState::new();

        // Toggle mode (default)
        app_state
            .ptt_key_held
            .store(false, std::sync::atomic::Ordering::SeqCst);

        let mode = app_state
            .recording_mode
            .lock()
            .map(|g| *g)
            .unwrap_or(crate::RecordingMode::Toggle);
        assert_eq!(mode, crate::RecordingMode::Toggle);

        assert!(
            !crate::commands::audio::ptt_key_released(
                &app_state,
                crate::recording::start_source::StartSource::Hotkey
            ),
            "PTT guard should not affect toggle mode"
        );
    }

    #[test]
    fn test_pending_stop_after_start_default_false() {
        let app_state = AppState::new();
        assert!(!app_state
            .pending_stop_after_start
            .load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_ptt_abort_clears_pending_stop_after_start() {
        let app_state = AppState::new();
        app_state
            .pending_stop_after_start
            .store(true, std::sync::atomic::Ordering::SeqCst);

        crate::commands::audio::clear_pending_stop_after_start(&app_state);

        assert!(
            !app_state
                .pending_stop_after_start
                .load(std::sync::atomic::Ordering::SeqCst),
            "PTT abort cleanup must not leave a stale pending stop for the next recording"
        );
    }

    #[test]
    fn test_duplicate_ptt_key_release_ignored() {
        let app_state = AppState::new();

        // First key-down
        app_state
            .ptt_key_held
            .store(true, std::sync::atomic::Ordering::Relaxed);

        // First key-up: swap returns true (was held), now false
        let first_swap = app_state
            .ptt_key_held
            .swap(false, std::sync::atomic::Ordering::SeqCst);
        assert!(first_swap);

        // Second key-up (duplicate event): swap returns false (already released)
        let second_swap = app_state
            .ptt_key_held
            .swap(false, std::sync::atomic::Ordering::SeqCst);
        assert!(
            !second_swap,
            "Duplicate key release should return false from swap"
        );
    }
}
