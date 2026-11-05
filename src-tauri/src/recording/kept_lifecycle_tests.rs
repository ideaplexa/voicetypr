//! Real retry/Cancel/Escape command paths; only native decoding/UI are mocked.
use super::*;
use crate::recording::{secure_input::StartPermit, start_source::StartSource};
use crate::{commands::audio::RecorderState, AppState, RecordingState};
use std::sync::{mpsc, Arc};
use tauri::{test::MockRuntime, Listener};

pub(super) struct Work {
    pub(super) started: tokio::sync::Notify,
    pub(super) finish: tokio::sync::Notify,
    pub(super) result: Mutex<Option<Result<(), String>>>,
    pub(super) selection: RetrySelection,
}

pub(super) struct Pause {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl Pause {
    pub(super) fn wait(&self) {
        self.entered.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
    }
}
pub(super) struct DeletePause(pub(super) Arc<Pause>);
struct BeforeIdle(Mutex<Option<Arc<Pause>>>);
pub(super) fn before_idle<R: tauri::Runtime>(app: &AppHandle<R>) {
    let pause = app
        .try_state::<BeforeIdle>()
        .and_then(|hook| hook.0.lock().unwrap().take());
    if let Some(pause) = pause {
        pause.wait();
    }
}
fn pause() -> (Arc<Pause>, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (entered, receiver) = mpsc::channel();
    let (sender, release) = mpsc::channel();
    (
        Arc::new(Pause {
            entered,
            release: Mutex::new(release),
        }),
        receiver,
        sender,
    )
}

fn publish_start(app: &AppHandle<MockRuntime>) -> u64 {
    let state = app.state::<AppState>();
    state
        .pending_start
        .claim(
            &state,
            StartSource::Hotkey,
            StartPermit::capture_state(&state),
        )
        .unwrap()
        .publish(app)
        .unwrap()
        .unwrap()
        .0
}

fn child(name: &str) -> bool {
    const CHILD: &str = "VOICETYPR_RETRY_LIFECYCLE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "--exact",
            &format!("recording::kept::lifecycle_tests::{name}"),
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("running 1 test"),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

struct Fixture {
    app: tauri::App<MockRuntime>,
    id: String,
    path: PathBuf,
    index: PathBuf,
    wav: Vec<u8>,
    metadata: Vec<u8>,
    recovery: serde_json::Value,
    _root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = root.path().to_str().unwrap().into();
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .manage(AppState::new())
            .manage(RecorderState(Mutex::new(
                crate::audio::recorder::AudioRecorder::new(),
            )))
            .manage(Work {
                started: Default::default(),
                finish: Default::default(),
                result: Mutex::new(Some(Ok(()))),
                selection: RetrySelection {
                    model: "small.en".into(),
                    engine: Some("whisper".into()),
                },
            })
            .build(context)
            .unwrap();
        assert_eq!(app.path().app_data_dir().unwrap(), root.path());
        let source = root.path().join("capture.wav");
        super::availability_tests::writer(&source)
            .finalize()
            .unwrap();
        let mut store = STORE.lock().unwrap();
        let recovery = store
            .retain(
                &root.path().join(persistent::DIRECTORY),
                &source,
                audio::current_recording_generation(),
                RecoveryKind::CloudFailed,
            )
            .unwrap();
        let id = recovery.id;
        let clip = store
            .clips
            .iter_mut()
            .find(|c| c.recovery.id == id)
            .unwrap();
        clip.recovery.retry_failed = true;
        clip.recovery.engine_short = "Soniox".into();
        clip.recovery
            .set_alternative(Some(("small.en".into(), "whisper".into())));
        clip.retry_selection = Some(RetrySelection {
            model: "original-cloud".into(),
            engine: Some("soniox".into()),
        });
        let recovery = serde_json::to_value(&clip.recovery).unwrap();
        let path = clip.path.clone();
        store.persist(&id);
        drop(store);
        let index = path.with_extension("json");
        Self {
            wav: std::fs::read(&path).unwrap(),
            metadata: std::fs::read(&index).unwrap(),
            app,
            id,
            path,
            index,
            recovery,
            _root: root,
        }
    }

    fn assert_retained(&self) {
        assert_eq!(std::fs::read(&self.path).unwrap(), self.wav);
        assert_eq!(std::fs::read(&self.index).unwrap(), self.metadata);
        let store = STORE.lock().unwrap();
        let clip = store
            .clips
            .iter()
            .find(|c| c.recovery.id == self.id)
            .unwrap();
        assert_eq!(serde_json::to_value(&clip.recovery).unwrap(), self.recovery);
        assert!(!clip.busy);
        assert_eq!(clip.retry_generation, None);
        assert_eq!(
            clip.retry_selection.as_ref().unwrap().model,
            "original-cloud"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        STORE.lock().unwrap().clear();
    }
}

async fn cancel_case(escape: bool) {
    for engine in [None, Some("small.en".to_owned())] {
        let fixture = Fixture::new();
        let app = fixture.app.handle();
        let state = app.state::<AppState>();
        let work = app.state::<Work>();
        let retry = retry_kept_dictation(app.clone(), fixture.id.clone(), engine);
        tokio::pin!(retry);
        tokio::select! {
            biased;
            result = &mut retry => panic!("retry did not reach decoding: {result:?}"),
            () = work.started.notified() => {}
        }
        assert_eq!(state.get_current_state(), RecordingState::Transcribing);
        crate::commands::pill_feedback::with_queue_for_test(|queued| queued.clear());
        if escape {
            let idle = Arc::new(tokio::sync::Notify::new());
            let signal = idle.clone();
            let listener = app.listen("recording-state-changed", move |event| {
                let payload: serde_json::Value = serde_json::from_str(event.payload()).unwrap();
                if payload["state"] == "idle" {
                    signal.notify_one();
                }
            });
            super::super::escape_handler::handle_escape_key_press(
                &state,
                app,
                keytrigger::KeyPhase::Pressed,
            )
            .await;
            assert_eq!(state.get_current_state(), RecordingState::Transcribing);
            assert!(state
                .esc_pressed_once
                .load(std::sync::atomic::Ordering::SeqCst));
            super::super::escape_handler::handle_escape_key_press(
                &state,
                app,
                keytrigger::KeyPhase::Pressed,
            )
            .await;
            tokio::time::timeout(Duration::from_secs(3), idle.notified())
                .await
                .unwrap();
            app.unlisten(listener);
        } else {
            audio::cancel_recording(app.clone()).await.unwrap();
        }
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(3), &mut retry)
                .await
                .unwrap(),
            Err("Retry discarded".into())
        );
        assert_eq!(state.get_current_state(), RecordingState::Idle);
        assert!(!state
            .esc_pressed_once
            .load(std::sync::atomic::Ordering::SeqCst));
        assert!(state.esc_timeout_handle.lock().unwrap().is_none());
        fixture.assert_retained();
        let restored = crate::commands::pill_feedback::with_queue_for_test(|queued| {
            assert!(!queued
                .iter()
                .any(|(event, _)| *event == "kept-dictation-expired"));
            queued
                .iter()
                .rev()
                .find(|(event, payload)| {
                    *event == "dictation-recovery" && payload["id"] == fixture.id
                })
                .map(|(_, payload)| payload.clone())
                .expect("cancelled retry must restore its card")
        });
        for key in [
            "generation",
            "id",
            "kind",
            "engine_short",
            "alt_engine_short",
            "alt_engine_id",
            "retry_ready",
            "retry_failed",
        ] {
            assert_eq!(restored[key], fixture.recovery[key], "{key}");
        }
        assert!(
            restored["expires_in_ms"].as_u64().unwrap()
                <= fixture.recovery["expires_in_ms"].as_u64().unwrap()
        );
        tokio::task::yield_now().await;
    }
}

#[test]
fn cancel_command_keeps_retry_audio_index_and_original_failed_card() {
    if !child("cancel_command_keeps_retry_audio_index_and_original_failed_card") {
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(cancel_case(false));
}

#[test]
fn double_escape_keeps_retry_audio_index_and_original_failed_card() {
    if !child("double_escape_keeps_retry_audio_index_and_original_failed_card") {
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(cancel_case(true));
}

#[test]
fn checked_retry_completion_serializes_idle_before_replacement_start() {
    if !child("checked_retry_completion_serializes_idle_before_replacement_start") {
        return;
    }
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            for succeeds in [true, false] {
                let fixture = Fixture::new();
                let app = fixture.app.handle();
                *app.state::<Work>().result.lock().unwrap() = Some(if succeeds {
                    Ok(())
                } else {
                    Err("engine failed".into())
                });
                let retry =
                    tokio::spawn(retry_kept_dictation(app.clone(), fixture.id.clone(), None));
                app.state::<Work>().started.notified().await;
                let (hook, entered, release) = pause();
                app.manage(BeforeIdle(Mutex::new(Some(hook))));
                app.state::<Work>().finish.notify_one();
                // A has passed its generation check, just before publishing Idle.
                entered.recv_timeout(Duration::from_secs(3)).unwrap();
                let replacement = app.clone();
                let (attempted_tx, attempted_rx) = mpsc::channel();
                let (started_tx, started_rx) = mpsc::channel();
                let start = std::thread::spawn(move || {
                    attempted_tx.send(()).unwrap();
                    started_tx.send(publish_start(&replacement)).unwrap();
                });
                attempted_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                let early = started_rx.recv_timeout(Duration::from_millis(100));
                let blocked = matches!(early, Err(mpsc::RecvTimeoutError::Timeout));
                // Always release and join before assertions.
                release.send(()).unwrap();
                let generation = early
                    .unwrap_or_else(|_| started_rx.recv_timeout(Duration::from_secs(3)).unwrap());
                start.join().unwrap();
                let result = retry.await.unwrap();
                assert_eq!(result.is_ok(), succeeds);
                assert!(blocked, "B published between A's check and Idle write");
                let state = app.state::<AppState>();
                assert_eq!(audio::current_recording_generation(), generation);
                assert_eq!(state.get_current_state(), RecordingState::Starting);
                assert!(!state.is_cancellation_requested());
                assert_eq!(
                    state.pending_start.with_active(&state, generation, || true),
                    Some(true)
                );
                assert_eq!(fixture.path.exists(), !succeeds);
                assert_eq!(fixture.index.exists(), !succeeds);
            }
        });
}

#[test]
fn replacement_published_before_completion_survives_cancelled_retry_unwind() {
    if !child("replacement_published_before_completion_survives_cancelled_retry_unwind") {
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let fixture = Fixture::new();
            let app = fixture.app.handle();
            let work = app.state::<Work>();
            let retry = retry_kept_dictation(app.clone(), fixture.id.clone(), None);
            tokio::pin!(retry);
            tokio::select! {
                biased;
                result = &mut retry => panic!("retry did not wait: {result:?}"),
                () = work.started.notified() => {}
            }
            let generation = publish_start(app);
            assert_eq!((&mut retry).await, Err("Retry discarded".into()));
            let state = app.state::<AppState>();
            assert_eq!(state.get_current_state(), RecordingState::Starting);
            assert_eq!(audio::current_recording_generation(), generation);
            assert!(!state.is_cancellation_requested());
            fixture.assert_retained();
        });
}

#[test]
fn delayed_delete_completion_cannot_cancel_or_idle_a_replacement() {
    if !child("delayed_delete_completion_cannot_cancel_or_idle_a_replacement") {
        return;
    }
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let fixture = Fixture::new();
            let app = fixture.app.handle();
            let lease = admission::admit(app, &fixture.id, false, None).unwrap();
            let (hook, entered, release) = pause();
            app.manage(DeletePause(hook));
            let deletion = tokio::spawn(discard_kept_dictation(app.clone(), fixture.id.clone()));
            entered.recv_timeout(Duration::from_secs(3)).unwrap();
            let generation = publish_start(app);
            // Also force the pre-I/O cancellation helper to resume with an old target.
            completion::cancelled(app, Some(lease.generation));
            release.send(()).unwrap();
            deletion.await.unwrap().unwrap();
            let state = app.state::<AppState>();
            assert_eq!(audio::current_recording_generation(), generation);
            assert_eq!(state.get_current_state(), RecordingState::Starting);
            assert!(!state.is_cancellation_requested());
            assert!(
                !fixture.path.exists(),
                "explicit Delete audio still deletes"
            );
            assert!(!fixture.index.exists());
            drop(lease);
        });
}

#[test]
fn shutdown_preserves_retry_audio_and_suppresses_late_state_and_card_publication() {
    if !child("shutdown_preserves_retry_audio_and_suppresses_late_state_and_card_publication") {
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let fixture = Fixture::new();
            let app = fixture.app.handle();
            let work = app.state::<Work>();
            let retry = retry_kept_dictation(app.clone(), fixture.id.clone(), None);
            tokio::pin!(retry);
            tokio::select! {
                biased;
                result = &mut retry => panic!("retry did not wait: {result:?}"),
                () = work.started.notified() => {}
            }
            crate::commands::pill_feedback::with_queue_for_test(|queued| queued.clear());
            let states = Arc::new(Mutex::new(Vec::new()));
            let observed = states.clone();
            let listener = app.listen("recording-state-changed", move |event| {
                observed.lock().unwrap().push(event.payload().to_owned());
            });
            cleanup();
            assert!(STORE.lock().unwrap().closed);
            assert_eq!((&mut retry).await, Err("Retry discarded".into()));
            assert!(states.lock().unwrap().is_empty());
            app.unlisten(listener);
            crate::commands::pill_feedback::with_queue_for_test(
                |queued| assert!(queued.is_empty()),
            );
            assert_eq!(std::fs::read(&fixture.path).unwrap(), fixture.wav);
            assert_eq!(std::fs::read(&fixture.index).unwrap(), fixture.metadata);
        });
}

#[test]
fn late_cancel_after_retry_failure_keeps_the_new_failed_card_and_audio() {
    if !child("late_cancel_after_retry_failure_keeps_the_new_failed_card_and_audio") {
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let fixture = Fixture::new();
            let app = fixture.app.handle();
            let work = app.state::<Work>();
            *work.result.lock().unwrap() = Some(Err("engine failed".into()));
            work.finish.notify_one();
            assert_eq!(
                retry_kept_dictation(app.clone(), fixture.id.clone(), None).await,
                Err("Retry failed; recording remains available until expiry".into())
            );
            assert_eq!(
                app.state::<AppState>().get_current_state(),
                RecordingState::Idle
            );
            let metadata = std::fs::read(&fixture.index).unwrap();
            let recovery = STORE.lock().unwrap().clips[0].recovery.clone();
            assert_eq!(recovery.generation, audio::current_recording_generation());
            audio::cancel_recording(app.clone()).await.unwrap();
            super::super::escape_handler::handle_escape_key_press(
                &app.state::<AppState>(),
                app,
                keytrigger::KeyPhase::Pressed,
            )
            .await;
            assert_eq!(std::fs::read(&fixture.path).unwrap(), fixture.wav);
            assert_eq!(std::fs::read(&fixture.index).unwrap(), metadata);
            assert_eq!(STORE.lock().unwrap().clips.len(), 1);
            crate::commands::pill_feedback::with_queue_for_test(|queued| {
                assert!(queued
                    .iter()
                    .any(|(event, payload)| *event == "dictation-recovery"
                        && payload["id"] == fixture.id
                        && payload["generation"] == recovery.generation));
                assert!(!queued
                    .iter()
                    .any(|(event, _)| *event == "kept-dictation-expired"));
            });
        });
}
