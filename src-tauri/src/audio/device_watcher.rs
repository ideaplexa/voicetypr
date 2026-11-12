use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::audio::recorder::AudioRecorder;
use crate::commands::settings::{get_settings, set_audio_device, update_tray_menu};
use crate::{get_recording_state, RecordingState};

/// Check if device watcher should start and start it if conditions are met.
/// Conditions: onboarding_completed = true AND microphone permission granted.
/// This is called from backend when either condition becomes true.
pub async fn try_start_device_watcher_if_ready(app: &AppHandle) {
    // Check onboarding status
    let onboarding_done = app
        .store("settings")
        .ok()
        .and_then(|store| store.get("onboarding_completed").and_then(|v| v.as_bool()))
        .unwrap_or(false);

    if !onboarding_done {
        log::debug!("Device watcher: onboarding not complete, skipping start");
        return;
    }

    // Check mic permission (using the same check as the permission command)
    #[cfg(target_os = "macos")]
    let has_mic_permission = {
        use tauri_plugin_macos_permissions::check_microphone_permission;
        check_microphone_permission().await
    };

    #[cfg(not(target_os = "macos"))]
    let has_mic_permission = true;

    if !has_mic_permission {
        log::debug!("Device watcher: mic permission not granted, skipping start");
        return;
    }

    // Both conditions met - start the device watcher
    log::info!("Device watcher: both conditions met, starting watcher");
    let device_watcher = app.state::<DeviceWatcher>();
    device_watcher.start();
}

/// Background watcher that monitors OS microphone devices and emits updates.
/// Created in a deferred state - does not poll devices until `start()` is called.
/// This prevents early mic permission prompts before user grants permission.
pub struct DeviceWatcher {
    stop: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
    app: AppHandle,
}

impl DeviceWatcher {
    /// Create a new DeviceWatcher in a deferred (not-started) state.
    /// Call `start()` after mic permission is granted to begin polling.
    pub fn new(app: AppHandle) -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            started: Arc::new(AtomicBool::new(false)),
            handle: Mutex::new(None),
            app,
        }
    }

    /// Check if the watcher is currently running.
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.started.load(Ordering::Relaxed)
    }

    /// Start the watcher on a dedicated thread.
    /// Safe to call multiple times - will no-op if already running.
    /// Should only be called after mic permission is granted.
    pub fn start(&self) {
        if self.started.swap(true, Ordering::SeqCst) {
            log::debug!("DeviceWatcher already running, skipping start");
            return;
        }

        log::info!("Starting DeviceWatcher after mic permission granted");

        let stop_flag = self.stop.clone();
        let app = self.app.clone();

        let handle = thread::spawn(move || {
            // The first enumeration is the startup baseline (the tray built during
            // setup already reflects it), so we skip just that one redundant rebuild.
            // Option<> means an empty initial list still counts as the baseline; every
            // real change afterward rebuilds.
            let mut last_devices: Option<Vec<String>> = None;

            while !stop_flag.load(Ordering::Relaxed) {
                let devices = AudioRecorder::get_devices();

                if last_devices.as_deref() != Some(devices.as_slice()) {
                    log::info!("Audio devices changed; count: {}", devices.len());

                    if let Err(err) = app.emit("audio-devices-updated", &devices) {
                        log::warn!("Failed to emit audio-devices-updated: {}", err);
                    }

                    let app_for_tasks = app.clone();
                    let devices_for_tasks = devices.clone();
                    let skip_tray_rebuild = last_devices.is_none();

                    tauri::async_runtime::spawn(async move {
                        // Refresh tray on real device changes only (the startup baseline
                        // is already reflected in the tray built during setup).
                        if !skip_tray_rebuild {
                            if let Err(err) = update_tray_menu(app_for_tasks.clone()).await {
                                log::warn!(
                                    "Failed to update tray menu after device change: {}",
                                    err
                                );
                            }
                        }

                        match get_settings(app_for_tasks.clone()).await {
                            Ok(settings) => {
                                if let Some(current) = settings.selected_microphone {
                                    if !devices_for_tasks.contains(&current) {
                                        let state = get_recording_state(&app_for_tasks);
                                        let is_recording = matches!(
                                            state,
                                            RecordingState::Starting
                                                | RecordingState::Recording
                                                | RecordingState::Stopping
                                                | RecordingState::Transcribing
                                        );

                                        if is_recording {
                                            log::info!("Selected microphone removed during recording; deferring auto-fallback");
                                        } else {
                                            log::info!("Selected microphone removed; falling back to default");

                                            if let Err(err) =
                                                set_audio_device(app_for_tasks.clone(), None).await
                                            {
                                                log::warn!(
                                                    "Failed to reset audio device after removal: {}",
                                                    err
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                log::warn!("Failed to read settings after device change: {}", err);
                            }
                        }
                    });

                    last_devices = Some(devices);
                }

                thread::sleep(Duration::from_millis(1500));
            }
        });

        if let Ok(mut guard) = self.handle.lock() {
            *guard = Some(handle);
        }
    }
}

impl Drop for DeviceWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);

        if let Ok(mut guard) = self.handle.lock() {
            if let Some(handle) = guard.take() {
                if let Err(err) = handle.join() {
                    log::debug!("DeviceWatcher thread join failed: {:?}", err);
                }
            }
        }
    }
}
