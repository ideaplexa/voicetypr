use crate::{AppState, RecordingState};
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn retired_start_after_settings_cannot_replace_or_remove_the_next_takes_wav() {
    let root = tempfile::tempdir().unwrap();
    let state = AppState::new();
    let a = 42;
    let b = 43;
    let files_a = crate::recording::storage::CaptureFiles::create(root.path()).unwrap();
    files_a.writer_stopped(false);
    let files_b = crate::recording::storage::CaptureFiles::create(root.path()).unwrap();
    files_b.writer_stopped(false);
    let path_b = files_b.audio_path();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path_b, spec).unwrap();
    writer.write_sample(4321_i16).unwrap();
    writer.finalize().unwrap();

    state.current_recording_path.begin(a);
    state.capture_files.begin(a);
    let interrupt_a = state.capture_interrupts.begin(a);
    state
        .recording_state
        .force_set(RecordingState::Starting)
        .unwrap();
    let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
    let effects = AtomicUsize::new(0);
    let (started, ()) = tokio::join!(
        async {
            resume_rx.await.unwrap(); // A's delayed settings/remote response.
            assert!(!state
                .current_recording_path
                .replace(a, files_a.audio_path()));
            assert!(!state.capture_files.replace(a, files_a.clone()));
            let result = state.pending_start.with_active(&state, a, || {
                effects.fetch_add(1, Ordering::SeqCst);
                state.recording_live_preview.store(false, Ordering::SeqCst);
                state.esc_pressed_once.store(false, Ordering::SeqCst);
                state
                    .recording_state
                    .force_set(RecordingState::Error)
                    .unwrap();
                true // The same guard surrounds native construction and publish.
            });
            // A's stale cleanup cannot consume B's path or capture-directory lease.
            assert!(state.current_recording_path.take(a).is_none());
            assert!(state.capture_files.take(a).is_none());
            result.unwrap_or(false)
        },
        async {
            tokio::task::yield_now().await;
            state.cancel_capture(a);
            state
                .recording_state
                .force_set(RecordingState::Idle)
                .unwrap();
            state.current_recording_path.begin(b);
            state.capture_files.begin(b);
            state.capture_interrupts.begin(b);
            state.clear_cancellation();
            state
                .recording_state
                .force_set(RecordingState::Starting)
                .unwrap();
            assert!(state.current_recording_path.replace(b, path_b.clone()));
            assert!(state.capture_files.replace(b, files_b.clone()));
            state
                .pending_start
                .with_active(&state, b, || {
                    state.recording_live_preview.store(true, Ordering::SeqCst);
                    state.esc_pressed_once.store(true, Ordering::SeqCst);
                    state
                        .recording_state
                        .force_set(RecordingState::Recording)
                        .unwrap();
                })
                .unwrap();
            resume_tx.send(()).unwrap();
        }
    );
    assert!(!started);
    assert!(interrupt_a.is_interrupted());
    assert_eq!(effects.load(Ordering::SeqCst), 0);
    assert_eq!(state.get_current_state(), RecordingState::Recording);
    assert!(state.recording_live_preview.load(Ordering::SeqCst));
    assert!(state.esc_pressed_once.load(Ordering::SeqCst));
    assert_eq!(state.current_recording_path.get(b), Some(path_b.clone()));

    // Production stop/transcription uses this same keyed take operation.
    let transcription_path = state.current_recording_path.take(b).unwrap();
    assert_eq!(transcription_path, path_b);
    let samples = hound::WavReader::open(transcription_path)
        .unwrap()
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(samples, [4321]);
    assert_eq!(state.capture_files.take(b).unwrap().audio_path(), path_b);
}

#[test]
fn ownership_survives_consumption_and_cannot_be_replaced_by_a_retired_generation() {
    let slot = super::CaptureSlot::default();
    slot.begin(42);
    assert!(slot.replace(42, "A"));
    slot.begin(43);
    assert!(!slot.replace(42, "late A"));
    assert!(slot.replace(43, "B"));
    assert!(slot.take(42).is_none());
    assert_eq!(slot.take(43), Some("B"));
    assert!(!slot.replace(42, "resurrected A"));
    assert!(slot.take(43).is_none());
}
