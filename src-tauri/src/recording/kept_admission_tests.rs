use super::*;
use crate::{
    commands::{
        audio::RecorderState,
        license::{CachedLicense, RuntimeLicenseCache},
    },
    license::{LicenseState, LicenseStatus},
    recording::{secure_input::StartPermit, start_source::StartSource},
    AppState, RecordingState,
};
use std::sync::{atomic::Ordering, mpsc, Barrier};
use tauri::test::MockRuntime;

fn child(name: &str) -> bool {
    const CHILD: &str = "VOICETYPR_RETRY_ADMISSION_CHILD";
    if std::env::var_os(CHILD).is_some() {
        return true;
    }
    // Generations, retained audio and feedback ownership are process-global.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "--exact",
            &format!("recording::kept::admission::tests::{name}"),
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
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
            .build(context)
            .unwrap();
        assert_eq!(app.path().app_data_dir().unwrap(), root.path());
        let source = root.path().join("capture.wav");
        super::super::availability_tests::writer(&source)
            .finalize()
            .unwrap();
        let recovery = STORE
            .lock()
            .unwrap()
            .retain(
                &root.path().join(persistent::DIRECTORY),
                &source,
                audio::current_recording_generation(),
                RecoveryKind::CloudFailed,
            )
            .unwrap();
        let path = root
            .path()
            .join(persistent::DIRECTORY)
            .join(format!("{}.wav", recovery.id));
        let index = path.with_extension("json");
        Self {
            wav: std::fs::read(&path).unwrap(),
            metadata: std::fs::read(&index).unwrap(),
            app,
            id: recovery.id,
            path,
            index,
            _root: root,
        }
    }

    fn assert_retained(&self) {
        assert_eq!(std::fs::read(&self.path).unwrap(), self.wav);
        assert_eq!(std::fs::read(&self.index).unwrap(), self.metadata);
        assert!(STORE
            .lock()
            .unwrap()
            .clips
            .iter()
            .any(|clip| clip.recovery.id == self.id));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        STORE.lock().unwrap().clear();
    }
}

fn ready() -> RuntimeLicenseCache {
    RuntimeLicenseCache::Ready(CachedLicense::new(LicenseStatus {
        status: LicenseState::Licensed,
        trial_days_left: None,
        license_type: None,
        license_key: None,
        expires_at: None,
        verification_state: None,
        verification_expires_at: None,
    }))
}

#[test]
fn retry_retires_license_waiting_start_without_late_microphone_open() {
    if !child("retry_retires_license_waiting_start_without_late_microphone_open") {
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let fixture = Fixture::new();
            let app = &fixture.app;
            let state = app.state::<AppState>();
            let before = audio::current_recording_generation();
            let start = audio::start_recording(
                app.handle().clone(),
                app.state::<RecorderState>(),
                Some(StartSource::Hotkey),
            );
            tokio::pin!(start);
            // Poll the real start command into its actual Loading license wait.
            tokio::select! {
                biased;
                result = &mut start => panic!("start did not wait: {result:?}"),
                () = std::future::ready(()) => {}
            }
            assert_eq!(state.get_current_state(), RecordingState::Idle);
            assert!(state
                .pending_start
                .claim(
                    &state,
                    StartSource::Hotkey,
                    StartPermit::capture_state(&state)
                )
                .is_none());

            let lease = admit(app.handle(), &fixture.id, false, None).unwrap();
            assert!(lease.generation > before);
            assert_eq!(state.get_current_state(), RecordingState::Transcribing);
            assert_eq!(std::fs::read(lease.file.path()).unwrap(), fixture.wav);
            *state.license_cache.write().await = ready();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), &mut start)
                    .await
                    .unwrap(),
                Ok(false)
            );
            assert_eq!(audio::current_recording_generation(), lease.generation);
            assert_eq!(state.get_current_state(), RecordingState::Transcribing);
            assert!(!app
                .state::<RecorderState>()
                .0
                .lock()
                .unwrap()
                .is_recording());
            assert!(!fixture._root.path().join("captures").exists());
            fixture.assert_retained();
            // Failed/cancelled retry unwinding must also keep the durable clip.
            drop(lease);
            fixture.assert_retained();
            assert!(!STORE.lock().unwrap().clips[0].busy);
        });
}

#[test]
fn rejected_retry_preserves_pending_start_and_durable_clip() {
    if !child("rejected_retry_preserves_pending_start_and_durable_clip") {
        return;
    }
    let fixture = Fixture::new();
    let state = fixture.app.state::<AppState>();
    let request = state
        .pending_start
        .claim(
            &state,
            StartSource::Hotkey,
            StartPermit::capture_state(&state),
        )
        .unwrap();
    let before = audio::current_recording_generation();
    state.request_cancellation();
    state.stop_in_flight.store(true, Ordering::SeqCst);
    assert_eq!(
        admit(fixture.app.handle(), &fixture.id, false, None)
            .err()
            .as_deref(),
        Some("Dictation is busy")
    );
    state.stop_in_flight.store(false, Ordering::SeqCst);
    for (id, anyway, expected) in [
        ("missing", false, "Recording expired or discarded"),
        (
            fixture.id.as_str(),
            true,
            "This action requires a no-speech clip",
        ),
    ] {
        assert_eq!(
            admit(fixture.app.handle(), id, anyway, None)
                .err()
                .as_deref(),
            Some(expected)
        );
    }
    assert_eq!(audio::current_recording_generation(), before);
    assert!(state.is_cancellation_requested());
    fixture.assert_retained();
    assert!(request.publish(fixture.app.handle()).unwrap().is_some());
    assert!(!state.is_cancellation_requested());
}

#[test]
fn concurrent_retry_start_and_cancel_keep_one_generation_owner() {
    if !child("concurrent_retry_start_and_cancel_keep_one_generation_owner") {
        return;
    }
    let fixture = Fixture::new();
    let app = fixture.app.handle();
    let state = app.state::<AppState>();
    for _ in 0..96 {
        let barrier = Barrier::new(4);
        let (started, retried, cancelled) = std::thread::scope(|scope| {
            let start = scope.spawn(|| {
                barrier.wait();
                state
                    .pending_start
                    .claim(
                        &state,
                        StartSource::Hotkey,
                        StartPermit::capture_state(&state),
                    )
                    .and_then(|request| request.publish(app).unwrap().map(|(g, _)| g))
            });
            let retry = scope.spawn(|| {
                barrier.wait();
                admit(app, &fixture.id, false, None)
            });
            let cancel = scope.spawn(|| {
                barrier.wait();
                state.pending_start.begin_cancel(&state)
            });
            barrier.wait();
            (
                start.join().unwrap(),
                retry.join().unwrap(),
                cancel.join().unwrap(),
            )
        });
        let retry_generation = retried.as_ref().ok().map(|lease| lease.generation);
        if let Err(error) = &retried {
            assert_eq!(error, "Dictation is busy");
        }
        let current = audio::current_recording_generation();
        assert_eq!(
            Some(current),
            started.into_iter().chain(retry_generation).max()
        );
        assert_eq!(
            state.get_current_state(),
            if retry_generation == Some(current) {
                RecordingState::Transcribing
            } else {
                RecordingState::Starting
            }
        );
        assert_eq!(state.is_cancellation_requested(), cancelled.0 == current);
        assert_eq!(
            state.pending_start.with_current(cancelled.0, || true),
            (cancelled.0 == current).then_some(true),
            "stale cancellation cannot clean up the winning generation"
        );
        assert!(!state.pending_start.cancel(), "no orphaned validation");
        drop(retried);
        fixture.assert_retained();
        crate::update_recording_state(app, RecordingState::Idle, None);
    }
}

#[test]
fn admission_holds_publication_lock_until_retry_phase_is_complete() {
    if !child("admission_holds_publication_lock_until_retry_phase_is_complete") {
        return;
    }
    let fixture = Fixture::new();
    let state = fixture.app.state::<AppState>();
    let state = &*state;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let (retry, start, cancel, blocked) = std::thread::scope(|scope| {
        let retry = scope.spawn(move || {
            state
                .pending_start
                .admit_retry(state, || {
                    let generation = audio::begin_recording_generation();
                    state.clear_cancellation();
                    state
                        .transition_recording_state(RecordingState::Starting)
                        .unwrap();
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    for phase in [
                        RecordingState::Recording,
                        RecordingState::Stopping,
                        RecordingState::Transcribing,
                    ] {
                        state.transition_recording_state(phase).unwrap();
                    }
                    Ok(generation)
                })
                .unwrap()
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let start_attempt = attempt_tx.clone();
        let start_done = done_tx.clone();
        let start = scope.spawn(move || {
            start_attempt.send(()).unwrap();
            let claimed = state
                .pending_start
                .claim(
                    state,
                    StartSource::Hotkey,
                    StartPermit::capture_state(state),
                )
                .is_some();
            start_done.send(()).unwrap();
            claimed
        });
        let cancel = scope.spawn(move || {
            attempt_tx.send(()).unwrap();
            let target = state.pending_start.begin_cancel(state);
            done_tx.send(()).unwrap();
            target
        });
        for _ in 0..2 {
            attempt_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        let blocked = matches!(
            done_rx.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        );
        // Always release before asserting, so a failed test cannot strand threads.
        release_tx.send(()).unwrap();
        (
            retry.join().unwrap(),
            start.join().unwrap(),
            cancel.join().unwrap(),
            blocked,
        )
    });
    assert!(
        blocked,
        "start/cancel entered partially published retry state"
    );
    assert!(start, "start must see the completed Transcribing phase");
    assert_eq!(cancel.0, retry);
    assert_eq!(cancel.2, RecordingState::Transcribing);
    assert!(state.is_cancellation_requested());
    fixture.assert_retained();
}
