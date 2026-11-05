use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn cancel_while_settings_are_suspended_never_constructs_the_microphone() {
    let app = tauri::test::mock_app();
    app.manage(crate::AppState::new());
    let state = app.state::<crate::AppState>();
    state
        .transition_recording_state(crate::RecordingState::Starting)
        .unwrap();
    let generation = 42;
    let interrupt = state.capture_interrupts.begin(generation);
    let permit = StartPermit::capture(app.handle());
    let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
    let constructions = AtomicUsize::new(0);
    let (started, ()) = tokio::join!(
        async {
            // The settings/remote lookup completes late.
            resume_rx.await.unwrap();
            // Other parallel tests advance the process-global generation; use
            // its current value so only the cancelled latch/flag can reject us.
            let result: Result<Option<bool>, ()> = permit.start_if_current(
                app.handle(),
                crate::commands::audio::current_recording_generation(),
                &interrupt,
                || {
                    constructions.fetch_add(1, Ordering::SeqCst);
                    Ok(true)
                },
            );
            // This is the command's construction gate: None returns Ok(false).
            result.unwrap().unwrap_or(false)
        },
        async {
            tokio::task::yield_now().await;
            state.cancel_capture(generation);
            assert!(interrupt.is_interrupted());
            state
                .transition_recording_state(crate::RecordingState::Idle)
                .unwrap();
            // Clearing a shared flag for another attempt cannot revive this one.
            state.clear_cancellation();
            resume_tx.send(()).unwrap();
        }
    );
    assert!(!started);
    assert_eq!(constructions.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancel_wakes_its_own_readiness_without_interrupting_the_next_capture() {
    let state = crate::AppState::new();
    let interrupted = state.capture_interrupts.begin(42);
    let (_ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    state.cancel_capture(42);
    let next = state.capture_interrupts.begin(43);
    state.clear_cancellation();
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        crate::audio::recorder::wait_for_readiness(ready_rx, &interrupted),
    )
    .await
    .unwrap();
    assert!(matches!(
        result,
        Some(crate::audio::recorder::RecordingReadiness::Failed(_))
    ));
    assert!(!next.is_interrupted());
}
