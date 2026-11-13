//! Onboarding-only input proof. No recording, samples retained, or transcription.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering}, mpsc, Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager};

static SESSION: Mutex<Option<TestSession>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct TestSession {
    id: String,
    stop: mpsc::Sender<()>,
    worker: std::thread::JoinHandle<()>,
}

fn stop_locked(session: &mut Option<TestSession>) {
    if let Some(session) = session.take() {
        let _ = session.stop.send(());
        let _ = session.worker.join();
    }
}

pub(crate) fn stop_all() {
    let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    stop_locked(&mut session);
}

#[derive(Clone, serde::Serialize)]
struct TestLevel {
    session_id: String,
    level: f32,
    ended: bool,
    failed: bool,
}

fn input_stream<T>(device: &cpal::Device, config: &cpal::StreamConfig,
    level: Arc<AtomicU32>, failed: Arc<AtomicBool>) -> Result<cpal::Stream, String>
where T: cpal::SizedSample, f32: cpal::FromSample<T> {
    device.build_input_stream(config, move |samples: &[T], _| {
        let mut sum = 0.0_f32;
        for &sample in samples {
            let value = <f32 as cpal::Sample>::from_sample(sample);
            sum += value * value;
        }
        let rms = if samples.is_empty() { 0.0 } else { (sum / samples.len() as f32).sqrt() };
        level.store(rms.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }, move |_| { failed.store(true, Ordering::Relaxed); }, None)
        .map_err(|_| "Could not open the microphone".to_string())
}

fn start(app: tauri::AppHandle, device_name: Option<String>) -> Result<String, String> {
    let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    stop_locked(&mut session);
    if app.state::<crate::AppState>().get_current_state() != crate::RecordingState::Idle {
        return Err("Finish your dictation before testing the microphone".into());
    }
    if !app.get_webview_window("main").is_some_and(|window| window.is_visible().unwrap_or(false)) {
        return Err("Open Voicetypr to test the microphone".into());
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed).to_string();
    let event_id = id.clone();
    let (stop, stopped) = mpsc::channel();
    let (ready, started) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let open = || -> Result<cpal::Stream, String> {
            let host = cpal::default_host();
            let device = match &device_name {
                Some(name) => crate::audio::recorder::find_input_device(&host, name)
                    .map_err(|_| "Could not find the microphone".to_string())?
                    .or_else(|| host.default_input_device()),
                None => host.default_input_device(),
            }.ok_or_else(|| "No microphone available".to_string())?;
            let config = device.default_input_config().map_err(|_| "Could not read microphone configuration".to_string())?;
            let level = Arc::new(AtomicU32::new(0));
            let failed = Arc::new(AtomicBool::new(false));
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => input_stream::<f32>(&device, &config.config(), level.clone(), failed.clone()),
                cpal::SampleFormat::I16 => input_stream::<i16>(&device, &config.config(), level.clone(), failed.clone()),
                cpal::SampleFormat::U16 => input_stream::<u16>(&device, &config.config(), level.clone(), failed.clone()),
                _ => Err("This microphone format is not supported".into()),
            }?;
            stream.play().map_err(|_| "Could not start the microphone".to_string())?;
            let _ = ready.send(Ok(()));
            while stopped.recv_timeout(Duration::from_millis(67)) == Err(mpsc::RecvTimeoutError::Timeout) {
                if failed.load(Ordering::Relaxed) { break; }
                let _ = app.emit_to("main", "microphone-test-level", TestLevel {
                    session_id: event_id.clone(),
                    level: f32::from_bits(level.swap(0, Ordering::Relaxed)),
                    ended: false,
                    failed: false,
                });
            }
            let _ = app.emit_to("main", "microphone-test-level", TestLevel {
                session_id: event_id.clone(), level: 0.0, ended: true,
                failed: failed.load(Ordering::Relaxed),
            });
            Ok(stream)
        };
        if let Err(error) = open() { let _ = ready.send(Err(error)); }
        // The stream is dropped on its owning thread before stop returns.
    });
    match started.recv() {
        Ok(Ok(())) => {
            *session = Some(TestSession { id: id.clone(), stop, worker });
            Ok(id)
        }
        result => {
            let _ = worker.join();
            Err(result.ok().and_then(Result::err).unwrap_or_else(|| "Could not start the microphone".into()))
        }
    }
}

#[tauri::command]
pub async fn start_microphone_test(app: tauri::AppHandle, device_name: Option<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || start(app, device_name)).await.map_err(|_| "Could not start microphone test".to_string())?
}

#[tauri::command]
pub async fn stop_microphone_test(session_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        if session.as_ref().is_some_and(|active| active.id == session_id) { stop_locked(&mut session); }
    }).await.map_err(|_| "Could not stop microphone test".to_string())
}

#[tauri::command]
pub async fn get_microphone_permission_status() -> Result<&'static str, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::{class, msg_send};
        use objc2_foundation::NSString;
        let media = NSString::from_str("soun");
        // The existing permissions plugin exposes only a bool; use the same native query.
        let status: isize = unsafe { msg_send![class!(AVCaptureDevice), authorizationStatusForMediaType: &*media] };
        Ok(match status { 0 => "not_determined", 3 => "granted", 1 => "restricted", _ => "denied" })
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(if crate::commands::permissions::check_microphone_permission().await? { "granted" } else { "denied" })
    }
}
