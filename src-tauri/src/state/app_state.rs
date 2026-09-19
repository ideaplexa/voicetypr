use crate::writing::ContextHint;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{Emitter, Manager};

use crate::state::unified_state::UnifiedRecordingState;
use crate::window_manager::WindowManager;

/// Recording state enum matching frontend
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, Default)]
pub enum RecordingState {
    #[default]
    Idle,
    Starting,
    Recording,
    Stopping,
    Transcribing,
    Error,
}

/// Recording mode enum to distinguish between toggle and push-to-talk
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingMode {
    Toggle,
    PushToTalk,
}

/// Queued event for the pill window
#[derive(Debug, Clone)]
pub struct QueuedPillEvent {
    pub event: String,
    pub payload: serde_json::Value,
}

/// Application state - managed by Tauri (runtime state only)
pub struct AppState {
    pub recording_state: UnifiedRecordingState,
    pub current_recording_path: Arc<Mutex<Option<PathBuf>>>,
    pub transcription_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    pub recording_mode: Arc<Mutex<RecordingMode>>,
    pub ptt_key_held: Arc<AtomicBool>,
    pub toggle_key_held: Arc<AtomicBool>,
    pub active_custom_hold_bindings: Arc<Mutex<HashSet<String>>>,
    pub active_custom_pressed_bindings: Arc<Mutex<HashSet<String>>>,
    pub trigger_engine: Arc<keytrigger::TriggerEngine>,
    pub engine_bindings: Arc<Mutex<Vec<crate::trigger::EngineBinding>>>,
    pub should_cancel_recording: Arc<AtomicBool>,
    pub stop_in_flight: Arc<AtomicBool>,
    pub pending_stop_after_start: Arc<AtomicBool>,
    pub esc_pressed_once: Arc<AtomicBool>,
    pub esc_timeout_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    pub window_manager: Arc<Mutex<Option<WindowManager>>>,
    pub recording_config_cache:
        Arc<tokio::sync::RwLock<Option<crate::commands::audio::RecordingConfig>>>,
    pub license_cache: Arc<tokio::sync::RwLock<crate::commands::license::RuntimeLicenseCache>>,
    pub license_validation_lock: Arc<tokio::sync::Mutex<()>>,
    pub pill_event_queue: Arc<Mutex<Vec<QueuedPillEvent>>>,
    pub last_toggle_press: Arc<Mutex<Option<Instant>>>,
    pub recording_app_context: Arc<Mutex<Option<ContextHint>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            recording_state: UnifiedRecordingState::new(),
            current_recording_path: Arc::new(Mutex::new(None)),
            transcription_task: Arc::new(Mutex::new(None)),
            recording_mode: Arc::new(Mutex::new(RecordingMode::Toggle)),
            ptt_key_held: Arc::new(AtomicBool::new(false)),
            toggle_key_held: Arc::new(AtomicBool::new(false)),
            active_custom_hold_bindings: Arc::new(Mutex::new(HashSet::new())),
            active_custom_pressed_bindings: Arc::new(Mutex::new(HashSet::new())),
            trigger_engine: Arc::new(keytrigger::TriggerEngine::new()),
            engine_bindings: Arc::new(Mutex::new(Vec::new())),
            should_cancel_recording: Arc::new(AtomicBool::new(false)),
            stop_in_flight: Arc::new(AtomicBool::new(false)),
            pending_stop_after_start: Arc::new(AtomicBool::new(false)),
            esc_pressed_once: Arc::new(AtomicBool::new(false)),
            esc_timeout_handle: Arc::new(Mutex::new(None)),
            window_manager: Arc::new(Mutex::new(None)),
            recording_config_cache: Arc::new(tokio::sync::RwLock::new(None)),
            license_cache: Arc::new(tokio::sync::RwLock::new(Default::default())),
            license_validation_lock: Arc::new(tokio::sync::Mutex::new(())),
            pill_event_queue: Arc::new(Mutex::new(Vec::new())),
            last_toggle_press: Arc::new(Mutex::new(None)),
            recording_app_context: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_window_manager(&self, manager: WindowManager) {
        if let Ok(mut wm_guard) = self.window_manager.lock() {
            *wm_guard = Some(manager);
        } else {
            log::error!("Failed to acquire window manager lock");
        }
    }

    pub fn get_window_manager(&self) -> Option<WindowManager> {
        match self.window_manager.lock() {
            Ok(guard) => guard.clone(),
            Err(e) => {
                log::error!("Failed to acquire window manager lock: {}", e);
                None
            }
        }
    }
    pub fn set_recording_app_context(&self, hint: ContextHint) {
        if let Ok(mut guard) = self.recording_app_context.lock() {
            *guard = Some(hint);
        }
    }
    pub fn take_recording_app_context(&self) -> Option<ContextHint> {
        self.recording_app_context
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    pub fn transition_recording_state(&self, new_state: RecordingState) -> Result<(), String> {
        self.recording_state.transition_to(new_state)
    }

    pub fn get_current_state(&self) -> RecordingState {
        self.recording_state.current()
    }

    pub fn request_cancellation(&self) {
        self.should_cancel_recording.store(true, Ordering::SeqCst);
        log::info!("Recording cancellation requested");
    }

    pub fn clear_cancellation(&self) {
        self.should_cancel_recording.store(false, Ordering::SeqCst);
    }

    pub fn is_cancellation_requested(&self) -> bool {
        self.should_cancel_recording.load(Ordering::SeqCst)
    }

    pub fn emit_to_window(
        &self,
        window: &str,
        event: &str,
        payload: impl serde::Serialize,
    ) -> Result<(), String> {
        if let Some(wm) = self.get_window_manager() {
            let json_payload = serde_json::to_value(payload)
                .map_err(|e| format!("Failed to serialize payload: {}", e))?;

            match window {
                "main" => wm.emit_to_main(event, json_payload),
                "pill" => wm.emit_to_pill(event, json_payload),
                _ => Err(format!("Unknown window: {}", window)),
            }
        } else {
            Err("WindowManager not initialized".to_string())
        }
    }

    pub fn queue_pill_event(&self, event: &str, payload: serde_json::Value) {
        const MAX_EVENTS: usize = 50;
        match self.pill_event_queue.lock() {
            Ok(mut queue) => {
                if queue.len() >= MAX_EVENTS {
                    queue.remove(0);
                }
                queue.push(QueuedPillEvent {
                    event: event.to_string(),
                    payload,
                });
            }
            Err(e) => {
                log::error!("Failed to lock pill_event_queue for enqueue: {}", e);
            }
        }
    }

    pub fn drain_queued_pill_events(&self) -> Vec<QueuedPillEvent> {
        match self.pill_event_queue.lock() {
            Ok(mut queue) => {
                let events = queue.clone();
                queue.clear();
                events
            }
            Err(e) => {
                log::error!("Failed to lock pill_event_queue for drain: {}", e);
                Vec::new()
            }
        }
    }
}

/// Helper function to update recording state and emit event
pub fn update_recording_state(
    app: &tauri::AppHandle,
    new_state: RecordingState,
    error: Option<String>,
) {
    let app_state = app.state::<AppState>();

    let final_state =
        match app_state
            .recording_state
            .transition_with_fallback(new_state, |current| {
                log::debug!(
                    "update_recording_state: {:?} -> {:?}, error: {:?}",
                    current,
                    new_state,
                    error
                );

                let should_force = match (current, new_state) {
                    (RecordingState::Error, RecordingState::Idle) => true,
                    (_, RecordingState::Error) => true,
                    (_, RecordingState::Idle) if error.is_some() => true,
                    _ => false,
                };

                if should_force {
                    log::warn!(
                        "Will force state transition from {:?} to {:?} for recovery",
                        current,
                        new_state
                    );
                    Some(new_state)
                } else {
                    log::error!(
                        "Invalid state transition from {:?} to {:?} - transition blocked",
                        current,
                        new_state
                    );
                    None
                }
            }) {
            Ok(state) => {
                log::debug!("Successfully transitioned to state: {:?}", state);
                state
            }
            Err(e) => {
                log::error!("Failed to transition state: {}", e);
                app_state.get_current_state()
            }
        };

    let payload = serde_json::json!({
        "state": match final_state {
            RecordingState::Idle => "idle",
            RecordingState::Starting => "starting",
            RecordingState::Recording => "recording",
            RecordingState::Stopping => "stopping",
            RecordingState::Transcribing => "transcribing",
            RecordingState::Error => "error",
        },
        "error": error
    });

    let _ = app.emit("recording-state-changed", payload);
}

/// Helper function to get current recording state
pub fn get_recording_state(app: &tauri::AppHandle) -> RecordingState {
    let app_state = app.state::<AppState>();
    app_state.get_current_state()
}

/// Helper function to emit events to specific windows
pub fn emit_to_window(
    app: &tauri::AppHandle,
    window: &str,
    event: &str,
    payload: impl serde::Serialize,
) -> Result<(), String> {
    let app_state = app.state::<AppState>();
    app_state.emit_to_window(window, event, payload)
}

/// Helper function to emit events to all windows
pub fn emit_to_all(
    app: &tauri::AppHandle,
    event: &str,
    payload: impl serde::Serialize + Clone,
) -> Result<(), String> {
    app.emit(event, payload)
        .map_err(|e| format!("Failed to emit to all windows: {}", e))
}

/// Flush any critical events that were queued for the pill window while it was unavailable.
pub async fn flush_pill_event_queue(app: &tauri::AppHandle) {
    let app_state = app.state::<AppState>();
    let events = app_state.drain_queued_pill_events();

    for event in events {
        if let Err(e) = emit_to_window(app, "pill", &event.event, event.payload.clone()) {
            log::warn!(
                "Failed to deliver queued pill event '{}' to pill window: {}",
                event.event,
                e
            );
        }
    }
}
