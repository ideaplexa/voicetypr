//! Exercise the actual start command, with only native I/O mocked.
use super::*;
use crate::commands::license::{CachedLicense, RuntimeLicenseCache};
use crate::license::{LicenseState, LicenseStatus};
use crate::recording::{secure_input, start_source::StartSource};
use tauri::test::MockRuntime;

struct BeforeCapture(Mutex<Option<Box<dyn FnOnce() + Send>>>);
struct PillProbe {
    shown: std::sync::mpsc::Sender<()>,
    visible: AtomicBool,
    settled: tokio::sync::Notify,
}

pub(super) fn pill_shown<R: Runtime>(app: &AppHandle<R>) {
    if let Some(probe) = app.try_state::<PillProbe>() {
        probe.visible.store(true, AtomicOrdering::SeqCst);
        probe.shown.send(()).unwrap();
    }
}

pub(super) fn pill_hidden<R: Runtime>(app: &AppHandle<R>) {
    if let Some(probe) = app.try_state::<PillProbe>() {
        probe.visible.store(false, AtomicOrdering::SeqCst);
    }
}

pub(super) fn pill_settled<R: Runtime>(app: &AppHandle<R>) {
    if let Some(probe) = app.try_state::<PillProbe>() {
        probe.settled.notify_one();
    }
}

pub(super) fn before_capture<R: Runtime>(app: &AppHandle<R>) {
    let work = app
        .try_state::<BeforeCapture>()
        .and_then(|hook| hook.0.lock().unwrap().take());
    if let Some(work) = work {
        work();
    }
}

#[derive(Clone, Copy)]
enum Case {
    Interrupted,
    ActiveError,
    Replaced,
}

fn isolated(name: &str, case: Case) {
    const CHILD: &str = "VOICETYPR_START_COMMAND_REGRESSION_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // Recording generations and feedback ownership are process-global.
        // A private child avoids racing unrelated parallel tests.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args([
                "--exact",
                &format!("commands::audio::start_command_tests::{name}"),
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout).to_string()
                + &String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run(case));
}

#[test]
fn secure_input_storage_failure_restores_the_full_start_command() {
    isolated(
        "secure_input_storage_failure_restores_the_full_start_command",
        Case::Interrupted,
    );
}

#[test]
fn ordinary_storage_failure_still_reports_an_error() {
    isolated(
        "ordinary_storage_failure_still_reports_an_error",
        Case::ActiveError,
    );
}

#[test]
fn retired_storage_failure_cannot_clean_up_a_replacement() {
    isolated(
        "retired_storage_failure_cannot_clean_up_a_replacement",
        Case::Replaced,
    );
}

async fn run(case: Case) {
    let root = tempfile::tempdir().unwrap();
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    // Absolute identifiers isolate the resolver and Store in this tempdir.
    context.config_mut().identifier = root.path().to_str().unwrap().into();
    let app = tauri::test::mock_builder()
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(AppState::new())
        .manage(RecorderState(Mutex::new(AudioRecorder::new())))
        .build(context)
        .unwrap();
    assert_eq!(app.path().app_data_dir().unwrap(), root.path());
    let settings = app.store("settings").unwrap();
    settings.set("pill_indicator_mode", serde_json::json!("when_recording"));
    settings.set("current_model_engine", serde_json::json!("whisper"));
    settings.set("current_model", serde_json::json!("small.en"));
    let _window =
        tauri::WebviewWindowBuilder::new(&app, "pill", tauri::WebviewUrl::App("pill.html".into()))
            .visible(false)
            .build()
            .unwrap();
    // MockRuntime's is_visible() is a constant true. Observe the actual show/
    // hide dispatches instead; the native rendering itself still needs smoke.
    let (shown, shown_rx) = std::sync::mpsc::channel();
    app.manage(PillProbe {
        shown,
        visible: AtomicBool::new(false),
        settled: tokio::sync::Notify::new(),
    });
    let state = app.state::<AppState>();
    *state.license_cache.write().await =
        RuntimeLicenseCache::Ready(CachedLicense::new(LicenseStatus {
            status: LicenseState::Licensed,
            trial_days_left: None,
            license_type: None,
            license_key: None,
            expires_at: None,
            verification_state: None,
            verification_expires_at: None,
        }));
    // A real filesystem error, without filling the disk or changing permissions:
    // captures cannot be a directory because a regular file occupies its name.
    std::fs::write(root.path().join("captures"), b"not a directory").unwrap();
    let replacement_path = root.path().join("replacement.wav");
    let replacement_for_hook = replacement_path.clone();
    let hook_app = app.handle().clone();
    app.manage(BeforeCapture(Mutex::new(Some(Box::new(move || {
        let state = hook_app.state::<AppState>();
        let generation = current_recording_generation();
        assert_eq!(state.get_current_state(), RecordingState::Starting);
        assert!(escape_bound(&hook_app));
        assert!(!state.pointer_recording.load(AtomicOrdering::SeqCst));
        // Ensure the command's own early show has happened before the edge.
        shown_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        state.esc_pressed_once.store(true, AtomicOrdering::SeqCst);
        match case {
            Case::ActiveError => {}
            Case::Interrupted => {
                let (sender, receiver) = std::sync::mpsc::channel();
                let callback_app = hook_app.clone();
                state
                    .trigger_engine
                    .start_with_source_and_status(
                        Arc::new(keytrigger::engine::MockSource::new(vec![(
                            keytrigger::Msg::Control(keytrigger::Control::SecureInput(true)),
                            Duration::ZERO,
                        )])),
                        |_| {},
                        move |edge| {
                            let state = callback_app.state::<AppState>();
                            let interrupted = secure_input::interrupt_capture(
                                &state,
                                generation,
                                edge.enabled,
                                false,
                            );
                            sender.send(interrupted).unwrap();
                        },
                    )
                    .unwrap();
                let interrupted = receiver.recv_timeout(Duration::from_secs(3));
                state.trigger_engine.stop();
                assert!(interrupted.unwrap());
                assert!(!state.capture_interrupts.is_active(generation));
                assert!(
                    !state.is_cancellation_requested(),
                    "Secure Input uses the own-take latch"
                );
                assert!(secure_input::system_stop_must_yield(
                    Some(generation),
                    generation,
                    state.get_current_state()
                ));
            }
            Case::Replaced => {
                state.cancel_capture(generation);
                update_recording_state(&hook_app, RecordingState::Idle, None);
                let replacement = begin_recording_generation();
                state.current_recording_path.begin(replacement);
                state.capture_files.begin(replacement);
                state.capture_interrupts.begin(replacement);
                state.clear_cancellation();
                std::fs::write(&replacement_for_hook, b"replacement").unwrap();
                assert!(state
                    .current_recording_path
                    .replace(replacement, replacement_for_hook));
                update_recording_state(&hook_app, RecordingState::Starting, None);
                crate::trigger::engine_host::rebuild_engine_bindings_for_generation(
                    &hook_app,
                    replacement,
                );
            }
        }
    })))));
    let result = start_recording(
        app.handle().clone(),
        app.state::<RecorderState>(),
        Some(StartSource::Hotkey),
    )
    .await;
    let probe = app.state::<PillProbe>();
    tokio::time::timeout(Duration::from_secs(3), probe.settled.notified())
        .await
        .unwrap();
    assert!(!app
        .state::<RecorderState>()
        .0
        .lock()
        .unwrap()
        .is_recording());
    assert!(!state.pending_stop_after_start.load(AtomicOrdering::SeqCst));
    assert!(state.pending_stop_requested.lock().unwrap().is_none());
    match case {
        Case::Interrupted => {
            assert_eq!(result, Ok(false));
            assert_eq!(state.get_current_state(), RecordingState::Idle);
            assert!(!escape_bound(app.handle()));
            assert!(!state.esc_pressed_once.load(AtomicOrdering::SeqCst));
            assert!(state.esc_timeout_handle.lock().unwrap().is_none());
            assert!(!crate::commands::pill_feedback::feedback_owned());
            assert!(state
                .current_recording_path
                .get(current_recording_generation())
                .is_none());
            assert!(!probe.visible.load(AtomicOrdering::SeqCst));
            // A later press can claim a new start, rather than queue another stop.
            assert!(state
                .pending_start
                .claim(
                    &state,
                    StartSource::Hotkey,
                    secure_input::StartPermit::capture(app.handle())
                )
                .is_some());
        }
        Case::ActiveError => {
            assert_eq!(result, Err("Capture storage unavailable".into()));
            assert_eq!(state.get_current_state(), RecordingState::Error);
            assert!(!escape_bound(app.handle()));
        }
        Case::Replaced => {
            assert_eq!(result, Ok(false));
            assert_eq!(state.get_current_state(), RecordingState::Starting);
            assert!(escape_bound(app.handle()));
            assert!(state.esc_pressed_once.load(AtomicOrdering::SeqCst));
            assert_eq!(
                state
                    .current_recording_path
                    .get(current_recording_generation()),
                Some(replacement_path.clone())
            );
            assert_eq!(std::fs::read(replacement_path).unwrap(), b"replacement");
            assert!(probe.visible.load(AtomicOrdering::SeqCst));
        }
    }
}

fn escape_bound(app: &AppHandle<MockRuntime>) -> bool {
    app.state::<AppState>()
        .engine_bindings
        .lock()
        .unwrap()
        .iter()
        .any(|binding| binding.id == "escape-cancel")
}
