use crate::state::app_state::AppState;
use crate::state::unified_state::UnifiedRecordingState;
use crate::state_machine::RecordingStateMachine;
use crate::RecordingState;
use std::sync::atomic::Ordering;

fn all_recording_states() -> [RecordingState; 6] {
    [
        RecordingState::Idle,
        RecordingState::Starting,
        RecordingState::Recording,
        RecordingState::Stopping,
        RecordingState::Transcribing,
        RecordingState::Error,
    ]
}

fn expected_transition_validity(from: RecordingState, to: RecordingState) -> bool {
    from == to
        || matches!(
            (from, to),
            (RecordingState::Idle, RecordingState::Starting)
                | (RecordingState::Idle, RecordingState::Error)
                | (RecordingState::Starting, RecordingState::Recording)
                | (RecordingState::Starting, RecordingState::Error)
                | (RecordingState::Starting, RecordingState::Idle)
                | (RecordingState::Recording, RecordingState::Stopping)
                | (RecordingState::Recording, RecordingState::Error)
                | (RecordingState::Stopping, RecordingState::Transcribing)
                | (RecordingState::Stopping, RecordingState::Error)
                | (RecordingState::Stopping, RecordingState::Idle)
                | (RecordingState::Transcribing, RecordingState::Idle)
                | (RecordingState::Transcribing, RecordingState::Error)
                | (RecordingState::Error, RecordingState::Idle)
        )
}

#[test]
fn recording_state_machine_matches_full_transition_validity_table() {
    for from in all_recording_states() {
        for to in all_recording_states() {
            let mut machine = RecordingStateMachine::new();
            machine.force_state(from);

            let result = machine.transition_to(to);
            let expected_valid = expected_transition_validity(from, to);

            if expected_valid {
                assert!(
                    result.is_ok(),
                    "transition {from:?} -> {to:?} should be valid"
                );
            } else {
                assert!(
                    result.is_err(),
                    "transition {from:?} -> {to:?} should be invalid"
                );
            }
            assert_eq!(
                machine.current(),
                if expected_valid { to } else { from },
                "transition {from:?} -> {to:?} left machine in unexpected state"
            );
        }
    }
}

#[test]
fn recording_state_machine_reset_returns_every_state_to_idle() {
    for state in all_recording_states() {
        let mut machine = RecordingStateMachine::new();
        machine.force_state(state);

        machine.reset();

        assert_eq!(machine.current(), RecordingState::Idle);
    }
}

#[test]
fn recording_state_machine_force_state_bypasses_transition_validation() {
    let mut machine = RecordingStateMachine::new();

    assert!(machine.transition_to(RecordingState::Recording).is_err());
    assert_eq!(machine.current(), RecordingState::Idle);

    machine.force_state(RecordingState::Recording);

    assert_eq!(machine.current(), RecordingState::Recording);
}

#[test]
fn transition_with_fallback_uses_primary_transition_when_valid() {
    let state = UnifiedRecordingState::new();

    let result = state
        .transition_with_fallback(RecordingState::Starting, |_| {
            panic!("fallback must not run for a valid primary transition")
        })
        .unwrap();

    assert_eq!(result, RecordingState::Starting);
    assert_eq!(state.current(), RecordingState::Starting);
}

#[test]
fn transition_with_fallback_invokes_fallback_for_invalid_transition() {
    let state = UnifiedRecordingState::new();

    let result = state
        .transition_with_fallback(RecordingState::Recording, |current| {
            assert_eq!(current, RecordingState::Idle);
            Some(RecordingState::Error)
        })
        .unwrap();

    assert_eq!(result, RecordingState::Error);
    assert_eq!(state.current(), RecordingState::Error);
}

#[test]
fn transition_with_fallback_none_errors_and_keeps_state() {
    let state = UnifiedRecordingState::new();

    let result = state.transition_with_fallback(RecordingState::Recording, |current| {
        assert_eq!(current, RecordingState::Idle);
        None
    });

    assert!(result.is_err());
    assert_eq!(state.current(), RecordingState::Idle);
}

#[test]
fn unified_force_set_round_trips_through_current() {
    let state = UnifiedRecordingState::new();

    for forced_state in all_recording_states() {
        state.force_set(forced_state).unwrap();
        assert_eq!(state.current(), forced_state);
    }
}

#[test]
fn request_cancellation_marks_cancellation_requested() {
    let state = AppState::new();

    state.request_cancellation();

    assert!(state.is_cancellation_requested());
}

#[test]
fn clear_cancellation_clears_flag_for_new_operation() {
    let state = AppState::new();
    state.request_cancellation();

    // Flag-level contract: clear_cancellation still clears stale cancellation
    // for a new operation. The start_recording race is fixed by clearing only
    // before Starting and checking the flag at commit before entering Recording.
    state.clear_cancellation();

    assert!(!state.is_cancellation_requested());
}

#[test]
fn pending_stop_after_start_swap_consumes_and_resets_flag() {
    let state = AppState::new();

    state.pending_stop_after_start.store(true, Ordering::SeqCst);

    assert!(state.pending_stop_after_start.swap(false, Ordering::SeqCst));
    assert!(!state.pending_stop_after_start.load(Ordering::SeqCst));
    assert!(!state.pending_stop_after_start.swap(false, Ordering::SeqCst));
}

#[test]
fn pending_stop_set_after_starting_publish_survives_to_recording_commit() {
    // Regression for the "stop during Starting is erased" race (finding 1).
    // `start_recording` must clear the stale `pending_stop_after_start` flag
    // BEFORE it publishes `Starting`. It previously cleared it afterwards,
    // which erased a stop that arrived in the Starting window (PTT key-up
    // while Starting sets the flag). This exercises the AppState-level contract
    // that ordering establishes: clear → publish Starting → stop arrives →
    // commit Recording ⇒ the stop is still honored.
    use crate::commands::audio::{begin_recording_generation, current_recording_generation};

    let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let app_state = AppState::new();

    // start_recording: open a new generation and clear stale flags (BEFORE
    // publishing Starting — this is the fix).
    let started_generation = begin_recording_generation();
    app_state.clear_cancellation();
    app_state
        .pending_stop_after_start
        .store(false, Ordering::SeqCst);
    assert_eq!(current_recording_generation(), started_generation);

    // publish Starting, then a stop arrives during the window.
    app_state
        .recording_state
        .force_set(RecordingState::Starting)
        .unwrap();
    app_state
        .pending_stop_after_start
        .store(true, Ordering::SeqCst);

    // commit to Recording and run the pending-stop check.
    app_state
        .recording_state
        .force_set(RecordingState::Recording)
        .unwrap();
    let honored = app_state
        .pending_stop_after_start
        .swap(false, Ordering::SeqCst);

    assert!(
        honored,
        "a stop requested during Starting must survive to the Recording commit"
    );
}

#[test]
fn stale_recording_generation_result_is_rejected() {
    // Backbone regression for finding 2: a transcription task captures its
    // recording's generation at spawn time and must reject its own result once
    // a newer recording has started beneath it. The global cancellation flag
    // alone cannot do this — `start_recording` clears that flag for its own
    // attempt — so a stale prior-generation result would otherwise be pasted
    // during the newer recording.
    use crate::commands::audio::{
        begin_recording_generation, current_recording_generation, recording_generation_is_stale,
    };

    let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // Recording generation 1 begins; its transcription task captures gen 1.
    let gen1 = begin_recording_generation();
    let task_generation = current_recording_generation();
    assert_eq!(gen1, task_generation);
    assert!(!recording_generation_is_stale(task_generation));

    // A newer recording (gen 2) starts while the gen-1 task is still running.
    let gen2 = begin_recording_generation();
    assert!(gen2 > gen1, "generation must advance per recording");

    // The gen-1 task's result is now stale and must be rejected.
    assert!(
        recording_generation_is_stale(task_generation),
        "a prior generation's result must be rejected once a newer recording starts"
    );
    // A gen-2 task is current and not rejected.
    assert!(!recording_generation_is_stale(gen2));
}

/// Race 3 (CRITICAL): the cancellation gate runs in the OUTER transcription
/// task, BEFORE the result is handed to the separate async delivery task. That
/// delivery task awaits `process_transcription`, hides the pill, sleeps, reads
/// settings, then inserts text and saves history WITHOUT rechecking. A cancel /
/// newer-generation arriving in that window is invisible to the outer gate, so
/// the inner task delivers + saves history for the cancelled/stale recording.
///
/// Reproduces the race through the REAL decision (`delivery_aborted`) and revoke
/// (`delete_persisted_recording`) the fix wires into the delivery task: the
/// outer gate passes (gen current), a NEWER recording starts beneath the task,
/// and the delivery checkpoints must abort before insertion and history. Pre-fix
/// the delivery task never rechecked, so it inserted text, saved history, and
/// left the audio on disk.
#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-wide test serialization lock; current-thread test runtime
async fn delivery_recheck_aborts_stale_result_before_insert_and_history() {
    use crate::commands::audio::{
        begin_recording_generation, current_recording_generation, delete_persisted_recording,
        delivery_aborted,
    };
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tempfile::TempDir;

    let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // OUTER task captured generation 1 and PASSED its pre-delivery gate (gen
    // current, no cancel). The window the outer gate cannot see is everything
    // after it, inside the delivery task.
    let task_generation = begin_recording_generation();
    assert_eq!(task_generation, current_recording_generation());
    assert!(
        !delivery_aborted(/* cancelled */ false, task_generation),
        "outer gate passed: result is not yet stale/cancelled"
    );

    // The delivery task owns a recording that was persisted (e.g. save_recordings).
    let dir = TempDir::new().unwrap();
    let recordings_dir = dir.path().join("recordings");
    std::fs::create_dir_all(&recordings_dir).unwrap();
    let filename = "2026-01-01_00-00-00_cafebabe.wav";
    let saved = recordings_dir.join(filename);
    std::fs::write(&saved, b"audio").unwrap();

    // A NEWER recording starts beneath this task while it awaits
    // process_transcription -> the captured generation is now stale.
    let _newer = begin_recording_generation();
    assert_ne!(task_generation, current_recording_generation());
    assert!(
        delivery_aborted(/* cancelled */ false, task_generation),
        "stale generation must abort delivery even with the cancel flag clear"
    );

    // Drive the real delivery checkpoints through the real decision + revoke
    // functions, exactly as the fix wires them into the delivery task.
    let inserted = Arc::new(AtomicBool::new(false));
    let history_saved = Arc::new(AtomicBool::new(false));
    let recordings_dir = Arc::new(recordings_dir);
    let filename = Arc::new(filename.to_string());
    let inserted_c = Arc::clone(&inserted);
    let history_saved_c = Arc::clone(&history_saved);
    let recordings_dir_c = Arc::clone(&recordings_dir);
    let filename_c = Arc::clone(&filename);

    let handle = tokio::spawn(async move {
        // (mirrors) await process_transcription / hide pill / sleep / settings ...
        tokio::task::yield_now().await;

        // Recheck IMMEDIATELY before text insertion (Race 3 fix).
        if delivery_aborted(false, task_generation) {
            delete_persisted_recording(&recordings_dir_c, &filename_c);
            return; // no insert, no history
        }
        inserted_c.store(true, std::sync::atomic::Ordering::SeqCst);

        // Recheck IMMEDIATELY before the history save (Race 3 fix).
        if delivery_aborted(false, task_generation) {
            delete_persisted_recording(&recordings_dir_c, &filename_c);
            return; // no history
        }
        history_saved_c.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    handle.await.unwrap();

    assert!(
        !saved.exists(),
        "stale delivery must revoke its saved audio (was left on disk pre-fix)"
    );
    assert!(
        !inserted.load(std::sync::atomic::Ordering::SeqCst),
        "a stale prior-generation result must NOT be inserted during a newer recording"
    );
    assert!(
        !history_saved.load(std::sync::atomic::Ordering::SeqCst),
        "a stale prior-generation result must NOT be saved to history"
    );
}
