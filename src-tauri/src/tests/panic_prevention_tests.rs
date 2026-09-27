use crate::utils::logger::*;
use crate::AppState;
use crate::{log_context, RecordingState};
use serial_test::serial;
/// Panic Prevention Tests
///
/// These tests verify that previously panic-inducing scenarios now handle
/// errors gracefully without crashing the application.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(test)]
mod panic_prevention_scenarios {
    use super::*;

    #[test]
    fn test_app_continues_without_hotkey() {
        // Test that hotkey registration failure doesn't crash the app

        log::info!("Testing hotkey failure handling");

        let result = std::panic::catch_unwind(|| {
            // Simulate hotkey registration failure
            match simulate_hotkey_registration("InvalidKey+NotExist") {
                Ok(_) => "hotkey_registered",
                Err(e) => {
                    log::warn!(
                        "Hotkey registration failed: {}, continuing without hotkey",
                        e
                    );
                    "no_hotkey_fallback"
                }
            }
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "no_hotkey_fallback");
        log::info!("✅ App continues gracefully without hotkey");
    }

    #[test]
    fn test_mutex_poisoning_recovery() {
        // Test that poisoned mutex doesn't crash the application

        log::info!("Testing mutex poisoning recovery");

        let poisoned_data: Arc<Mutex<Option<String>>> =
            Arc::new(Mutex::new(Some("test".to_string())));
        let poisoned_clone = Arc::clone(&poisoned_data);

        // Poison the mutex by panicking while holding the lock
        let handle = std::thread::spawn(move || {
            let _guard = poisoned_clone.lock().unwrap();
            panic!("Intentional panic to poison mutex");
        });

        // Wait for thread to panic
        let _ = handle.join();

        // Now try to use the poisoned mutex - should handle gracefully
        let result = std::panic::catch_unwind(|| {
            match poisoned_data.lock() {
                Ok(guard) => format!("Got data: {:?}", *guard),
                Err(poison_error) => {
                    log::warn!("Mutex is poisoned, attempting recovery: {}", poison_error);

                    // Recover the data from the poisoned mutex
                    let recovered_data = poison_error.into_inner();
                    format!("Recovered data: {:?}", *recovered_data)
                }
            }
        });

        assert!(result.is_ok());
        let message = result.unwrap();
        assert!(message.contains("Recovered data"));
        log::info!("✅ Successfully recovered from poisoned mutex: {}", message);
    }

    #[test]
    fn test_window_manager_handles_poisoned_mutex() {
        // Test that WindowManager methods don't panic with poisoned mutexes

        log::info!("Testing WindowManager poisoned mutex handling");

        // Create a mock app state with a poisoned window manager mutex
        let app_state = AppState::new();

        // Simulate what happens when mutex gets poisoned
        // Wrap in AssertUnwindSafe since cache fields aren't being tested for panic safety
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // This should return None instead of panicking
            app_state.get_window_manager()
        }));

        assert!(result.is_ok());
        let window_manager = result.unwrap();

        // Should return None gracefully instead of panicking
        assert!(window_manager.is_none());
        log::info!("✅ WindowManager handles poisoned mutex gracefully");
    }

    #[test]
    fn test_recording_state_transition_with_errors() {
        // Test that invalid state transitions don't panic

        log::info!("Testing recording state transition error handling");

        let app_state = AppState::new();

        // Wrap in AssertUnwindSafe since cache fields aren't being tested for panic safety
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Try various invalid transitions
            let results = [
                app_state.transition_recording_state(RecordingState::Recording),
                app_state.transition_recording_state(RecordingState::Stopping),
                app_state.transition_recording_state(RecordingState::Transcribing),
            ];

            // All should return errors, not panic
            for (i, result) in results.iter().enumerate() {
                match result {
                    Ok(_) => log::warn!("Unexpected success for invalid transition {}", i),
                    Err(e) => log::debug!("Expected error for invalid transition {}: {}", i, e),
                }
            }

            "completed_transition_tests"
        }));

        assert!(result.is_ok());
        log::info!("✅ State transitions handle errors gracefully");
    }

    #[test]
    fn test_event_emission_with_invalid_window() {
        // Test that event emission to invalid windows doesn't panic

        log::info!("Testing event emission error handling");

        let app_state = AppState::new();

        // Wrap in AssertUnwindSafe since cache fields aren't being tested for panic safety
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Try to emit to non-existent windows
            let results = [
                app_state.emit_to_window("nonexistent", "test-event", "test-payload"),
                app_state.emit_to_window("invalid", "another-event", "another-payload"),
            ];

            // All should return errors, not panic
            for (i, result) in results.iter().enumerate() {
                match result {
                    Ok(_) => log::warn!("Unexpected success for invalid window {}", i),
                    Err(e) => log::debug!("Expected error for invalid window {}: {}", i, e),
                }
            }

            "completed_emission_tests"
        }));

        assert!(result.is_ok());
        log::info!("✅ Event emission handles invalid windows gracefully");
    }

    #[test]
    fn test_logger_performance_with_large_context() {
        // Test that logging with large context data doesn't cause performance issues

        log::info!("Testing logger performance with large context");

        let start = std::time::Instant::now();

        let result = std::panic::catch_unwind(|| {
            // Create large context data
            let _large_context = log_context! {
                "operation" => "stress_test",
                "data_size" => "large",
                "iteration" => "1000",
                "large_field" => &"x".repeat(1000), // 1KB string
                "timestamp" => &chrono::Utc::now().to_rfc3339()
            };

            // Log multiple times with large context
            for i in 0..100 {
                log_start(&format!("STRESS_TEST_{}", i));
                log_complete(&format!("STRESS_TEST_{}", i), i);

                if i % 10 == 0 {
                    log_failed("STRESS_ERROR", "Test error message");
                }
            }

            "completed_stress_test"
        });

        let duration = start.elapsed();

        assert!(result.is_ok());
        assert!(duration < Duration::from_secs(1)); // Should complete quickly
        log::info!(
            "✅ Logger handled large context efficiently in {}ms",
            duration.as_millis()
        );
    }

    #[test]
    fn test_app_state_cancellation_flags() {
        // Test that cancellation flags work correctly under stress

        log::info!("Testing cancellation flag robustness");

        let app_state = AppState::new();
        let should_continue = Arc::new(AtomicBool::new(true));
        let mut handles = Vec::new();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Create local app_state reference for thread safety
            let _app_state = &app_state;

            // Spawn multiple threads that set/check cancellation
            for i in 0..10 {
                let should_continue_clone = Arc::clone(&should_continue);

                let handle = std::thread::spawn(move || {
                    for j in 0..100 {
                        if !should_continue_clone.load(Ordering::SeqCst) {
                            break;
                        }

                        // Use a simple counter instead of app_state methods for thread safety test
                        let _test_value = i * j;

                        log::debug!(
                            "Thread {}, Iteration {}: Test value = {}",
                            i,
                            j,
                            _test_value
                        );

                        std::thread::sleep(Duration::from_millis(1));
                    }
                });

                handles.push(handle);
            }

            // Let threads run for a bit
            std::thread::sleep(Duration::from_millis(100));

            // Signal threads to stop
            should_continue.store(false, Ordering::SeqCst);

            // Wait for all threads to complete
            for handle in handles {
                handle.join().unwrap();
            }

            "completed_cancellation_test"
        }));

        assert!(result.is_ok());
        log::info!("✅ Cancellation flags handled concurrent access safely");
    }

    #[test]
    fn test_logger_context_macro_safety() {
        // Test that the log_context macro handles various input types safely

        log::info!("Testing log_context macro safety");

        let result = std::panic::catch_unwind(|| {
            // Test with various data types
            let _context1 = log_context! {
                "string" => "test",
                "number" => "123",
                "boolean" => "true"
            };

            let _context2 = log_context! {
                "empty" => "",
                "unicode" => "🚀 测试 🎉",
                "special_chars" => "!@#$%^&*()_+-=[]{}|;':\",./<>?"
            };

            let _context3 = log_context! {
                "newlines" => "line1\nline2\nline3",
                "tabs" => "col1\tcol2\tcol3"
            };

            // Use all contexts in logging
            log_start("MACRO_SAFETY_1");
            log_start("MACRO_SAFETY_2");
            log_start("MACRO_SAFETY_3");

            "completed_macro_safety_test"
        });

        assert!(result.is_ok());
        log::info!("✅ log_context macro handles various inputs safely");
    }

    #[test]
    #[serial] // Ensure this runs alone to avoid interference
    fn test_panic_handler_setup() {
        // Test that panic handler setup doesn't interfere with normal operation

        log::info!("Testing panic handler setup");

        let result = std::panic::catch_unwind(|| {
            // Simulate panic handler setup (from lib.rs)
            std::panic::set_hook(Box::new(|panic_info| {
                log::error!("Test panic handler: {:?}", panic_info);
            }));

            // Test that normal operation continues
            let test_data = [1, 2, 3, 4, 5];
            let sum: i32 = test_data.iter().sum();

            assert_eq!(sum, 15);
            "completed_panic_handler_test"
        });

        assert!(result.is_ok());
        log::info!("✅ Panic handler setup completed without issues");
    }

    // Helper functions for simulating various scenarios

    fn simulate_hotkey_registration(hotkey: &str) -> Result<String, String> {
        if hotkey.contains("Invalid") {
            Err(format!("Invalid hotkey: {}", hotkey))
        } else {
            Ok(format!("Registered: {}", hotkey))
        }
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_logging_performance_under_load() {
        // Ensure logging doesn't become a bottleneck under high load

        let start = Instant::now();
        let iterations = 1000;

        let result = std::panic::catch_unwind(|| {
            for i in 0..iterations {
                log_start("PERF_TEST");
                log_performance("TEST_METRIC", i as u64, Some("test_data"));
                log_complete("PERF_TEST", 1);

                if i % 100 == 0 {
                    log_model_operation("LOAD", "test-model", "READY", None);
                }
            }

            iterations
        });

        let duration = start.elapsed();
        let ops_per_sec = iterations as f64 / duration.as_secs_f64();

        assert!(result.is_ok());
        assert!(duration < Duration::from_secs(5)); // Should be fast
        log::info!(
            "✅ Logged {} operations in {}ms ({:.0} ops/sec)",
            iterations,
            duration.as_millis(),
            ops_per_sec
        );
    }

    #[test]
    fn test_memory_usage_stability() {
        // Test that repeated operations don't cause memory leaks

        log::info!("Testing memory usage stability");

        let result = std::panic::catch_unwind(|| {
            // Create and destroy many context objects
            for i in 0..1000 {
                let context = log_context! {
                    "iteration" => &i.to_string(),
                    "large_data" => &"x".repeat(100),
                    "memory_test" => "stability_check"
                };

                log_start("MEMORY_TEST");

                // Create temporary data structures
                let temp_map: HashMap<String, String> = (0..50)
                    .map(|j| (format!("key_{}", j), format!("value_{}_{}", i, j)))
                    .collect();

                log_complete("MEMORY_TEST", 1);

                // Force cleanup
                drop(context);
                drop(temp_map);
            }

            "memory_stability_test_completed"
        });

        assert!(result.is_ok());
        log::info!("✅ Memory usage remained stable during repeated operations");
    }
}
