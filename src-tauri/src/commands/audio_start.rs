//! Generation-owned startup effects. Never keep an ownership lock across await
//! or a synchronous UI-thread hop.
use super::*;
use crate::audio::recorder::CaptureInterrupt;
use crate::commands::recording_runtime::RecordingRuntime;
use crate::recording::{secure_input::StartPermit, storage::CaptureFiles};

/// Derive Escape bindings from live state on every command exit, not just success.
pub(super) struct RebuildBindingsOnExit<R: RecordingRuntime>(
    pub(super) AppHandle<R>,
    pub(super) u64,
);

impl<R: RecordingRuntime> Drop for RebuildBindingsOnExit<R> {
    fn drop(&mut self) {
        crate::trigger::engine_host::rebuild_engine_bindings_for_generation(&self.0, self.1);
    }
}

pub(super) struct PreviewSettings {
    pub tap: bool,
    pub engine: bool,
    pub live: bool,
}

pub(super) fn prepare<R: Runtime>(app: &AppHandle<R>, generation: u64) -> Option<PreviewSettings> {
    let state = app.state::<AppState>();
    state.pending_start.with_active(&state, generation, || {
        state.esc_pressed_once.store(false, AtomicOrdering::SeqCst);
        if let Ok(mut timeout) = state.esc_timeout_handle.lock() {
            if let Some(task) = timeout.take() {
                task.abort();
            }
        }
        let store = app.store("settings").ok();
        let enabled = |key| {
            store
                .as_ref()
                .and_then(|s| s.get(key))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        };
        let live = store
            .as_ref()
            .and_then(|s| s.get("transcription_mode"))
            .and_then(|v| v.as_str().map(|s| s == "live_preview"))
            .unwrap_or(false);
        state
            .recording_live_preview
            .store(live, AtomicOrdering::SeqCst);
        if enabled("pause_media_during_recording") {
            MEDIA_CONTROLLER.pause_if_playing();
        } else {
            // An overlapping retired stop cannot resume after this take owns
            // startup. Apply the new take's disabled preference here instead.
            MEDIA_CONTROLLER.resume_if_we_paused();
        }
        PreviewSettings {
            tap: live || enabled("streaming_tap_enabled"),
            engine: live || enabled("streaming_engine_enabled"),
            live,
        }
    })
}

pub(super) struct CaptureSetup<'a> {
    pub config: &'a RecordingConfig,
    pub preview: &'a PreviewSettings,
    pub path: &'a Path,
    pub microphone: Option<String>,
    pub remote_online: bool,
    pub generation: u64,
    pub interrupt: &'a CaptureInterrupt,
    pub permit: StartPermit,
    pub started: Instant,
}

pub(super) struct CaptureChannels {
    pub levels: Option<std::sync::mpsc::Receiver<f64>>,
    pub silence: Option<std::sync::mpsc::Receiver<SilenceDetectorEvent>>,
    pub readiness: tokio::sync::oneshot::Receiver<RecordingReadiness>,
}

pub(super) fn construct(
    app: &AppHandle,
    setup: CaptureSetup<'_>,
) -> Result<Option<CaptureChannels>, String> {
    let state = app.state::<AppState>();
    state
        .pending_start
        .with_active(&state, setup.generation, || {
            // Ownership is checked BEFORE inspecting a possibly newer recorder or
            // constructing factories that register generation-keyed stream state.
            if !setup.permit.is_current(app) {
                setup.interrupt.interrupt();
                return Ok(None);
            }
            let recorder_state = app.state::<RecorderState>();
            let mut recorder = recorder_state
                .inner()
                .0
                .lock()
                .map_err(|e| format!("Failed to acquire recorder lock: {e}"))?;
            if recorder.is_recording() {
                return Err("Already recording".into());
            }
            let path = setup.path.to_str().ok_or("Invalid path encoding")?;
            let config = setup.config;
            let realtime = config.current_engine == "soniox"
                && config.transcription_task == TRANSCRIPTION_TASK_TRANSCRIBE
                && !setup.remote_online;
            let supported = matches!(
                config.current_engine.as_str(),
                "parakeet" | "whisper" | "soniox" | "deepgram"
            );
            let tap = (setup.preview.tap || realtime) && supported;
            let engine = (setup.preview.engine || realtime) && supported;
            let factory = match config.current_engine.as_str() {
                "soniox" | "deepgram" if setup.remote_online => None,
                "parakeet" => build_parakeet_stream_sink_factory(
                    app,
                    config,
                    tap,
                    engine,
                    setup.preview.live,
                    setup.generation,
                ),
                "whisper" => build_whisper_stream_sink_factory(
                    app,
                    config,
                    tap,
                    engine,
                    setup.preview.live,
                    setup.generation,
                ),
                "soniox" => build_soniox_stream_sink_factory(
                    app,
                    config,
                    tap,
                    engine,
                    setup.preview.live,
                    setup.generation,
                ),
                "deepgram" => build_deepgram_stream_sink_factory(
                    app,
                    config,
                    tap,
                    engine,
                    setup.preview.live,
                    setup.generation,
                ),
                _ => None,
            };
            let cancellation = state.should_cancel_recording.clone();
            let interrupt = setup.interrupt.clone();
            let cancelled: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
                interrupt.is_interrupted() || cancellation.load(AtomicOrdering::SeqCst)
            });
            let ready =
                setup
                    .permit
                    .start_if_current(app, setup.generation, setup.interrupt, || {
                        recorder.start_recording(
                            path,
                            setup.microphone,
                            tap,
                            setup.generation,
                            setup.interrupt.clone(),
                            cancelled,
                            factory,
                            setup.started,
                        )
                    })?;
            let Some(readiness) = ready else {
                return Ok(None);
            };
            if !recorder.is_recording() {
                return Err("Failed to start recording".into());
            }
            Ok(Some(CaptureChannels {
                levels: recorder.take_audio_level_receiver(),
                silence: recorder.take_silence_event_receiver(),
                readiness,
            }))
        })
        .unwrap_or(Ok(None))
}

fn stop_owned<R: Runtime>(
    app: &AppHandle<R>,
    files: Option<&CaptureFiles>,
    generation: u64,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.capture_interrupts.interrupt(generation);
    let recorder_state = app.state::<RecorderState>();
    let result = recorder_state
        .inner()
        .0
        .lock()
        .map_err(|e| format!("Failed to acquire recorder lock: {e}"))
        .and_then(|mut recorder| {
            if recorder.is_recording() {
                recorder.stop_recording()
            } else {
                Ok(String::new())
            }
        });
    if let Some(files) = files {
        files.writer_stopped(
            result
                .as_ref()
                .err()
                .is_some_and(|e| crate::recording::storage::writer_may_be_running(e)),
        );
    }
    clear_pending_stop_after_start(&state);
    state.esc_pressed_once.store(false, AtomicOrdering::SeqCst);
    if let Ok(mut timeout) = state.esc_timeout_handle.lock() {
        if let Some(task) = timeout.take() {
            task.abort();
        }
    }
    MEDIA_CONTROLLER.resume_if_we_paused();
    if let Some(path) = state.current_recording_path.take(generation) {
        crate::recording::storage::remove_finished_audio(&path);
    }
    result.map(|_| ())
}

/// Stop before publishing Idle; never stop a replacement after a late await.
pub(crate) fn abort_start<R: RecordingRuntime>(
    app: &AppHandle<R>,
    files: Option<&CaptureFiles>,
    generation: u64,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    state
        .pending_start
        .with_current(generation, || {
            let result = stop_owned(app, files, generation);
            if result.is_ok() {
                crate::update_recording_state(app, RecordingState::Idle, None);
            }
            result
        })
        .unwrap_or(Ok(()))
}

pub(super) fn fail_start<R: RecordingRuntime>(
    app: &AppHandle<R>,
    files: Option<&CaptureFiles>,
    generation: u64,
    error: String,
    microphone_error: bool,
) -> Result<bool, String> {
    let state = app.state::<AppState>();
    state
        .pending_start
        .with_current(generation, || {
            if !matches!(
                state.get_current_state(),
                RecordingState::Starting | RecordingState::Recording
            ) {
                return Ok(false);
            }
            // Interruption revokes capture permission, not cleanup ownership.
            // Check and restore under the same publication lock so a retired
            // failure can never stop or reset a replacement.
            let interrupted = !state.capture_interrupts.is_active(generation)
                || state.is_cancellation_requested();
            if let Err(stop_error) = stop_owned(app, files, generation) {
                log::warn!("Recorder cleanup after start failure: {stop_error}");
            }
            if interrupted {
                crate::update_recording_state(app, RecordingState::Idle, None);
                return Ok(false);
            }
            if microphone_error {
                if let Some(native) = R::native(app) {
                    island::mic_blocked(native, generation, &error);
                }
            } else {
                crate::commands::island_notice::notice_at(
                    app,
                    crate::commands::island_notice::NoticeKind::RecordingFailed,
                    generation,
                );
            }
            crate::update_recording_state(app, RecordingState::Error, Some(error.clone()));
            Err(error)
        })
        .unwrap_or(Ok(false))
}

pub(super) fn update_state(
    app: &AppHandle,
    generation: u64,
    next: RecordingState,
    error: Option<String>,
) {
    app.state::<AppState>()
        .pending_start
        .with_current(generation, || {
            crate::update_recording_state(app, next, error);
        });
}
