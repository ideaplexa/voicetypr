//! Cancel restores only its captured generation, before any telemetry await.
use super::*;

pub(super) async fn cancel<R: RecordingRuntime>(app: AppHandle<R>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (cancel_generation, pending_only, _) = state.pending_start.begin_cancel(&state);
    if pending_only {
        return Ok(());
    }
    let Some(phase) = state.pending_start.with_current(cancel_generation, || {
        super::super::ai_lifecycle::cancel_recording();
        crate::recording::kept::cancel_generation(&app, cancel_generation);
        if let Ok(mut task) = state.transcription_task.lock() {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
        if let Some(audio) = take_in_flight_transcription_audio() {
            let _ = std::fs::remove_file(audio);
        }
        let phase = state.get_current_state();
        #[cfg(target_os = "windows")]
        if let Some(native) = R::native(&app).filter(|_| phase == RecordingState::Transcribing) {
            native
                .state::<crate::whisper::gpu_sidecar::GpuSidecarClient>()
                .request_abort();
        }
        phase
    }) else {
        return Ok(());
    };
    #[cfg(not(target_os = "windows"))]
    let _ = phase;
    #[cfg(target_os = "windows")]
    if let Some(native) = R::native(&app).filter(|_| phase == RecordingState::Transcribing) {
        native
            .state::<crate::whisper::gpu_sidecar::GpuSidecarClient>()
            .reap_aborted_process_if(|| !recording_generation_is_stale(cancel_generation))
            .await;
    }

    let requested = Instant::now();
    let Some((result, metrics)) = state.pending_start.with_current(cancel_generation, || {
        let recorder = app.state::<RecorderState>();
        let (result, metrics) = match recorder.inner().0.lock() {
            Err(error) => (
                Err(format!("Failed to acquire recorder lock: {error}")),
                None,
            ),
            Ok(mut recorder) if recorder.is_recording() => {
                let result = recorder.stop_recording().map(|_| ());
                (result, Some(recorder.take_last_capture_metrics()))
            }
            Ok(_) => (Ok(()), None),
        };
        if let Some(path) = state.current_recording_path.take(cancel_generation) {
            if result.is_ok() {
                crate::recording::storage::remove_finished_audio(&path);
            }
        }
        let capture_files = state.capture_files.take(cancel_generation);
        if let Some(files) = &capture_files {
            files.writer_stopped(
                result
                    .as_ref()
                    .err()
                    .is_some_and(|error| crate::recording::storage::writer_may_be_running(error)),
            );
        }
        MEDIA_CONTROLLER.resume_if_we_paused();
        state.esc_pressed_once.store(false, AtomicOrdering::SeqCst);
        if let Ok(mut timeout) = state.esc_timeout_handle.lock() {
            if let Some(task) = timeout.take() {
                task.abort();
            }
        }
        match state.get_current_state() {
            RecordingState::Recording => {
                update_recording_state(&app, RecordingState::Stopping, None)
            }
            RecordingState::Transcribing => update_recording_state(
                &app,
                RecordingState::Error,
                Some("Transcription cancelled".into()),
            ),
            _ => {}
        }
        update_recording_state(&app, RecordingState::Idle, None);
        // Local files drop here; replacement publication cannot consume them.
        drop(capture_files);
        (result, metrics)
    }) else {
        return Ok(());
    };
    // Both operations derive LIVE state. The hide decision executes on the UI
    // thread, so it cannot hide a replacement's visible island after a hop.
    hide_unneeded_early_pill(&app);
    crate::trigger::engine_host::rebuild_engine_bindings_for_generation(&app, cancel_generation);
    if let (Some(metrics), Some(native)) = (metrics, R::native(&app)) {
        let mut completion =
            DictationCompletionGuard::new(native, requested, metrics, cancel_generation).await;
        completion.facts.outcome = if result.is_ok() {
            crate::product_analytics::DictationOutcome::Cancelled
        } else {
            crate::product_analytics::DictationOutcome::Failed
        };
    }
    // Restoration precedes propagation, including stop timeout/error paths.
    result
}
