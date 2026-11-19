//! Onboarding's First try: one dictation recorded and transcribed for the
//! setup window alone, before any trial or license. It never enters the
//! dictation pipeline, so nothing is pasted or typed, saved to History, kept
//! for retry, or offered to the tray, CLI, sharing or file jobs. The gate is
//! `license::access::try_allowance`. A try belongs to the armed First try
//! step: disarming, reloading, hiding or leaving the window ends it, and its
//! text is returned only if it still belongs when the engine answers.
use crate::audio::recorder::{AudioRecorder, RecordingReadiness};
use crate::license::access::{self, TRY_WINDOW};
use crate::{AppState, RecordingState};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_plugin_store::StoreExt;

/// A try is a first sentence, not a transcription service.
const CAPTURE_LIMIT: Duration = Duration::from_secs(60);
/// A capped try the window never collected is thrown away after this.
const COLLECT_LIMIT: Duration = Duration::from_secs(30);
const READY_TIMEOUT: Duration = Duration::from_secs(5);
const GONE: &str = "That try ended. Try again.";
const DIRECTORY: &str = "onboarding-try";

static SLOT: Mutex<Option<Session>> = Mutex::new(None);
/// The First try step is on screen, so the user's shortcut talks to it.
static ARMED: AtomicBool = AtomicBool::new(false);
/// Whether the setup window is in front, kept from window events so the
/// shortcut never waits on the UI thread.
static FOCUSED: AtomicBool = AtomicBool::new(false);
/// Every disarm, blur, close or reload ends the tries that began before it.
static EPOCH: AtomicU64 = AtomicU64::new(0);

enum CaptureState {
    Recording(AudioRecorder),
    Finished,
    Failed,
}

type Capture = Arc<Mutex<CaptureState>>;

struct Session {
    id: u64,
    epoch: u64,
    /// The try was counted, so its text may exist.
    charged: bool,
    /// The engine is transcribing it; no other try starts meanwhile.
    decoding: bool,
    capture: Capture,
    path: PathBuf,
}

impl Drop for Session {
    fn drop(&mut self) {
        end_capture(&self.capture).ok();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Stops the microphone once; a later caller waits for and gets the same outcome.
fn end_capture(capture: &Capture) -> Result<(), String> {
    let mut state = capture.lock().unwrap_or_else(|e| e.into_inner());
    if let CaptureState::Recording(recorder) = &mut *state {
        let stopped = recorder.stop_recording();
        *state = if stopped.is_ok() {
            CaptureState::Finished
        } else {
            CaptureState::Failed
        };
    }
    match *state {
        CaptureState::Failed => Err("Could not finish the recording. Try again.".to_string()),
        _ => Ok(()),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TryStarted {
    /// Opaque to the window; a string so no digits are lost on the way.
    id: String,
    /// Tries left on this device; `None` with a trial or license.
    left: Option<u32>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TryText {
    /// The engine's text, before library rules or Polish.
    heard: String,
    /// What built-in Polish made of it, when the switch is on and it ran.
    cleaned: Option<String>,
}

fn lock() -> std::sync::MutexGuard<'static, Option<Session>> {
    SLOT.lock().unwrap_or_else(|e| e.into_inner())
}

fn current(session: &Session, id: u64) -> bool {
    session.id == id && session.epoch == EPOCH.load(Ordering::SeqCst)
}

/// Claims a counted, current try for decoding; it stays in the slot.
fn claim_for_decoding(slot: &mut Option<Session>, id: u64) -> Option<(Capture, PathBuf, u64)> {
    let session = slot
        .as_mut()
        .filter(|session| current(session, id) && session.charged && !session.decoding)?;
    session.decoding = true;
    Some((session.capture.clone(), session.path.clone(), session.epoch))
}

fn parse_id(id: &str) -> Option<u64> {
    u64::from_str_radix(id, 16).ok()
}

/// The text may be shown only if nothing ended the try since it was claimed.
fn may_reply(app: &AppHandle, epoch: u64) -> bool {
    epoch == EPOCH.load(Ordering::SeqCst) && step_in_front(app)
}

/// Takes the session only for the try that started it.
fn take_owned(slot: &mut Option<Session>, id: u64) -> Option<Session> {
    if slot.as_ref().is_some_and(|session| session.id == id) {
        slot.take()
    } else {
        None
    }
}

/// Drops every try that began before `epoch`.
fn take_stale(slot: &mut Option<Session>, epoch: u64) -> Option<Session> {
    if slot.as_ref().is_some_and(|session| session.epoch < epoch) {
        slot.take()
    } else {
        None
    }
}

/// While a try runs, the dictation recorder does not start.
pub(crate) fn active() -> bool {
    lock().is_some()
}

/// Ends every try started so far, at once for its text and off this thread
/// for its microphone and file.
pub(crate) fn cancel() {
    let epoch = EPOCH.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn_blocking(move || {
        let stale = take_stale(&mut lock(), epoch);
        drop(stale);
    });
}

/// On quit the try's file goes at once; its recorder ends with the process.
pub(crate) fn forget_on_exit() {
    if let Some(session) = lock().as_ref() {
        let _ = std::fs::remove_file(&session.path);
    }
}

/// The setup window gained or lost focus; losing it ends any try.
pub(crate) fn focus_changed(focused: bool) {
    FOCUSED.store(focused, Ordering::SeqCst);
    if !focused {
        cancel();
    }
}

/// The setup window reloaded: the step is gone until it mounts again.
pub(crate) fn window_gone() {
    ARMED.store(false, Ordering::SeqCst);
    FOCUSED.store(false, Ordering::SeqCst);
    cancel();
}

/// Removes tries left by a quit or crash. Called at startup.
pub(crate) fn discard_leftovers(app: &AppHandle) {
    if let Ok(data) = app.path().app_data_dir() {
        let _ = std::fs::remove_dir_all(data.join(DIRECTORY));
    }
}

fn onboarding_open(app: &AppHandle) -> bool {
    let Ok(store) = app.store("settings") else {
        return false;
    };
    store
        .get("onboarding_completed")
        .map(|value| value.as_bool() == Some(false))
        .unwrap_or(true)
}

/// Uses the window events only, so it never waits on the UI thread.
fn step_in_front(app: &AppHandle) -> bool {
    ARMED.load(Ordering::SeqCst) && FOCUSED.load(Ordering::SeqCst) && onboarding_open(app)
}

fn window_allows(app: &AppHandle, window: &WebviewWindow) -> bool {
    ARMED.load(Ordering::SeqCst)
        && access::try_window_allows(
            window.label(),
            window.is_visible().unwrap_or(false),
            window.is_focused().unwrap_or(false),
            !onboarding_open(app),
        )
}

/// While the First try step is armed and its window is in front, the
/// dictation shortcut is handed to the step instead of the recorder.
pub(crate) fn claim_hotkey(app: &AppHandle, phase: keytrigger::KeyPhase) -> bool {
    if !step_in_front(app) {
        return false;
    }
    let pressed = matches!(phase, keytrigger::KeyPhase::Pressed);
    let _ = app.emit_to("main", "onboarding-try-key", pressed);
    true
}

#[tauri::command]
pub fn arm_onboarding_try(window: WebviewWindow, armed: bool) {
    if window.label() != "main" {
        return;
    }
    ARMED.store(armed, Ordering::SeqCst);
    if armed {
        FOCUSED.store(window.is_focused().unwrap_or(false), Ordering::SeqCst);
    } else {
        cancel();
    }
}

#[tauri::command]
pub async fn get_onboarding_try(app: AppHandle) -> access::TryAllowance {
    access::try_allowance(&app).await
}

fn try_path(app: &AppHandle, id: u64) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "Could not prepare the try".to_string())?
        .join(DIRECTORY);
    crate::recording::file_jobs::private_directory(&dir)
        .map_err(|_| "Could not prepare the try".to_string())?;
    Ok(dir.join(format!("try-{id:016x}.wav")))
}

fn admit(
    app: &AppHandle,
    id: u64,
    epoch: u64,
    device: Option<String>,
) -> Result<tokio::sync::oneshot::Receiver<RecordingReadiness>, String> {
    let mut slot = lock();
    let state = app.state::<AppState>().get_current_state();
    if slot.is_some() || !matches!(state, RecordingState::Idle | RecordingState::Error) {
        return Err("Dictation is busy. Try again in a moment.".to_string());
    }
    if epoch != EPOCH.load(Ordering::SeqCst) {
        return Err(TRY_WINDOW.to_string());
    }
    crate::audio::mic_test::stop_all();
    let path = try_path(app, id)?;
    let mut recorder = AudioRecorder::new();
    let ready = recorder.start_recording(
        path.to_str()
            .ok_or_else(|| "Could not prepare the try".to_string())?,
        device,
        false,
        id,
        Default::default(),
        Arc::new(|| false),
        None,
        Instant::now(),
    )?;
    *slot = Some(Session {
        id,
        epoch,
        charged: false,
        decoding: false,
        capture: Arc::new(Mutex::new(CaptureState::Recording(recorder))),
        path,
    });
    Ok(ready)
}

/// Counts the try under the slot lock, so two starts never share a count.
fn charge(
    app: &AppHandle,
    id: u64,
    allowance: access::TryAllowance,
) -> Result<Option<u32>, String> {
    let mut slot = lock();
    let session = slot
        .as_mut()
        .filter(|session| current(session, id))
        .ok_or_else(|| TRY_WINDOW.to_string())?;
    if !step_in_front(app) {
        return Err(TRY_WINDOW.to_string());
    }
    let left = access::spend_try(app, allowance)?;
    session.charged = true;
    Ok(left)
}

fn discard(id: u64) {
    tauri::async_runtime::spawn_blocking(move || {
        let owned = take_owned(&mut lock(), id);
        drop(owned);
    });
}

/// Ends the capture at the limit, and throws the try away if the window
/// never collects it.
fn watch(app: AppHandle, id: u64) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CAPTURE_LIMIT).await;
        let capture = lock()
            .as_ref()
            .filter(|session| session.id == id)
            .map(|session| session.capture.clone());
        let Some(capture) = capture else {
            return;
        };
        let ended = tauri::async_runtime::spawn_blocking(move || end_capture(&capture)).await;
        let _ = app.emit_to("main", "onboarding-try-capped", format!("{id:016x}"));
        if matches!(ended, Ok(Ok(()))) {
            tokio::time::sleep(COLLECT_LIMIT).await;
        }
        tauri::async_runtime::spawn_blocking(move || {
            let uncollected = {
                let mut slot = lock();
                if slot
                    .as_ref()
                    .is_some_and(|session| session.id == id && !session.decoding)
                {
                    slot.take()
                } else {
                    None
                }
            };
            drop(uncollected);
        });
    });
}

#[tauri::command]
pub async fn start_onboarding_try(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<TryStarted, String> {
    let epoch = EPOCH.load(Ordering::SeqCst);
    if !window_allows(&app, &window) {
        return Err(TRY_WINDOW.to_string());
    }
    FOCUSED.store(true, Ordering::SeqCst);
    let allowance = access::try_allowance(&app).await;
    if allowance == access::TryAllowance::UsedUp {
        return Err(access::TRY_USED_UP.to_string());
    }
    let device = crate::commands::settings::get_settings(app.clone())
        .await?
        .selected_microphone;
    let id = uuid::Uuid::new_v4().as_u64_pair().0;
    let admitted = app.clone();
    let ready = tauri::async_runtime::spawn_blocking(move || admit(&admitted, id, epoch, device))
        .await
        .map_err(|_| "Could not start the try".to_string())??;
    watch(app.clone(), id);
    if !matches!(
        tokio::time::timeout(READY_TIMEOUT, ready).await,
        Ok(Ok(RecordingReadiness::Ready { .. }))
    ) {
        discard(id);
        return Err("Could not start the microphone".to_string());
    }
    let charging = app.clone();
    let charged = tauri::async_runtime::spawn_blocking(move || charge(&charging, id, allowance))
        .await
        .unwrap_or_else(|_| Err(TRY_WINDOW.to_string()));
    match charged {
        Ok(left) => Ok(TryStarted {
            id: format!("{id:016x}"),
            left,
        }),
        Err(error) => {
            discard(id);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn stop_onboarding_try(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> Result<TryText, String> {
    if window.label() != "main" {
        return Err(TRY_WINDOW.to_string());
    }
    let id = parse_id(&id).ok_or_else(|| GONE.to_string())?;
    let (capture, path, epoch) =
        tauri::async_runtime::spawn_blocking(move || claim_for_decoding(&mut lock(), id))
            .await
            .ok()
            .flatten()
            .ok_or_else(|| GONE.to_string())?;
    // From here this call owns the try's cleanup.
    let finished = tauri::async_runtime::spawn_blocking(move || end_capture(&capture))
        .await
        .unwrap_or_else(|_| Err(GONE.to_string()));
    let result = match finished {
        Ok(()) if may_reply(&app, epoch) => transcribe(&app, &path).await,
        Ok(()) => Err(GONE.to_string()),
        Err(error) => Err(error),
    };
    discard(id);
    let heard = result?;
    if !may_reply(&app, epoch) {
        return Err(GONE.to_string());
    }
    let cleaned =
        crate::ai::builtin::polish_first_try(&app, &heard, || may_reply(&app, epoch)).await;
    if !may_reply(&app, epoch) {
        return Err(GONE.to_string());
    }
    Ok(TryText { heard, cleaned })
}

async fn transcribe(app: &AppHandle, path: &Path) -> Result<String, String> {
    let settings = crate::commands::settings::get_settings(app.clone()).await?;
    let remote = app
        .state::<tauri::async_runtime::Mutex<crate::remote::settings::RemoteSettings>>()
        .lock()
        .await
        .get_active_connection()
        .is_some();
    let (result, _) = crate::commands::audio::transcribe_upload_audio(
        app,
        &path.to_string_lossy(),
        settings.current_model,
        // No hint lets a chosen computer on the network do the work.
        (!remote).then_some(settings.current_model_engine),
        None,
        None,
        None,
        None,
        false,
    )
    .await
    .map_err(|_| "Voicetypr could not transcribe that. Try again.".to_string())?;
    Ok(result.raw_text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: u64, epoch: u64, path: PathBuf) -> Session {
        Session {
            id,
            epoch,
            charged: true,
            decoding: false,
            capture: Arc::new(Mutex::new(CaptureState::Finished)),
            path,
        }
    }

    #[test]
    fn only_the_try_that_started_can_take_its_text() {
        let dir = tempfile::tempdir().unwrap();
        let epoch = EPOCH.load(Ordering::SeqCst);
        let mut slot = Some(session(7, epoch, dir.path().join("try-7.wav")));
        assert!(claim_for_decoding(&mut slot, 6).is_none());
        assert!(claim_for_decoding(&mut slot, 7).is_some());
        assert!(
            claim_for_decoding(&mut slot, 7).is_none(),
            "one decode per try"
        );
        assert!(take_owned(&mut slot, 6).is_none());
        assert_eq!(take_owned(&mut slot, 7).map(|s| s.id), Some(7));
    }

    #[test]
    fn an_uncounted_try_gives_no_text() {
        let dir = tempfile::tempdir().unwrap();
        let mut uncounted = session(
            3,
            EPOCH.load(Ordering::SeqCst),
            dir.path().join("try-3.wav"),
        );
        uncounted.charged = false;
        let mut slot = Some(uncounted);
        assert!(claim_for_decoding(&mut slot, 3).is_none());
    }

    #[test]
    fn leaving_the_step_ends_only_the_tries_before_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut slot = Some(session(1, 4, dir.path().join("try-1.wav")));
        assert!(
            take_stale(&mut slot, 4).is_none(),
            "a try started after the cancel stays"
        );
        assert!(take_stale(&mut slot, 5).is_some());
    }

    #[test]
    fn a_failed_capture_stays_failed_for_every_caller() {
        let capture: Capture = Arc::new(Mutex::new(CaptureState::Failed));
        assert!(end_capture(&capture).is_err());
        assert!(end_capture(&capture).is_err());
        let finished: Capture = Arc::new(Mutex::new(CaptureState::Finished));
        assert!(end_capture(&finished).is_ok());
    }

    #[test]
    fn try_ids_survive_the_window_as_text() {
        let id = u64::MAX - 1;
        assert_eq!(parse_id(&format!("{id:016x}")), Some(id));
        assert_eq!(parse_id("1.8446744073709552e19"), None);
    }

    #[test]
    fn an_ended_try_leaves_no_audio_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("try-3.wav");
        std::fs::write(&path, b"RIFF").unwrap();
        drop(session(3, 0, path.clone()));
        assert!(!path.exists());
    }
}
