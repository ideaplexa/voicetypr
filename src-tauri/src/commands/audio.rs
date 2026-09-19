use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ai::error::{user_facing_message, AiProviderError};
use crate::audio::recorder::AudioRecorder;
use crate::audio::silence_detector::SilenceDetectorEvent;
use crate::audio::speech_evidence::{
    classify_speech_evidence, SpeechEvidenceAttempt, SpeechEvidenceOutcome,
};
use crate::commands::settings::{
    get_settings, normalize_final_text_language, normalize_speech_language_for_model,
    normalize_transcription_task, recording_retention_days_from_store, resolve_pill_indicator_mode,
    task_uses_translate_to_english, Settings, TRANSCRIPTION_TASK_TRANSCRIBE,
};
use crate::license::LicenseState;
use crate::media::MediaPauseController;
use crate::parakeet::manager::ParakeetTranscriptionOptions;
use crate::parakeet::messages::{ParakeetResponse, ParakeetSegment};
use crate::parakeet::ParakeetManager;
use crate::provider_capabilities::ProviderEngine;
use crate::remote::client::{
    self, timeout_ms_for_wav_file, RemoteClientError, RemoteServerConnection,
    TranscriptionRequest as RemoteTranscriptionRequest, TranscriptionSource as RemoteTimeoutSource,
};
use crate::remote::settings::RemoteSettings;
use crate::transcription::error::TranscriptionErrorCode;
use crate::transcription::executor::transcribe_with_app;
use crate::transcription::request::{
    AudioFormatHint, CancellationToken, CleanupPolicy, EngineSelection, RequestContext,
    TimeoutPolicy, TranscriptionAudio, TranscriptionRequest,
};
use crate::transcription::{
    TranscriptionJob, TranscriptionResult, TranscriptionSegment, TranscriptionSource,
    TranscriptionWord,
};
use crate::utils::logger::*;
#[cfg(debug_assertions)]
use crate::utils::system_monitor;
use crate::whisper::cache::TranscriberCache;
use crate::whisper::manager::WhisperManager;
use crate::whisper::transcriber::WhisperTranscriptionOutput;
use crate::{
    emit_to_all, emit_to_window, update_recording_state, AppState, RecordingMode, RecordingState,
};
use cpal::traits::{DeviceTrait, HostTrait};
use once_cell::sync::Lazy;
use serde_json;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri_plugin_store::StoreExt;
use uuid::Uuid;

pub(crate) const PTT_START_ABORTED_AFTER_RELEASE: &str =
    "PTT key released before recording could start";

/// Atomic counter for toast IDs to prevent race conditions
static TOAST_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Global media pause controller for pausing/resuming system media during recording
static MEDIA_CONTROLLER: Lazy<MediaPauseController> = Lazy::new(MediaPauseController::new);

/// Restore any output device the media pause controller muted before the
/// application exits (macOS mute layer). Player pause state is untouched.
#[cfg(target_os = "macos")]
pub fn cleanup_media_pause_on_exit() {
    MEDIA_CONTROLLER.cleanup_on_exit();
}

/// Monotonically increasing recording-generation counter. `start_recording`
/// bumps it to open a new generation; a transcription task captures the value
/// at spawn time and rejects its own result when the generation has advanced
/// beneath it (a newer recording started). This is the backbone that prevents
/// a prior generation's cancelled/stale transcription from being delivered
/// under a newer recording, even after `start_recording` clears the global
/// cancellation flag for its own attempt. SeqCst keeps the bump (start),
/// capture (stop/spawn) and check (deliver) linearizable.
static RECORDING_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Open a new recording generation. Called at the top of `start_recording`
/// before `Starting` is published, so every stop/cancel and spawned
/// transcription task within this attempt observes the same generation.
pub(crate) fn begin_recording_generation() -> u64 {
    RECORDING_GENERATION.fetch_add(1, AtomicOrdering::SeqCst) + 1
}

/// The generation of the most recently begun recording.
pub(crate) fn current_recording_generation() -> u64 {
    RECORDING_GENERATION.load(AtomicOrdering::SeqCst)
}

/// True when `captured` belongs to a recording generation that is no longer
/// current — i.e. a newer recording started while this result was in flight.
pub(crate) fn recording_generation_is_stale(captured: u64) -> bool {
    captured != current_recording_generation()
}

/// Audio file owned by the currently in-flight transcription task, keyed by
/// the recording generation that owns it. `cancel_recording` takes the current
/// slot and deletes that file; a task's own cleanup only clears the slot when
/// its captured generation still owns it, so a stale task can never erase a
/// newer recording's tracker.
static IN_FLIGHT_TRANSCRIPTION_AUDIO: Lazy<Mutex<Option<(u64, PathBuf)>>> =
    Lazy::new(|| Mutex::new(None));

/// Record the audio path the in-flight transcription task owns for this
/// generation, so a `cancel_recording` that aborts that task can still delete
/// the file.
pub(crate) fn set_in_flight_transcription_audio(generation: u64, path: PathBuf) {
    if let Ok(mut guard) = IN_FLIGHT_TRANSCRIPTION_AUDIO.lock() {
        *guard = Some((generation, path));
    }
}

/// Remove and return the currently tracked in-flight transcription path. Used
/// by `cancel_recording`, which always cancels the current recording attempt.
pub(crate) fn take_in_flight_transcription_audio() -> Option<PathBuf> {
    IN_FLIGHT_TRANSCRIPTION_AUDIO
        .lock()
        .ok()
        .and_then(|mut guard| guard.take().map(|(_, path)| path))
}

fn clear_in_flight_transcription_audio_for_generation(generation: u64) {
    if let Ok(mut guard) = IN_FLIGHT_TRANSCRIPTION_AUDIO.lock() {
        if guard
            .as_ref()
            .map(|(tracked_generation, _)| *tracked_generation == generation)
            .unwrap_or(false)
        {
            *guard = None;
        }
    }
}

/// Remove the task-owned temp recording and release the in-flight tracker slot
/// only if this task's generation still owns that slot. The file removal is
/// path-specific, but the tracker clear is generation-checked so a stale task
/// cannot clear a newer recording's cancellation handle.
pub(crate) fn finalize_in_flight_audio(generation: u64, audio_path: &Path) {
    if let Err(e) = std::fs::remove_file(audio_path) {
        log::warn!("Failed to remove temporary audio file: {}", e);
    }
    clear_in_flight_transcription_audio_for_generation(generation);
}

/// Single post-transcription side-effect chokepoint. The generation/cancel
/// snapshot and the synchronous irreversible commit happen in one call with no
/// `.await` between them. Any post-transcription audio persistence, text
/// delivery, or history write must enter here at its true write/call site.
pub(crate) fn persist_if_current<R>(
    app_state: &AppState,
    generation: u64,
    commit: impl FnOnce() -> R,
) -> Option<R> {
    if delivery_aborted(app_state.is_cancellation_requested(), generation) {
        None
    } else {
        Some(commit())
    }
}

/// True when delivery of a `captured_generation` result must be aborted: the
/// user cancelled, or a newer recording started beneath this task (its
/// generation advanced). The generation arm is load-bearing because
/// `start_recording` clears the cancellation flag for its own attempt, so the
/// flag alone would let a stale prior-generation result be delivered during a
/// newer recording. Used at every delivery checkpoint so a cancel/stale that
/// arrives AFTER the outer task's pre-delivery gate is still caught.
pub(crate) fn delivery_aborted(cancelled: bool, captured_generation: u64) -> bool {
    cancelled || recording_generation_is_stale(captured_generation)
}

/// Delete a recording file previously persisted into `recordings_dir`. Used to
/// REVOKE a save that a cancel/staleness arriving during (or just after) the
/// synchronous copy turned into a privacy leak: the pre-copy snapshot let the
/// copy through, but the dictation is now cancelled/stale and must not persist.
/// A NotFound result means the file was never saved (or already revoked).
pub(crate) fn delete_persisted_recording(recordings_dir: &Path, filename: &str) {
    let target = recordings_dir.join(filename);
    match std::fs::remove_file(&target) {
        Ok(()) => log::info!(
            "Revoked saved recording after late cancel/stale: {}",
            filename
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("Failed to revoke saved recording '{}': {}", filename, e),
    }
}

/// Resolve the app's recordings directory and delete a previously-saved
/// recording there. Thin AppHandle-backed wrapper over
/// `delete_persisted_recording` for the spawn-internal recheck sites, where
/// only the saved filename (not the dir) is in scope.
async fn revoke_saved_recording(app: &AppHandle, filename: &str) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    delete_persisted_recording(&dir.join("recordings"), filename);
}
struct StopInFlightGuard(Arc<AtomicBool>);

impl StopInFlightGuard {
    fn try_acquire(flag: Arc<AtomicBool>) -> Option<Self> {
        flag.compare_exchange(false, true, AtomicOrdering::SeqCst, AtomicOrdering::SeqCst)
            .ok()
            .map(|_| Self(flag))
    }
}

impl Drop for StopInFlightGuard {
    fn drop(&mut self) {
        self.0.store(false, AtomicOrdering::SeqCst);
    }
}
/// Decode journey (PostHog) + terminal outcome. The GlitchTip log-funnel
/// transaction/span plumbing was removed (plan 060 pivot: logs never alerted);
/// failure events now go through `telemetry::capture_transcription_failure`.
/// Cancellation (None) is the default so aborting the Tokio task during an
/// await still emits a terminal journey event.
struct DecodeJourneyGuard {
    started: Instant,
    /// None = cancelled/aborted before a terminal outcome was recorded.
    succeeded: Option<bool>,
    analytics_engine: crate::product_analytics::EngineKind,
}

impl DecodeJourneyGuard {
    fn new(analytics_engine: crate::product_analytics::EngineKind) -> Self {
        Self {
            started: Instant::now(),
            succeeded: None,
            analytics_engine,
        }
    }

    fn set_outcome(&mut self, succeeded: bool) {
        self.succeeded = Some(succeeded);
    }
}

impl Drop for DecodeJourneyGuard {
    fn drop(&mut self) {
        let duration_ms = self.started.elapsed().as_millis() as u64;
        let outcome = match self.succeeded {
            Some(true) => crate::product_analytics::JourneyOutcome::Succeeded,
            Some(false) => crate::product_analytics::JourneyOutcome::Failed,
            None => crate::product_analytics::JourneyOutcome::Cancelled,
        };
        crate::product_analytics::capture(crate::product_analytics::ProductEvent::StageFinished {
            stage: crate::product_analytics::JourneyStage::Decode,
            outcome,
            duration_ms,
            engine: Some(self.analytics_engine),
        });
    }
}

/// Delivery journey (PostHog). A task abandoned before delivery is cancelled;
/// only an attempted paste or clipboard operation can succeed or fail.
struct DeliveryJourneyGuard {
    started: Instant,
    succeeded: Option<bool>,
}

impl DeliveryJourneyGuard {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            succeeded: None,
        }
    }

    fn mark_succeeded(&mut self) {
        self.succeeded = Some(true);
    }

    fn mark_failed(&mut self) {
        self.succeeded = Some(false);
    }

    fn outcome(&self) -> crate::product_analytics::JourneyOutcome {
        match self.succeeded {
            Some(true) => crate::product_analytics::JourneyOutcome::Succeeded,
            Some(false) => crate::product_analytics::JourneyOutcome::Failed,
            None => crate::product_analytics::JourneyOutcome::Cancelled,
        }
    }
}

impl Drop for DeliveryJourneyGuard {
    fn drop(&mut self) {
        let duration_ms = self.started.elapsed().as_millis() as u64;
        crate::product_analytics::capture(crate::product_analytics::ProductEvent::StageFinished {
            stage: crate::product_analytics::JourneyStage::Delivery,
            outcome: self.outcome(),
            duration_ms,
            engine: None,
        });
    }
}

#[cfg(test)]
mod delivery_journey_tests {
    use super::DeliveryJourneyGuard;
    use crate::product_analytics::JourneyOutcome;

    #[test]
    fn cancelled_or_stale_task_before_delivery_is_not_a_failure() {
        assert_eq!(
            DeliveryJourneyGuard::new().outcome(),
            JourneyOutcome::Cancelled
        );
    }

    #[test]
    fn attempted_delivery_records_its_result_even_if_history_is_cancelled() {
        let mut delivered = DeliveryJourneyGuard::new();
        delivered.mark_succeeded();
        assert_eq!(delivered.outcome(), JourneyOutcome::Succeeded);
        let mut failed = DeliveryJourneyGuard::new();
        failed.mark_failed();
        assert_eq!(failed.outcome(), JourneyOutcome::Failed);
    }
}

/// If `stop_recording` finds no active recorder, only force Idle when the
/// caller entered from a state that is not already owned by stop/transcribe.
fn stop_should_reset_to_idle(current: RecordingState) -> bool {
    !matches!(
        current,
        RecordingState::Stopping | RecordingState::Transcribing
    )
}
/// Whether a transcription task is currently running (spawned and not yet
/// finished). Used to distinguish a genuinely stuck `Stopping` state (no work
/// will ever advance it) from a `Stopping` state that is merely waiting for a
/// just-spawned transcription task to flip to `Transcribing`.
fn transcription_task_in_flight(app_state: &AppState) -> bool {
    app_state
        .transcription_task
        .lock()
        .map(|guard| guard.as_ref().map(|h| !h.is_finished()).unwrap_or(false))
        .unwrap_or(false)
}

fn take_and_remove_current_recording_path(app_state: &AppState, reason: &str) {
    let audio_path = match app_state.current_recording_path.lock() {
        Ok(mut path_guard) => path_guard.take(),
        Err(e) => {
            log::warn!("Failed to acquire recording path lock for cleanup: {}", e);
            None
        }
    };

    if let Some(audio_path) = audio_path {
        log::info!("Removing {} recording file", reason);
        if let Err(e) = std::fs::remove_file(&audio_path) {
            log::warn!("Failed to remove {} recording: {}", reason, e);
        }
    }
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PillToastAction {
    Show,
    Clear,
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PillToastVariant {
    Info,
    Warning,
}

/// Payload emitted to the frontend. Optional fields preserve the legacy
/// severity-inference path: ordinary `pill_toast` calls emit no explicit variant.
#[derive(serde::Serialize, Clone)]
pub(crate) struct PillToastEventPayload {
    pub id: u64,
    pub message: String,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<PillToastAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<PillToastVariant>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub persistent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

fn next_toast_id() -> u64 {
    TOAST_ID_COUNTER
        .fetch_add(1, AtomicOrdering::SeqCst)
        .wrapping_add(1)
}

fn toast_clear_is_current(counter: &AtomicU64, toast_id: u64) -> bool {
    counter
        .compare_exchange(
            toast_id,
            toast_id.wrapping_add(1),
            AtomicOrdering::SeqCst,
            AtomicOrdering::SeqCst,
        )
        .is_ok()
}

fn emit_pill_toast(
    app: &AppHandle,
    message: &str,
    duration_ms: u64,
    variant: Option<PillToastVariant>,
    persistent: bool,
    suggestion: Option<&str>,
) -> u64 {
    let id = next_toast_id();

    if let Some(toast_window) = app.get_webview_window("toast") {
        let _ = toast_window.show();

        if !persistent {
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(duration_ms)).await;
                if TOAST_ID_COUNTER.load(AtomicOrdering::SeqCst) == id {
                    if let Some(tw) = app_clone.get_webview_window("toast") {
                        let _ = tw.hide();
                    }
                }
            });
        }
    } else {
        // The toast window may not be registered yet during early startup. Retry the
        // show once after a short delay so the message is not silently dropped; the
        // `toast` event is emitted below regardless, so a late-mounting frontend still
        // renders it.
        log::warn!(
            "pill_toast: toast window not found, retrying show shortly: {}",
            message
        );
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if let Some(toast_window) = app_clone.get_webview_window("toast") {
                let _ = toast_window.show();
                if !persistent {
                    tokio::time::sleep(std::time::Duration::from_millis(duration_ms)).await;
                    if TOAST_ID_COUNTER.load(AtomicOrdering::SeqCst) == id {
                        if let Some(tw) = app_clone.get_webview_window("toast") {
                            let _ = tw.hide();
                        }
                    }
                }
            } else {
                log::warn!("pill_toast: toast window still missing after retry");
            }
        });
    }

    let payload = PillToastEventPayload {
        id,
        message: message.to_string(),
        duration_ms,
        action: if persistent {
            Some(PillToastAction::Show)
        } else {
            None
        },
        variant,
        persistent,
        suggestion: suggestion.map(|s| s.to_string()),
    };
    let _ = app.emit("toast", payload);
    id
}

/// Show a toast message on the pill's toast window (above the pill).
/// Existing call sites intentionally emit no variant, preserving frontend
/// severity inference.
pub fn pill_toast(app: &AppHandle, message: &str, duration_ms: u64) -> u64 {
    emit_pill_toast(app, message, duration_ms, None, false, None)
}

pub fn pill_toast_with_variant(
    app: &AppHandle,
    message: &str,
    duration_ms: u64,
    variant: PillToastVariant,
) -> u64 {
    emit_pill_toast(app, message, duration_ms, Some(variant), false, None)
}

pub fn pill_toast_persistent(app: &AppHandle, message: &str, variant: PillToastVariant) -> u64 {
    emit_pill_toast(app, message, 0, Some(variant), true, None)
}

/// Show a toast with a remediation suggestion rendered below the message.
pub fn pill_toast_with_suggestion(
    app: &AppHandle,
    message: &str,
    suggestion: &str,
    duration_ms: u64,
    variant: Option<PillToastVariant>,
) -> u64 {
    emit_pill_toast(app, message, duration_ms, variant, false, Some(suggestion))
}

pub fn clear_pill_toast(app: &AppHandle, toast_id: u64) {
    if !toast_clear_is_current(&TOAST_ID_COUNTER, toast_id) {
        return;
    }

    if let Some(toast_window) = app.get_webview_window("toast") {
        let _ = toast_window.hide();
    }
    let payload = PillToastEventPayload {
        id: toast_id,
        message: String::new(),
        duration_ms: 0,
        action: Some(PillToastAction::Clear),
        variant: None,
        persistent: false,
        suggestion: None,
    };

    let _ = app.emit("toast", payload);
}

fn should_hide_pill_when_idle(mode: &str) -> bool {
    mode != "always"
}

/// Check if pill should be hidden based on pill_indicator_mode setting.
/// Returns true if pill should be hidden, false if it should stay visible.
/// Called when transitioning to idle state (after recording ends).
/// - "never" → always hide (return true)
/// - "always" → never hide (return false)
/// - "when_recording" → hide when idle (return true)
///   Fails open: on error, returns true (default to when_recording behavior).
pub async fn should_hide_pill(app: &AppHandle) -> bool {
    let store = match app.store("settings") {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Failed to load settings for pill visibility: {}", e);
            return true; // Default to when_recording behavior (hide when idle)
        }
    };

    let stored_mode = store
        .get("pill_indicator_mode")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let legacy_show = store.get("show_pill_indicator").and_then(|v| v.as_bool());
    let pill_indicator_mode = resolve_pill_indicator_mode(
        stored_mode.clone(),
        legacy_show,
        Settings::default().pill_indicator_mode,
    );
    let caller = std::panic::Location::caller();
    log::debug!(
        "pill_visibility: should_hide_pill caller={} stored={:?} legacy_show={:?} resolved='{}'",
        caller,
        stored_mode,
        legacy_show,
        pill_indicator_mode
    );

    let result = should_hide_pill_when_idle(&pill_indicator_mode);
    log::debug!(
        "should_hide_pill: pill_indicator_mode='{}', should_hide={}",
        pill_indicator_mode,
        result
    );

    result
}

struct NormalizedTempFile {
    path: PathBuf,
}

impl NormalizedTempFile {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for NormalizedTempFile {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    "Failed to remove normalized temp file {:?}: {}",
                    self.path,
                    error
                );
            }
        }
    }
}

const RETRANSCRIPTION_SESSION_MARKER_FIELD: &str = "retranscription_session_marker";

const RETRANSCRIPTION_FAILURE_DETAIL_FIELD: &str = "failure_detail";

const STALE_RETRANSCRIPTION_FAILURE_TEXT: &str = "Retranscription interrupted before completion";

static RETRANSCRIPTION_SESSION_MARKER: Lazy<Uuid> = Lazy::new(Uuid::new_v4);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionStatus {
    InProgress,
    Completed,
    Failed,
}

impl TranscriptionStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

fn current_retranscription_session_marker() -> String {
    RETRANSCRIPTION_SESSION_MARKER.to_string()
}

fn transcription_status_value(status: TranscriptionStatus) -> serde_json::Value {
    serde_json::Value::String(status.as_str().to_string())
}

fn normalize_transcription_status(status: Option<TranscriptionStatus>) -> TranscriptionStatus {
    status.unwrap_or(TranscriptionStatus::Completed)
}

fn parse_transcription_status(value: Option<&serde_json::Value>) -> Option<TranscriptionStatus> {
    value
        .and_then(serde_json::Value::as_str)
        .and_then(TranscriptionStatus::from_str)
}

fn should_replace_placeholder_text(text: Option<&str>) -> bool {
    text.map(|text| text.is_empty() || text == "In progress...")
        .unwrap_or(true)
}

fn apply_retranscription_status(
    map: &mut serde_json::Map<String, serde_json::Value>,
    status: Option<TranscriptionStatus>,
) -> TranscriptionStatus {
    let effective_status = normalize_transcription_status(status);
    map.insert(
        "status".to_string(),
        transcription_status_value(effective_status),
    );
    map.insert(
        "is_retranscription".to_string(),
        serde_json::Value::Bool(true),
    );

    match effective_status {
        TranscriptionStatus::InProgress => {
            map.insert(
                RETRANSCRIPTION_SESSION_MARKER_FIELD.to_string(),
                serde_json::Value::String(current_retranscription_session_marker()),
            );
        }
        TranscriptionStatus::Completed | TranscriptionStatus::Failed => {
            map.remove(RETRANSCRIPTION_SESSION_MARKER_FIELD);
        }
    }

    effective_status
}

fn sync_retranscription_failure_metadata(
    map: &mut serde_json::Map<String, serde_json::Value>,
    status: TranscriptionStatus,
    text: &str,
) {
    match status {
        TranscriptionStatus::Completed | TranscriptionStatus::InProgress => {
            map.remove("error_kind");
            map.remove("error_detail");
            map.remove("error_body");
            map.remove("can_retry_from_history");
        }
        TranscriptionStatus::Failed => {
            map.remove("error_kind");
            map.remove("error_body");
            map.insert(
                "error_detail".to_string(),
                serde_json::Value::String(text.to_string()),
            );
            if map.contains_key("recording_file") {
                map.insert(
                    "can_retry_from_history".to_string(),
                    serde_json::Value::Bool(true),
                );
            } else {
                map.remove("can_retry_from_history");
            }
        }
    }
}

pub(crate) fn reconcile_transcription_history_entry(
    entry: serde_json::Value,
    current_session_marker: &str,
) -> serde_json::Value {
    let Some(original) = entry.as_object() else {
        return entry;
    };

    let status = match original.get("status") {
        Some(status_value) => parse_transcription_status(Some(status_value)),
        None => None,
    };

    match status {
        None => {
            if original.contains_key("status") {
                return entry;
            }

            let mut reconciled = entry.clone();
            if let Some(map) = reconciled.as_object_mut() {
                map.insert(
                    "status".to_string(),
                    transcription_status_value(TranscriptionStatus::Completed),
                );
                map.remove(RETRANSCRIPTION_SESSION_MARKER_FIELD);
            }
            reconciled
        }
        Some(TranscriptionStatus::InProgress) => {
            let is_retranscription = original
                .get("is_retranscription")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                || original
                    .get("source_recording_id")
                    .and_then(serde_json::Value::as_str)
                    .is_some();

            if !is_retranscription {
                return entry;
            }

            let stored_marker = original
                .get(RETRANSCRIPTION_SESSION_MARKER_FIELD)
                .and_then(serde_json::Value::as_str);

            if stored_marker == Some(current_session_marker) {
                return entry;
            }

            let mut reconciled = entry.clone();
            if let Some(map) = reconciled.as_object_mut() {
                if should_replace_placeholder_text(
                    map.get("text").and_then(serde_json::Value::as_str),
                ) {
                    map.insert(
                        "text".to_string(),
                        serde_json::Value::String(STALE_RETRANSCRIPTION_FAILURE_TEXT.to_string()),
                    );
                }
                map.insert(
                    "status".to_string(),
                    transcription_status_value(TranscriptionStatus::Failed),
                );
                map.remove(RETRANSCRIPTION_SESSION_MARKER_FIELD);
                map.insert(
                    RETRANSCRIPTION_FAILURE_DETAIL_FIELD.to_string(),
                    serde_json::json!({
                        "kind": "stale_retranscription_session",
                        "current_session_marker": current_session_marker,
                        "stale_session_marker": stored_marker,
                    }),
                );
            }
            reconciled
        }
        Some(TranscriptionStatus::Completed) | Some(TranscriptionStatus::Failed) => entry,
    }
}

fn parse_history_key(key: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(key)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&chrono::Utc))
}

pub(crate) fn page_history_keys(mut keys: Vec<String>, limit: usize) -> Vec<String> {
    keys.sort_unstable_by(|a, b| match (parse_history_key(a), parse_history_key(b)) {
        (Some(a_timestamp), Some(b_timestamp)) => {
            b_timestamp.cmp(&a_timestamp).then_with(|| b.cmp(a))
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.cmp(a),
    });
    keys.truncate(limit);
    keys
}

pub(crate) fn is_duplicate_transcription(
    latest_key: &str,
    latest: &serde_json::Value,
    text: &str,
    model: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    let same_text = latest
        .get("text")
        .and_then(|x| x.as_str())
        .map(|s| s == text)
        .unwrap_or(false);
    let same_model = latest
        .get("model")
        .and_then(|x| x.as_str())
        .map(|s| s == model)
        .unwrap_or(false);
    let within_window = chrono::DateTime::parse_from_rfc3339(latest_key)
        .ok()
        .and_then(|t| {
            t.with_timezone(&chrono::Utc)
                .signed_duration_since(now)
                .num_seconds()
                .checked_abs()
        })
        .map(|secs| secs <= 2)
        .unwrap_or(false);

    same_text && same_model && within_window
}
/// Closed-vocabulary engine label for failure-event tags (plan 060).
fn engine_kind_label(selection: &ActiveEngineSelection) -> &'static str {
    match selection {
        ActiveEngineSelection::Whisper { .. } => "whisper",
        ActiveEngineSelection::Parakeet { .. } => "parakeet",
        ActiveEngineSelection::Cloud { provider, .. } => provider.id(),
        ActiveEngineSelection::Remote { .. } => "remote",
    }
}

/// Model name for failure-event tags; empty for engines without one
/// (remote servers carry user-chosen names — never sent).
fn engine_model_label(selection: &ActiveEngineSelection) -> String {
    match selection {
        ActiveEngineSelection::Whisper { model_name, .. }
        | ActiveEngineSelection::Parakeet { model_name, .. }
        | ActiveEngineSelection::Cloud { model_name, .. } => model_name.clone(),
        ActiveEngineSelection::Remote { .. } => String::new(),
    }
}

/// Closed failure-class vocabulary driving `flow.transcription.failed.*`
/// event names. Executor failures retain their typed code; legacy desktop
/// failures still use the string marker fallback.
fn transcription_failure_class(failure: &TranscriptionFailure) -> String {
    let class = match failure {
        TranscriptionFailure::Local {
            code: Some(code), ..
        } => match code {
            TranscriptionErrorCode::Timeout => "timeout",
            TranscriptionErrorCode::StorageLimitExceeded => "cloud_storage_limit",
            TranscriptionErrorCode::TransportFailed => "transport",
            TranscriptionErrorCode::Unauthorized => "auth",
            TranscriptionErrorCode::ModelUnavailable
            | TranscriptionErrorCode::EngineUnavailable => "model_unavailable",
            _ => "engine_failed",
        },
        TranscriptionFailure::Local {
            message,
            code: None,
        } => {
            let lower = message.to_ascii_lowercase();
            if lower.contains("timed out") {
                "timeout"
            } else if lower.contains("storage limit") {
                "cloud_storage_limit"
            } else if lower.contains("could not reach")
                || lower.contains("rate limit")
                || lower.contains("network")
            {
                "transport"
            } else if lower.contains("invalid api key") || lower.contains("authentication") {
                "auth"
            } else if lower.contains("model") {
                "model_unavailable"
            } else {
                "engine_failed"
            }
        }
        TranscriptionFailure::Remote(err) => match err {
            RemoteClientError::AuthFailed { .. } => "remote_auth",
            RemoteClientError::Timeout { .. } => "remote_timeout",
            RemoteClientError::ConnectFailed { .. } => "remote_connect",
            RemoteClientError::HttpStatus { .. } => "remote_http",
            RemoteClientError::ResponseDecode { .. } | RemoteClientError::ResponseSchema { .. } => {
                "remote_response"
            }
            RemoteClientError::RequestBuild { .. } | RemoteClientError::JoinFailed { .. } => {
                "remote_internal"
            }
        },
    };
    class.to_string()
}

#[derive(Debug, Clone)]
pub(crate) enum TranscriptionFailure {
    Local {
        message: String,
        code: Option<TranscriptionErrorCode>,
    },
    Remote(RemoteClientError),
}

impl TranscriptionFailure {
    fn local(message: String) -> Self {
        Self::Local {
            message,
            code: None,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::Local { message, .. } => message.clone(),
            Self::Remote(error) => error.to_string(),
        }
    }

    fn error_kind(&self) -> &'static str {
        match self {
            Self::Local { .. } => "local",
            Self::Remote(error) => remote_client_error_kind(error),
        }
    }

    fn server_error_body(&self) -> Option<&str> {
        match self {
            Self::Local { .. } => None,
            Self::Remote(error) => error.server_error_body(),
        }
    }

    /// Whether a failed attempt's recording should be preserved for retry: genuine
    /// engine/network failures, not user cancellation or a too-short clip.
    fn is_retryable_failure(&self) -> bool {
        match self {
            Self::Remote(_) => true,
            Self::Local { message, .. } => {
                !message.contains("cancelled")
                    && !message.contains("Cancelled")
                    && !message.contains("too short")
            }
        }
    }
}

fn remote_client_error_kind(error: &RemoteClientError) -> &'static str {
    match error {
        RemoteClientError::AuthFailed { .. } => "remote_auth_failed",
        RemoteClientError::Timeout { .. } => "remote_timeout",
        RemoteClientError::ConnectFailed { .. } => "remote_connect_failed",
        RemoteClientError::HttpStatus { .. } => "remote_http_status",
        RemoteClientError::ResponseDecode { .. } => "remote_response_decode",
        RemoteClientError::ResponseSchema { .. } => "remote_response_schema",
        RemoteClientError::RequestBuild { .. } => "remote_request_build",
        RemoteClientError::JoinFailed { .. } => "remote_join_failed",
    }
}

fn remote_server_error_pill_message(can_retry_from_history: bool) -> &'static str {
    if can_retry_from_history {
        "Remote transcription failed. Go to History to re-transcribe, or select a different model."
    } else {
        "Remote transcription failed. Check the remote server and try again."
    }
}

/// Classification of a `TranscriptionFailure::Local` message for pill-toast
/// dispatch.  Auth and model failures are not fixed by retrying; everything else
/// is a transient fault where "try again" is appropriate.
#[derive(Debug, PartialEq)]
enum LocalFailureKind {
    /// Cloud provider rejected the API key (401).
    AuthInvalid,
    /// Selected model or engine was unavailable at runtime.
    ModelUnavailable,
    /// Transient or unclassified fault — retrying may help.
    Generic,
}

/// Classify a `TranscriptionFailure::Local` message so the pill-toast can give
/// actionable guidance instead of a generic "try again" for auth/model faults.
/// Matches are anchored to the `user_message_for_code` strings in
/// `transcription::error`, which are the deterministic prefixes present in the
/// failure string whether or not a raw detail was appended.
fn classify_local_failure(e: &str) -> LocalFailureKind {
    if e.starts_with("Authentication failed for the transcription service") {
        LocalFailureKind::AuthInvalid
    } else if e.starts_with("The selected transcription model is unavailable")
        || e.starts_with("The selected transcription engine is unavailable")
    {
        LocalFailureKind::ModelUnavailable
    } else {
        LocalFailureKind::Generic
    }
}

fn build_remote_server_error_payload(
    failure: &TranscriptionFailure,
    can_retry_from_history: bool,
) -> serde_json::Value {
    serde_json::json!({
        "title": "Remote Transcription Failed",
        "message": failure.message(),
        "error_kind": failure.error_kind(),
        "can_retry_from_history": can_retry_from_history,
    })
}

fn build_failed_transcription_row(
    failure: &TranscriptionFailure,
    model: &str,
    recording_file: &str,
) -> serde_json::Value {
    serde_json::json!({
        "text": "Transcription failed - re-transcribe after resolving the issue",
        "model": model,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "recording_file": recording_file,
        "status": "failed",
        "error_kind": failure.error_kind(),
        "error_detail": failure.message(),
        "error_body": failure.server_error_body(),
        "can_retry_from_history": true,
    })
}

fn build_transcription_job(
    source: TranscriptionSource,
    engine: impl Into<String>,
    model: impl Into<String>,
    spoken_language: Option<String>,
    translate_to_english: bool,
) -> TranscriptionJob {
    TranscriptionJob::from_legacy_settings(
        source,
        engine,
        model,
        spoken_language,
        translate_to_english,
    )
}

fn seconds_to_duration_ms(duration_seconds: Option<f32>) -> Option<u64> {
    duration_seconds.map(|seconds| (seconds.max(0.0) * 1000.0) as u64)
}

pub(crate) fn transcription_watchdog_budget(audio_duration_ms: Option<u64>) -> std::time::Duration {
    const MIN_SECONDS: u64 = 180;
    const MAX_SECONDS: u64 = 30 * 60;

    let budget_seconds = audio_duration_ms
        .map(|duration_ms| duration_ms.saturating_add(999) / 1000)
        .map(|duration_seconds| duration_seconds.saturating_mul(4).saturating_add(60))
        .unwrap_or(MIN_SECONDS)
        .clamp(MIN_SECONDS, MAX_SECONDS);

    std::time::Duration::from_secs(budget_seconds)
}

/// Build a [`TranscriptionRequest`] for the desktop record→insert hot path from an
/// already-resolved [`ActiveEngineSelection`]. The desktop owns recording history
/// and cleanup, so it passes `CleanupPolicy::CallerOwns`; the executor enforces the
/// interactive timeout/watchdog, Whisper retry, and (idempotent) normalization.
fn build_desktop_transcription_request(
    app: &AppHandle,
    active: &ActiveEngineSelection,
    job: &TranscriptionJob,
    spoken_language: Option<String>,
    audio_path: PathBuf,
) -> Result<TranscriptionRequest, TranscriptionFailure> {
    let engine = ProviderEngine::from_engine_str(active.engine_name()).ok_or_else(|| {
        TranscriptionFailure::local(format!(
            "Unknown transcription engine: {}",
            active.engine_name()
        ))
    })?;
    let initial_prompt = if matches!(active, ActiveEngineSelection::Whisper { .. }) {
        compile_whisper_initial_prompt(app, spoken_language.as_deref())
    } else {
        None
    };
    let cancellation =
        CancellationToken::from_arc(app.state::<AppState>().should_cancel_recording.clone());

    Ok(TranscriptionRequest {
        source: TranscriptionSource::DesktopRecording,
        audio: TranscriptionAudio::Path {
            path: audio_path,
            format_hint: Some(AudioFormatHint::Wav),
            cleanup: CleanupPolicy::CallerOwns,
        },
        engine: EngineSelection::Explicit {
            engine,
            model: active.model_name().to_string(),
        },
        spoken_language,
        task: job.task,
        context: RequestContext::default(),
        timeout: TimeoutPolicy::Interactive,
        cancellation,
        initial_prompt,
    })
}

/// Map the executor's typed [`TranscriptionError`] back onto the desktop's
/// `TranscriptionFailure`, preserving the existing failure dispatch (cancel /
/// timeout / generic). Translation failures never reach here — they are a
/// writing-stage outcome, not a transcription failure.
fn desktop_failure_from_transcription_error(
    error: crate::transcription::error::TranscriptionError,
) -> TranscriptionFailure {
    let message = match error.code {
        TranscriptionErrorCode::Cancelled => "Transcription cancelled".to_string(),
        TranscriptionErrorCode::Timeout => "Transcription timed out".to_string(),
        _ => match error.detail {
            Some(detail) if !detail.is_empty() => format!("{}: {}", error.user_message, detail),
            _ => error.user_message,
        },
    };
    TranscriptionFailure::Local {
        message,
        code: Some(error.code),
    }
}

fn is_non_speech_transcript(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "" | "[blank_audio]"
            | "[sound]"
            | "[music]"
            | "[noise]"
            | "[inaudible]"
            | "(silence)"
            | "(music)"
            | "(noise)"
    )
}

pub(crate) fn parakeet_segments_to_transcription_segments(
    segments: Vec<ParakeetSegment>,
) -> Vec<TranscriptionSegment> {
    segments
        .into_iter()
        .map(|segment| TranscriptionSegment {
            text: segment.text,
            start_ms: seconds_to_duration_ms(segment.start),
            end_ms: seconds_to_duration_ms(segment.end),
            speaker_id: None,
        })
        .collect()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UploadDiarizationSegment {
    pub speaker_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UploadTranscription {
    pub text: String,
    pub words: Option<Vec<TranscriptionWord>>,
    pub metadata: Option<serde_json::Value>,
}

/// Group diarized words into speaker-attributed paragraphs.
///
/// Words with the same `speaker_id` are joined into a single paragraph prefixed
/// with `"Speaker N: "`. Words without a `speaker_id` continue the current run.
/// Paragraphs are separated by `"\n\n"`.
///
/// Token spacing is handled by [`join_tokens`]: Deepgram bare words get a space
/// inserted between them; Soniox tokens that already carry leading whitespace or
/// start with punctuation are appended as-is.
pub(crate) fn group_words_into_speaker_text(words: &[TranscriptionWord]) -> String {
    if words.is_empty() {
        return String::new();
    }

    let mut paragraphs: Vec<(Option<String>, Vec<String>)> = Vec::new();
    let mut current_speaker: Option<String> = None;
    let mut current_words: Vec<String> = Vec::new();

    for word in words {
        match &word.speaker_id {
            Some(spk) => {
                if Some(spk) != current_speaker.as_ref() && !current_words.is_empty() {
                    paragraphs.push((current_speaker.clone(), std::mem::take(&mut current_words)));
                    current_speaker = Some(spk.clone());
                } else if current_words.is_empty() {
                    current_speaker = Some(spk.clone());
                }
            }
            None => {
                // No speaker tag — treat as continuation of the current run.
            }
        }
        current_words.push(word.text.clone());
    }
    if !current_words.is_empty() {
        paragraphs.push((current_speaker, current_words));
    }

    paragraphs
        .into_iter()
        .map(|(speaker, tokens)| {
            let prefix = match speaker {
                Some(s) => format!("{s}: "),
                None => String::new(),
            };
            format!("{}{}", prefix, join_tokens(&tokens))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Join transcript tokens with spacing awareness.
///
/// - First token: leading whitespace stripped (handles Soniox leading spaces).
/// - Subsequent tokens: appended as-is if the token starts with whitespace or
///   a punctuation character; otherwise a single space is prepended.
///
/// This keeps Deepgram bare words (`"Hello"`, `"world"`) space-joined while
/// rendering Soniox pre-spaced tokens (`"How"`, `" are"`, `" you"`, `"?"`)
/// correctly without double spaces or stray spaces before punctuation.
fn join_tokens(tokens: &[String]) -> String {
    const PUNCT: &[char] = &[
        '.', ',', '!', '?', ';', ':', ')', ']', '}', '\'', '"', '\u{2026}',
    ];
    let mut out = String::new();
    for (i, token) in tokens.iter().enumerate() {
        if i == 0 {
            out.push_str(token.trim_start());
        } else if token.starts_with(|c: char| c.is_whitespace()) || token.starts_with(PUNCT) {
            out.push_str(token);
        } else {
            out.push(' ');
            out.push_str(token);
        }
    }
    out
}

fn build_remote_transcription_result(
    job: &TranscriptionJob,
    response: crate::remote::server::TranscribeResponse,
) -> TranscriptionResult {
    let mut result = TranscriptionResult::new(job, response.text)
        .with_processing_duration_ms(Some(response.duration_ms));
    result.model = response.model;
    result.transcript_language = response.transcript_language;
    result
}

fn build_writing_history_metadata(
    transcription: &TranscriptionResult,
    writing: Option<&crate::writing::WritingResult>,
) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "source".into(),
        serde_json::to_value(transcription.source).unwrap_or(serde_json::Value::Null),
    );
    map.insert("engine".into(), transcription.engine.clone().into());
    if let Some(v) = transcription.timings.audio_duration_ms {
        map.insert("audio_duration_ms".into(), v.into());
    }
    if let Some(v) = transcription.timings.processing_duration_ms {
        map.insert("processing_duration_ms".into(), v.into());
    }
    map.insert("diarized".into(), transcription.words.is_some().into());
    if let Some(wr) = writing {
        map.insert(
            "mode".into(),
            serde_json::to_value(wr.mode).unwrap_or(serde_json::Value::Null),
        );
        map.insert("output_language".into(), wr.output_language.clone().into());
        map.insert(
            "transcript_language".into(),
            serde_json::json!(transcription.transcript_language),
        );
        map.insert(
            "spoken_language".into(),
            serde_json::json!(transcription.spoken_language),
        );
        map.insert("ai_applied".into(), wr.ai_applied.into());
        map.insert(
            "applied_operations".into(),
            serde_json::to_value(&wr.applied_operations)
                .unwrap_or(serde_json::Value::Array(vec![])),
        );
        map.insert(
            "warnings".into(),
            serde_json::to_value(&wr.warnings).unwrap_or(serde_json::Value::Array(vec![])),
        );
        map.insert(
            "context_hint".into(),
            serde_json::to_value(&wr.context_hint).unwrap_or(serde_json::Value::Null),
        );
        map.insert(
            "stage_timings".into(),
            serde_json::to_value(&wr.stage_timings).unwrap_or(serde_json::Value::Null),
        );
        if wr.ai_applied && wr.raw_text != wr.final_text {
            map.insert("original_text".into(), wr.raw_text.clone().into());
        }
        if let Some(execution) = wr.ai_execution.as_ref() {
            if !execution.provider_id.is_empty() {
                map.insert("ai_provider".into(), execution.provider_id.clone().into());
            }
            if !execution.model_id.is_empty() {
                map.insert("ai_model".into(), execution.model_id.clone().into());
            }
        }
    }
    serde_json::Value::Object(map)
}

fn record_insertion_timing(metadata: &mut Option<serde_json::Value>, insertion_ms: u64) {
    let Some(serde_json::Value::Object(map)) = metadata.as_mut() else {
        return;
    };
    let stage_timings = map
        .entry("stage_timings".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if let serde_json::Value::Object(stage_map) = stage_timings {
        stage_map.insert("insertion_ms".to_string(), insertion_ms.into());
    }
}

/// Metadata marking a history row whose required AI translation failed: the saved
/// `text` is the raw, untranslated transcript (kept so the user does not lose
/// their words). The frontend surfaces this as a "translation failed" badge so the
/// untranslated row is not mistaken for a successful translation.
fn build_translation_failed_history_metadata(target_language: &str) -> serde_json::Value {
    serde_json::json!({
        "translation_failed": true,
        "target_language": target_language,
    })
}
fn ai_failure_category(error: &AiProviderError) -> &'static str {
    match error {
        AiProviderError::MissingApiKey => "missing_api_key",
        AiProviderError::InvalidApiKey => "invalid_api_key",
        AiProviderError::InvalidModel => "invalid_model",
        AiProviderError::UnsupportedProvider => "unsupported_provider",
        AiProviderError::Timeout => "timeout",
        AiProviderError::Canceled => "canceled",
        AiProviderError::RateLimited => "rate_limited",
        AiProviderError::ServiceUnavailable => "service_unavailable",
        AiProviderError::Network => "network",
        AiProviderError::BadResponse => "bad_response",
        AiProviderError::Internal => "internal",
        AiProviderError::AgentCli(_) => "cli_error",
    }
}

fn ai_failure_notice(error: &AiProviderError) -> &'static str {
    match error {
        AiProviderError::MissingApiKey => "AI key missing — check Settings",
        AiProviderError::InvalidApiKey => "AI key invalid — check Settings",
        AiProviderError::InvalidModel => "AI model unavailable",
        AiProviderError::UnsupportedProvider => "AI provider not supported",
        AiProviderError::Timeout => "AI service timed out",
        AiProviderError::Canceled => "AI formatting cancelled",
        AiProviderError::RateLimited => "AI rate limited",
        AiProviderError::ServiceUnavailable => "AI service unavailable",
        AiProviderError::Network => "Couldn't reach the AI service",
        AiProviderError::BadResponse => "AI service error",
        AiProviderError::Internal => "AI formatting failed",
        AiProviderError::AgentCli(_) => "Polish failed",
    }
}

fn ai_failure_payload(error: &AiProviderError) -> serde_json::Value {
    serde_json::json!({
        "category": ai_failure_category(error),
        "message": user_facing_message(error),
    })
}

fn is_ai_auth_error(error: &AiProviderError) -> bool {
    matches!(
        error,
        AiProviderError::MissingApiKey | AiProviderError::InvalidApiKey
    )
}

fn emit_enhancing_failed(app: &AppHandle, error: &AiProviderError) {
    if app.webview_windows().is_empty() {
        return;
    }
    let _ = app.emit("enhancing-failed", ai_failure_payload(error));
}

fn notify_ai_polish_failure(app: &AppHandle, error: &AiProviderError) {
    emit_enhancing_failed(app, error);
    pill_toast_with_variant(
        app,
        ai_failure_notice(error),
        1500,
        PillToastVariant::Warning,
    );
    if is_ai_auth_error(error) {
        let _ = emit_to_window(
            app,
            "main",
            "ai-enhancement-auth-error",
            "Please check your AI API key in settings.",
        );
    }
}

async fn save_ai_polish_fallback_history(
    app: AppHandle,
    transcription: &TranscriptionResult,
    writing_result: &crate::writing::WritingResult,
) -> Result<(), String> {
    save_transcription_with_recording(
        app.clone(),
        writing_result.final_text.clone(),
        transcription.model.clone(),
        None,
        Some(build_writing_history_metadata(
            transcription,
            Some(writing_result),
        )),
    )
    .await?;
    let _ = emit_to_window(&app, "main", "history-updated", ());
    Ok(())
}

#[derive(Debug)]
struct DesktopWritingSuccessPlan {
    final_text: String,
    writing_metadata: Option<serde_json::Value>,
    should_deliver: bool,
    save_history_entries: usize,
}

fn plan_desktop_writing_success(
    transcription: &TranscriptionResult,
    writing_result: &crate::writing::WritingResult,
) -> DesktopWritingSuccessPlan {
    DesktopWritingSuccessPlan {
        final_text: writing_result.final_text.clone(),
        writing_metadata: Some(build_writing_history_metadata(
            transcription,
            Some(writing_result),
        )),
        should_deliver: true,
        save_history_entries: 1,
    }
}

fn resolve_transcription_task_for_audio(
    app: &AppHandle,
    legacy_translate_to_english: bool,
    stored_transcription_task: Option<&str>,
) -> Result<String, String> {
    if crate::writing::effective_personal_dictation_mode(app)? {
        Ok(TRANSCRIPTION_TASK_TRANSCRIBE.to_string())
    } else {
        Ok(normalize_transcription_task(
            stored_transcription_task,
            legacy_translate_to_english,
        ))
    }
}

pub fn compile_remote_request_context(
    app: &tauri::AppHandle,
    transcript_language: Option<&str>,
) -> Option<String> {
    let settings = crate::writing::load_writing_settings(app).ok()?;
    crate::writing::compile_context_for_target(
        &settings,
        transcript_language,
        crate::writing::ProviderContextTarget::WhisperInitialPrompt,
    )
}

pub(crate) fn compile_parakeet_custom_vocabulary_for_transcription(
    app: &AppHandle,
    language: Option<&str>,
) -> Vec<crate::parakeet::messages::ParakeetVocabularyTerm> {
    let Ok(settings) = crate::writing::load_writing_settings(app) else {
        return Vec::new();
    };

    if settings.custom_words.is_empty() {
        return Vec::new();
    }

    crate::writing::compile_parakeet_custom_vocabulary(&settings, language)
}

fn compile_whisper_initial_prompt(app: &AppHandle, language: Option<&str>) -> Option<String> {
    compile_remote_request_context(app, language)
}

#[cfg(target_os = "windows")]
const DEFAULT_TRANSCRIPTION_ACCELERATION: &str = "auto";
#[cfg(target_os = "windows")]
fn normalize_transcription_acceleration(value: Option<&str>) -> String {
    match value {
        Some("cpu") => "cpu".to_string(),
        Some("gpu") => "gpu".to_string(),
        _ => DEFAULT_TRANSCRIPTION_ACCELERATION.to_string(),
    }
}
#[cfg(target_os = "windows")]
async fn transcription_acceleration_mode(app: &AppHandle) -> String {
    if let Ok(store) = app.store("settings") {
        let value = store
            .get("transcription_acceleration")
            .and_then(|v| v.as_str().map(str::to_owned));
        return normalize_transcription_acceleration(value.as_deref());
    }
    DEFAULT_TRANSCRIPTION_ACCELERATION.to_string()
}

/// Best-effort warm of the Windows Vulkan sidecar when a Whisper model is preloaded.
/// No-op on non-Windows platforms and when CPU acceleration is selected.
pub(crate) async fn warm_whisper_gpu_sidecar_on_model_preload(
    app: &AppHandle,
    model_path: &Path,
) -> bool {
    #[cfg(target_os = "windows")]
    {
        let mode = transcription_acceleration_mode(app).await;
        let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        let gpu_available = gpu_client.status().await.gpu_available;
        gpu_client
            .warm_on_preload(app, model_path, &mode, gpu_available)
            .await
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, model_path);
        false
    }
}

pub(crate) async fn transcribe_whisper_with_acceleration<F>(
    app: &AppHandle,
    model_path: &Path,
    audio_path: &Path,
    language: Option<&str>,
    translate: bool,
    initial_prompt: Option<&str>,
    should_cancel: F,
) -> Result<WhisperTranscriptionOutput, String>
where
    F: Fn() -> bool + Clone + Send + 'static,
{
    #[cfg(target_os = "windows")]
    let mode = transcription_acceleration_mode(app).await;

    #[cfg(target_os = "windows")]
    let mut preserve_gpu_status = false;

    #[cfg(target_os = "windows")]
    if mode != "cpu" {
        if should_cancel() {
            return Err("Transcription cancelled".to_string());
        }

        let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        let status = gpu_client.status().await;
        let should_try_gpu = mode == "gpu" || status.gpu_available != Some(false);

        if should_try_gpu {
            // Plan 060.1: mark the ACTUAL attempt BEFORE the await. Success-only
            // marking left the backend tag stale through the whole sidecar
            // attempt, so a failure event (or a later read) could attribute this
            // recording to whatever backend the PREVIOUS run used.
            crate::whisper::transcriber::record_attempt_backend("sidecar");
            let gpu_result = gpu_client
                .transcribe(
                    app,
                    crate::whisper::gpu_sidecar::GpuTranscribeRequest {
                        model_path,
                        audio_path,
                        language,
                        translate,
                        initial_prompt,
                        mode: &mode,
                    },
                )
                .await;

            match gpu_result {
                Ok(output) => return Ok(output),
                Err(error)
                    if error == "Transcription cancelled"
                        || error == crate::whisper::gpu_sidecar::SIDECAR_ABORT_ERROR =>
                {
                    // User cancel / watchdog abort: not a GPU fault. Surface the
                    // canonical cancellation — no CPU re-run, no GPU-status change.
                    gpu_client.abort_active_process().await;
                    return Err("Transcription cancelled".to_string());
                }
                Err(error) => {
                    preserve_gpu_status = true;
                    log::warn!(
                        "GPU sidecar failed, unloading sidecar before CPU fallback: {error}"
                    );
                    gpu_client.abort_active_process().await;
                    if mode == "gpu" {
                        pill_toast(app, "GPU unavailable, using CPU", 4000);
                    }
                }
            }
        } else {
            preserve_gpu_status = true;
            log::info!("Skipping Vulkan sidecar in auto mode after previous GPU failure");
        }
    }

    // Plan 060.1: the CPU attempt's mark — placed BEFORE model init and the
    // CPU transcription, so a failure anywhere below attributes "cpu" (or the
    // sidecar above), never a previous recording's backend.
    #[cfg(target_os = "windows")]
    crate::whisper::transcriber::record_attempt_backend("cpu");

    let transcriber = {
        let cache_state = app.state::<AsyncMutex<TranscriberCache>>();
        let mut cache = cache_state.lock().await;
        cache.get_or_create(model_path)?
    };
    // Plan 060.1: warm-cache attempts use the loaded instance's backend.
    // Cold initialization records each selected backend before attempting it,
    // so a failed initialization retains the final attempted backend as well.
    crate::whisper::transcriber::record_attempt_backend(transcriber.backend());

    let audio_path = audio_path.to_path_buf();
    let language = language.map(str::to_owned);
    let initial_prompt = initial_prompt.map(str::to_owned);
    let should_cancel_for_decode = should_cancel.clone();
    let result = tokio::task::spawn_blocking(move || {
        transcriber.transcribe_with_metadata_with_prompt(
            &audio_path,
            language.as_deref(),
            translate,
            initial_prompt.as_deref(),
            should_cancel_for_decode,
        )
    })
    .await
    .map_err(|error| format!("Whisper transcription worker failed: {error}"))?;

    #[cfg(target_os = "windows")]
    {
        if result.is_ok() && !preserve_gpu_status {
            let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
            gpu_client
                .set_cpu_status(&mode, "Last transcription used CPU mode.")
                .await;
        }
    }

    result
}

fn build_remote_upload_transcription_request(
    audio_path: &Path,
    audio_data: Vec<u8>,
    job: Option<&TranscriptionJob>,
    request_context: Option<String>,
) -> (RemoteTranscriptionRequest, u64) {
    let audio_path = audio_path.to_string_lossy();
    let timeout_ms = timeout_ms_for_wav_file(audio_path.as_ref(), RemoteTimeoutSource::Upload);
    let request = RemoteTranscriptionRequest::new(audio_data, RemoteTimeoutSource::Upload)
        .with_language_and_task(
            job.and_then(|job| job.spoken_language.clone()),
            job.map(|job| transcription_task_header_value(job.task)),
        )
        .with_context(request_context);

    (request, timeout_ms)
}

fn transcription_task_header_value(task: crate::transcription::TranscriptionTask) -> String {
    match task {
        crate::transcription::TranscriptionTask::Transcribe => "transcribe".to_string(),
        crate::transcription::TranscriptionTask::TranslateToEnglish => {
            "translate_to_english".to_string()
        }
    }
}

fn classify_polish_outcome(
    polish_enabled: bool,
    ai_failed: bool,
    ai_applied: bool,
    preset: crate::ai::prompts::EnhancementPreset,
    ai_execution_recorded: bool,
) -> crate::product_analytics::PolishOutcome {
    if !polish_enabled {
        crate::product_analytics::PolishOutcome::Disabled
    } else if ai_failed {
        crate::product_analytics::PolishOutcome::Fallback
    } else if ai_applied {
        crate::product_analytics::PolishOutcome::Applied
    } else if preset == crate::ai::prompts::EnhancementPreset::PersonalDictation
        || !ai_execution_recorded
    {
        crate::product_analytics::PolishOutcome::Skipped
    } else {
        crate::product_analytics::PolishOutcome::Unchanged
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ai_failure_category, ai_failure_notice, ai_failure_payload, begin_recording_generation,
        build_failed_transcription_row, build_remote_server_error_payload,
        build_remote_transcription_result, build_remote_upload_transcription_request,
        build_transcription_job, build_translation_failed_history_metadata,
        build_writing_history_metadata, classify_local_failure, classify_polish_outcome,
        finalize_in_flight_audio, is_ai_auth_error, is_non_speech_transcript, persist_if_current,
        plan_desktop_writing_success, recording_license_state, recording_started_cue_eligible,
        remote_server_error_pill_message, set_in_flight_transcription_audio,
        should_hide_pill_when_idle, should_use_active_remote, silence_event_runs_in_state,
        silence_timeout_disposition, stop_should_reset_to_idle,
        sync_retranscription_failure_metadata, take_in_flight_transcription_audio,
        toast_clear_is_current, transcript_ready_cue_eligible, transcription_watchdog_budget,
        ActiveEngineSelection, LocalFailureKind, NormalizedTempFile, PillToastEventPayload,
        RecordingLicenseState, SilenceDetectorEvent, SilenceTimeoutDisposition, StopInFlightGuard,
        TranscriptionFailure, TranscriptionStatus,
    };
    use crate::commands::license::{CachedLicense, RuntimeLicenseCache};
    use crate::license::{LicenseState, LicenseStatus};
    use crate::remote::client::{
        calculate_timeout_ms, RemoteClientError, RemoteEndpoint, TranscriptionSource,
    };
    use crate::{AppState, RecordingState};
    use reqwest::StatusCode;
    use std::fs;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;

    fn cached_license(status: LicenseState) -> CachedLicense {
        CachedLicense::new(LicenseStatus {
            status,
            trial_days_left: None,
            license_type: None,
            license_key: None,
            expires_at: None,
            verification_state: None,
            verification_expires_at: None,
        })
    }

    #[test]
    fn recording_started_cue_requires_no_pending_stop() {
        assert!(recording_started_cue_eligible(false));
        assert!(!recording_started_cue_eligible(true));
    }

    #[test]
    fn transcript_ready_cue_requires_successful_writing_and_delivery() {
        assert!(transcript_ready_cue_eligible(true, true));
        assert!(!transcript_ready_cue_eligible(false, true));
        assert!(!transcript_ready_cue_eligible(true, false));
        assert!(!transcript_ready_cue_eligible(false, false));
    }

    #[test]
    fn polish_attempt_analytics_exclude_literal_preservation() {
        use crate::ai::prompts::EnhancementPreset;
        use crate::product_analytics::PolishOutcome;

        assert_eq!(
            classify_polish_outcome(true, false, false, EnhancementPreset::CleanDictation, false,),
            PolishOutcome::Skipped
        );
        assert_eq!(
            classify_polish_outcome(true, false, false, EnhancementPreset::CleanDictation, true,),
            PolishOutcome::Unchanged
        );
    }

    #[test]
    fn remote_upload_transcription_request_uses_upload_timeout_policy() {
        let audio_path = std::path::Path::new("missing-remote-upload.wav");
        let audio_data = vec![0x12, 0x34, 0x56];

        let (request, timeout_ms) =
            build_remote_upload_transcription_request(audio_path, audio_data.clone(), None, None);

        assert_eq!(request.audio_data, audio_data);
        assert_eq!(request.source, TranscriptionSource::Upload);
        assert_eq!(
            timeout_ms,
            calculate_timeout_ms(0, TranscriptionSource::Upload)
        );
    }

    #[test]
    fn remote_clipboard_transcription_request_uses_upload_timeout_policy() {
        let audio_path = std::path::Path::new("missing-remote-clipboard.wav");
        let audio_data = vec![0x9a, 0xbc, 0xde];

        let (request, timeout_ms) =
            build_remote_upload_transcription_request(audio_path, audio_data.clone(), None, None);

        assert_eq!(request.audio_data, audio_data);
        assert_eq!(request.source, TranscriptionSource::Upload);
        assert_eq!(
            timeout_ms,
            calculate_timeout_ms(0, TranscriptionSource::Upload)
        );
    }

    #[test]
    fn remote_upload_transcription_request_includes_language_and_task() {
        let audio_path = std::path::Path::new("sample.wav");
        let audio_data = vec![1, 2, 3];
        let job = build_transcription_job(
            crate::transcription::TranscriptionSource::AudioFile,
            "whisper",
            "base",
            Some("es".to_string()),
            true,
        );

        let (request, _) =
            build_remote_upload_transcription_request(audio_path, audio_data, Some(&job), None);

        assert_eq!(request.spoken_language.as_deref(), Some("es"));
        assert_eq!(
            request.transcription_task.as_deref(),
            Some("translate_to_english")
        );
    }

    #[test]
    fn remote_upload_transcription_request_attaches_provided_context() {
        let audio_path = std::path::Path::new("sample.wav");

        let (with_context, _) = build_remote_upload_transcription_request(
            audio_path,
            vec![1, 2, 3],
            None,
            Some("Preferred spellings: Voicetypr.".to_string()),
        );
        assert_eq!(
            with_context.context.as_deref(),
            Some("Preferred spellings: Voicetypr.")
        );

        let (without_context, _) =
            build_remote_upload_transcription_request(audio_path, vec![1, 2, 3], None, None);
        assert!(without_context.context.is_none());
    }

    #[test]
    fn transcription_watchdog_budget_defaults_to_minimum_for_unknown_duration() {
        assert_eq!(
            transcription_watchdog_budget(None),
            std::time::Duration::from_secs(180)
        );
    }

    #[test]
    fn transcription_watchdog_budget_floors_to_minimum_for_short_audio() {
        assert_eq!(
            transcription_watchdog_budget(Some(10_000)),
            std::time::Duration::from_secs(180)
        );
    }

    #[test]
    fn transcription_watchdog_budget_scales_duration_and_adds_sixty_seconds() {
        assert_eq!(
            transcription_watchdog_budget(Some(60_000)),
            std::time::Duration::from_secs(300)
        );
    }

    #[test]
    fn transcription_watchdog_budget_ceilings_partial_seconds_and_clamps_maximum() {
        assert_eq!(
            transcription_watchdog_budget(Some(60_001)),
            std::time::Duration::from_secs(304)
        );
        assert_eq!(
            transcription_watchdog_budget(Some(600_000)),
            std::time::Duration::from_secs(1800)
        );
    }

    #[test]
    fn is_non_speech_transcript_matches_blank_and_noise_tokens() {
        assert!(is_non_speech_transcript("[SOUND]"));
        assert!(is_non_speech_transcript("(silence)"));
        assert!(is_non_speech_transcript("[music]"));
        assert!(is_non_speech_transcript("  [NOISE]\n"));
        assert!(is_non_speech_transcript("[InAuDiBlE]"));
    }

    #[test]
    fn is_non_speech_transcript_rejects_prose_and_embedded_tokens() {
        assert!(!is_non_speech_transcript("Please write this down."));
        assert!(!is_non_speech_transcript(
            "The intro has [MUSIC] before speech."
        ));
    }

    #[test]
    fn is_non_speech_transcript_preserves_deliberate_punctuation_and_symbols() {
        for transcript in [".", ". ", "-", "...", "?,", "@", "#", "✅", "42", "....."] {
            assert!(!is_non_speech_transcript(transcript));
        }
    }
    #[test]
    fn remote_transcription_result_preserves_server_metadata() {
        let job = build_transcription_job(
            crate::transcription::TranscriptionSource::DesktopRecording,
            "remote",
            "remote-placeholder",
            Some("en".to_string()),
            false,
        );
        let result = build_remote_transcription_result(
            &job,
            crate::remote::server::TranscribeResponse {
                text: "hello world".to_string(),
                duration_ms: 1234,
                model: "base.en".to_string(),
                transcript_language: Some("en".to_string()),
            },
        );

        assert_eq!(result.raw_text, "hello world");
        assert_eq!(result.model, "base.en");
        assert_eq!(result.transcript_language.as_deref(), Some("en"));
        assert_eq!(result.timings.processing_duration_ms, Some(1234));
    }

    #[test]
    fn build_writing_history_metadata_uses_safe_fields_only() {
        let transcription = crate::transcription::TranscriptionResult::new(
            &build_transcription_job(
                crate::transcription::TranscriptionSource::DesktopRecording,
                "whisper",
                "base",
                Some("en".to_string()),
                false,
            ),
            "raw transcript",
        )
        .with_transcript_language(Some("en".to_string()));
        let writing_result = crate::writing::WritingResult {
            raw_text: "raw transcript".to_string(),
            final_text: "final transcript".to_string(),
            output_language: "en".to_string(),
            mode: crate::ai::prompts::EnhancementPreset::CleanDictation,
            ai_applied: true,
            applied_operations: vec![crate::writing::AppliedWritingOperation {
                kind: crate::writing::WritingOperationKind::AiCleanup,
                detail: "Applied cleanup".to_string(),
            }],
            warnings: vec![],
            context_hint: Some(crate::writing::ContextHint {
                app_name: Some("Slack".to_string()),
                window_title: Some("Secret DM subject line".to_string()),
                process_path: Some("/Applications/Slack.app".to_string()),
                category: Some(crate::writing::AppCategory::Chat),
            }),
            stage_timings: crate::writing::WritingStageTimings {
                deterministic_ms: 12,
                ai_polish_ms: Some(34),
                insertion_ms: None,
            },
            polish_enabled: true,
            ai_execution: Some(crate::writing::AiExecutionMetadata {
                provider_id: "pi".to_string(),
                model_id: "gpt-5.6-luna".to_string(),
            }),
            ai_error: None,
        };

        let metadata = build_writing_history_metadata(&transcription, Some(&writing_result));
        assert_eq!(metadata["output_language"], "en");
        assert!(metadata.get("raw_text").is_none());
        assert!(metadata.get("final_text").is_none());
        assert_eq!(metadata["original_text"], "raw transcript");
        assert_eq!(metadata["stage_timings"]["deterministic_ms"], 12);
        assert_eq!(metadata["stage_timings"]["ai_polish_ms"], 34);

        // Privacy: window_title must NEVER be serialized into history.
        let hint = &metadata["context_hint"];
        assert_eq!(hint["app_name"].as_str().unwrap(), "Slack");
        assert_eq!(hint["category"].as_str().unwrap(), "chat");
        assert!(
            hint.get("window_title").is_none(),
            "window_title must NOT be serialized into history"
        );
        assert_eq!(metadata["ai_provider"].as_str(), Some("pi"));
        assert_eq!(metadata["ai_model"].as_str(), Some("gpt-5.6-luna"));
    }

    #[test]
    fn build_writing_history_metadata_omits_original_text_when_ai_not_applied() {
        let transcription = crate::transcription::TranscriptionResult::new(
            &build_transcription_job(
                crate::transcription::TranscriptionSource::DesktopRecording,
                "whisper",
                "base",
                Some("en".to_string()),
                false,
            ),
            "raw transcript",
        )
        .with_transcript_language(Some("en".to_string()));
        let writing_result = crate::writing::WritingResult {
            raw_text: "raw transcript".to_string(),
            final_text: "deterministic transcript".to_string(),
            output_language: "en".to_string(),
            mode: crate::ai::prompts::EnhancementPreset::CleanDictation,
            ai_applied: false,
            applied_operations: vec![],
            warnings: vec![],
            context_hint: None,
            stage_timings: crate::writing::WritingStageTimings::default(),
            polish_enabled: true,
            ai_execution: None,
            ai_error: None,
        };

        let metadata = build_writing_history_metadata(&transcription, Some(&writing_result));
        assert!(metadata.get("original_text").is_none());
    }

    #[test]
    fn build_writing_history_metadata_omits_original_text_when_raw_equals_final() {
        let transcription = crate::transcription::TranscriptionResult::new(
            &build_transcription_job(
                crate::transcription::TranscriptionSource::DesktopRecording,
                "whisper",
                "base",
                Some("en".to_string()),
                false,
            ),
            "same text",
        )
        .with_transcript_language(Some("en".to_string()));
        let writing_result = crate::writing::WritingResult {
            raw_text: "same text".to_string(),
            final_text: "same text".to_string(),
            output_language: "en".to_string(),
            mode: crate::ai::prompts::EnhancementPreset::CleanDictation,
            ai_applied: true,
            applied_operations: vec![],
            warnings: vec![],
            context_hint: None,
            stage_timings: crate::writing::WritingStageTimings::default(),
            polish_enabled: true,
            ai_execution: None,
            ai_error: None,
        };

        let metadata = build_writing_history_metadata(&transcription, Some(&writing_result));
        assert!(metadata.get("original_text").is_none());
    }

    #[test]
    fn build_translation_failed_history_metadata_marks_untranslated_row() {
        let metadata = build_translation_failed_history_metadata("es");
        assert_eq!(metadata["translation_failed"].as_bool(), Some(true));
        assert_eq!(metadata["target_language"].as_str(), Some("es"));
    }

    #[test]
    fn desktop_ai_polish_failure_delivers_saves_once_and_emits_failure() {
        let transcription = crate::transcription::TranscriptionResult::new(
            &build_transcription_job(
                crate::transcription::TranscriptionSource::DesktopRecording,
                "whisper",
                "base",
                Some("en".to_string()),
                false,
            ),
            "raw transcript",
        )
        .with_transcript_language(Some("en".to_string()));
        let writing_result = crate::writing::WritingResult {
            raw_text: "raw transcript".to_string(),
            final_text: "deterministic transcript".to_string(),
            output_language: "en".to_string(),
            mode: crate::ai::prompts::EnhancementPreset::CleanDictation,
            ai_applied: false,
            applied_operations: vec![crate::writing::AppliedWritingOperation {
                kind: crate::writing::WritingOperationKind::Replacement,
                detail: "Applied replacement".to_string(),
            }],
            warnings: vec![crate::writing::WritingWarning {
                code: "ai_formatting_failed".to_string(),
                message: "AI formatting failed (timed out); used deterministic text instead"
                    .to_string(),
            }],
            context_hint: None,
            stage_timings: crate::writing::WritingStageTimings::default(),
            polish_enabled: true,
            ai_execution: None,
            ai_error: Some(crate::ai::error::AiProviderError::Timeout),
        };

        let plan = plan_desktop_writing_success(&transcription, &writing_result);

        assert!(plan.should_deliver);
        assert_eq!(plan.final_text, "deterministic transcript");
        assert_eq!(plan.save_history_entries, 1);
        assert_eq!(
            writing_result.ai_error,
            Some(crate::ai::error::AiProviderError::Timeout)
        );
        assert_eq!(
            ai_failure_payload(writing_result.ai_error.as_ref().unwrap())["category"].as_str(),
            Some("timeout")
        );
        let metadata = plan.writing_metadata.unwrap();
        assert_eq!(metadata["ai_applied"].as_bool(), Some(false));
        assert_eq!(
            metadata["warnings"][0]["code"].as_str(),
            Some("ai_formatting_failed")
        );
    }

    #[test]
    fn ai_polish_failure_payload_keeps_enhancing_failed_compatible() {
        let payload = ai_failure_payload(&crate::ai::error::AiProviderError::RateLimited);

        assert_eq!(payload["category"], "rate_limited");
        assert_eq!(payload["message"], "rate limited");
    }

    #[test]
    fn ai_polish_failure_notice_returns_short_human_message() {
        let notice = ai_failure_notice(&crate::ai::error::AiProviderError::BadResponse);
        assert_eq!(notice, "AI service error");
        assert!(!notice.contains("unpolished"), "must not say 'unpolished'");
        assert!(
            !notice.contains("bad response"),
            "must not leak raw variant label"
        );
    }

    #[test]
    fn ai_polish_auth_errors_are_detected_for_settings_notice() {
        assert!(is_ai_auth_error(
            &crate::ai::error::AiProviderError::MissingApiKey
        ));
        assert!(is_ai_auth_error(
            &crate::ai::error::AiProviderError::InvalidApiKey
        ));
        assert!(!is_ai_auth_error(
            &crate::ai::error::AiProviderError::Timeout
        ));
    }

    #[test]
    fn ai_polish_failure_categories_cover_empty_and_timeout_fallbacks() {
        assert_eq!(
            ai_failure_category(&crate::ai::error::AiProviderError::BadResponse),
            "bad_response"
        );
        assert_eq!(
            ai_failure_category(&crate::ai::error::AiProviderError::Timeout),
            "timeout"
        );
    }

    #[test]
    fn completed_retranscription_clears_stale_failure_metadata() {
        let mut map = serde_json::Map::new();
        map.insert(
            "recording_file".to_string(),
            serde_json::Value::String("sample.wav".to_string()),
        );
        map.insert(
            "error_kind".to_string(),
            serde_json::Value::String("remote_timeout".to_string()),
        );
        map.insert(
            "error_detail".to_string(),
            serde_json::Value::String("timed out".to_string()),
        );
        map.insert(
            "error_body".to_string(),
            serde_json::Value::String("body".to_string()),
        );
        map.insert(
            "can_retry_from_history".to_string(),
            serde_json::Value::Bool(true),
        );

        sync_retranscription_failure_metadata(&mut map, TranscriptionStatus::Completed, "done");

        assert!(!map.contains_key("error_kind"));
        assert!(!map.contains_key("error_detail"));
        assert!(!map.contains_key("error_body"));
        assert!(!map.contains_key("can_retry_from_history"));
    }

    #[test]
    fn failed_retranscription_rewrites_failure_metadata() {
        let mut map = serde_json::Map::new();
        map.insert(
            "recording_file".to_string(),
            serde_json::Value::String("sample.wav".to_string()),
        );
        map.insert(
            "error_kind".to_string(),
            serde_json::Value::String("remote_timeout".to_string()),
        );
        map.insert(
            "error_detail".to_string(),
            serde_json::Value::String("timed out".to_string()),
        );
        map.insert(
            "error_body".to_string(),
            serde_json::Value::String("body".to_string()),
        );

        sync_retranscription_failure_metadata(
            &mut map,
            TranscriptionStatus::Failed,
            "Re-transcription failed: Error: remote offline",
        );

        assert!(!map.contains_key("error_kind"));
        assert!(!map.contains_key("error_body"));
        assert_eq!(
            map.get("error_detail").and_then(serde_json::Value::as_str),
            Some("Re-transcription failed: Error: remote offline")
        );
        assert_eq!(
            map.get("can_retry_from_history")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn stop_in_flight_guard_blocks_duplicates_and_resets_on_drop() {
        let flag = Arc::new(AtomicBool::new(false));

        {
            let _guard = StopInFlightGuard::try_acquire(flag.clone())
                .expect("first stop owner should acquire guard");
            assert!(
                StopInFlightGuard::try_acquire(flag.clone()).is_none(),
                "duplicate stop owner must be rejected"
            );
        }

        assert!(
            StopInFlightGuard::try_acquire(flag).is_some(),
            "dropping the guard should release the stop owner flag"
        );
    }

    #[test]
    fn duplicate_stop_no_recorder_does_not_reset_idle_over_owned_flow() {
        assert!(stop_should_reset_to_idle(RecordingState::Recording));
        assert!(stop_should_reset_to_idle(RecordingState::Idle));
        assert!(stop_should_reset_to_idle(RecordingState::Starting));
        assert!(stop_should_reset_to_idle(RecordingState::Error));
        assert!(!stop_should_reset_to_idle(RecordingState::Stopping));
        assert!(!stop_should_reset_to_idle(RecordingState::Transcribing));
    }

    #[test]
    fn stale_clear_pill_toast_cannot_advance_newer_toast_id() {
        let counter = AtomicU64::new(2);

        assert!(!toast_clear_is_current(&counter, 1));
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        assert!(toast_clear_is_current(&counter, 2));
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn silence_terminal_events_are_ignored_outside_recording() {
        assert!(silence_event_runs_in_state(RecordingState::Recording));
        assert!(!silence_event_runs_in_state(RecordingState::Starting));
        assert!(!silence_event_runs_in_state(RecordingState::Stopping));
        assert!(!silence_event_runs_in_state(RecordingState::Transcribing));
        assert!(!silence_event_runs_in_state(RecordingState::Idle));
        assert!(!silence_event_runs_in_state(RecordingState::Error));
    }

    #[test]
    fn silence_timeout_with_speech_transcribes_and_no_speech_discards() {
        // Never-lose-speech: a timeout AFTER captured speech must stop+transcribe,
        // never discard.
        assert_eq!(
            silence_timeout_disposition(SilenceDetectorEvent::TimeoutWithSpeech),
            Some(SilenceTimeoutDisposition::StopAndTranscribe)
        );
        // A timeout with no speech for the whole window discards.
        assert_eq!(
            silence_timeout_disposition(SilenceDetectorEvent::TimeoutNoSpeech),
            Some(SilenceTimeoutDisposition::CancelAndDiscard)
        );
        // Non-terminal events carry no terminal disposition.
        for event in [
            SilenceDetectorEvent::Clear,
            SilenceDetectorEvent::DeadMicWarn,
            SilenceDetectorEvent::LongSilenceWarn,
        ] {
            assert_eq!(silence_timeout_disposition(event), None);
        }
    }

    #[test]
    fn should_hide_pill_when_idle_for_never() {
        assert!(should_hide_pill_when_idle("never"));
    }

    #[test]
    fn should_hide_pill_when_idle_for_when_recording() {
        assert!(should_hide_pill_when_idle("when_recording"));
    }

    #[test]
    fn should_hide_pill_when_idle_for_always() {
        assert!(!should_hide_pill_when_idle("always"));
    }

    #[test]
    fn recording_license_state_is_loading_when_cache_absent() {
        assert_eq!(
            recording_license_state(&RuntimeLicenseCache::Loading),
            RecordingLicenseState::Loading
        );
    }

    #[test]
    fn recording_license_state_requires_recovery_after_failed_check() {
        assert_eq!(
            recording_license_state(&RuntimeLicenseCache::Failed),
            RecordingLicenseState::CheckFailed
        );
    }

    #[test]
    fn recording_license_state_blocks_expired_license() {
        let cached = cached_license(LicenseState::Expired);
        assert_eq!(
            recording_license_state(&RuntimeLicenseCache::Ready(cached)),
            RecordingLicenseState::Blocked
        );
    }

    #[test]
    fn recording_license_state_requires_verification_after_offline_deadline() {
        let mut cached = cached_license(LicenseState::Licensed);
        cached.status.verification_state = Some(crate::license::LicenseVerificationState::Verified);
        cached.status.verification_expires_at =
            Some(chrono::Utc::now() - chrono::Duration::seconds(1));
        assert_eq!(
            recording_license_state(&RuntimeLicenseCache::Ready(cached)),
            RecordingLicenseState::VerificationRequired
        );
    }

    #[test]
    fn recording_license_state_blocks_missing_license() {
        let cached = cached_license(LicenseState::None);
        assert_eq!(
            recording_license_state(&RuntimeLicenseCache::Ready(cached)),
            RecordingLicenseState::Blocked
        );
    }

    #[test]
    fn recording_license_state_allows_trial_and_licensed() {
        let trial = cached_license(LicenseState::Trial);
        let licensed = cached_license(LicenseState::Licensed);
        assert_eq!(
            recording_license_state(&RuntimeLicenseCache::Ready(trial)),
            RecordingLicenseState::Ready
        );
        assert_eq!(
            recording_license_state(&RuntimeLicenseCache::Ready(licensed)),
            RecordingLicenseState::Ready
        );
    }

    #[test]
    fn normalized_temp_file_removes_file_on_drop() {
        let path =
            std::env::temp_dir().join(format!("voicetypr-normalized-{}.wav", std::process::id()));
        fs::write(&path, b"temp audio").unwrap();

        {
            let temp_file = NormalizedTempFile::new(path.clone());
            assert!(temp_file.path().exists());
        }

        assert!(!path.exists());
    }

    #[test]
    fn explicit_engine_hint_bypasses_active_remote() {
        assert!(!should_use_active_remote(Some("whisper")));
        assert!(!should_use_active_remote(Some("parakeet")));
        assert!(!should_use_active_remote(Some("soniox")));
    }

    #[test]
    fn missing_engine_hint_allows_active_remote() {
        assert!(should_use_active_remote(None));
    }

    #[test]
    fn retryable_preserved_remote_failure_emits_history_capable_payload_and_copy() {
        let failure = TranscriptionFailure::Remote(RemoteClientError::Timeout {
            endpoint: RemoteEndpoint::Transcribe,
            timeout_ms: 120_000,
            detail: "timed out while waiting for response".to_string(),
        });
        let payload = build_remote_server_error_payload(&failure, true);

        assert_eq!(
            payload["title"].as_str().unwrap(),
            "Remote Transcription Failed"
        );
        assert!(payload["message"].as_str().unwrap().contains("timed out"));
        assert_eq!(payload["error_kind"].as_str().unwrap(), "remote_timeout");
        assert!(payload["can_retry_from_history"].as_bool().unwrap());
        assert!(remote_server_error_pill_message(true).contains("History"));
    }

    #[test]
    fn non_retryable_no_recording_remote_failure_omits_history_guidance() {
        let failure = TranscriptionFailure::Remote(RemoteClientError::ConnectFailed {
            endpoint: RemoteEndpoint::Transcribe,
            detail: "connection refused".to_string(),
        });
        let payload = build_remote_server_error_payload(&failure, false);

        assert_eq!(
            payload["title"].as_str().unwrap(),
            "Remote Transcription Failed"
        );
        assert!(payload["message"]
            .as_str()
            .unwrap()
            .contains("connection refused"));
        assert_eq!(
            payload["error_kind"].as_str().unwrap(),
            "remote_connect_failed"
        );
        assert!(!payload["can_retry_from_history"].as_bool().unwrap());
        assert!(!remote_server_error_pill_message(false).contains("History"));
    }

    #[test]
    fn failed_history_row_content_is_truthful_and_structured() {
        let row = build_failed_transcription_row(
            &TranscriptionFailure::Remote(RemoteClientError::HttpStatus {
                endpoint: RemoteEndpoint::Transcribe,
                status: StatusCode::BAD_GATEWAY,
                body: Some("upstream unavailable".to_string()),
            }),
            "base.en",
            "recordings/failure.wav",
        );

        assert_eq!(row["status"].as_str().unwrap(), "failed");
        assert_eq!(row["error_kind"].as_str().unwrap(), "remote_http_status");
        assert_eq!(
            row["error_detail"].as_str().unwrap(),
            "Server error: 502 Bad Gateway"
        );
        assert_eq!(
            row["recording_file"].as_str().unwrap(),
            "recordings/failure.wav"
        );
        assert_eq!(row["model"].as_str().unwrap(), "base.en");
        assert!(row["can_retry_from_history"].as_bool().unwrap());
        assert_ne!(
            row["text"].as_str().unwrap(),
            "Remote server unreachable - re-transcribe to get text"
        );
    }

    #[test]
    fn failed_history_row_supports_local_engine_failures() {
        let row = build_failed_transcription_row(
            &TranscriptionFailure::local("Transcription timed out".to_string()),
            "base.en",
            "recordings/failure.wav",
        );
        assert_eq!(row["status"].as_str().unwrap(), "failed");
        assert_eq!(row["error_kind"].as_str().unwrap(), "local");
        assert_eq!(
            row["error_detail"].as_str().unwrap(),
            "Transcription timed out"
        );
        assert!(row["can_retry_from_history"].as_bool().unwrap());
        assert!(row["error_body"].is_null());
    }

    #[test]
    fn is_retryable_failure_excludes_cancellation_and_too_short() {
        assert!(
            TranscriptionFailure::local("Transcription timed out".to_string())
                .is_retryable_failure()
        );
        assert!(TranscriptionFailure::local("OpenAI error: 500".to_string()).is_retryable_failure());
        assert!(
            !TranscriptionFailure::local("Transcription cancelled".to_string())
                .is_retryable_failure()
        );
        assert!(
            !TranscriptionFailure::local("Recording too short".to_string()).is_retryable_failure()
        );
    }

    // --- build_writing_history_metadata tests ---

    fn minimal_transcription_result() -> crate::transcription::TranscriptionResult {
        use crate::transcription::{TranscriptionSource, TranscriptionTask, TranscriptionTimings};
        crate::transcription::TranscriptionResult {
            raw_text: "hello world".into(),
            engine: "whisper".into(),
            model: "base.en".into(),
            spoken_language: Some("en".into()),
            transcript_language: Some("en".into()),
            task: TranscriptionTask::Transcribe,
            source: TranscriptionSource::AudioFile,
            segments: None,
            words: None,
            timings: TranscriptionTimings {
                audio_duration_ms: Some(5000),
                processing_duration_ms: Some(1200),
            },
        }
    }

    fn minimal_writing_result_with_hint() -> crate::writing::WritingResult {
        crate::writing::WritingResult {
            raw_text: "hello world".into(),
            final_text: "hello world".into(),
            output_language: "en".into(),
            mode: crate::ai::prompts::EnhancementPreset::PersonalDictation,
            ai_applied: true,
            applied_operations: vec![],
            warnings: vec![],
            context_hint: Some(crate::writing::ContextHint {
                app_name: Some("Finder".into()),
                ..Default::default()
            }),
            stage_timings: crate::writing::WritingStageTimings::default(),
            polish_enabled: true,
            ai_execution: None,
            ai_error: None,
        }
    }

    #[test]
    fn writing_metadata_without_writing_has_base_fields_only() {
        let tr = minimal_transcription_result();
        let meta = build_writing_history_metadata(&tr, None);
        let obj = meta.as_object().unwrap();

        // Always-present fields
        assert_eq!(obj["source"].as_str().unwrap(), "audio_file");
        assert_eq!(obj["engine"].as_str().unwrap(), "whisper");
        assert!(!obj["diarized"].as_bool().unwrap());
        assert_eq!(obj["audio_duration_ms"].as_u64().unwrap(), 5000);
        assert_eq!(obj["processing_duration_ms"].as_u64().unwrap(), 1200);

        // Writing-specific fields must be ABSENT
        assert!(
            !obj.contains_key("context_hint"),
            "context_hint must be absent"
        );
        assert!(!obj.contains_key("mode"), "mode must be absent");
        assert!(!obj.contains_key("ai_applied"), "ai_applied must be absent");
        assert!(
            !obj.contains_key("output_language"),
            "output_language must be absent"
        );
        assert!(
            !obj.contains_key("applied_operations"),
            "applied_operations must be absent"
        );
        assert!(!obj.contains_key("warnings"), "warnings must be absent");
    }

    #[test]
    fn writing_metadata_diarized_true_when_words_present() {
        let mut tr = minimal_transcription_result();
        tr.words = Some(vec![]);
        let meta = build_writing_history_metadata(&tr, None);
        assert!(meta["diarized"].as_bool().unwrap());
    }

    #[test]
    fn writing_metadata_timings_omitted_when_none() {
        let mut tr = minimal_transcription_result();
        tr.timings.audio_duration_ms = None;
        tr.timings.processing_duration_ms = None;
        let meta = build_writing_history_metadata(&tr, None);
        let obj = meta.as_object().unwrap();
        assert!(!obj.contains_key("audio_duration_ms"));
        assert!(!obj.contains_key("processing_duration_ms"));
    }

    #[test]
    fn writing_metadata_with_writing_result_includes_all_fields() {
        let tr = minimal_transcription_result();
        let wr = minimal_writing_result_with_hint();
        let meta = build_writing_history_metadata(&tr, Some(&wr));
        let obj = meta.as_object().unwrap();

        // Base fields still present
        assert_eq!(obj["source"].as_str().unwrap(), "audio_file");
        assert_eq!(obj["engine"].as_str().unwrap(), "whisper");
        assert!(!obj["diarized"].as_bool().unwrap());

        // Writing fields present
        assert_eq!(obj["mode"].as_str().unwrap(), "PersonalDictation");
        assert_eq!(obj["output_language"].as_str().unwrap(), "en");
        assert!(obj["ai_applied"].as_bool().unwrap());
        assert!(obj.contains_key("applied_operations"));
        assert!(obj.contains_key("warnings"));

        // context_hint present with expected values
        let hint = &obj["context_hint"];
        assert_eq!(hint["app_name"].as_str().unwrap(), "Finder");
    }

    // --- save_transcription metadata persistence ---

    #[test]
    fn save_transcription_with_metadata_sets_writing_key() {
        // Verify the JSON assembly logic: Some(metadata) → data["writing"] is populated.
        let metadata =
            serde_json::json!({ "source": "audio_file", "engine": "whisper", "diarized": false });
        let mut data = serde_json::json!({ "text": "hi", "model": "base.en", "timestamp": "t" });
        if let Some(m) = Some(metadata.clone()) {
            data["writing"] = m;
        }
        assert_eq!(data["writing"]["source"].as_str().unwrap(), "audio_file");
        assert!(!data["writing"]["diarized"].as_bool().unwrap());
    }

    #[test]
    fn save_transcription_without_metadata_no_writing_key() {
        // Verify the JSON assembly logic: None → data["writing"] is absent.
        let mut data = serde_json::json!({ "text": "hi", "model": "base.en", "timestamp": "t" });
        let writing_metadata: Option<serde_json::Value> = None;
        if let Some(m) = writing_metadata {
            data["writing"] = m;
        }
        assert!(!data.as_object().unwrap().contains_key("writing"));
    }

    #[test]
    fn pill_toast_event_payload_serializes_suggestion_when_present() {
        let payload = PillToastEventPayload {
            id: 1,
            message: "Microphone access failed".to_string(),
            duration_ms: 1500,
            action: None,
            variant: None,
            persistent: false,
            suggestion: Some("Enable Microphone access in System Settings".to_string()),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(
            json["suggestion"].as_str(),
            Some("Enable Microphone access in System Settings")
        );
        assert_eq!(json["message"].as_str(), Some("Microphone access failed"));
    }

    #[test]
    fn pill_toast_event_payload_omits_suggestion_when_none() {
        let payload = PillToastEventPayload {
            id: 2,
            message: "Recording error".to_string(),
            duration_ms: 1500,
            action: None,
            variant: None,
            persistent: false,
            suggestion: None,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert!(
            json.get("suggestion").is_none(),
            "suggestion key must be absent when None"
        );
    }

    #[test]
    fn ai_failure_notice_network_returns_short_human_message() {
        use crate::ai::error::AiProviderError;
        let notice = ai_failure_notice(&AiProviderError::Network);
        assert_eq!(notice, "Couldn't reach the AI service");
        // Must not expose internal error jargon or the old "unpolished text" phrasing
        assert!(!notice.contains("unpolished"));
        assert!(!notice.contains("inserted"));
        assert!(notice.len() < 60);
    }

    #[test]
    fn ai_failure_notice_auth_errors_stay_short_and_calm() {
        use crate::ai::error::AiProviderError;
        for variant in [
            AiProviderError::MissingApiKey,
            AiProviderError::InvalidApiKey,
        ] {
            let notice = ai_failure_notice(&variant);
            assert!(
                !notice.contains("unpolished"),
                "must not say 'unpolished': {notice}"
            );
            assert!(
                !notice.contains("AI polish failed"),
                "must not expose internal prefix: {notice}"
            );
            assert!(notice.len() < 60, "must be short: {notice}");
        }
    }

    #[test]
    fn ai_failure_notice_covers_all_variants_without_internal_strings() {
        use crate::ai::error::AiProviderError;
        let variants = [
            AiProviderError::MissingApiKey,
            AiProviderError::InvalidApiKey,
            AiProviderError::InvalidModel,
            AiProviderError::UnsupportedProvider,
            AiProviderError::Timeout,
            AiProviderError::Canceled,
            AiProviderError::RateLimited,
            AiProviderError::ServiceUnavailable,
            AiProviderError::Network,
            AiProviderError::BadResponse,
            AiProviderError::Internal,
        ];
        for variant in variants {
            let notice = ai_failure_notice(&variant);
            assert!(
                !notice.contains("unpolished"),
                "internal phrasing in: {notice}"
            );
            assert!(
                !notice.contains("AI polish failed"),
                "internal prefix in: {notice}"
            );
            assert!(notice.len() < 60, "too long: {notice}");
            // Must start with something legible (not lowercase "ai" etc.)
            assert!(
                notice.starts_with(|c: char| c.is_uppercase() || c == '\''),
                "should start with uppercase or apostrophe: {notice}"
            );
        }
    }

    #[test]
    fn classify_local_failure_detects_auth_errors() {
        // Exact user_message string (no detail appended)
        assert_eq!(
            classify_local_failure("Authentication failed for the transcription service."),
            LocalFailureKind::AuthInvalid
        );
        // With raw detail appended by desktop_failure_from_transcription_error
        assert_eq!(
            classify_local_failure(
                "Authentication failed for the transcription service.: 401 Unauthorized"
            ),
            LocalFailureKind::AuthInvalid
        );
    }

    #[test]
    fn classify_local_failure_detects_model_errors() {
        assert_eq!(
            classify_local_failure("The selected transcription model is unavailable."),
            LocalFailureKind::ModelUnavailable
        );
        assert_eq!(
            classify_local_failure("The selected transcription engine is unavailable."),
            LocalFailureKind::ModelUnavailable
        );
    }

    #[test]
    fn classify_local_failure_treats_everything_else_as_generic() {
        // Transient failures: retrying may help
        assert_eq!(
            classify_local_failure("Transcription timed out"),
            LocalFailureKind::Generic
        );
        assert_eq!(
            classify_local_failure("Transcription failed. Please try again.: Parakeet error"),
            LocalFailureKind::Generic
        );
        // Raw internal strings that slip through
        assert_eq!(
            classify_local_failure("Unknown transcription engine: custom-engine"),
            LocalFailureKind::Generic
        );
        assert_eq!(
            classify_local_failure("Failed to read audio file: permission denied (os error 13)"),
            LocalFailureKind::Generic
        );
    }

    static POST_TRANSCRIPTION_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn unique_side_effect_path(label: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("voicetypr-{label}-{}-{n}.json", std::process::id()))
    }

    #[test]
    fn persist_if_current_skips_stale_generation_and_cancel() {
        let _guard = POST_TRANSCRIPTION_TEST_LOCK.lock().unwrap();
        let app_state = AppState::new();
        let generation = begin_recording_generation();
        let mut commits = 0;

        let committed = persist_if_current(&app_state, generation, || {
            commits += 1;
            "committed"
        });
        assert_eq!(committed, Some("committed"));
        assert_eq!(commits, 1);

        let stale_generation = generation;
        let _new_generation = begin_recording_generation();
        let skipped_stale = persist_if_current(&app_state, stale_generation, || {
            commits += 1;
            "stale"
        });
        assert_eq!(skipped_stale, None);
        assert_eq!(commits, 1, "stale generation must not run commit");

        let current_generation = begin_recording_generation();
        app_state.request_cancellation();
        let skipped_cancel = persist_if_current(&app_state, current_generation, || {
            commits += 1;
            "cancelled"
        });
        assert_eq!(skipped_cancel, None);
        assert_eq!(commits, 1, "cancelled generation must not run commit");
    }

    #[tokio::test(flavor = "current_thread")]
    #[allow(clippy::await_holding_lock)] // process-wide test serialization lock; current-thread runtime
    async fn stale_task_cannot_clear_newer_in_flight_tracker() {
        let _guard = POST_TRANSCRIPTION_TEST_LOCK.lock().unwrap();
        let stale_generation = begin_recording_generation();
        let stale_path = unique_side_effect_path("stale-audio");
        fs::write(&stale_path, b"stale audio").unwrap();
        set_in_flight_transcription_audio(stale_generation, stale_path.clone());

        let newer_generation = begin_recording_generation();
        let newer_path = unique_side_effect_path("newer-audio");
        fs::write(&newer_path, b"newer audio").unwrap();
        set_in_flight_transcription_audio(newer_generation, newer_path.clone());

        let stale_path_for_task = stale_path.clone();
        tokio::spawn(async move {
            finalize_in_flight_audio(stale_generation, &stale_path_for_task);
        })
        .await
        .unwrap();

        assert!(
            !stale_path.exists(),
            "stale task still removes its own temp file"
        );
        let tracked = take_in_flight_transcription_audio();
        assert_eq!(
            tracked.as_ref(),
            Some(&newer_path),
            "stale finalization must not clear the newer generation tracker"
        );
        if let Some(path) = tracked {
            let _ = fs::remove_file(path);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    #[allow(clippy::await_holding_lock)] // process-wide test serialization lock; current-thread runtime
    async fn failed_history_after_late_cancel_is_skipped_at_commit_site() {
        let _guard = POST_TRANSCRIPTION_TEST_LOCK.lock().unwrap();
        let generation = begin_recording_generation();
        let app_state = Arc::new(AppState::new());
        let history_path = unique_side_effect_path("failed-history");
        let row = build_failed_transcription_row(
            &TranscriptionFailure::local("Transcription timed out".to_string()),
            "base.en",
            "recording.wav",
        );
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (go_tx, go_rx) = tokio::sync::oneshot::channel();
        let state_for_task = app_state.clone();
        let path_for_task = history_path.clone();

        let task = tokio::spawn(async move {
            ready_tx.send(()).unwrap();
            go_rx.await.unwrap();
            persist_if_current(state_for_task.as_ref(), generation, || {
                fs::write(&path_for_task, row.to_string()).unwrap();
            })
        });

        ready_rx.await.unwrap();
        app_state.request_cancellation();
        go_tx.send(()).unwrap();

        assert_eq!(task.await.unwrap(), None);
        assert!(
            !history_path.exists(),
            "late cancel must skip the failed-history write"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    #[allow(clippy::await_holding_lock)] // process-wide test serialization lock; current-thread runtime
    async fn translation_failed_history_after_late_cancel_is_skipped_at_commit_site() {
        let _guard = POST_TRANSCRIPTION_TEST_LOCK.lock().unwrap();
        let generation = begin_recording_generation();
        let app_state = Arc::new(AppState::new());
        let history_path = unique_side_effect_path("translation-history");
        let row = serde_json::json!({
            "text": "raw transcript",
            "model": "base.en",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "writing": build_translation_failed_history_metadata("es"),
        });
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (go_tx, go_rx) = tokio::sync::oneshot::channel();
        let state_for_task = app_state.clone();
        let path_for_task = history_path.clone();

        let task = tokio::spawn(async move {
            ready_tx.send(()).unwrap();
            go_rx.await.unwrap();
            persist_if_current(state_for_task.as_ref(), generation, || {
                fs::write(&path_for_task, row.to_string()).unwrap();
            })
        });

        ready_rx.await.unwrap();
        app_state.request_cancellation();
        go_tx.send(()).unwrap();

        assert_eq!(task.await.unwrap(), None);
        assert!(
            !history_path.exists(),
            "late cancel must skip the translation-failed history write"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    #[allow(clippy::await_holding_lock)] // process-wide test serialization lock; current-thread runtime
    async fn cancel_between_gate_and_spawned_history_save_is_rechecked_inside_task() {
        let _guard = POST_TRANSCRIPTION_TEST_LOCK.lock().unwrap();
        let generation = begin_recording_generation();
        let app_state = Arc::new(AppState::new());
        let history_path = unique_side_effect_path("spawned-history");
        assert!(
            !super::delivery_aborted(app_state.is_cancellation_requested(), generation),
            "outer delivery gate passes before the spawned save is queued"
        );

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (go_tx, go_rx) = tokio::sync::oneshot::channel();
        let state_for_task = app_state.clone();
        let path_for_task = history_path.clone();

        let save_task = tokio::spawn(async move {
            ready_tx.send(()).unwrap();
            go_rx.await.unwrap();
            persist_if_current(state_for_task.as_ref(), generation, || {
                fs::write(&path_for_task, "history row").unwrap();
            })
        });

        ready_rx.await.unwrap();
        app_state.request_cancellation();
        go_tx.send(()).unwrap();

        assert_eq!(save_task.await.unwrap(), None);
        assert!(
            !history_path.exists(),
            "spawned history task must recheck cancellation at the write site"
        );
    }
    #[test]
    fn active_engine_routes_are_stable_for_evidence_logs() {
        let selections = [
            ActiveEngineSelection::Whisper {
                model_name: "base".to_string(),
                model_path: std::path::PathBuf::new(),
            },
            ActiveEngineSelection::Parakeet {
                model_name: "parakeet".to_string(),
            },
            ActiveEngineSelection::Cloud {
                provider: crate::cloud_stt::CloudProvider::Openai,
                model_name: "gpt-4o-mini-transcribe".to_string(),
            },
            ActiveEngineSelection::Remote {
                server_id: "server".to_string(),
                server_name: "Remote".to_string(),
                host: "127.0.0.1".to_string(),
                port: 47842,
                password: None,
            },
        ];

        assert_eq!(selections[0].route(), "local");
        assert_eq!(selections[1].route(), "local");
        assert_eq!(selections[2].route(), "cloud");
        assert_eq!(selections[3].route(), "remote");
    }
}

/// Cached recording configuration to avoid repeated store access during transcription flow
/// Cache is invalidated when settings change via update hooks
#[derive(Clone, Debug)]
pub struct RecordingConfig {
    pub show_pill_widget: bool,
    pub pill_indicator_mode: String, // "never", "always", or "when_recording"
    pub ai_enabled: bool,
    pub ai_provider: String,
    pub ai_model: String,
    pub current_model: String,
    pub current_engine: String,
    pub speech_language: String,
    pub transcription_task: String,
    pub final_text_language: String,
    pub show_recording_status: bool,
    // Internal cache metadata
    loaded_at: Instant,
}

impl RecordingConfig {
    /// Maximum age of cache before considering it stale (5 minutes)
    const MAX_CACHE_AGE: std::time::Duration = std::time::Duration::from_secs(5 * 60);

    /// Load all recording-relevant settings from store in one operation
    pub async fn load_from_store(app: &AppHandle) -> Result<Self, String> {
        let store = app.store("settings").map_err(|e| e.to_string())?;

        let show_pill_widget = store
            .get("show_pill_widget")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let stored_mode = store
            .get("pill_indicator_mode")
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let legacy_show = store.get("show_pill_indicator").and_then(|v| v.as_bool());
        let pill_indicator_mode = resolve_pill_indicator_mode(
            stored_mode.clone(),
            legacy_show,
            Settings::default().pill_indicator_mode,
        );
        log::debug!(
            "pill_visibility: recording config loaded show_pill_widget={} pill_indicator_mode='{}' stored={:?} legacy_show={:?}",
            show_pill_widget,
            pill_indicator_mode,
            stored_mode,
            legacy_show
        );

        let legacy_speech_language = store
            .get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().speech_language.clone());
        let legacy_translate_to_english = store
            .get("translate_to_english")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let speech_language = store
            .get("speech_language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or(legacy_speech_language);
        let ai_enabled = store
            .get("ai_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let stored_transcription_task = store
            .get("transcription_task")
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let transcription_task = normalize_transcription_task(
            stored_transcription_task.as_deref(),
            legacy_translate_to_english,
        );
        let stored_final_text_language = store
            .get("final_text_language")
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let final_text_language = normalize_final_text_language(
            stored_final_text_language.as_deref(),
            &transcription_task,
        );

        let config = Self {
            show_pill_widget,
            pill_indicator_mode,
            ai_enabled,
            ai_provider: store
                .get("ai_provider")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default(),
            ai_model: store
                .get("ai_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "".to_string()),
            current_model: store
                .get("current_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "".to_string()),
            current_engine: store
                .get("current_model_engine")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "whisper".to_string()),
            speech_language,
            transcription_task,
            final_text_language,
            show_recording_status: store
                .get("show_recording_status")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            loaded_at: Instant::now(),
        };
        let mut config = config;
        config.speech_language = normalize_speech_language_for_model(
            &config.current_engine,
            &config.current_model,
            &config.speech_language,
        );
        Ok(config)
    }

    /// Check if this cache entry is still fresh
    pub fn is_fresh(&self) -> bool {
        self.loaded_at.elapsed() < Self::MAX_CACHE_AGE
    }
}

// Implement UnwindSafe traits for panic testing compatibility
impl UnwindSafe for RecordingConfig {}
impl RefUnwindSafe for RecordingConfig {}

/// Decide whether the audio for a just-finished transcription should be
/// persisted to the recordings directory.
///
/// `discard` is true when the result must NOT reach the user — either because
/// they cancelled, or because a newer recording started beneath this task
/// (stale generation). PRIVACY: discarded audio is never written to disk, even
/// on a success or a normally-saveable (retryable) failure.
///
/// - `discard == true`           ⇒ never save.
/// - `Ok` (failure is `None`)    ⇒ save (preserves re-transcribable speech).
/// - retryable failure           ⇒ save (preserves the clip for History retry).
/// - non-retryable failure       ⇒ don't save (too-short clip, cancelled engine…).
pub(crate) fn should_save_recording_audio(
    discard: bool,
    failure: Option<&TranscriptionFailure>,
) -> bool {
    if discard {
        return false;
    }
    match failure {
        None => true,
        Some(failure) => failure.is_retryable_failure(),
    }
}

async fn maybe_save_recording_if_current(
    app: &AppHandle,
    generation: u64,
    audio_path: &Path,
) -> Option<String> {
    save_recording_internal(app, audio_path, true, Some(generation)).await
}

/// Internal function to save recording with optional settings check
async fn save_recording_internal(
    app: &AppHandle,
    audio_path: &Path,
    check_settings: bool,
    generation: Option<u64>,
) -> Option<String> {
    // Get settings store for retention policy and save_recordings check.
    let store = match app.store("settings") {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Failed to get settings store: {}", e);
            // If we can't get settings, still save for preservation purposes
            if check_settings {
                return None;
            }
            // For forced saves (preserve on failure), continue without store
            // We'll skip retention cleanup in this case
            return save_recording_without_cleanup(app, audio_path, generation).await;
        }
    };

    if check_settings {
        let save_recordings = store
            .get("save_recordings")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !save_recordings {
            log::debug!("save_recordings is disabled, skipping recording persistence");
            return None;
        }
    }

    // Get recordings directory
    let recordings_dir = match app.path().app_data_dir() {
        Ok(dir) => dir.join("recordings"),
        Err(e) => {
            log::error!("Failed to get app data directory: {}", e);
            return None;
        }
    };

    // Create recordings directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
        log::error!("Failed to create recordings directory: {}", e);
        return None;
    }

    // Generate filename: timestamp_uuid.wav
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let uuid_part = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let filename = format!("{}_{}.wav", timestamp, uuid_part);
    let dest_path = recordings_dir.join(&filename);

    // Copy the file to persistent storage. The gated production path enters the
    // chokepoint immediately before the synchronous copy.
    let copy_result = match generation {
        Some(generation) => {
            let app_state = app.state::<AppState>();
            persist_if_current(&app_state, generation, || {
                std::fs::copy(audio_path, &dest_path)
            })
        }
        None => Some(std::fs::copy(audio_path, &dest_path)),
    };

    match copy_result {
        None => {
            log::info!(
                "Skipped recording persistence for stale/cancelled generation {}",
                generation.unwrap_or_default()
            );
            None
        }
        Some(Ok(_)) => {
            log::info!("Saved recording to: {:?}", dest_path);

            // Cleanup old recordings by retention period.
            let retention_days = recording_retention_days_from_store(&store);

            if let Some(days) = retention_days {
                cleanup_old_recordings(&recordings_dir, days);
            }

            Some(filename)
        }
        Some(Err(e)) => {
            log::error!("Failed to save recording: {}", e);
            None
        }
    }
}

/// Save recording without cleanup (fallback when store is unavailable)
async fn save_recording_without_cleanup(
    app: &AppHandle,
    audio_path: &Path,
    generation: Option<u64>,
) -> Option<String> {
    let recordings_dir = match app.path().app_data_dir() {
        Ok(dir) => dir.join("recordings"),
        Err(e) => {
            log::error!("Failed to get app data directory: {}", e);
            return None;
        }
    };

    if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
        log::error!("Failed to create recordings directory: {}", e);
        return None;
    }

    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let uuid_part = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let filename = format!("{}_{}.wav", timestamp, uuid_part);
    let dest_path = recordings_dir.join(&filename);

    let copy_result = match generation {
        Some(generation) => {
            let app_state = app.state::<AppState>();
            persist_if_current(&app_state, generation, || {
                std::fs::copy(audio_path, &dest_path)
            })
        }
        None => Some(std::fs::copy(audio_path, &dest_path)),
    };

    match copy_result {
        None => {
            log::info!(
                "Skipped recording persistence fallback for stale/cancelled generation {}",
                generation.unwrap_or_default()
            );
            None
        }
        Some(Ok(_)) => {
            log::info!("Saved recording (no cleanup) to: {:?}", dest_path);
            Some(filename)
        }
        Some(Err(e)) => {
            log::error!("Failed to save recording: {}", e);
            None
        }
    }
}

/// Clean up recordings older than the retention period.
fn cleanup_old_recordings(recordings_dir: &Path, retention_days: u32) {
    let cutoff = match std::time::SystemTime::now().checked_sub(std::time::Duration::from_secs(
        u64::from(retention_days) * 24 * 60 * 60,
    )) {
        Some(cutoff) => cutoff,
        None => return,
    };

    let recordings = match std::fs::read_dir(recordings_dir) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("Failed to read recordings directory for cleanup: {}", e);
            return;
        }
    };

    for entry in recordings.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let is_wav = path.extension().map(|ext| ext == "wav").unwrap_or(false);

        if !is_wav {
            continue;
        }

        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified().or_else(|_| metadata.created()));

        let Ok(modified) = modified else {
            continue;
        };

        if modified >= cutoff {
            continue;
        }

        if let Err(e) = std::fs::remove_file(&path) {
            log::warn!("Failed to remove old recording {:?}: {}", path, e);
        } else {
            log::info!("Cleaned up old recording: {:?}", path);
        }
    }
}

/// Get the full path to the recordings directory
#[tauri::command]
pub async fn get_recordings_directory(app: AppHandle) -> Result<String, String> {
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");

    // Create if it doesn't exist
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|e| format!("Failed to create recordings directory: {}", e))?;

    Ok(recordings_dir.to_string_lossy().to_string())
}

/// Open the recordings directory in the system file manager
#[tauri::command]
pub async fn open_recordings_folder(app: AppHandle) -> Result<(), String> {
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?
        .join("recordings");

    // Create directory if it doesn't exist
    if !recordings_dir.exists() {
        std::fs::create_dir_all(&recordings_dir)
            .map_err(|e| format!("Failed to create recordings directory: {}", e))?;
    }

    // Open the directory using the system's file manager
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&recordings_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        std::process::Command::new("explorer")
            .arg(&recordings_dir)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(())
}

#[derive(Clone)]
pub(crate) enum ActiveEngineSelection {
    Whisper {
        model_name: String,
        model_path: PathBuf,
    },
    Parakeet {
        model_name: String,
    },
    Cloud {
        provider: crate::cloud_stt::CloudProvider,
        model_name: String,
    },
    Remote {
        server_id: String,
        server_name: String,
        host: String,
        port: u16,
        password: Option<String>,
    },
}

impl ActiveEngineSelection {
    pub(crate) fn engine_name(&self) -> &'static str {
        match self {
            ActiveEngineSelection::Whisper { .. } => "whisper",
            ActiveEngineSelection::Parakeet { .. } => "parakeet",
            ActiveEngineSelection::Cloud { provider, .. } => provider.id(),
            ActiveEngineSelection::Remote { .. } => "remote",
        }
    }

    pub(crate) const fn analytics_kind(&self) -> crate::product_analytics::EngineKind {
        match self {
            Self::Whisper { .. } => crate::product_analytics::EngineKind::Whisper,
            Self::Parakeet { .. } => crate::product_analytics::EngineKind::Parakeet,
            Self::Cloud { .. } => crate::product_analytics::EngineKind::Cloud,
            Self::Remote { .. } => crate::product_analytics::EngineKind::Remote,
        }
    }

    pub(crate) const fn route(&self) -> &'static str {
        match self {
            ActiveEngineSelection::Whisper { .. } | ActiveEngineSelection::Parakeet { .. } => {
                "local"
            }
            ActiveEngineSelection::Cloud { .. } => "cloud",
            ActiveEngineSelection::Remote { .. } => "remote",
        }
    }

    pub(crate) fn model_name(&self) -> &str {
        match self {
            ActiveEngineSelection::Whisper { model_name, .. } => model_name,
            ActiveEngineSelection::Parakeet { model_name } => model_name,
            ActiveEngineSelection::Cloud { model_name, .. } => model_name,
            ActiveEngineSelection::Remote { server_name, .. } => server_name,
        }
    }
}

async fn abort_due_to_missing_model(
    app: &AppHandle,
    audio_path: &Path,
    log_message: &str,
    user_message: &str,
) -> Result<String, String> {
    log::error!("{}", log_message);
    update_recording_state(app, RecordingState::Error, Some(user_message.to_string()));

    if let Err(e) = std::fs::remove_file(audio_path) {
        log::warn!("Failed to remove audio file: {}", e);
    }

    // Show pill toast for no models error
    pill_toast(app, user_message, 2000);
    // Bring the dashboard forward so the no-models error is actionable
    // (the main window normally stays hidden in tray/pill mode).
    let _ = crate::commands::window::focus_main_window(app.clone()).await;

    // Also emit domain event for main window
    let _ = emit_to_window(
        app,
        "main",
        "no-models-error",
        serde_json::json!({
            "title": "No Models Installed",
            "message": user_message,
            "action": "open-settings"
        }),
    );

    if should_hide_pill(app).await {
        if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
            log::error!("Failed to hide pill window: {}", e);
        }
    }

    update_recording_state(app, RecordingState::Idle, None);

    Err(log_message.to_string())
}

fn should_use_active_remote(engine_hint: Option<&str>) -> bool {
    engine_hint.is_none()
}

pub(crate) async fn resolve_engine_for_model(
    app: &AppHandle,
    model_name: &str,
    engine_hint: Option<&str>,
) -> Result<ActiveEngineSelection, String> {
    let remote_settings = app.state::<AsyncMutex<RemoteSettings>>();
    let active_remote = {
        let settings = remote_settings.lock().await;
        settings.get_active_connection().cloned()
    };

    if should_use_active_remote(engine_hint) {
        if let Some(remote_conn) = active_remote {
            if matches!(
                remote_conn.status,
                crate::remote::settings::ConnectionStatus::Online
            ) {
                return Ok(ActiveEngineSelection::Remote {
                    server_id: remote_conn.id.clone(),
                    server_name: remote_conn.display_name(),
                    host: remote_conn.host,
                    port: remote_conn.port,
                    password: remote_conn.password,
                });
            }

            return Err(
                "Selected remote unavailable. Reconnect or choose another source.".to_string(),
            );
        }
    }

    let whisper_state = app.state::<AsyncRwLock<WhisperManager>>();
    let parakeet_manager = app.state::<ParakeetManager>();

    match engine_hint.map(|e| e.to_lowercase()) {
        Some(ref engine) if crate::cloud_stt::CloudProvider::from_id(engine).is_some() => {
            let provider = crate::cloud_stt::CloudProvider::from_id(engine).unwrap();
            if crate::secure_store::secure_has(app, provider.key_name()).unwrap_or(false) {
                Ok(ActiveEngineSelection::Cloud {
                    provider,
                    model_name: model_name.to_string(),
                })
            } else {
                Err(format!(
                    "{} key not configured. Please configure it in Models.",
                    provider.display_name()
                ))
            }
        }
        Some(ref engine) if engine == "parakeet" => {
            let status = parakeet_manager
                .list_models()
                .into_iter()
                .find(|m| m.name == model_name);

            match status {
                Some(info) if info.downloaded => Ok(ActiveEngineSelection::Parakeet {
                    model_name: model_name.to_string(),
                }),
                Some(_) => Err(format!(
                    "Parakeet model '{}' is not downloaded. Please download it first.",
                    model_name
                )),
                None => Err(format!(
                    "Parakeet model '{}' not found in registry.",
                    model_name
                )),
            }
        }
        Some(ref engine) if engine == "whisper" || engine == "whisper.cpp" => {
            let path = whisper_state
                .read()
                .await
                .get_model_path(model_name)
                .ok_or_else(|| format!("Whisper model '{}' not found", model_name))?;

            Ok(ActiveEngineSelection::Whisper {
                model_name: model_name.to_string(),
                model_path: path,
            })
        }
        Some(engine) => Err(format!("Unknown model engine '{}'.", engine)),
        None => {
            if let Some(provider) = crate::cloud_stt::CloudProvider::from_id(model_name) {
                if crate::secure_store::secure_has(app, provider.key_name()).unwrap_or(false) {
                    return Ok(ActiveEngineSelection::Cloud {
                        provider,
                        model_name: model_name.to_string(),
                    });
                } else {
                    return Err(format!(
                        "{} key not configured. Please configure it in Models.",
                        provider.display_name()
                    ));
                }
            }
            if let Some(path) = whisper_state.read().await.get_model_path(model_name) {
                return Ok(ActiveEngineSelection::Whisper {
                    model_name: model_name.to_string(),
                    model_path: path,
                });
            }

            let status = parakeet_manager
                .list_models()
                .into_iter()
                .find(|m| m.name == model_name);

            if let Some(info) = status {
                if info.downloaded {
                    return Ok(ActiveEngineSelection::Parakeet {
                        model_name: model_name.to_string(),
                    });
                } else {
                    return Err(format!(
                        "Model '{}' is a Parakeet model but not downloaded. Please download it first.",
                        model_name
                    ));
                }
            }

            Err(format!(
                "Model '{}' not found in Whisper or Parakeet registries",
                model_name
            ))
        }
    }
}

/// Helper function to invalidate recording config cache when settings change
pub async fn invalidate_recording_config_cache(app: &AppHandle) {
    let app_state = app.state::<AppState>();
    let mut cache = app_state.recording_config_cache.write().await;
    *cache = None;
    log::debug!("Recording config cache invalidated due to settings change");
}

/// Helper function to get cached recording config or load from store
pub async fn get_recording_config(app: &AppHandle) -> Result<RecordingConfig, String> {
    let app_state = app.state::<AppState>();

    // Try to get from cache first
    {
        let cache = app_state.recording_config_cache.read().await;
        if let Some(config) = cache.as_ref() {
            if config.is_fresh() {
                log::debug!(
                    "Using cached recording config (age: {:?})",
                    config.loaded_at.elapsed()
                );
                return Ok(config.clone());
            } else {
                log::debug!("Recording config cache is stale, will reload");
            }
        }
    }

    // Cache miss or stale - load from store
    let config = RecordingConfig::load_from_store(app).await?;

    // Update cache
    {
        let mut cache = app_state.recording_config_cache.write().await;
        *cache = Some(config.clone());
        log::debug!("Recording config cached successfully");
    }

    Ok(config)
}

// Global audio recorder state
pub struct RecorderState(pub Mutex<AudioRecorder>);

/// Select the best fallback model based on available models
/// Prioritizes models by size (smaller to larger for better performance)
fn select_best_fallback_model(
    available_models: &[String],
    requested: &str,
    model_priority: &[String],
) -> String {
    // First try to find a model similar to the requested one
    if !requested.is_empty() {
        // If requested "large-v3", try other large variants first
        for model in available_models {
            if model.starts_with(requested.split('-').next().unwrap_or(requested)) {
                return model.clone();
            }
        }
    }

    // Otherwise use priority order from WhisperManager
    for priority_model in model_priority {
        if available_models.contains(priority_model) {
            return priority_model.clone();
        }
    }

    // If no priority model found, return first available
    available_models.first().cloned().unwrap_or_else(|| {
        log::error!("No models available for fallback selection");
        // This should never happen as we check for empty models before calling this function
        // But return a default to prevent panic
        "base.en".to_string()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordingLicenseState {
    Ready,
    Loading,
    CheckFailed,
    Blocked,
    VerificationRequired,
}

fn recording_license_state(
    cache: &crate::commands::license::RuntimeLicenseCache,
) -> RecordingLicenseState {
    use crate::commands::license::RuntimeLicenseCache;
    match cache {
        RuntimeLicenseCache::Ready(cached)
            if cached
                .status
                .verification_window_expired(chrono::Utc::now()) =>
        {
            RecordingLicenseState::VerificationRequired
        }
        RuntimeLicenseCache::Ready(cached)
            if matches!(
                cached.status.status,
                LicenseState::Expired | LicenseState::None
            ) =>
        {
            RecordingLicenseState::Blocked
        }
        RuntimeLicenseCache::Ready(_) => RecordingLicenseState::Ready,
        RuntimeLicenseCache::Loading => RecordingLicenseState::Loading,
        RuntimeLicenseCache::Failed => RecordingLicenseState::CheckFailed,
    }
}
/// Pre-recording validation using the readiness state
async fn validate_recording_requirements(app: &AppHandle) -> Result<(), String> {
    let validate_start = std::time::Instant::now();
    log::debug!("⏱️ [VALIDATE] starting recognition_availability_snapshot");
    let availability = crate::recognition_availability_snapshot(app).await;
    log::debug!(
        "⏱️ [VALIDATE] recognition_availability_snapshot complete (+{}ms)",
        validate_start.elapsed().as_millis()
    );

    if !availability.any_available()
        || (availability.remote_selected && !availability.remote_available)
    {
        log::error!("No usable speech recognition engines are ready");
        let (title, message, error_text) =
            if availability.remote_selected && !availability.remote_available {
                (
                    "Selected Remote Unavailable",
                    "Selected remote unavailable. Reconnect or choose another source.",
                    "Selected remote unavailable. Reconnect or choose another source.".to_string(),
                )
            } else if availability.cloud_selected && !availability.cloud_ready {
                (
                    "No Speech Recognition Sources",
                    "Please configure your cloud transcription key in Models before recording.",
                    "Cloud transcription key missing".to_string(),
                )
            } else {
                (
                "No Speech Recognition Sources",
                "Connect a cloud provider or download a local model in Models before recording.",
                "No speech recognition sources available. Please configure a source first."
                    .to_string(),
            )
            };
        // Bring the dashboard forward so the error toast + onboarding are visible
        // (the main window normally stays hidden in tray/pill mode).
        let _ = crate::commands::window::focus_main_window(app.clone()).await;
        let _ = emit_to_window(
            app,
            "main",
            "no-models-error",
            serde_json::json!({
                "title": title,
                "message": message,
                "action": "open-settings"
            }),
        );
        return Err(error_text);
    }

    // Check cached license status (warmed during startup/license transitions - no network call)
    let app_state = app.state::<AppState>();
    let license_state = recording_license_state(&*app_state.license_cache.read().await);

    match license_state {
        RecordingLicenseState::CheckFailed => {
            log::warn!("Recording blocked: license check failed; recovery required");
            let message = "License check failed. Open License and retry, or re-enter your existing license key to activate it again.";
            let _ = crate::commands::window::focus_main_window(app.clone()).await;
            let _ = emit_to_all(
                app,
                "license-required",
                serde_json::json!({
                    "title": "License Check Failed",
                    "message": message,
                    "action": "restore"
                }),
            );
            return Err(message.to_string());
        }
        RecordingLicenseState::VerificationRequired => {
            log::warn!("Recording blocked: offline license verification window has ended");
            let _ = crate::commands::window::focus_main_window(app.clone()).await;
            let _ = emit_to_all(
                app,
                "license-required",
                serde_json::json!({
                    "title": "License Verification Required",
                    "message": "Connect to the internet and revalidate your license to continue recording.",
                    "action": "revalidate"
                }),
            );
            return Err("License verification required to record".to_string());
        }
        RecordingLicenseState::Blocked => {
            log::warn!("Recording blocked: no active license or trial");

            let _ = crate::commands::window::focus_main_window(app.clone()).await;

            let _ = emit_to_all(
                app,
                "license-required",
                serde_json::json!({
                    "title": "License Required",
                    "message": "No active license or trial was found. If you already purchased a license, re-enter your existing key here to activate it again.",
                    "action": "restore"
                }),
            );
            return Err("License required to record".to_string());
        }
        RecordingLicenseState::Ready => {}
        RecordingLicenseState::Loading => {
            log::warn!("Recording blocked: license cache not initialized yet");
            let _ = emit_to_window(
                app,
                "main",
                "license-loading",
                serde_json::json!({
                    "title": "Checking License",
                    "message": "License status is still loading. Please try again in a moment.",
                    "action": "wait"
                }),
            );
            return Err(
                "License status is still loading. Please try again in a moment.".to_string(),
            );
        }
    }

    log::debug!(
        "⏱️ [VALIDATE] validation complete (+{}ms)",
        validate_start.elapsed().as_millis()
    );
    Ok(())
}

pub(crate) fn clear_pending_stop_after_start(app_state: &AppState) {
    app_state
        .pending_stop_after_start
        .store(false, std::sync::atomic::Ordering::SeqCst);
}

fn recording_started_cue_eligible(pending_stop_consumed: bool) -> bool {
    !pending_stop_consumed
}

fn transcript_ready_cue_eligible(writing_succeeded: bool, should_deliver: bool) -> bool {
    writing_succeeded && should_deliver
}

fn silence_event_runs_in_state(state: RecordingState) -> bool {
    matches!(state, RecordingState::Recording)
}

/// Command-layer disposition for a *terminal* silence event. Pure so the
/// never-lose-speech routing (captured speech ⇒ transcribe, never discard) is
/// unit-testable without an `AppHandle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SilenceTimeoutDisposition {
    /// Speech or uncertain audio was captured → stop normally and transcribe.
    StopAndTranscribe,
    /// No finite nonzero signal for the entire timeout window → cancel.
    CancelAndDiscard,
}

fn silence_timeout_disposition(event: SilenceDetectorEvent) -> Option<SilenceTimeoutDisposition> {
    match event {
        SilenceDetectorEvent::TimeoutWithSpeech => {
            Some(SilenceTimeoutDisposition::StopAndTranscribe)
        }
        SilenceDetectorEvent::TimeoutNoSpeech => Some(SilenceTimeoutDisposition::CancelAndDiscard),
        SilenceDetectorEvent::Clear
        | SilenceDetectorEvent::DeadMicWarn
        | SilenceDetectorEvent::LongSilenceWarn => None,
    }
}

fn clear_active_silence_toast(app: &AppHandle, active_toast_id: &mut Option<u64>) {
    if let Some(toast_id) = active_toast_id.take() {
        clear_pill_toast(app, toast_id);
    }
}

async fn stop_recording_after_long_silence(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<String, String> {
    stop_recording(app, state).await
}

fn spawn_silence_event_listener(
    app: AppHandle,
    silence_event_rx: std::sync::mpsc::Receiver<SilenceDetectorEvent>,
) {
    std::thread::spawn(move || {
        let mut active_silence_toast_id: Option<u64> = None;

        while let Ok(event) = silence_event_rx.recv() {
            let current_state = crate::get_recording_state(&app);
            if !silence_event_runs_in_state(current_state) {
                clear_active_silence_toast(&app, &mut active_silence_toast_id);
                break;
            }

            match event {
                SilenceDetectorEvent::Clear => {
                    clear_active_silence_toast(&app, &mut active_silence_toast_id);
                }
                SilenceDetectorEvent::DeadMicWarn => {
                    clear_active_silence_toast(&app, &mut active_silence_toast_id);
                    active_silence_toast_id = Some(pill_toast_persistent(
                        &app,
                        "No audio detected — check your microphone",
                        PillToastVariant::Warning,
                    ));
                }
                SilenceDetectorEvent::LongSilenceWarn => {
                    clear_active_silence_toast(&app, &mut active_silence_toast_id);
                    active_silence_toast_id = Some(pill_toast_persistent(
                        &app,
                        "Long silence detected",
                        PillToastVariant::Warning,
                    ));
                }
                event @ (SilenceDetectorEvent::TimeoutWithSpeech
                | SilenceDetectorEvent::TimeoutNoSpeech) => {
                    clear_active_silence_toast(&app, &mut active_silence_toast_id);
                    match silence_timeout_disposition(event) {
                        Some(SilenceTimeoutDisposition::StopAndTranscribe) => {
                            // Speech captured → stop normally so it is transcribed.
                            pill_toast_with_variant(
                                &app,
                                "Ended after long silence",
                                1500,
                                PillToastVariant::Info,
                            );
                            let app_for_stop = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let recorder_state = app_for_stop.state::<RecorderState>();
                                if let Err(e) = stop_recording_after_long_silence(
                                    app_for_stop.clone(),
                                    recorder_state,
                                )
                                .await
                                {
                                    log::error!("Long-silence stop failed: {}", e);
                                }
                            });
                        }
                        Some(SilenceTimeoutDisposition::CancelAndDiscard) => {
                            // No signal the whole window → cancel and discard.
                            let app_for_cancel = app.clone();
                            tauri::async_runtime::spawn(async move {
                                match cancel_recording(app_for_cancel.clone()).await {
                                    Ok(()) => {
                                        pill_toast_with_suggestion(
                                            &app_for_cancel,
                                            "No audio captured",
                                            "Try recording again",
                                            1500,
                                            Some(PillToastVariant::Warning),
                                        );
                                    }
                                    Err(e) => {
                                        log::error!("No-speech timeout cancel failed: {}", e);
                                        pill_toast_with_variant(
                                            &app_for_cancel,
                                            "Recording error",
                                            1500,
                                            PillToastVariant::Warning,
                                        );
                                    }
                                }
                            });
                        }
                        None => {}
                    }
                    break;
                }
            }
        }

        clear_active_silence_toast(&app, &mut active_silence_toast_id);
    });
}

pub(crate) fn ptt_key_released(app_state: &AppState) -> bool {
    let mode = match app_state.recording_mode.lock() {
        Ok(guard) => *guard,
        Err(poisoned) => {
            log::warn!("recording_mode mutex poisoned; recovering value for PTT guard");
            *poisoned.into_inner()
        }
    };

    mode == RecordingMode::PushToTalk
        && !app_state
            .ptt_key_held
            .load(std::sync::atomic::Ordering::SeqCst)
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<(), String> {
    let recording_start = Instant::now();

    log_start("RECORDING_START");
    log::debug!("⏱️ [REC TIMING] start_recording called (+0ms)");
    log_with_context(
        log::Level::Debug,
        "Recording command started",
        &[
            ("command", "start_recording"),
            ("timestamp", &chrono::Utc::now().to_rfc3339()),
        ],
    );

    // If we're stuck in Error, recover to Idle before attempting a new start
    let current_state = crate::get_recording_state(&app);
    if matches!(current_state, crate::RecordingState::Error) {
        crate::update_recording_state(
            &app,
            crate::RecordingState::Idle,
            Some("recover".to_string()),
        );
    }
    log::debug!(
        "⏱️ [REC TIMING] state check complete (+{}ms)",
        recording_start.elapsed().as_millis()
    );

    // Validate all requirements upfront
    let validation_start = Instant::now();
    log::debug!(
        "⏱️ [REC TIMING] starting validation (+{}ms)",
        recording_start.elapsed().as_millis()
    );
    match validate_recording_requirements(&app).await {
        Ok(_) => {
            log::debug!(
                "⚡ PERF: RECORDING_VALIDATION took {}ms validation_passed",
                validation_start.elapsed().as_millis()
            );
        }
        Err(e) => {
            log_failed("RECORDING_START", &e);
            log_with_context(
                log::Level::Debug,
                "Validation failed",
                &[
                    ("stage", "validation"),
                    (
                        "validation_time_ms",
                        validation_start.elapsed().as_millis().to_string().as_str(),
                    ),
                ],
            );
            return Err(e);
        }
    }

    // PTT guard: if recording mode is PushToTalk and the key was already released
    // while validation was running, abort now. This prevents recording from starting
    // after the user has already released the PTT key (e.g., during slow license checks).
    {
        let app_state = app.state::<AppState>();
        if ptt_key_released(&app_state) {
            log::info!("PTT: Key was released during validation; aborting recording start");
            return Err(PTT_START_ABORTED_AFTER_RELEASE.to_string());
        }
    }

    // Idempotent fast-path: if a recording is already starting or active — e.g. a
    // redundant start from the in-app hotkey fallback racing the native hotkey
    // path for one physical press — no-op BEFORE any side effects. This region is
    // await-free through `update_recording_state(Starting)` below, so whichever
    // caller publishes `Starting` first makes the other observe it here and return
    // Ok, never bumping the generation, clobbering flags, or hitting the
    // `Recording -> Starting` state-machine rejection.
    {
        let live_state = crate::get_recording_state(&app);
        if matches!(
            live_state,
            crate::RecordingState::Starting | crate::RecordingState::Recording
        ) {
            log::debug!(
                "start_recording: already {:?}; treating redundant start as no-op",
                live_state
            );
            return Ok(());
        }
    }

    // All validation passed, update state to starting
    log::debug!(
        "⏱️ [REC TIMING] validation complete (+{}ms)",
        recording_start.elapsed().as_millis()
    );
    log_state_transition("RECORDING", "idle", "starting", true, None);
    // Open a new recording generation and clear stale flags from a previous
    // attempt BEFORE publishing `Starting`. Clearing `pending_stop_after_start`
    // after `Starting` is published would erase a stop that arrived during the
    // Starting window (PTT key-up while Starting sets the flag) — so the clear
    // must happen first. From this point on, any stop/cancel observed after
    // `Starting` targets THIS attempt and must win.
    {
        let app_state = app.state::<AppState>();
        begin_recording_generation();
        app_state.clear_cancellation();
        clear_pending_stop_after_start(&app_state);
    }
    if let Some(hint) = crate::writing::capture_active_app_context() {
        if let Some(app_state) = app.try_state::<AppState>() {
            app_state.set_recording_app_context(hint);
        }
    }
    update_recording_state(&app, RecordingState::Starting, None);
    // Ensure transition actually happened; if blocked, abort early
    if !matches!(
        crate::get_recording_state(&app),
        crate::RecordingState::Starting
    ) {
        return Err("Cannot start recording in current state".to_string());
    }

    // Pause system media if enabled (default: off)
    let mut resume_media_on_error = false;
    if let Ok(store) = app.store("settings") {
        let pause_media = store
            .get("pause_media_during_recording")
            .and_then(|v| v.as_bool())
            .unwrap_or(false); // Default to off
        if pause_media {
            log::info!("🎵 Pause media during recording is enabled");
            let paused = MEDIA_CONTROLLER.pause_if_playing();
            resume_media_on_error = paused;
            log::debug!("🎵 Media pause result: {}", paused);
        } else {
            log::debug!("🎵 Pause media during recording is disabled");
        }
    }

    let resume_media_if_needed = || {
        if resume_media_on_error {
            MEDIA_CONTROLLER.resume_if_we_paused();
        }
    };

    // Load recording config once to avoid repeated store access
    log::debug!(
        "⏱️ [REC TIMING] loading recording config (+{}ms)",
        recording_start.elapsed().as_millis()
    );
    let config = match get_recording_config(&app).await {
        Ok(config) => config,
        Err(e) => {
            log::error!("Failed to load recording config: {}", e);
            resume_media_if_needed();
            return Err(format!("Configuration error: {}", e));
        }
    };
    log::debug!(
        "Using recording config: show_pill={} pill_indicator_mode='{}' ai_enabled={} model={}",
        config.show_pill_widget,
        config.pill_indicator_mode,
        config.ai_enabled,
        config.current_model
    );
    // Warm the active cloud provider's connection so the first transcription skips the handshake (skipped when a remote handles dispatch).
    if let Some(provider) = crate::cloud_stt::CloudProvider::from_id(&config.current_engine) {
        let app = app.clone();
        tokio::spawn(async move {
            let remote_active = {
                let remote = app.state::<AsyncMutex<RemoteSettings>>();
                let guard = remote.lock().await;
                guard.get_active_connection().is_some()
            };
            if !remote_active
                && crate::secure_store::secure_has(&app, provider.key_name()).unwrap_or(false)
            {
                provider.warm_up().await;
            }
        });
    }
    // Prefetch the LLM polish path too (runs post-transcription regardless of the STT engine):
    // HTTP providers get a pooled HEAD, agent-CLI providers a binary + capability probe.
    if config.ai_enabled && !config.ai_provider.is_empty() {
        let app = app.clone();
        let provider_id = config.ai_provider.clone();
        tokio::spawn(async move {
            crate::commands::ai::prefetch_ai_provider(app, provider_id).await;
        });
    }
    // Get app data directory for recordings
    let recordings_dir = match app.path().app_data_dir() {
        Ok(dir) => dir.join("recordings"),
        Err(e) => {
            resume_media_if_needed();
            return Err(e.to_string());
        }
    };

    // Ensure recordings directory exists
    if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
        resume_media_if_needed();
        return Err(format!("Failed to create recordings directory: {}", e));
    }

    let timestamp = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(e) => {
            resume_media_if_needed();
            return Err(format!("Time error: {}", e));
        }
    };
    let audio_path = recordings_dir.join(format!("recording_{}.wav", timestamp));

    // Store path for later use. The stale `pending_stop_after_start` flag is
    // cleared before `Starting` is published (above), so a stop arriving
    // during the Starting window survives to the Recording-commit check.
    let app_state = app.state::<AppState>();
    // Save current recording path
    match app_state.current_recording_path.lock() {
        Ok(mut guard) => {
            guard.replace(audio_path.clone());
        }
        Err(e) => {
            resume_media_if_needed();
            return Err(format!("Failed to acquire path lock: {}", e));
        }
    }

    // Get selected microphone from settings (before acquiring recorder lock)
    log::debug!(
        "⏱️ [REC TIMING] getting microphone settings (+{}ms)",
        recording_start.elapsed().as_millis()
    );
    let selected_microphone = match get_settings(app.clone()).await {
        Ok(settings) => {
            if let Some(mic) = settings.selected_microphone {
                log::info!("Using selected microphone: {}", mic);
                Some(mic)
            } else {
                log::info!("Using default microphone");
                None
            }
        }
        Err(e) => {
            log::warn!(
                "Failed to get settings for microphone selection: {}. Using default.",
                e
            );
            None
        }
    };

    // Start recording (scoped to release mutex before async operations)
    log::debug!(
        "⏱️ [REC TIMING] acquiring recorder lock (+{}ms)",
        recording_start.elapsed().as_millis()
    );
    let (audio_level_rx_to_spawn, silence_event_rx_to_spawn) = {
        let mut recorder = match state.inner().0.lock() {
            Ok(recorder) => recorder,
            Err(e) => {
                resume_media_if_needed();
                return Err(format!("Failed to acquire recorder lock: {}", e));
            }
        };
        log::debug!(
            "⏱️ [REC TIMING] recorder lock acquired (+{}ms)",
            recording_start.elapsed().as_millis()
        );

        // Check if already recording
        if recorder.is_recording() {
            // Reaching here means the entry idempotent guard did not catch this
            // (backend state was not Starting/Recording) yet the recorder already
            // has an active handle — a genuine inconsistency, so surface it.
            log::warn!("Already recording!");
            resume_media_if_needed();
            return Err("Already recording".to_string());
        }

        // Log the current audio device before starting
        log::debug!(
            "⏱️ [REC TIMING] checking audio device (+{}ms)",
            recording_start.elapsed().as_millis()
        );
        log_start("AUDIO_DEVICE_CHECK");
        log_with_context(
            log::Level::Debug,
            "Checking audio device",
            &[("stage", "pre_recording")],
        );

        if let Ok(host) = std::panic::catch_unwind(cpal::default_host) {
            if let Some(device) = host.default_input_device() {
                if let Ok(name) = device.name() {
                    log::info!("🎙️ Audio device available: {}", name);
                    log_with_context(
                        log::Level::Info,
                        "🎮 MICROPHONE",
                        &[("device_name", &name), ("status", "available")],
                    );
                } else {
                    log::warn!("⚠️  Could not get device name, but device is available");
                    log_with_context(
                        log::Level::Info,
                        "🎮 MICROPHONE",
                        &[("status", "available_unnamed")],
                    );
                }
            } else {
                log_failed("AUDIO_DEVICE", "No default input device found");
                log_with_context(
                    log::Level::Debug,
                    "Device detection failed",
                    &[("component", "audio_device"), ("stage", "device_detection")],
                );
            }
        }

        // Try to start recording with graceful error handling
        log::debug!(
            "⏱️ [REC TIMING] about to call recorder.start_recording (+{}ms)",
            recording_start.elapsed().as_millis()
        );
        let recorder_init_start = Instant::now();
        let audio_path_str = match audio_path.to_str() {
            Some(path) => path,
            None => {
                resume_media_if_needed();
                return Err("Invalid path encoding".to_string());
            }
        };

        log_file_operation("RECORDING_START", audio_path_str, false, None, None);

        // Start recording and get side-channel receivers
        let (audio_level_rx, silence_event_rx) =
            match recorder.start_recording(audio_path_str, selected_microphone.clone()) {
                Ok(_) => {
                    log::debug!(
                        "⏱️ [REC TIMING] recorder.start_recording returned Ok (+{}ms)",
                        recording_start.elapsed().as_millis()
                    );
                    // Verify recording actually started
                    let is_recording = recorder.is_recording();

                    // Get receivers before potentially dropping recorder
                    let level_rx = recorder.take_audio_level_receiver();
                    let silence_rx = recorder.take_silence_event_receiver();

                    if !is_recording {
                        drop(recorder); // Release the lock if we're erroring out
                        log_failed(
                            "RECORDER_INIT",
                            "Recording failed to start after initialization",
                        );
                        log_with_context(
                            log::Level::Debug,
                            "Recorder initialization failed",
                            &[
                                ("audio_path", audio_path_str),
                                (
                                    "init_time_ms",
                                    recorder_init_start
                                        .elapsed()
                                        .as_millis()
                                        .to_string()
                                        .as_str(),
                                ),
                            ],
                        );

                        update_recording_state(
                            &app,
                            RecordingState::Error,
                            Some("Microphone initialization failed".to_string()),
                        );

                        // Emit user-friendly error via pill toast
                        pill_toast_with_suggestion(
                        &app,
                        "Microphone access failed",
                        "Enable Microphone access in System Settings \u{25b8} Privacy & Security",
                        1500,
                        None,
                    );

                        resume_media_if_needed();
                        return Err("Failed to start recording".to_string());
                    } else {
                        log_performance(
                            "RECORDER_INIT",
                            recorder_init_start.elapsed().as_millis() as u64,
                            Some(&format!("file={}", audio_path_str)),
                        );
                        log::info!("✅ Recording started successfully");

                        // Monitor system resources at recording start
                        #[cfg(debug_assertions)]
                        system_monitor::log_resources_before_operation("RECORDING_START");
                    }

                    (level_rx, silence_rx)
                }
                Err(e) => {
                    log_failed("RECORDER_START", &e);
                    log_with_context(
                        log::Level::Debug,
                        "Recorder start failed",
                        &[
                            ("audio_path", audio_path_str),
                            (
                                "init_time_ms",
                                recorder_init_start
                                    .elapsed()
                                    .as_millis()
                                    .to_string()
                                    .as_str(),
                            ),
                        ],
                    );

                    update_recording_state(&app, RecordingState::Error, Some(e.to_string()));

                    // Provide specific error messages for common issues
                    let (user_message, suggestion) =
                        if e.contains("permission") || e.contains("access") {
                            (
                        "Microphone permission denied",
                        "Enable Microphone access in System Settings \u{25b8} Privacy & Security",
                    )
                        } else if e.contains("device") || e.contains("not found") {
                            ("No microphone found", "Connect a microphone and try again")
                        } else if e.contains("in use") || e.contains("busy") {
                            ("Microphone busy", "Close other apps using the microphone")
                        } else {
                            ("Recording failed", "Try recording again")
                        };

                    pill_toast_with_suggestion(&app, user_message, suggestion, 1500, None);

                    resume_media_if_needed();
                    return Err(e);
                }
            };

        // Release the recorder lock after successful start
        drop(recorder);
        (audio_level_rx, silence_event_rx)
    }; // MutexGuard dropped here

    // Now perform async operations after mutex is released

    // If cancellation was requested while we were starting (e.g. Escape
    // during slow device init), abort instead of committing to Recording.
    if app_state.is_cancellation_requested() {
        log::info!("Cancellation requested during start; aborting before Recording state");
        let recorder_state_handle = app.state::<RecorderState>();
        let stop_result = recorder_state_handle
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))
            .and_then(|mut recorder| {
                if recorder.is_recording() {
                    recorder.stop_recording()
                } else {
                    Ok(String::new())
                }
            });

        clear_pending_stop_after_start(&app_state);
        MEDIA_CONTROLLER.resume_if_we_paused();

        if let Ok(mut path_guard) = app_state.current_recording_path.lock() {
            if let Some(path) = path_guard.take() {
                if let Err(error) = std::fs::remove_file(&path) {
                    log::warn!(
                        "Failed to remove cancelled recording file {}: {}",
                        path.display(),
                        error
                    );
                }
            }
        }

        update_recording_state(&app, RecordingState::Idle, None);
        stop_result?;
        return Err("Recording start cancelled".to_string());
    }

    // Second PTT guard: check again right before committing to Recording state.
    // Audio capture has already started; if PTT key was released between the first
    // guard (before Starting) and now (e.g., during audio device init), stop immediately.
    if ptt_key_released(&app_state) {
        log::info!("PTT: Key was released during audio init; stopping recorder immediately");
        // Stop the audio recorder synchronously before transitioning state.
        // If this fails, do not pretend the app is idle: propagate the
        // failure so the hotkey handler moves to Error and the recorder
        // remains visible for recovery instead of orphaning capture.
        let recorder_state_handle = app.state::<RecorderState>();
        let stop_result = recorder_state_handle
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))
            .and_then(|mut recorder| {
                if recorder.is_recording() {
                    recorder.stop_recording()
                } else {
                    Ok(String::new())
                }
            });

        clear_pending_stop_after_start(&app_state);
        MEDIA_CONTROLLER.resume_if_we_paused();

        let cleanup_recording_path = || {
            if let Ok(mut path_guard) = app_state.current_recording_path.lock() {
                if let Some(path) = path_guard.take() {
                    if let Err(error) = std::fs::remove_file(&path) {
                        log::warn!(
                            "Failed to remove aborted recording file {}: {}",
                            path.display(),
                            error
                        );
                    }
                }
            }
        };

        if let Err(error) = stop_result {
            cleanup_recording_path();
            return Err(error);
        }

        // Clean up the audio file
        cleanup_recording_path();

        update_recording_state(&app, RecordingState::Idle, None);
        return Err(PTT_START_ABORTED_AFTER_RELEASE.to_string());
    }

    // Update state to recording
    update_recording_state(&app, RecordingState::Recording, None);
    crate::product_analytics::capture(crate::product_analytics::ProductEvent::RecordingStarted);

    // If a stop was requested while starting (toggle or PTT), honor it immediately
    // after entering Recording state. For PTT, key-up in Starting state sets this flag.
    // The second PTT guard above handles key-up during audio init; this handles the
    // narrow window between Starting transition and this point.
    let pending_stop_consumed = app_state
        .pending_stop_after_start
        .swap(false, std::sync::atomic::Ordering::SeqCst);
    if pending_stop_consumed {
        log::info!("Toggle: pending stop triggered right after start; stopping now");
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let recorder_state = app_handle.state::<RecorderState>();
            if let Err(e) = stop_recording(app_handle.clone(), recorder_state).await {
                log::error!("Toggle: pending stop failed: {}", e);
            }
        });
    } else if recording_started_cue_eligible(pending_stop_consumed) {
        crate::commands::audio_feedback::play_audio_feedback(
            &app,
            crate::commands::audio_feedback::AudioFeedbackCue::RecordingStarted,
        );
    }
    if let Some(silence_event_rx) = silence_event_rx_to_spawn {
        spawn_silence_event_listener(app.clone(), silence_event_rx);
    }

    if let Some(audio_level_rx) = audio_level_rx_to_spawn {
        let app_for_levels = app.clone();
        // Use a thread instead of tokio spawn for std::sync::mpsc
        std::thread::spawn(move || {
            let mut last_emit = std::time::Instant::now();
            let emit_interval = std::time::Duration::from_millis(100); // Throttle to 10fps
            let mut last_emitted_level = 0.0f64;
            const LEVEL_CHANGE_THRESHOLD: f64 = 0.05; // Only emit if change > 5%

            while let Ok(level) = audio_level_rx.recv() {
                // Check both time throttling and significant change
                let level_changed = (level - last_emitted_level).abs() > LEVEL_CHANGE_THRESHOLD;

                if last_emit.elapsed() >= emit_interval && level_changed {
                    // Only emit to pill window - main window doesn't need audio levels
                    let _ = emit_to_window(&app_for_levels, "pill", "audio-level", level);
                    last_emit = std::time::Instant::now();
                    last_emitted_level = level;
                }
            }
        });
    }

    // Show pill widget if enabled and mode is not "never" (graceful degradation)
    let should_show_pill = config.show_pill_widget && config.pill_indicator_mode != "never";
    log::info!(
        "pill_visibility: start_recording show_pill_widget={} pill_indicator_mode='{}' should_show={}",
        config.show_pill_widget,
        config.pill_indicator_mode,
        should_show_pill
    );
    if should_show_pill {
        match crate::commands::window::show_pill_widget(app.clone()).await {
            Ok(_) => log::debug!("Pill widget shown successfully"),
            Err(e) => {
                log::warn!("Failed to show pill widget: {}. Recording will continue without visual feedback.", e);

                // Emit event so frontend knows pill isn't visible
                let _ = emit_to_window(
                    &app,
                    "main",
                    "pill-widget-error",
                    "Recording indicator unavailable. Recording is still active.",
                );
            }
        }
    } else if config.pill_indicator_mode == "never" {
        log::debug!("Pill widget hidden (pill_indicator_mode=never)");
    }

    // Also emit legacy event for compatibility
    let _ = emit_to_window(&app, "pill", "recording-started", ());

    // Log successful recording start
    log_complete(
        "RECORDING_START",
        recording_start.elapsed().as_millis() as u64,
    );
    log_with_context(
        log::Level::Debug,
        "Recording started successfully",
        &[
            ("audio_path", format!("{:?}", audio_path).as_str()),
            ("state", "recording"),
        ],
    );

    // Route ESC cancellation through the native trigger engine while recording is active.
    let app_state = app.state::<AppState>();

    // Clear ESC state
    app_state
        .esc_pressed_once
        .store(false, std::sync::atomic::Ordering::SeqCst);

    // Cancel any existing ESC timeout
    if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
        if let Some(handle) = timeout_guard.take() {
            handle.abort();
        }
    }

    crate::trigger::engine_host::rebuild_engine_bindings(&app);

    Ok(())
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<String, String> {
    #[cfg(debug_assertions)]
    let stop_start = Instant::now();

    log_start("RECORDING_STOP");
    log_with_context(
        log::Level::Debug,
        "Stop recording command",
        &[
            ("command", "stop_recording"),
            ("timestamp", chrono::Utc::now().to_rfc3339().as_str()),
        ],
    );

    let app_state = app.state::<AppState>();
    let Some(_stop_guard) = StopInFlightGuard::try_acquire(app_state.stop_in_flight.clone()) else {
        log::debug!("stop_recording: a stop is already in flight; ignoring duplicate call");
        return Ok(String::new());
    };
    let entry_state = app_state.get_current_state();

    // Update state to stopping
    log_state_transition("RECORDING", "recording", "stopping", true, None);
    update_recording_state(&app, RecordingState::Stopping, None);
    // DO NOT request cancellation here - we want transcription to complete!
    // Cancellation should only happen in cancel_recording command

    let capture_metrics;
    let mut stop_unfinalized = false;
    let mut stop_integrity_failure = false;
    // Stop recording (lock only within this scope to stay Send)
    log::info!("🛑 Stopping recording...");
    {
        let mut recorder = state
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))?;

        // Check if actually recording first
        if !recorder.is_recording() {
            log::warn!("stop_recording called but not currently recording");
            // Don't error - just return empty result; only reset if this stop owns the flow.
            drop(recorder); // Drop the lock before updating state
            if stop_should_reset_to_idle(entry_state) {
                update_recording_state(&app, RecordingState::Idle, None);
            } else if entry_state == RecordingState::Stopping
                && !transcription_task_in_flight(&app_state)
            {
                // A prior stop left us stuck in Stopping with no in-flight
                // transcription to advance the state (e.g. it errored before
                // spawning the transcription task). Recover to Idle so the
                // hotkey/UI is not frozen until restart. Only safe when no
                // transcription task is running; otherwise the legit task will
                // flip Stopping -> Transcribing on its own.
                log::warn!(
                    "stop_recording: recovering from stuck Stopping state (no transcription in flight)"
                );
                update_recording_state(&app, RecordingState::Idle, None);
            } else {
                log::debug!(
                    "stop_recording: stop/transcribe already in progress (entry state={:?}); not overriding",
                    entry_state
                );
            }
            return Ok(String::new());
        }

        let stop_message = match recorder.stop_recording() {
            Ok(msg) => msg,
            Err(e) => {
                log::error!("Recorder stop returned error: {}", e);
                if crate::audio::recorder::stop_error_is_integrity_failure(&e) {
                    stop_integrity_failure = true;
                } else if crate::audio::recorder::stop_error_is_unfinalized(&e) {
                    stop_unfinalized = true;
                }
                format!("Recorder stop error: {}", e)
            }
        };
        capture_metrics = recorder.take_last_capture_metrics();
        log::info!("{}", stop_message);

        // Resume system media if we paused it
        MEDIA_CONTROLLER.resume_if_we_paused();

        // Monitor system resources after recording stop
        #[cfg(debug_assertions)]
        system_monitor::log_resources_after_operation(
            "RECORDING_STOP",
            stop_start.elapsed().as_millis() as u64,
        );
    } // MutexGuard dropped here BEFORE any await

    crate::trigger::engine_host::rebuild_engine_bindings(&app);

    // Clean up ESC state
    app_state
        .esc_pressed_once
        .store(false, std::sync::atomic::Ordering::SeqCst);

    // Cancel any ESC timeout
    if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
        if let Some(handle) = timeout_guard.take() {
            handle.abort();
        }
    }

    log::debug!("Unregistered ESC key and cleaned up state");

    // A recorder error where the worker FINISHED still finalizes the captured WAV (e.g. a
    // device error), so we fall through to the normal transcription path below and recover the
    // speech (never-lose-speech). Integrity failures mean the WAV finalized after losing chunks:
    // do not transcribe gappy audio or write failed history rows. Only when the worker did NOT
    // finish (stop timeout / thread panic / finalize failure) is there no usable WAV: surface an
    // error and reset, having already run the media-resume + ESC cleanup above.
    if stop_integrity_failure {
        let user_message = "Recording was interrupted — please try again";
        take_and_remove_current_recording_path(&app_state, "interrupted");
        pill_toast_with_suggestion(
            &app,
            "Recording was interrupted",
            "Try recording again",
            2000,
            None,
        );
        update_recording_state(&app, RecordingState::Error, Some(user_message.to_string()));
        let app_for_reset = app.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if should_hide_pill(&app_for_reset).await {
                if let Err(e) =
                    crate::commands::window::hide_pill_widget(app_for_reset.clone()).await
                {
                    log::error!("Failed to hide pill window: {}", e);
                }
            }
            update_recording_state(&app_for_reset, RecordingState::Idle, None);
        });
        return Ok(String::new());
    }

    if stop_unfinalized {
        // The recording worker may STILL hold the WAV open: `stop_recording`'s
        // bounded join detached it when it missed the finalize deadline. hound
        // finalizes the file via its `WavWriter::drop` on the worker's
        // eventual exit, so we must NOT delete the file here — deleting
        // mid-write would race the detached worker and could remove a file
        // another thread still has open. Just drop our reference to the path;
        // the eventually-finalized file is orphaned for the OS temp-dir
        // cleanup. (The file-deleting helper is still used by the integrity
        // branch above, where the worker has already fully joined + finalized.)
        if let Ok(mut path_guard) = app_state.current_recording_path.lock() {
            path_guard.take();
        }
        pill_toast(&app, "Recording error", 1500);
        if should_hide_pill(&app).await {
            if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
                log::error!("Failed to hide pill window: {}", e);
            }
        }
        update_recording_state(&app, RecordingState::Idle, None);
        return Ok(String::new());
    }

    // Check if cancellation was requested
    if app_state.is_cancellation_requested() {
        log::info!("Recording was cancelled, skipping transcription");

        // Clean up audio file if it exists
        if let Ok(path_guard) = app_state.current_recording_path.lock() {
            if let Some(audio_path) = path_guard.as_ref() {
                log::info!("Removing cancelled recording file");
                if let Err(e) = std::fs::remove_file(audio_path) {
                    log::warn!("Failed to remove cancelled recording: {}", e);
                }
            }
        }

        // Hide pill window (only if show_pill_indicator is false)
        if should_hide_pill(&app).await {
            if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
                log::error!("Failed to hide pill window: {}", e);
            }
        }

        // Transition to idle
        update_recording_state(&app, RecordingState::Idle, None);

        return Ok("".to_string());
    }

    // Get the audio file path
    let audio_path = app_state
        .current_recording_path
        .lock()
        .map_err(|e| format!("Failed to acquire path lock: {}", e))?
        .take();

    // If no audio path, there was no recording
    let audio_path = match audio_path {
        Some(path) => {
            // Check if file exists and has content
            if let Ok(metadata) = std::fs::metadata(&path) {
                log::debug!("Audio file size: {} bytes", metadata.len());
            } else {
                log::error!("Audio file does not exist at path: {:?}", path);
            }
            path
        }
        None => {
            log::warn!("No audio file found - no recording was made");
            // Make sure to transition back to Idle state
            update_recording_state(&app, RecordingState::Idle, None);
            return Ok("".to_string());
        }
    };
    let task_generation = current_recording_generation();
    // Register the file the upcoming transcription task will own as EARLY as
    // possible — the moment `stop_recording` takes ownership of the recording
    // path, before model selection / normalization. A `cancel_recording` that
    // arrives in this pre-spawn window (recorder already stopped, task not yet
    // spawned) can otherwise find no task to abort and no tracked path, leaving
    // the cancelled dictation's audio on disk. The slot is re-set just before
    // the spawn below (to the final, possibly-normalized, path), and the task's
    // own early-cancel finalize also removes the file — so a stale slot left by
    // an early-return path that removed the file itself is harmless (a later
    // cancel hits NotFound, the next registration overwrites it).
    set_in_flight_transcription_audio(task_generation, audio_path.clone());

    // Fast-path: handle header-only/empty WAV files before normalization
    if let Ok(meta) = std::fs::metadata(&audio_path) {
        // A valid WAV header is typically 44 bytes; <= 44 implies no audio samples were written
        if meta.len() <= 44 {
            pill_toast_with_suggestion(
                &app,
                "No audio captured",
                "Try recording again",
                1000,
                None,
            );
            if let Err(e) = std::fs::remove_file(&audio_path) {
                log::debug!("Failed to remove empty audio file: {}", e);
            }
            // Frontend will hide pill after showing feedback
            update_recording_state(&app, RecordingState::Idle, None);
            return Ok("".to_string());
        }
    }
    crate::product_analytics::capture(crate::product_analytics::ProductEvent::RecordingStopped {
        duration_ms: capture_metrics.as_ref().map(|metrics| metrics.duration_ms),
    });

    let evidence_class = classify_speech_evidence(capture_metrics, None);
    if evidence_class.would_skip_engine() {
        let mut speech_evidence_attempt =
            SpeechEvidenceAttempt::new("none".to_string(), "pre_engine", capture_metrics);
        if evidence_class
            == crate::audio::speech_evidence::SpeechEvidenceClass::HighConfidenceNoSpeech
        {
            speech_evidence_attempt.set_outcome(SpeechEvidenceOutcome::SkippedNoSpeech);
            log::info!(
                "Skipping speech engine: capture below calibrated no-speech floor (no sustained speech, negligible energy)"
            );
            if let Err(error) = std::fs::remove_file(&audio_path) {
                log::debug!("Failed to remove no-speech recording: {}", error);
            }
            update_recording_state(&app, RecordingState::Idle, None);
            pill_toast_with_suggestion(
                &app,
                "No speech detected",
                "Try speaking closer to the microphone",
                1500,
                None,
            );
            if should_hide_pill(&app).await {
                if let Err(error) = crate::commands::window::hide_pill_widget(app.clone()).await {
                    log::error!(
                        "Failed to hide pill window after no-speech recording: {}",
                        error
                    );
                }
            }
            return Ok(String::new());
        }

        speech_evidence_attempt.set_outcome(SpeechEvidenceOutcome::SkippedNoInput);
        log::info!("Skipping speech engine: capture contained only exact digital zero samples");
        if let Err(error) = std::fs::remove_file(&audio_path) {
            log::debug!("Failed to remove no-input recording: {}", error);
        }
        update_recording_state(&app, RecordingState::Idle, None);
        if should_hide_pill(&app).await {
            if let Err(error) = crate::commands::window::hide_pill_widget(app.clone()).await {
                log::error!(
                    "Failed to hide pill window after no-input recording: {}",
                    error
                );
            }
        }
        return Ok(String::new());
    }

    // Decide engine early to optionally skip normalization for cloud providers
    let config = get_recording_config(&app).await.map_err(|e| {
        log::error!("Failed to load recording config: {}", e);
        format!("Configuration error: {}", e)
    })?;

    let whisper_manager = app.state::<AsyncRwLock<WhisperManager>>();

    // Check for active remote server FIRST - if set, use remote transcription
    let remote_settings = app.state::<AsyncMutex<RemoteSettings>>();
    let active_remote = {
        let settings = remote_settings.lock().await;
        log::info!(
            "🔍 [REMOTE DEBUG] Checking remote settings: active_connection_id={:?}, saved_connections={}",
            settings.active_connection_id,
            settings.saved_connections.len()
        );
        let conn = settings.get_active_connection().cloned();
        log::info!(
            "🔍 [REMOTE DEBUG] get_active_connection returned: {:?}",
            conn.as_ref().map(|c| &c.id)
        );
        conn
    };

    log::info!(
        "🔍 [REMOTE DEBUG] active_remote is_some={}",
        active_remote.is_some()
    );

    let engine_selection = if let Some(remote_conn) = active_remote {
        if matches!(
            remote_conn.status,
            crate::remote::settings::ConnectionStatus::Online
        ) {
            log::info!(
                "🌐 Using remote server for transcription: {} ({}:{})",
                remote_conn.display_name(),
                remote_conn.host,
                remote_conn.port
            );
            ActiveEngineSelection::Remote {
                server_id: remote_conn.id.clone(),
                server_name: remote_conn.display_name(),
                host: remote_conn.host,
                port: remote_conn.port,
                password: remote_conn.password,
            }
        } else {
            return abort_due_to_missing_model(
                &app,
                &audio_path,
                "Selected remote unavailable",
                "Selected remote unavailable. Reconnect or choose another source.",
            )
            .await;
        }
    } else {
        match config.current_engine.as_str() {
            "parakeet" => {
                if config.current_model.is_empty() {
                    return abort_due_to_missing_model(
                        &app,
                        &audio_path,
                        "No Parakeet model selected",
                        "Please select a Parakeet model before recording.",
                    )
                    .await;
                }

                let parakeet_manager = app.state::<ParakeetManager>();
                let models = parakeet_manager.list_models();
                if let Some(status) = models.into_iter().find(|m| m.name == config.current_model) {
                    if !status.downloaded {
                        return abort_due_to_missing_model(
                            &app,
                            &audio_path,
                            "Selected Parakeet model is not downloaded",
                            "Please download the selected Parakeet model before recording.",
                        )
                        .await;
                    }
                } else {
                    return abort_due_to_missing_model(
                        &app,
                        &audio_path,
                        "Selected Parakeet model is not available",
                        "The selected Parakeet model is unavailable. Please download it again.",
                    )
                    .await;
                }

                ActiveEngineSelection::Parakeet {
                    model_name: config.current_model.clone(),
                }
            }
            engine if crate::cloud_stt::CloudProvider::from_id(engine).is_some() => {
                let provider = crate::cloud_stt::CloudProvider::from_id(engine).unwrap();
                if config.current_model.is_empty() {
                    return abort_due_to_missing_model(
                        &app,
                        &audio_path,
                        "No cloud transcription model configured",
                        "Please choose a cloud transcription model from Models before recording.",
                    )
                    .await;
                }

                if !crate::secure_store::secure_has(&app, provider.key_name()).unwrap_or(false) {
                    return abort_due_to_missing_model(
                        &app,
                        &audio_path,
                        &format!("{} key not configured", provider.display_name()),
                        &format!(
                            "Please configure your {} key in Models before recording.",
                            provider.display_name()
                        ),
                    )
                    .await;
                }

                ActiveEngineSelection::Cloud {
                    provider,
                    model_name: config.current_model.clone(),
                }
            }
            _ => {
                let downloaded_models = whisper_manager.read().await.get_downloaded_model_names();
                log::debug!("Downloaded Whisper models: {:?}", downloaded_models);

                if downloaded_models.is_empty() {
                    return abort_due_to_missing_model(
                    &app,
                    &audio_path,
                    "No speech recognition models installed",
                    "Please download at least one speech recognition model from Models to use Voicetypr.",
                )
                .await;
                }

                log_start("MODEL_SELECTION");
                log_with_context(
                    log::Level::Debug,
                    "Selecting model",
                    &[(
                        "available_count",
                        downloaded_models.len().to_string().as_str(),
                    )],
                );

                let configured_model = if !config.current_model.is_empty() {
                    Some(config.current_model.clone())
                } else {
                    None
                };

                let chosen_model = if let Some(configured_model) = configured_model {
                    if downloaded_models.contains(&configured_model) {
                        log_model_operation(
                            "SELECTION",
                            &configured_model,
                            "CONFIGURED_AVAILABLE",
                            None,
                        );
                        configured_model
                    } else {
                        let models_by_size = whisper_manager.read().await.get_models_by_size();
                        let fallback_model = select_best_fallback_model(
                            &downloaded_models,
                            &configured_model,
                            &models_by_size,
                        );

                        log_model_operation(
                            "FALLBACK",
                            &fallback_model,
                            "SELECTED",
                            Some(&{
                                let mut ctx = std::collections::HashMap::new();
                                ctx.insert("requested".to_string(), configured_model.clone());
                                ctx.insert(
                                    "reason".to_string(),
                                    "configured_not_available".to_string(),
                                );
                                ctx
                            }),
                        );

                        let _ = emit_to_window(
                            &app,
                            "pill",
                            "model-fallback",
                            serde_json::json!({
                                "requested": configured_model,
                                "fallback": fallback_model
                            }),
                        );

                        fallback_model
                    }
                } else {
                    let models_by_size = whisper_manager.read().await.get_models_by_size();
                    let best_model =
                        select_best_fallback_model(&downloaded_models, "", &models_by_size);

                    log_model_operation(
                        "AUTO_SELECTION",
                        &best_model,
                        "SELECTED",
                        Some(&{
                            let mut ctx = std::collections::HashMap::new();
                            ctx.insert("reason".to_string(), "no_model_configured".to_string());
                            ctx.insert("strategy".to_string(), "best_available".to_string());
                            ctx
                        }),
                    );

                    best_model
                };

                let model_path = whisper_manager
                    .read()
                    .await
                    .get_model_path(&chosen_model)
                    .ok_or_else(|| format!("Model '{}' path not found", chosen_model))?;

                ActiveEngineSelection::Whisper {
                    model_name: chosen_model,
                    model_path,
                }
            }
        }
    };
    let engine_route = engine_selection.route();
    let mut speech_evidence_attempt = SpeechEvidenceAttempt::new(
        engine_selection.engine_name().to_string(),
        engine_route,
        capture_metrics,
    );

    let mut prepared_metrics = None;
    // For Whisper/Parakeet: normalize and duration gate; for Cloud/Remote: skip both
    let audio_path = match &engine_selection {
        ActiveEngineSelection::Cloud { provider, .. } => {
            log::info!(
                "[RECORD] {} selected — skipping normalization",
                provider.display_name()
            );
            audio_path
        }
        ActiveEngineSelection::Remote { server_name, .. } => {
            log::info!(
                "[RECORD] Remote server '{}' selected — skipping normalization",
                server_name
            );
            audio_path
        }
        _ => {
            let normalization_started = std::time::Instant::now();
            // Normalize captured audio to Whisper contract (WAV PCM s16, mono, 16k):
            // try in-process first (off the async runtime), fall back to ffmpeg sidecar.
            let parent_dir = audio_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::Path::new(".").to_path_buf());

            let normalized_path = {
                let a = audio_path.clone();
                let d = parent_dir.clone();
                let in_proc = tokio::task::spawn_blocking(move || {
                    crate::audio::normalizer::normalize_to_whisper_wav_with_metrics(&a, &d)
                })
                .await;
                match in_proc {
                    Ok(Ok(normalized)) => {
                        prepared_metrics = Some(normalized.metrics);
                        normalized.path
                    }
                    other => {
                        let other_err = match &other {
                            Ok(Ok(_)) => unreachable!(),
                            Ok(Err(e)) => e.clone(),
                            Err(e) => e.to_string(),
                        };
                        log::warn!(
                            "In-process audio normalization failed; falling back to ffmpeg: {:?}",
                            other_err
                        );
                        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                        let out_path = parent_dir.join(format!("normalized_{}.wav", ts));
                        if let Err(e) =
                            crate::ffmpeg::normalize_streaming(&app, &audio_path, &out_path).await
                        {
                            speech_evidence_attempt
                                .set_outcome(SpeechEvidenceOutcome::PreparationFailure);
                            log::error!("Audio normalization (ffmpeg) failed: {}", e);
                            update_recording_state(
                                &app,
                                RecordingState::Error,
                                Some("Audio normalization failed".to_string()),
                            );
                            let _ = std::fs::remove_file(&audio_path);
                            return Err("Audio normalization failed".to_string());
                        }
                        out_path
                    }
                }
            };
            log::info!(
                "transcription_stage_timing stage=audio_preparation duration_ms={}",
                normalization_started.elapsed().as_millis()
            );
            speech_evidence_attempt.set_prepared(prepared_metrics);

            // Remove raw capture after successful normalization
            if let Err(e) = std::fs::remove_file(&audio_path) {
                log::debug!("Failed to remove raw audio: {}", e);
            }

            // Determine min duration based on recording mode (PTT vs Toggle) once
            let (min_duration_s_f32, min_duration_label) = {
                let app_state = app.state::<AppState>();
                let mode = app_state
                    .recording_mode
                    .lock()
                    .ok()
                    .map(|g| *g)
                    .unwrap_or(RecordingMode::Toggle);
                match mode {
                    RecordingMode::PushToTalk => (0.5f32, "0.5".to_string()),
                    RecordingMode::Toggle => (0.5f32, "0.5".to_string()),
                }
            };

            // Duration gate (mode-specific) using normalized file
            let duration_gate = (|| -> Result<(bool, u64), String> {
                let reader = hound::WavReader::open(&normalized_path)
                    .map_err(|e| format!("Failed to open normalized wav: {}", e))?;
                let spec = reader.spec();
                let frames = reader.duration() / spec.channels as u32; // mono expected
                let duration = frames as f32 / spec.sample_rate as f32;
                let duration_ms = ((frames as u64).saturating_mul(1000))
                    .saturating_add(spec.sample_rate as u64 - 1)
                    / spec.sample_rate as u64;
                log_with_context(
                    log::Level::Info,
                    "NORMALIZED_AUDIO",
                    &[
                        ("path", format!("{:?}", normalized_path).as_str()),
                        ("sample_rate", spec.sample_rate.to_string().as_str()),
                        ("channels", spec.channels.to_string().as_str()),
                        ("bits", spec.bits_per_sample.to_string().as_str()),
                        ("duration_s", format!("{:.2}", duration).as_str()),
                    ],
                );
                Ok((duration < min_duration_s_f32, duration_ms))
            })();

            if matches!(duration_gate, Ok((true, _))) {
                speech_evidence_attempt.set_outcome(SpeechEvidenceOutcome::RecordingTooShort);
                // Emit friendly feedback and stop here
                let _ = emit_to_window(
                    &app,
                    "pill",
                    "recording-too-short",
                    format!("Recording shorter than {} seconds", min_duration_label),
                );
                if let Err(e) = std::fs::remove_file(&normalized_path) {
                    log::debug!("Failed to remove short normalized audio: {}", e);
                }
                // Frontend will hide pill after showing feedback
                update_recording_state(&app, RecordingState::Idle, None);
                return Ok("".to_string());
            }

            normalized_path
        }
    };

    log_with_context(
        log::Level::Debug,
        "Proceeding to transcription",
        &[
            ("audio_path", format!("{:?}", audio_path).as_str()),
            ("stage", "pre_transcription"),
        ],
    );
    log::debug!(
        "Using cached config: model={}, speech_language={}, transcription_task={}, final_text_language={}, ai_enabled={}",
        config.current_model,
        config.speech_language,
        config.transcription_task,
        config.final_text_language,
        config.ai_enabled
    );

    let language = if config.speech_language.is_empty() {
        None
    } else {
        Some(normalize_speech_language_for_model(
            engine_selection.engine_name(),
            engine_selection.model_name(),
            &config.speech_language,
        ))
    };
    let transcription_task = resolve_transcription_task_for_audio(
        &app,
        false,
        Some(config.transcription_task.as_str()),
    )?;
    let translate_to_english = task_uses_translate_to_english(&transcription_task);

    let engine_label = engine_selection.engine_name().to_string();
    let selected_model_name = engine_selection.model_name().to_string();

    log::info!(
        "🤖 Using {} model for transcription: {}",
        engine_label,
        selected_model_name
    );
    log::info!(
        "[LANGUAGE] stop_recording: language={:?}, transcription_task={}, translate={}",
        language.as_deref(),
        transcription_task,
        translate_to_english
    );

    let transcription_job = build_transcription_job(
        TranscriptionSource::DesktopRecording,
        engine_label.clone(),
        selected_model_name.clone(),
        language.clone(),
        translate_to_english,
    );
    let audio_path_clone = audio_path.clone();
    // Use the generation captured when stop_recording took ownership of this
    // audio path, before any model-selection/normalization awaits. Capturing
    // here would let a stale stop adopt a newer recording's generation.
    set_in_flight_transcription_audio(task_generation, audio_path_clone.clone());
    let engine_selection_for_task = engine_selection;
    let language_for_task = language.clone();
    let selected_model_name_for_task = selected_model_name.clone();
    let transcription_job_for_task = transcription_job.clone();
    let speech_evidence_attempt_for_task = speech_evidence_attempt;
    // Spawn and track the transcription task
    let app_for_task = app.clone();
    let task_handle = tokio::spawn(
        crate::whisper::transcriber::ATTEMPT_BACKEND.scope(
            std::cell::Cell::new(None),
            async move {
        let mut speech_evidence_attempt = speech_evidence_attempt_for_task;
        log::debug!("Transcription task started");

        // Update state to transcribing
        update_recording_state(&app_for_task, RecordingState::Transcribing, None);
        // Also emit legacy event to pill window
        let _ = emit_to_window(&app_for_task, "pill", "transcription-started", ());
        // Give UI a moment to render the loader before heavy CPU work
        tokio::task::yield_now().await;

        // Check for cancellation before loading model
        let app_state = app_for_task.state::<AppState>();
        if app_state.is_cancellation_requested() {
            speech_evidence_attempt.set_outcome(SpeechEvidenceOutcome::CancelledBeforeEngine);
            log::info!("Transcription cancelled before model loading");
            // The task observed cancellation itself (cancel set the flag but
            // either did not, or could not, abort this handle in time). Remove
            // the task-owned temp recording and release the tracker so the
            // cancelled dictation's audio is never left on disk. This is the
            // SAME cleanup the normal completion path runs below; the old
            // early-cancel branch returned here without it, orphaning the file.
            finalize_in_flight_audio(task_generation, &audio_path_clone);

            // Hide pill window since we're cancelling (only if show_pill_indicator is false)
            if should_hide_pill(&app_for_task).await {
                if let Err(e) =
                    crate::commands::window::hide_pill_widget(app_for_task.clone()).await
                {
                    log::error!("Failed to hide pill window on cancellation: {}", e);
                }
            }

            update_recording_state(&app_for_task, RecordingState::Idle, None);
            return;
        }

        let mut decode_journey =
            DecodeJourneyGuard::new(engine_selection_for_task.analytics_kind());
        let decode_started = Instant::now();

        let transcription_result: Result<TranscriptionResult, TranscriptionFailure> =
            match &engine_selection_for_task {
                // Local + cloud run through the shared transcription executor (plan
                // 020 Stage 2): it owns normalization, the interactive watchdog /
                // shared cancel flag, Whisper retry, and the cloud network timeout.
                ActiveEngineSelection::Whisper { .. }
                | ActiveEngineSelection::Parakeet { .. }
                | ActiveEngineSelection::Cloud { .. } => {
                    match build_desktop_transcription_request(
                        &app_for_task,
                        &engine_selection_for_task,
                        &transcription_job_for_task,
                        language_for_task.clone(),
                        audio_path_clone.clone(),
                    ) {
                        Ok(request) => transcribe_with_app(&app_for_task, request)
                            .await
                            .map_err(desktop_failure_from_transcription_error),
                        Err(failure) => Err(failure),
                    }
                }
                ActiveEngineSelection::Remote {
                    server_id,
                    server_name,
                    host,
                    port,
                    password,
                    ..
                } => {
                    async {
                        let remote_start = std::time::Instant::now();
                        log::info!(
                            "🌐 [Remote] Starting transcription to '{}' ({}:{})",
                            server_name,
                            host,
                            port
                        );

                        let audio_data = std::fs::read(&audio_path_clone).map_err(|e| {
                            TranscriptionFailure::local(format!("Failed to read audio file: {}", e))
                        })?;

                        let audio_size_kb = audio_data.len() as f64 / 1024.0;
                        log::info!(
                            "🌐 [Remote] Sending {:.1} KB audio to '{}' (+{}ms)",
                            audio_size_kb,
                            server_name,
                            remote_start.elapsed().as_millis()
                        );

                        let server_conn =
                            RemoteServerConnection::new(host.clone(), *port, password.clone());

                        let request_context =
                            crate::commands::remote::resolve_remote_request_context(
                                &app_for_task,
                                server_id,
                                transcription_job_for_task.spoken_language.as_deref(),
                            )
                            .await;

                        let request = RemoteTranscriptionRequest::new(
                            audio_data,
                            RemoteTimeoutSource::LiveRecording,
                        )
                        .with_language_and_task(
                            transcription_job_for_task.spoken_language.clone(),
                            Some(transcription_task_header_value(
                                transcription_job_for_task.task,
                            )),
                        )
                        .with_context(request_context);
                        let timeout_ms = timeout_ms_for_wav_file(
                            audio_path_clone.to_string_lossy().as_ref(),
                            RemoteTimeoutSource::LiveRecording,
                        );
                        match client::transcribe_audio(&server_conn, request, timeout_ms).await {
                            Ok(response) => {
                                log::info!(
                                "🌐 [Remote] Transcription COMPLETED from '{}': {} chars received",
                                server_name,
                                response.text.len()
                            );
                                Ok(build_remote_transcription_result(
                                    &transcription_job_for_task,
                                    response,
                                ))
                            }
                            Err(error) => {
                                log::warn!(
                                "🌐 [Remote] Remote transcription FAILED to '{}' after {}ms: {}",
                                server_name,
                                remote_start.elapsed().as_millis(),
                                error
                            );
                                Err(TranscriptionFailure::Remote(error))
                            }
                        }
                    }
                    .await
                }
            };
        // Plan 060: terminal decode failures become alertable GlitchTip
        // events (fixed class-suffixed message + closed-vocabulary tags).
        // Cancelled dictations are user intent, not failures — never sent.
        // The PostHog decode journey records success/failure/cancel.
        {
            let cancelled = match &transcription_result {
                Err(TranscriptionFailure::Local { message, .. }) => {
                    message.contains("cancelled") || message.contains("Cancelled")
                }
                _ => false,
            };
            if !cancelled {
                decode_journey.set_outcome(transcription_result.is_ok());
            }
            if let Err(failure) = &transcription_result {
                if !cancelled {
                    let backend = if matches!(
                        engine_selection_for_task,
                        ActiveEngineSelection::Whisper { .. }
                    ) {
                        crate::whisper::transcriber::attempt_backend()
                    } else {
                        None
                    };
                    crate::telemetry::capture_transcription_failure(
                        engine_kind_label(&engine_selection_for_task),
                        &engine_model_label(&engine_selection_for_task),
                        backend,
                        &transcription_failure_class(failure),
                        Some(decode_started.elapsed().as_millis() as u64),
                    );
                }
            }
        }
        drop(decode_journey);

        speech_evidence_attempt.set_outcome(if transcription_result.is_ok() {
            SpeechEvidenceOutcome::EngineSuccess
        } else {
            SpeechEvidenceOutcome::EngineFailure
        });

        // Decide persistence BEFORE touching the file. PRIVACY: a cancelled
        // dictation — or one whose recording generation has gone stale (a newer
        // recording started beneath this task) — is never written to disk, even
        // when the transcription succeeded or failed with a normally-saveable
        // (retryable) failure. This is the PRE-copy snapshot: it reads the
        // flags before the (synchronous) copy so a cancel already in effect
        // skips the write.
        let pre_discard =
            app_state.is_cancellation_requested() || recording_generation_is_stale(task_generation);
        let mut recording_file =
            if should_save_recording_audio(pre_discard, transcription_result.as_ref().err()) {
                maybe_save_recording_if_current(&app_for_task, task_generation, &audio_path_clone)
                    .await
            } else {
                None
            };

        // POST-COPY RECHECK (Race 2): a cancel/newer-generation arriving
        // DURING the copy slipped past the `pre_discard` snapshot. The delivery
        // gate below catches it and discards the text, but the audio was
        // already persisted — revoke it now so a cancelled dictation is never
        // left on disk, and drop the filename so history does not reference a
        // file we just deleted.
        if delivery_aborted(app_state.is_cancellation_requested(), task_generation) && !pre_discard
        {
            if let Some(ref saved) = recording_file {
                revoke_saved_recording(&app_for_task, saved).await;
            }
            recording_file = None;
        }

        // Clean up the task-owned temp recording and release the in-flight
        // tracker slot regardless of outcome (so a concurrent cancel cannot
        // resurrect a removed path). Shared with the early-cancel branch.
        finalize_in_flight_audio(task_generation, &audio_path_clone);

        match transcription_result {
            Ok(transcription) => {
                // Final gate before delivering a result: reject if the user
                // cancelled, OR if a newer recording started beneath this task
                // (its generation advanced). The generation check is load-
                // bearing: `start_recording` clears the cancellation flag for
                // its own attempt, so the flag alone would let a stale prior-
                // generation result slip through and paste during a newer
                // recording.
                let cancelled = app_state.is_cancellation_requested();
                let stale = recording_generation_is_stale(task_generation);
                if cancelled || stale {
                    log::info!(
                        "Transcription result discarded (cancelled={}, stale_generation={})",
                        cancelled,
                        stale
                    );
                    // Revoke any audio saved for this result. The post-copy
                    // recheck above already handles a cancel that arrived DURING
                    // the copy; this closes the residual window where a cancel
                    // arrives between that recheck and this gate (the audio was
                    // persisted and would otherwise be left on disk).
                    if let Some(ref saved) = recording_file {
                        revoke_saved_recording(&app_for_task, saved).await;
                    }

                    // Hide pill window since we're discarding (only if show_pill_indicator is false)
                    if should_hide_pill(&app_for_task).await {
                        if let Err(e) =
                            crate::commands::window::hide_pill_widget(app_for_task.clone()).await
                        {
                            log::error!("Failed to hide pill window on discard: {}", e);
                        }
                    }

                    update_recording_state(&app_for_task, RecordingState::Idle, None);
                    return;
                }

                log::debug!(
                    "Transcription successful, {} chars",
                    transcription.raw_text.len()
                );

                // Check if transcription is empty or just noise
                if is_non_speech_transcript(&transcription.raw_text) {
                    log::info!("Whisper returned empty transcription - no speech detected");

                    // Emit graceful feedback to user via pill toast
                    pill_toast_with_suggestion(
                        &app_for_task,
                        "No speech detected",
                        "Try speaking closer to the microphone",
                        1500,
                        None,
                    );

                    // Wait for feedback to show before hiding pill
                    let app_for_hide = app_for_task.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

                        // Hide pill window (only if show_pill_indicator is false)
                        if should_hide_pill(&app_for_hide).await {
                            if let Err(e) =
                                crate::commands::window::hide_pill_widget(app_for_hide.clone())
                                    .await
                            {
                                log::error!("Failed to hide pill window: {}", e);
                            }
                        }

                        // Transition back to Idle
                        update_recording_state(&app_for_hide, RecordingState::Idle, None);
                    });

                    return;
                }

                let should_emit_enhancing =
                    crate::writing::effective_pipeline_config(&app_for_task)
                        .map(|config| config.preset.requires_ai_formatting() && config.ai_effective)
                        .unwrap_or(false);

                if should_emit_enhancing {
                    let _ = app_for_task.emit("enhancing-started", ());
                }

                // Backend handles the complete flow
                let app_for_process = app_for_task.clone();
                let text_for_process = transcription.raw_text.clone();
                let model_for_process = transcription.model.clone();
                let transcription_for_process = transcription.clone();
                let should_emit_enhancing_for_task = should_emit_enhancing;
                let recording_file_for_task = recording_file.clone();

                (async move {
                    let formatting_started = Instant::now();

                    // 1. Process the transcription and enhancement
                    let (final_text, mut writing_metadata, should_deliver, writing_succeeded) =
                        match crate::writing::process_transcription(
                            app_for_process.clone(),
                            transcription_for_process.clone(),
                        )
                        .await
                        {
                            Ok(writing_result) => {
                                let writing_succeeded = writing_result.ai_error.is_none();
                                if let Some(error) = writing_result.ai_error.as_ref() {
                                    log::warn!(
                                        "AI polish failed with {}; delivering deterministic text",
                                        ai_failure_category(error)
                                    );
                                    if should_emit_enhancing_for_task {
                                        emit_enhancing_failed(&app_for_process, error);
                                    }
                                    pill_toast_with_variant(
                                        &app_for_process,
                                        ai_failure_notice(error),
                                        1500,
                                        PillToastVariant::Warning,
                                    );
                                    if is_ai_auth_error(error) {
                                        let _ = emit_to_window(
                                            &app_for_process,
                                            "main",
                                            "ai-enhancement-auth-error",
                                            "Please check your AI API key in settings.",
                                        );
                                    }
                                } else if should_emit_enhancing_for_task {
                                    let _ = app_for_process.emit("enhancing-completed", ());
                                }

                                if writing_result.ai_applied {
                                    log::info!("AI enhancement applied successfully");
                                } else if !writing_result.polish_enabled {
                                    log::debug!("AI enhancement is disabled, using original text");
                                }
                                let polish_outcome = classify_polish_outcome(
                                    writing_result.polish_enabled,
                                    writing_result.ai_error.is_some(),
                                    writing_result.ai_applied,
                                    writing_result.mode,
                                    writing_result.ai_execution.is_some(),
                                );
                                let (provider_id, model_id) = writing_result
                                    .ai_execution
                                    .as_ref()
                                    .map(|execution| {
                                        (
                                            execution.provider_id.clone(),
                                            execution.model_id.clone(),
                                        )
                                    })
                                    .unwrap_or_default();
                                crate::product_analytics::capture(
                                    crate::product_analytics::ProductEvent::PolishFinished {
                                        outcome: polish_outcome,
                                        preset: writing_result.mode.into(),
                                        provider_id,
                                        model_id,
                                    },
                                );
                                let plan = plan_desktop_writing_success(
                                    &transcription_for_process,
                                    &writing_result,
                                );
                                debug_assert_eq!(plan.save_history_entries, 1);
                                (
                                    plan.final_text,
                                    plan.writing_metadata,
                                    plan.should_deliver,
                                    writing_succeeded,
                                )
                            }
                            Err(crate::writing::WritingError::TranslationFailed {
                                target_language,
                                ..
                            }) => {
                                log::warn!("Translation failed after transcription; saving raw transcript to history without delivery");
                                if should_emit_enhancing_for_task {
                                    let _ = app_for_process.emit("enhancing-failed", ());
                                }

                                let saved = save_transcription_with_recording_if_current(
                                    app_for_process.clone(),
                                    task_generation,
                                    transcription_for_process.raw_text.clone(),
                                    model_for_process.clone(),
                                    recording_file_for_task.clone(),
                                    Some(build_translation_failed_history_metadata(
                                        &target_language,
                                    )),
                                )
                                .await;

                                match saved {
                                    None => {
                                        log::info!(
                                            "Skipped translation-failed history for stale/cancelled generation {}",
                                            task_generation
                                        );
                                    }
                                    Some(Err(save_err)) => {
                                        // History save failed: fall back to clipboard so the
                                        // transcript is never lost (the old path always pasted).
                                        log::error!(
                                            "Failed to save raw transcript after translation failure: {}; copying to clipboard",
                                            save_err
                                        );
                                        let app_state = app_for_process.state::<AppState>();
                                        let copy_result =
                                            persist_if_current(&app_state, task_generation, || {
                                                crate::commands::text::copy_text_to_clipboard(
                                                    transcription_for_process.raw_text.clone(),
                                                )
                                            });
                                        let message = match copy_result {
                                            None => {
                                                "Translation failed - cancelled before clipboard fallback"
                                            }
                                            Some(copy_future) => match copy_future.await {
                                                Ok(_) => "Translation failed - copied to clipboard",
                                                Err(copy_err) => {
                                                    log::error!(
                                                        "Clipboard fallback also failed after translation failure: {}",
                                                        copy_err
                                                    );
                                                    "Translation failed - transcript could not be saved"
                                                }
                                            },
                                        };
                                        pill_toast(&app_for_process, message, 6000);
                                    }
                                    Some(Ok(())) => {
                                        pill_toast(
                                            &app_for_process,
                                            "Translation failed - saved to history, not pasted",
                                            6000,
                                        );
                                    }
                                }

                                (text_for_process.clone(), None, false, false)
                            }
                            Err(crate::writing::WritingError::OutputLanguageRequiresAi) => {
                                log::warn!("Formatting failed: Final output language requires AI enhancement or native translation");
                                if should_emit_enhancing_for_task {
                                    let _ = app_for_process.emit("enhancing-failed", ());
                                }

                                pill_toast(
                                    &app_for_process,
                                    "Final output language requires AI enhancement",
                                    1500,
                                );

                                (text_for_process.clone(), None, false, false)
                            }
                            Err(crate::writing::WritingError::Config(e)) => {
                                log::warn!("Formatting failed: {}", e);
                                if should_emit_enhancing_for_task {
                                    let _ = app_for_process.emit("enhancing-failed", ());
                                }

                                pill_toast(&app_for_process, "Formatting failed", 1500);

                                (text_for_process.clone(), None, false, false)
                            }
                        };
                    // PostHog formatting journey (telemetry funnel removed, plan 047).
                    crate::product_analytics::capture(crate::product_analytics::ProductEvent::StageFinished {
                        stage: crate::product_analytics::JourneyStage::Formatting,
                        outcome: if writing_succeeded {
                            crate::product_analytics::JourneyOutcome::Succeeded
                        } else {
                            crate::product_analytics::JourneyOutcome::Failed
                        },
                        duration_ms: formatting_started.elapsed().as_millis() as u64,
                        engine: None,
                    });

                    // 2. Hide pill window first, then insert text with reduced delay
                    let app_state = app_for_process.state::<AppState>();
                    // Recheck (Race 3) after process_transcription: a cancel /
                    // newer-generation arriving during the (long) AI-polish
                    // await is invisible to the outer task's pre-delivery gate,
                    // which already passed. Abort before ANY side effect: no
                    // pill toast, no text insertion, no history; revoke audio.
                    if delivery_aborted(app_state.is_cancellation_requested(), task_generation) {
                        log::info!(
                            "Delivery discarded after enhancement (cancelled/stale gen={})",
                            task_generation
                        );
                        if let Some(ref saved) = recording_file_for_task {
                            revoke_saved_recording(&app_for_process, saved).await;
                        }
                        if should_hide_pill(&app_for_process).await {
                            if let Err(e) =
                                crate::commands::window::hide_pill_widget(app_for_process.clone())
                                    .await
                            {
                                log::error!(
                                    "Failed to hide pill window on discarded delivery: {}",
                                    e
                                );
                            }
                        }
                        update_recording_state(&app_for_process, RecordingState::Idle, None);
                        return;
                    }

                    // Hide pill window first (only if show_pill_indicator is false)
                    if should_hide_pill(&app_for_process).await {
                        if let Some(window_manager) = app_state.get_window_manager() {
                            if let Err(e) = window_manager.hide_pill_window().await {
                                log::error!("Failed to hide pill window: {}", e);
                            }
                        } else {
                            log::error!("WindowManager not initialized");
                        }
                    }

                    // Reduced delay to ensure focus stability after pill hide (was 50ms)
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

                    if !should_deliver {
                        update_recording_state(&app_for_process, RecordingState::Idle, None);
                        return;
                    }

                    let mut delivery_journey = DeliveryJourneyGuard::new();

                    // Now handle text insertion or clipboard copy based on auto_paste_transcription.
                    // Missing setting keys default inside get_settings; actual settings-read failures fail closed
                    // to avoid surprising paste into the wrong app.
                    let auto_paste = match get_settings(app_for_process.clone()).await {
                        Ok(settings) => settings.auto_paste_transcription,
                        Err(error) => {
                            log::error!("Failed to read auto-paste setting: {}", error);
                            false
                        }
                    };
                    // Recheck (Race 3) IMMEDIATELY before text insertion: a
                    // cancel arriving during the pill-hide / sleep / settings-
                    // read window above must not paste stale/cancelled text.
                    if delivery_aborted(app_state.is_cancellation_requested(), task_generation) {
                        log::info!(
                            "Delivery discarded before insertion (cancelled/stale gen={})",
                            task_generation
                        );
                        if let Some(ref saved) = recording_file_for_task {
                            revoke_saved_recording(&app_for_process, saved).await;
                        }
                        update_recording_state(&app_for_process, RecordingState::Idle, None);
                        return;
                    }

                    if transcript_ready_cue_eligible(writing_succeeded, should_deliver) {
                        let cue_committed = persist_if_current(&app_state, task_generation, || {
                            crate::commands::audio_feedback::play_audio_feedback(
                                &app_for_process,
                                crate::commands::audio_feedback::AudioFeedbackCue::TranscriptReady,
                            );
                        });
                        if cue_committed.is_none() {
                            log::info!(
                                "Skipped transcript-ready cue for stale/cancelled generation {}",
                                task_generation
                            );
                            if let Some(saved) = &recording_file_for_task {
                                revoke_saved_recording(&app_for_process, saved).await;
                            }
                            update_recording_state(&app_for_process, RecordingState::Idle, None);
                            return;
                        }
                    }

                    if auto_paste {
                        // Auto-paste enabled: insert text at cursor
                        let insert_result = persist_if_current(&app_state, task_generation, || {
                            crate::commands::text::insert_text(
                                app_for_process.clone(),
                                final_text.clone(),
                            )
                        });
                        let Some(insert_future) = insert_result else {
                            log::info!(
                                "Skipped text insertion for stale/cancelled generation {}",
                                task_generation
                            );
                            if let Some(ref saved) = recording_file_for_task {
                                revoke_saved_recording(&app_for_process, saved).await;
                            }
                            update_recording_state(&app_for_process, RecordingState::Idle, None);
                            return;
                        };
                        let insertion_start = Instant::now();
                        match insert_future.await {
                            Ok(_) => {
                                delivery_journey.mark_succeeded();
                                log::debug!("Text inserted at cursor successfully");
                            }
                            Err(e) => {
                                delivery_journey.mark_failed();
                                log::error!("Failed to insert text: {}", e);
                                crate::telemetry::capture_paste_failure("insert");


                                // Check if it's an accessibility permission issue
                                if e.contains("accessibility") || e.contains("permission") {
                                    // Show pill toast for accessibility permission error
                                    pill_toast_with_suggestion(
                                        &app_for_process,
                                        "Text copied",
                                        "Grant Accessibility permission to enable auto-paste",
                                        1500,
                                        None,
                                    );
                                } else {
                                    // Generic paste error
                                    pill_toast_with_suggestion(
                                        &app_for_process,
                                        "Text copied",
                                        "Grant Accessibility permission to enable auto-paste",
                                        1500,
                                        None,
                                    );
                                }
                            }
                        }
                        let insertion_ms = insertion_start
                            .elapsed()
                            .as_millis()
                            .min(u128::from(u64::MAX))
                            as u64;
                        log::info!(
                            "transcription_stage_timing stage=insertion method=auto_paste duration_ms={}",
                            insertion_ms
                        );
                        record_insertion_timing(&mut writing_metadata, insertion_ms);
                    } else {
                        // Auto-paste disabled: copy to clipboard and notify
                        let copy_result = persist_if_current(&app_state, task_generation, || {
                            crate::commands::text::copy_text_to_clipboard(final_text.clone())
                        });
                        let Some(copy_future) = copy_result else {
                            log::info!(
                                "Skipped clipboard copy for stale/cancelled generation {}",
                                task_generation
                            );
                            if let Some(ref saved) = recording_file_for_task {
                                revoke_saved_recording(&app_for_process, saved).await;
                            }
                            update_recording_state(&app_for_process, RecordingState::Idle, None);
                            return;
                        };
                        let insertion_start = Instant::now();
                        match copy_future.await {
                            Ok(_) => {
                                delivery_journey.mark_succeeded();
                                log::debug!("Text copied to clipboard (auto-paste disabled)");
                                pill_toast(&app_for_process, "Transcription copied", 1500);
                            }
                            Err(e) => {
                                delivery_journey.mark_failed();
                                log::error!("Failed to copy text to clipboard: {}", e);
                                crate::telemetry::capture_paste_failure("clipboard");
                                pill_toast(&app_for_process, "Copy failed", 1500);
                            }
                        }
                        let insertion_ms = insertion_start
                            .elapsed()
                            .as_millis()
                            .min(u128::from(u64::MAX))
                            as u64;
                        log::info!(
                            "transcription_stage_timing stage=insertion method=clipboard duration_ms={}",
                            insertion_ms
                        );
                        record_insertion_timing(&mut writing_metadata, insertion_ms);
                    }

                    // Recheck (Race 3) IMMEDIATELY before history save: a cancel
                    // arriving during text insertion must not persist a history
                    // row (or reference a recording) for the cancelled/stale
                    // dictation. Revoke any saved audio too.
                    if delivery_aborted(app_state.is_cancellation_requested(), task_generation) {
                        log::info!(
                            "Delivery discarded before history save (cancelled/stale gen={})",
                            task_generation
                        );
                        if let Some(ref saved) = recording_file_for_task {
                            revoke_saved_recording(&app_for_process, saved).await;
                        }
                        update_recording_state(&app_for_process, RecordingState::Idle, None);
                        return;
                    }

                    // 5. Save transcription to history (async, non-blocking)
                    let app_for_history = app_for_process.clone();
                    let history_text = final_text.clone();
                    let history_model = model_for_process.clone();
                    let recording_file_for_history = recording_file_for_task.clone();
                    let writing_metadata_for_history = writing_metadata.clone();
                    let generation_for_history = task_generation;
                    tokio::spawn(async move {
                        match save_transcription_with_recording_if_current(
                            app_for_history.clone(),
                            generation_for_history,
                            history_text,
                            history_model,
                            recording_file_for_history,
                            writing_metadata_for_history,
                        )
                        .await
                        {
                            Some(Ok(())) => {
                                // Emit history-updated event to refresh UI
                                let _ =
                                    emit_to_window(&app_for_history, "main", "history-updated", ());
                                log::debug!("Transcription saved to history successfully");
                            }
                            Some(Err(e)) => {
                                log::error!("Failed to save transcription to history: {}", e)
                            }
                            None => log::info!(
                                "Skipped spawned history save for stale/cancelled generation {}",
                                generation_for_history
                            ),
                        }
                    });

                    // 6. Transition to idle state
                    update_recording_state(&app_for_process, RecordingState::Idle, None);
                })
                .await;
            }
            Err(failure) => {
                match &failure {
                    TranscriptionFailure::Local { message: e, .. }
                        if e.contains("cancelled") || e.contains("Cancelled") =>
                    {
                        log::info!("Handling transcription cancellation");
                        // For cancellation, hide pill (only if show_pill_indicator is false) and go to Idle
                        if should_hide_pill(&app_for_task).await {
                            if let Err(hide_err) =
                                crate::commands::window::hide_pill_widget(app_for_task.clone())
                                    .await
                            {
                                log::error!(
                                    "Failed to hide pill window on cancellation: {}",
                                    hide_err
                                );
                            }
                        }
                        update_recording_state(&app_for_task, RecordingState::Idle, None);
                    }
                    TranscriptionFailure::Local { message: e, .. } if e.contains("too short") => {
                        // Handle "too short" errors with specific user feedback
                        log::info!("Recording was too short: {}", e);

                        // Clean up the audio file
                        if let Err(cleanup_err) = std::fs::remove_file(&audio_path_clone) {
                            log::warn!("Failed to remove short audio file: {}", cleanup_err);
                        }

                        // Emit specific feedback via pill toast
                        pill_toast(&app_for_task, "Recording too short", 1000);

                        // Hide pill after showing feedback
                        let app_for_reset = app_for_task.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

                            // Only hide if show_pill_indicator is false
                            if should_hide_pill(&app_for_reset).await {
                                if let Err(e) =
                                    crate::commands::window::hide_pill_widget(app_for_reset.clone())
                                        .await
                                {
                                    log::error!("Failed to hide pill window: {}", e);
                                }
                            }

                            update_recording_state(&app_for_reset, RecordingState::Idle, None);
                        });
                    }
                    TranscriptionFailure::Remote(remote_error) => {
                        // Remote server error - emit specific event for system notification
                        log::warn!("Remote server error: {}", remote_error);

                        let can_retry_from_history =
                            if let Some(ref saved_recording) = recording_file {
                                let app_for_history = app_for_task.clone();
                                let model_name = selected_model_name_for_task.clone();
                                let recording_filename = saved_recording.clone();
                                match save_failed_transcription_if_current(
                                    &app_for_history,
                                    task_generation,
                                    &failure,
                                    model_name,
                                    recording_filename,
                                )
                                .await
                                {
                                    Some(Ok(())) => true,
                                    Some(Err(save_err)) => {
                                        log::error!(
                                            "Failed to save failed transcription: {}",
                                            save_err
                                        );
                                        false
                                    }
                                    None => false,
                                }
                            } else {
                                false
                            };

                        // Emit event for frontend to show system notification with guidance
                        let _ = app_for_task.emit(
                            "remote-server-error",
                            build_remote_server_error_payload(&failure, can_retry_from_history),
                        );

                        // Update pill message to guide user to History only when retry is durable
                        pill_toast(
                            &app_for_task,
                            remote_server_error_pill_message(can_retry_from_history),
                            if can_retry_from_history { 6000 } else { 2000 },
                        );

                        update_recording_state(
                            &app_for_task,
                            RecordingState::Error,
                            Some(failure.message()),
                        );

                        // Transition back to Idle after showing the error
                        let app_for_reset = app_for_task.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            if should_hide_pill(&app_for_reset).await {
                                if let Err(e) =
                                    crate::commands::window::hide_pill_widget(app_for_reset.clone())
                                        .await
                                {
                                    log::error!("Failed to hide pill window: {}", e);
                                }
                            }
                            update_recording_state(&app_for_reset, RecordingState::Idle, None);
                        });
                    }
                    TranscriptionFailure::Local { message: e, .. } => {
                        // Genuine local/cloud failure. If the recording was preserved
                        // (save_recordings on), write a retryable failed row so the user
                        // can re-transcribe from History instead of losing the dictation.
                        let can_retry_from_history =
                            if let Some(ref saved_recording) = recording_file {
                                match save_failed_transcription_if_current(
                                    &app_for_task,
                                    task_generation,
                                    &failure,
                                    selected_model_name_for_task.clone(),
                                    saved_recording.clone(),
                                )
                                .await
                                {
                                    Some(Ok(())) => true,
                                    Some(Err(save_err)) => {
                                        log::error!(
                                            "Failed to save failed transcription: {}",
                                            save_err
                                        );
                                        false
                                    }
                                    None => false,
                                }
                            } else {
                                false
                            };

                        update_recording_state(
                            &app_for_task,
                            RecordingState::Error,
                            Some(e.clone()),
                        );

                        // Log the full internal detail before any toast so nothing is lost.
                        log::warn!("Local transcription failure: {}", e);

                        if can_retry_from_history {
                            pill_toast(
                                &app_for_task,
                                "Transcription failed. Go to History to re-transcribe, or try again.",
                                6000,
                            );
                        } else {
                            match classify_local_failure(e) {
                                LocalFailureKind::AuthInvalid => {
                                    pill_toast_with_suggestion(
                                        &app_for_task,
                                        "Transcription key rejected",
                                        "Update the API key in Models",
                                        4000,
                                        Some(PillToastVariant::Warning),
                                    );
                                }
                                LocalFailureKind::ModelUnavailable => {
                                    pill_toast_with_suggestion(
                                        &app_for_task,
                                        "Transcription model unavailable",
                                        "Select a different model in Models",
                                        4000,
                                        None,
                                    );
                                }
                                LocalFailureKind::Generic => {
                                    pill_toast(
                                        &app_for_task,
                                        "Transcription failed — try again",
                                        1500,
                                    );
                                }
                            }
                        }

                        // Transition back to Idle after a delay so we don't get stuck.
                        let app_for_reset = app_for_task.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            log::debug!(
                                "Resetting from Error to Idle state after transcription failure"
                            );
                            if should_hide_pill(&app_for_reset).await {
                                if let Err(e) =
                                    crate::commands::window::hide_pill_widget(app_for_reset.clone())
                                        .await
                                {
                                    log::error!("Failed to hide pill window: {}", e);
                                }
                            }
                            update_recording_state(&app_for_reset, RecordingState::Idle, None);
                        });
                    }
                }
            }
        }
            },
        ),
    );

    // Track the transcription task
    let app_state = app.state::<AppState>();
    if let Ok(mut task_guard) = app_state.transcription_task.lock() {
        // Cancel any existing task
        if let Some(existing_task) = task_guard.take() {
            existing_task.abort();
        }
        // Store the new task handle
        *task_guard = Some(task_handle);
    }

    // Return immediately so front-end promise resolves before timeout
    Ok(String::new())
}

/// Get available audio input devices.
/// Returns empty list if onboarding not completed (to avoid triggering permission prompt).
#[tauri::command]
pub async fn get_audio_devices(app: AppHandle) -> Result<Vec<String>, String> {
    // Check onboarding status - don't enumerate devices until onboarding is complete
    // This prevents early mic permission prompts from CPAL's input_devices() enumeration
    let onboarding_done = {
        use tauri_plugin_store::StoreExt;
        app.store("settings")
            .ok()
            .and_then(|store| store.get("onboarding_completed").and_then(|v| v.as_bool()))
            .unwrap_or(false)
    };

    if !onboarding_done {
        log::debug!("get_audio_devices: onboarding not complete, returning empty list");
        return Ok(Vec::new());
    }

    Ok(AudioRecorder::get_devices())
}

/// Get the current default audio input device.
/// Returns error if onboarding not completed (to avoid triggering permission prompt).
#[tauri::command]
pub async fn get_current_audio_device(app: AppHandle) -> Result<String, String> {
    // Check onboarding status - don't access devices until onboarding is complete
    // This prevents early mic permission prompts from CPAL's default_input_device() access
    let onboarding_done = {
        use tauri_plugin_store::StoreExt;
        app.store("settings")
            .ok()
            .and_then(|store| store.get("onboarding_completed").and_then(|v| v.as_bool()))
            .unwrap_or(false)
    };

    if !onboarding_done {
        log::debug!("get_current_audio_device: onboarding not complete, returning error");
        return Err("Onboarding not completed".to_string());
    }

    let host = cpal::default_host();

    host.default_input_device()
        .and_then(|device| device.name().ok())
        .ok_or_else(|| "No default input device found".to_string())
}

#[tauri::command]
pub async fn cleanup_old_transcriptions(app: AppHandle, days: Option<u32>) -> Result<(), String> {
    if let Some(days) = days {
        let store = app.store("transcriptions").map_err(|e| e.to_string())?;

        let cutoff_date = chrono::Utc::now() - chrono::Duration::days(days as i64);

        // Get all keys
        let keys: Vec<String> = store.keys().into_iter().map(|k| k.to_string()).collect();

        // Remove old entries
        for key in keys {
            if let Ok(date) = chrono::DateTime::parse_from_rfc3339(&key) {
                if date < cutoff_date {
                    store.delete(&key);
                }
            }
        }

        store.save().map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Save transcription to history without a recording file
#[tauri::command]
pub async fn save_transcription(
    app: AppHandle,
    text: String,
    model: String,
    metadata: Option<serde_json::Value>,
) -> Result<(), String> {
    save_transcription_with_recording(app, text, model, None, metadata).await
}

/// Save transcription to history with optional recording file reference
pub async fn save_transcription_with_recording(
    app: AppHandle,
    text: String,
    model: String,
    recording_file: Option<String>,
    writing_metadata: Option<serde_json::Value>,
) -> Result<(), String> {
    save_transcription_with_recording_internal(
        app,
        text,
        model,
        recording_file,
        writing_metadata,
        None,
    )
    .await
    .unwrap_or(Ok(()))
}

async fn save_transcription_with_recording_if_current(
    app: AppHandle,
    generation: u64,
    text: String,
    model: String,
    recording_file: Option<String>,
    writing_metadata: Option<serde_json::Value>,
) -> Option<Result<(), String>> {
    save_transcription_with_recording_internal(
        app,
        text,
        model,
        recording_file,
        writing_metadata,
        Some(generation),
    )
    .await
}

async fn save_transcription_with_recording_internal(
    app: AppHandle,
    text: String,
    model: String,
    recording_file: Option<String>,
    writing_metadata: Option<serde_json::Value>,
    generation: Option<u64>,
) -> Option<Result<(), String>> {
    // De-dup guard: skip saving if the most recent entry matches the same text & model within a short window
    if let Ok(store) = app.store("transcriptions") {
        let latest_key = page_history_keys(store.keys(), 1).into_iter().next();

        if let Some(key) = latest_key {
            if let Some(value) = store.get(&key) {
                if is_duplicate_transcription(&key, &value, &text, &model, chrono::Utc::now()) {
                    log::info!("Skipping duplicate transcription save (same text/model within 2s)");
                    return Some(Ok(()));
                }
            }
        }
    }

    // Save transcription to store with current timestamp
    let store = match app.store("transcriptions") {
        Ok(store) => store,
        Err(e) => return Some(Err(format!("Failed to get transcriptions store: {}", e))),
    };

    let timestamp = chrono::Utc::now().to_rfc3339();
    let mut transcription_data = serde_json::json!({
        "text": text.clone(),
        "model": model,
        "timestamp": timestamp.clone()
    });

    // Add recording_file if present
    if let Some(ref file) = recording_file {
        transcription_data["recording_file"] = serde_json::json!(file);
        log::info!("Saving transcription with recording file: {}", file);
    }
    if let Some(metadata) = writing_metadata {
        transcription_data["writing"] = metadata;
    }

    let commit_result = match generation {
        Some(generation) => {
            let app_state = app.state::<AppState>();
            persist_if_current(&app_state, generation, || {
                store.set(&timestamp, transcription_data.clone());
                store
                    .save()
                    .map_err(|e| format!("Failed to save transcription: {}", e))
            })
        }
        None => Some({
            store.set(&timestamp, transcription_data.clone());
            store
                .save()
                .map_err(|e| format!("Failed to save transcription: {}", e))
        }),
    };

    match commit_result {
        None => {
            log::info!(
                "Skipped transcription history save for stale/cancelled generation {}",
                generation.unwrap_or_default()
            );
            None
        }
        Some(Err(e)) => Some(Err(e)),
        Some(Ok(())) => {
            // Emit the new transcription data to frontend for append-only update
            let _ = emit_to_window(&app, "main", "transcription-added", transcription_data);

            // Refresh tray menu (best-effort) so Recent Transcriptions stays updated
            if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
                log::warn!(
                    "Failed to update tray menu after saving transcription: {}",
                    e
                );
            }

            log::info!("Saved transcription with {} characters", text.len());
            Some(Ok(()))
        }
    }
}

async fn save_failed_transcription_if_current(
    app: &AppHandle,
    generation: u64,
    failure: &TranscriptionFailure,
    model: String,
    recording_file: String,
) -> Option<Result<(), String>> {
    save_failed_transcription_internal(app, Some(generation), failure, model, recording_file).await
}

async fn save_failed_transcription_internal(
    app: &AppHandle,
    generation: Option<u64>,
    failure: &TranscriptionFailure,
    model: String,
    recording_file: String,
) -> Option<Result<(), String>> {
    let store = match app.store("transcriptions") {
        Ok(store) => store,
        Err(e) => return Some(Err(format!("Failed to get transcriptions store: {}", e))),
    };

    let transcription_data = build_failed_transcription_row(failure, &model, &recording_file);
    let timestamp = match transcription_data["timestamp"].as_str() {
        Some(timestamp) => timestamp.to_string(),
        None => {
            return Some(Err(
                "Failed to build failed transcription timestamp".to_string()
            ))
        }
    };

    let commit_result = match generation {
        Some(generation) => {
            let app_state = app.state::<AppState>();
            persist_if_current(&app_state, generation, || {
                store.set(&timestamp, transcription_data.clone());
                store
                    .save()
                    .map_err(|e| format!("Failed to save failed transcription: {}", e))
            })
        }
        None => Some({
            store.set(&timestamp, transcription_data.clone());
            store
                .save()
                .map_err(|e| format!("Failed to save failed transcription: {}", e))
        }),
    };

    match commit_result {
        None => {
            log::info!(
                "Skipped failed-transcription history save for stale/cancelled generation {}",
                generation.unwrap_or_default()
            );
            None
        }
        Some(Err(e)) => Some(Err(e)),
        Some(Ok(())) => {
            // Emit the new transcription data to frontend
            let _ = emit_to_window(app, "main", "transcription-added", transcription_data);

            // Refresh tray menu
            if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
                log::warn!(
                    "Failed to update tray menu after saving failed transcription: {}",
                    e
                );
            }

            log::info!(
                "Saved failed transcription with recording file: {}",
                recording_file
            );
            Some(Ok(()))
        }
    }
}

#[tauri::command]
pub async fn get_transcription_history(
    app: AppHandle,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let store = app.store("transcriptions").map_err(|e| e.to_string())?;
    let current_session_marker = current_retranscription_session_marker();

    let limit = limit.unwrap_or(50);
    let keys = page_history_keys(store.keys(), limit);

    let mut entries: Vec<serde_json::Value> = Vec::with_capacity(limit.min(keys.len()));
    let mut pending_updates: Vec<(String, serde_json::Value)> = Vec::new();

    // Reconcile only the requested page; stale rows beyond this page are handled lazily.
    for key in keys {
        if let Some(value) = store.get(&key) {
            let reconciled =
                reconcile_transcription_history_entry(value.clone(), &current_session_marker);
            if reconciled != value {
                pending_updates.push((key, reconciled.clone()));
            }
            entries.push(reconciled);
        }
    }

    if !pending_updates.is_empty() {
        for (key, value) in pending_updates {
            store.set(&key, value);
        }

        store
            .save()
            .map_err(|e| format!("Failed to save reconciled transcription history: {}", e))?;

        if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
            log::warn!(
                "Failed to update tray menu after reconciling transcription history: {}",
                e
            );
        }
    }

    Ok(entries)
}

/// Get the total count of transcriptions in history
/// This is more efficient than loading all history when only the count is needed
#[tauri::command]
pub async fn get_transcription_count(app: AppHandle) -> Result<usize, String> {
    let store = app.store("transcriptions").map_err(|e| e.to_string())?;
    Ok(store.keys().len())
}

#[tauri::command]
pub async fn transcribe_audio_file(
    app: AppHandle,
    file_path: String,
    model_name: String,
    model_engine: Option<String>,
) -> Result<UploadTranscription, String> {
    transcribe_audio_file_impl(app, file_path, model_name, model_engine, true).await
}

pub async fn transcribe_audio_file_for_cli(
    app: AppHandle,
    file_path: String,
    model_name: String,
    model_engine: Option<String>,
) -> Result<UploadTranscription, String> {
    transcribe_audio_file_impl(app, file_path, model_name, model_engine, false).await
}

async fn transcribe_audio_file_impl(
    app: AppHandle,
    file_path: String,
    model_name: String,
    model_engine: Option<String>,
    validate_requirements: bool,
) -> Result<UploadTranscription, String> {
    log::info!(
        "[UPLOAD] transcribe_audio_file START | file_path={:?}, model_name={}, engine_hint={:?}",
        file_path,
        model_name,
        model_engine
    );
    if validate_requirements {
        validate_recording_requirements(&app).await?;
    }

    // Use the provided file path directly
    let audio_path = std::path::Path::new(&file_path);

    // Validate file exists
    if !audio_path.exists() {
        return Err(format!("Audio file not found: {}", file_path));
    }

    // Convert to WAV if needed
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");

    std::fs::create_dir_all(&recordings_dir)
        .map_err(|e| format!("Failed to create recordings directory: {}", e))?;

    // No pre-conversion needed; ffmpeg normalizer can read most formats directly.
    let wav_path = audio_path.to_path_buf();
    log::info!("[UPLOAD] Input ready at {:?}", wav_path);

    // Resolve engine (whisper/parakeet/cloud) for the requested model
    let engine_selection =
        resolve_engine_for_model(&app, &model_name, model_engine.as_deref()).await?;
    log::info!(
        "[UPLOAD] Engine resolved to: {}",
        engine_selection.engine_name()
    );

    // Get language and translation settings
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let legacy_speech_language = store
        .get("language")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "en".to_string());
    let legacy_translate_to_english = store
        .get("translate_to_english")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let language = store
        .get("speech_language")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or(legacy_speech_language);
    let stored_transcription_task = store
        .get("transcription_task")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let transcription_task = resolve_transcription_task_for_audio(
        &app,
        legacy_translate_to_english,
        stored_transcription_task.as_deref(),
    )?;
    let translate_to_english = task_uses_translate_to_english(&transcription_task);

    let language = normalize_speech_language_for_model(
        engine_selection.engine_name(),
        engine_selection.model_name(),
        &language,
    );

    log::info!(
        "[LANGUAGE] transcribe_audio_file using language: {}, transcription_task={}, translate: {}",
        language,
        transcription_task,
        translate_to_english
    );

    let transcription_job = build_transcription_job(
        TranscriptionSource::AudioFile,
        engine_selection.engine_name().to_string(),
        engine_selection.model_name().to_string(),
        Some(language.clone()),
        translate_to_english,
    );

    // For cloud providers, skip normalization and send original wav_path
    let transcription_result = match engine_selection {
        ActiveEngineSelection::Whisper { model_path, .. } => {
            // Normalize to Whisper contract
            log::debug!("[UPLOAD] Normalizing to Whisper WAV (16k mono s16)...");
            let normalized_file = NormalizedTempFile::new({
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &wav_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            });
            log::info!("[UPLOAD] Normalized WAV at {:?}", normalized_file.path());
            let initial_prompt = compile_whisper_initial_prompt(&app, Some(&language));
            let output = transcribe_whisper_with_acceleration(
                &app,
                &model_path,
                normalized_file.path(),
                Some(&language),
                translate_to_english,
                initial_prompt.as_deref(),
                || false,
            )
            .await?;
            TranscriptionResult::new(&transcription_job, output.raw_text)
                .with_transcript_language(output.transcript_language)
                .with_segments(output.segments)
                .with_audio_duration_ms(Some(output.audio_duration_ms))
                .with_processing_duration_ms(Some(output.processing_duration_ms))
        }
        ActiveEngineSelection::Parakeet { model_name } => {
            // Normalize to Whisper/Parakeet contract first
            log::debug!("[UPLOAD] Normalizing to Whisper WAV (16k mono s16)...");
            let normalized_file = NormalizedTempFile::new({
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &wav_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            });
            log::info!("[UPLOAD] Normalized WAV at {:?}", normalized_file.path());
            let parakeet_manager = app.state::<ParakeetManager>();

            parakeet_manager
                .load_model(&app, &model_name)
                .await
                .map_err(|e| format!("Failed to load Parakeet model: {}", e))?;

            let custom_vocabulary =
                compile_parakeet_custom_vocabulary_for_transcription(&app, Some(&language));

            match parakeet_manager
                .transcribe_with_custom_vocabulary(
                    &app,
                    &model_name,
                    normalized_file.path().to_path_buf(),
                    ParakeetTranscriptionOptions {
                        language: Some(language.clone()),
                        translate: translate_to_english,
                        custom_vocabulary,
                        cancel_flag: None,
                    },
                )
                .await
            {
                Ok(ParakeetResponse::Transcription {
                    text,
                    segments,
                    language,
                    duration,
                }) => TranscriptionResult::new(&transcription_job, text)
                    .with_transcript_language(language)
                    .with_segments(parakeet_segments_to_transcription_segments(segments))
                    .with_audio_duration_ms(seconds_to_duration_ms(duration)),
                Ok(other) => {
                    return Err(format!("Unexpected Parakeet response: {:?}", other));
                }
                Err(err) => {
                    return Err(format!("Parakeet transcription failed: {}", err));
                }
            }
        }
        ActiveEngineSelection::Cloud { provider, .. } => {
            log::debug!("[UPLOAD] Normalizing to WAV for cloud transcription...");
            let normalized_file = NormalizedTempFile::new({
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &wav_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            });
            let cloud_transcript = provider
                .transcribe_diarized(&app, normalized_file.path(), Some(&language))
                .await?;

            // If the provider returned speaker-attributed words, group them and
            // return directly — no AI polish for diarized uploads.
            if !cloud_transcript.words.is_empty() {
                let words = cloud_transcript.words;
                let text = group_words_into_speaker_text(&words);
                log::info!(
                    "[UPLOAD] Diarized cloud transcript: {} words, {} chars",
                    words.len(),
                    text.len()
                );
                let mut diarized_result =
                    TranscriptionResult::new(&transcription_job, text.clone());
                diarized_result.words = Some(words.clone());
                let metadata = Some(build_writing_history_metadata(&diarized_result, None));
                return Ok(UploadTranscription {
                    text,
                    words: Some(words),
                    metadata,
                });
            }

            let cloud_job = build_transcription_job(
                TranscriptionSource::AudioFile,
                transcription_job.engine.clone(),
                transcription_job.model.clone(),
                transcription_job.spoken_language.clone(),
                false,
            );
            TranscriptionResult::new(&cloud_job, cloud_transcript.text)
        }
        ActiveEngineSelection::Remote {
            server_id,
            server_name,
            host,
            port,
            password,
            ..
        } => {
            // Normalize to Whisper contract (16k mono s16 WAV) for remote transcription
            log::debug!("[UPLOAD] Normalizing to Whisper WAV for remote transcription...");
            let normalized_file = NormalizedTempFile::new({
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &wav_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            });
            log::info!("[UPLOAD] Normalized WAV at {:?}", normalized_file.path());

            log::info!(
                "🌐 [Remote Upload] Starting transcription to '{}' ({}:{})",
                server_name,
                host,
                port
            );

            // Read the normalized audio file
            let audio_data = std::fs::read(normalized_file.path())
                .map_err(|e| format!("Failed to read audio file: {}", e))?;

            let audio_size_kb = audio_data.len() as f64 / 1024.0;
            log::info!(
                "🌐 [Remote Upload] Sending {:.1} KB audio to '{}'",
                audio_size_kb,
                server_name
            );

            // Create HTTP client connection
            let server_conn = RemoteServerConnection::new(host.clone(), port, password.clone());

            let request_context = crate::commands::remote::resolve_remote_request_context(
                &app,
                &server_id,
                transcription_job.spoken_language.as_deref(),
            )
            .await;

            let (request, timeout_ms) = build_remote_upload_transcription_request(
                normalized_file.path(),
                audio_data,
                Some(&transcription_job),
                request_context,
            );

            let response = client::transcribe_audio(&server_conn, request, timeout_ms)
                .await
                .map_err(|e| {
                    log::warn!(
                        "🌐 [Remote Upload] Remote transcription FAILED to '{}': {}",
                        server_name,
                        e
                    );
                    e.to_string()
                })?;

            log::info!(
                "🌐 [Remote Upload] Transcription COMPLETED from '{}': {} chars received",
                server_name,
                response.text.len()
            );

            build_remote_transcription_result(&transcription_job, response)
        }
    };

    log::info!(
        "[UPLOAD] Completed transcription, {} characters",
        transcription_result.raw_text.len()
    );
    let writing_result =
        crate::writing::process_transcription(app.clone(), transcription_result.clone())
            .await
            .map_err(|e| e.user_message())?;
    if let Some(error) = writing_result.ai_error.as_ref() {
        log::warn!(
            "AI polish failed with {}; returning deterministic upload text",
            ai_failure_category(error)
        );
        notify_ai_polish_failure(&app, error);
        // Upload history is persisted by the frontend after this command returns non-blank text.
    }
    let metadata = Some(build_writing_history_metadata(
        &transcription_result,
        Some(&writing_result),
    ));
    Ok(UploadTranscription {
        text: writing_result.final_text,
        words: None,
        metadata,
    })
}

#[tauri::command]
pub async fn diarize_audio_file(
    app: AppHandle,
    file_path: String,
) -> Result<Vec<UploadDiarizationSegment>, String> {
    validate_recording_requirements(&app).await?;

    let audio_path = std::path::PathBuf::from(&file_path);
    if !audio_path.exists() {
        return Err(format!("Audio file not found: {}", file_path));
    }
    let parakeet_manager = app.state::<ParakeetManager>();
    match parakeet_manager
        .diarize(&app, audio_path)
        .await
        .map_err(|err| format!("Parakeet diarization failed: {}", err))?
    {
        ParakeetResponse::Diarization { segments } => Ok(segments
            .into_iter()
            .map(|segment| UploadDiarizationSegment {
                speaker_id: segment.speaker_id,
                start_ms: seconds_to_duration_ms(Some(segment.start)).unwrap_or_default(),
                end_ms: seconds_to_duration_ms(Some(segment.end)).unwrap_or_default(),
            })
            .collect()),
        other => Err(format!(
            "Unexpected Parakeet diarization response: {:?}",
            other
        )),
    }
}

#[tauri::command]
pub async fn transcribe_audio(
    app: AppHandle,
    audio_data: Vec<u8>,
    model_name: String,
    model_engine: Option<String>,
) -> Result<String, String> {
    log::info!(
        "[UPLOAD] transcribe_audio (bytes) START | bytes={}, model_name={}, engine_hint={:?}",
        audio_data.len(),
        model_name,
        model_engine
    );
    // Validate requirements (includes license check)
    validate_recording_requirements(&app).await?;

    // Save audio data to app data directory
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");

    // Ensure directory exists
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|e| format!("Failed to create recordings directory: {}", e))?;

    let temp_path = recordings_dir.join("temp_audio.wav");

    std::fs::write(&temp_path, audio_data).map_err(|e| e.to_string())?;

    let engine_selection =
        resolve_engine_for_model(&app, &model_name, model_engine.as_deref()).await?;

    // Get language and translation settings
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let legacy_speech_language = store
        .get("language")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "en".to_string());
    let legacy_translate_to_english = store
        .get("translate_to_english")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let language = store
        .get("speech_language")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or(legacy_speech_language);
    let stored_transcription_task = store
        .get("transcription_task")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let transcription_task = resolve_transcription_task_for_audio(
        &app,
        legacy_translate_to_english,
        stored_transcription_task.as_deref(),
    )?;
    let translate_to_english = task_uses_translate_to_english(&transcription_task);

    let language = normalize_speech_language_for_model(
        engine_selection.engine_name(),
        engine_selection.model_name(),
        &language,
    );

    log::info!(
        "[LANGUAGE] transcribe_audio using language: {}, transcription_task={}, translate: {}",
        language,
        transcription_task,
        translate_to_english
    );

    let transcription_job = build_transcription_job(
        TranscriptionSource::AudioBytes,
        engine_selection.engine_name().to_string(),
        engine_selection.model_name().to_string(),
        Some(language.clone()),
        translate_to_english,
    );

    let transcription_result = match engine_selection {
        ActiveEngineSelection::Whisper { model_path, .. } => {
            let initial_prompt = compile_whisper_initial_prompt(&app, Some(language.as_str()));
            let output = transcribe_whisper_with_acceleration(
                &app,
                &model_path,
                &temp_path,
                Some(language.as_str()),
                translate_to_english,
                initial_prompt.as_deref(),
                || false,
            )
            .await?;
            TranscriptionResult::new(&transcription_job, output.raw_text)
                .with_transcript_language(output.transcript_language)
                .with_segments(output.segments)
                .with_audio_duration_ms(Some(output.audio_duration_ms))
                .with_processing_duration_ms(Some(output.processing_duration_ms))
        }
        ActiveEngineSelection::Parakeet { model_name } => {
            let parakeet_manager = app.state::<ParakeetManager>();

            parakeet_manager
                .load_model(&app, &model_name)
                .await
                .map_err(|e| format!("Failed to load Parakeet model: {}", e))?;

            let custom_vocabulary =
                compile_parakeet_custom_vocabulary_for_transcription(&app, Some(&language));

            match parakeet_manager
                .transcribe_with_custom_vocabulary(
                    &app,
                    &model_name,
                    temp_path.clone(),
                    ParakeetTranscriptionOptions {
                        language: Some(language.clone()),
                        translate: translate_to_english,
                        custom_vocabulary,
                        cancel_flag: None,
                    },
                )
                .await
            {
                Ok(ParakeetResponse::Transcription {
                    text,
                    segments,
                    language,
                    duration,
                }) => TranscriptionResult::new(&transcription_job, text)
                    .with_transcript_language(language)
                    .with_segments(parakeet_segments_to_transcription_segments(segments))
                    .with_audio_duration_ms(seconds_to_duration_ms(duration)),
                Ok(other) => return Err(format!("Unexpected Parakeet response: {:?}", other)),
                Err(err) => return Err(format!("Parakeet transcription failed: {}", err)),
            }
        }
        ActiveEngineSelection::Cloud { provider, .. } => {
            let text = provider
                .transcribe(&app, &temp_path, Some(&language))
                .await?;
            let cloud_job = build_transcription_job(
                TranscriptionSource::AudioBytes,
                transcription_job.engine.clone(),
                transcription_job.model.clone(),
                transcription_job.spoken_language.clone(),
                false,
            );
            TranscriptionResult::new(&cloud_job, text)
        }
        ActiveEngineSelection::Remote {
            server_id,
            server_name,
            host,
            port,
            password,
            ..
        } => {
            // Normalize to Whisper contract (16k mono s16 WAV) for remote transcription
            log::debug!("[CLIPBOARD] Normalizing to Whisper WAV for remote transcription...");
            let normalized_file = NormalizedTempFile::new({
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_clipboard_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &temp_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            });
            log::info!("[CLIPBOARD] Normalized WAV at {:?}", normalized_file.path());

            log::info!(
                "🌐 [Remote Clipboard] Starting transcription to '{}' ({}:{})",
                server_name,
                host,
                port
            );

            // Read the normalized audio file
            let audio_data = std::fs::read(normalized_file.path())
                .map_err(|e| format!("Failed to read audio file: {}", e))?;

            let audio_size_kb = audio_data.len() as f64 / 1024.0;
            log::info!(
                "🌐 [Remote Clipboard] Sending {:.1} KB audio to '{}'",
                audio_size_kb,
                server_name
            );

            // Create HTTP client connection
            let server_conn = RemoteServerConnection::new(host.clone(), port, password.clone());

            let request_context = crate::commands::remote::resolve_remote_request_context(
                &app,
                &server_id,
                transcription_job.spoken_language.as_deref(),
            )
            .await;

            let (request, timeout_ms) = build_remote_upload_transcription_request(
                normalized_file.path(),
                audio_data,
                Some(&transcription_job),
                request_context,
            );

            let response = client::transcribe_audio(&server_conn, request, timeout_ms)
                .await
                .map_err(|e| {
                    log::warn!(
                        "🌐 [Remote Clipboard] Remote transcription FAILED to '{}': {}",
                        server_name,
                        e
                    );
                    e.to_string()
                })?;

            log::info!(
                "🌐 [Remote Clipboard] Transcription COMPLETED from '{}': {} chars received",
                server_name,
                response.text.len()
            );

            build_remote_transcription_result(&transcription_job, response)
        }
    };

    // Clean up
    if let Err(e) = std::fs::remove_file(&temp_path) {
        log::warn!("Failed to remove test audio file: {}", e);
    }

    let writing_result =
        crate::writing::process_transcription(app.clone(), transcription_result.clone())
            .await
            .map_err(|e| e.user_message())?;
    if let Some(error) = writing_result.ai_error.as_ref() {
        log::warn!(
            "AI polish failed with {}; returning and saving deterministic test transcription text",
            ai_failure_category(error)
        );
        notify_ai_polish_failure(&app, error);
        save_ai_polish_fallback_history(app.clone(), &transcription_result, &writing_result)
            .await?;
    }
    Ok(writing_result.final_text)
}

#[tauri::command]
pub async fn cancel_recording(app: AppHandle) -> Result<(), String> {
    log::info!("=== CANCEL RECORDING CALLED ===");

    // Request cancellation FIRST
    let app_state = app.state::<AppState>();
    app_state.request_cancellation();
    log::info!("Cancellation requested in app state");

    // Get current state
    let current_state = app_state.get_current_state();
    log::info!("Current state when cancelling: {:?}", current_state);

    // Abort any ongoing transcription task
    if let Ok(mut task_guard) = app_state.transcription_task.lock() {
        if let Some(task) = task_guard.take() {
            log::info!("Aborting transcription task");
            task.abort();
        }
    }
    // JoinHandle::abort preempts the task at its next await and skips the
    // task's own remove_file cleanup, so explicitly delete the temp recording
    // the aborted task owned. Without this the cancelled dictation's audio is
    // left on disk. A NotFound result means the task already cleaned up.
    if let Some(cancelled_audio) = take_in_flight_transcription_audio() {
        log::info!("Removing transcription task's temp recording after abort");
        if let Err(e) = std::fs::remove_file(&cancelled_audio) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("Failed to remove cancelled transcription audio: {}", e);
            }
        }
    }

    #[cfg(target_os = "windows")]
    if matches!(current_state, RecordingState::Transcribing) {
        let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        gpu_client.abort_active_process().await;
    }

    // Stop recording if active
    //
    // Plan 060.1 — restoration before propagation: the stop outcome is
    // COLLECTED, not propagated with `?`. Every cancellation cleanup below
    // (media resume, ESC state, pill, state transitions) must run even when
    // the recorder stop fails, and the error is surfaced only AFTER the
    // user's paused media is restored — a failed ESC-cancel must never
    // strand a paused track or a stuck recording state.
    let recorder_state = app.state::<RecorderState>();
    let stop_error: Option<String> = (|| -> Result<(), String> {
        let mut guard = recorder_state
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))?;
        if !guard.is_recording() {
            return Ok(());
        }
        log::info!("Stopping recorder");
        // Just stop the recorder, don't do full stop_recording flow.
        // A stop error keeps the WAV on disk for the orphan cleanup when the
        // worker never finalized (same policy as stop_unfinalized); only a
        // clean stop may delete the cancelled recording.
        match guard.stop_recording() {
            Ok(_) => {
                // Clean up audio file if it exists
                if let Ok(path_guard) = app_state.current_recording_path.lock() {
                    if let Some(audio_path) = path_guard.as_ref() {
                        log::info!("Removing cancelled recording file");

                        if let Err(e) = std::fs::remove_file(audio_path) {
                            log::warn!("Failed to remove cancelled recording: {}", e);
                        }
                    }
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    })()
    .err();

    // Resume system media if we paused it
    MEDIA_CONTROLLER.resume_if_we_paused();

    // Clean up ESC state
    app_state
        .esc_pressed_once
        .store(false, std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
        if let Some(handle) = timeout_guard.take() {
            handle.abort();
        }
    }

    // Hide pill window immediately (only if show_pill_indicator is false)
    if should_hide_pill(&app).await {
        if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
            log::error!("Failed to hide pill window: {}", e);
        }
    }

    // Properly transition through states based on current state
    match current_state {
        RecordingState::Recording => {
            // First transition to Stopping
            update_recording_state(&app, RecordingState::Stopping, None);
            // Then transition to Idle
            update_recording_state(&app, RecordingState::Idle, None);
        }
        RecordingState::Starting => {
            // Starting can go directly to Idle
            update_recording_state(&app, RecordingState::Idle, None);
        }
        RecordingState::Stopping => {
            // Already stopping, just go to Idle
            update_recording_state(&app, RecordingState::Idle, None);
        }
        RecordingState::Transcribing => {
            // Can't go directly to Idle from Transcribing, need to go through Error
            update_recording_state(
                &app,
                RecordingState::Error,
                Some("Transcription cancelled".to_string()),
            );
            update_recording_state(&app, RecordingState::Idle, None);
        }
        _ => {
            // For other states (Idle, Error), try to transition to Idle
            update_recording_state(&app, RecordingState::Idle, None);
        }
    }
    crate::trigger::engine_host::rebuild_engine_bindings(&app);

    // Plan 060.1: restoration is complete (media resumed, ESC state cleared,
    // state machine landed on Idle/Error) — NOW surface the stop failure.
    if let Some(e) = stop_error {
        log::error!("Cancellation stop failed after cleanup: {}", e);
        return Err(e);
    }

    log::info!("=== CANCEL RECORDING COMPLETED ===");
    Ok(())
}

#[tauri::command]
pub async fn delete_transcription_entry(app: AppHandle, timestamp: String) -> Result<(), String> {
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    // Delete the entry
    store.delete(&timestamp);

    // Save the store
    store
        .save()
        .map_err(|e| format!("Failed to save store after deletion: {}", e))?;

    // Emit event to update UI
    let _ = emit_to_window(&app, "main", "history-updated", ());

    // Refresh tray menu to reflect removal
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!("Failed to update tray menu after deletion: {}", e);
    }

    log::info!("Deleted transcription entry: {}", timestamp);
    Ok(())
}

#[tauri::command]
pub async fn clear_all_transcriptions(app: AppHandle) -> Result<(), String> {
    log::info!("[Clear All] Clearing all transcriptions");

    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    // Get all keys and delete them
    let keys: Vec<String> = store.keys().into_iter().map(|k| k.to_string()).collect();
    let count = keys.len();

    for key in keys {
        store.delete(&key);
    }

    // Save the store
    store
        .save()
        .map_err(|e| format!("Failed to save store after clearing: {}", e))?;

    // Emit event to update UI
    let _ = emit_to_window(&app, "main", "history-updated", ());

    // Refresh tray menu after clearing
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!("Failed to update tray menu after clearing history: {}", e);
    }

    log::info!("Cleared all transcription entries: {} items", count);
    Ok(())
}

#[derive(serde::Serialize)]
pub struct RecordingStateResponse {
    state: String,
    error: Option<String>,
}

#[tauri::command]
pub fn get_current_recording_state(app: AppHandle) -> RecordingStateResponse {
    let app_state = app.state::<AppState>();
    let current_state = app_state.get_current_state();

    RecordingStateResponse {
        state: match current_state {
            RecordingState::Idle => "idle",
            RecordingState::Starting => "starting",
            RecordingState::Recording => "recording",
            RecordingState::Stopping => "stopping",
            RecordingState::Transcribing => "transcribing",
            RecordingState::Error => "error",
        }
        .to_string(),
        error: None,
    }
}

/// Validate that a recording filename is safe (no path traversal)
fn validate_recording_filename(filename: &str) -> Result<(), String> {
    use std::path::Component;
    let path = std::path::Path::new(filename);

    // Reject empty filenames
    if filename.is_empty() {
        return Err("Empty filename".to_string());
    }

    // Reject absolute paths
    if path.is_absolute() {
        return Err("Absolute paths are not allowed".to_string());
    }

    // Reject any non-Normal components (../, ./, prefix, root)
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            other => {
                return Err(format!("Invalid path component: {:?}", other));
            }
        }
    }

    Ok(())
}

/// Check if a recording file exists in the recordings directory
#[tauri::command]
pub async fn check_recording_exists(app: AppHandle, filename: String) -> Result<bool, String> {
    validate_recording_filename(&filename)?;
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");
    Ok(recordings_dir.join(&filename).exists())
}

/// Get the full path to a recording file for playback
#[tauri::command]
pub async fn get_recording_path(app: AppHandle, filename: String) -> Result<String, String> {
    validate_recording_filename(&filename)?;
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");
    let file_path = recordings_dir.join(&filename);
    if !file_path.exists() {
        return Err(format!("Recording file not found: {}", filename));
    }
    Ok(file_path.to_string_lossy().to_string())
}

/// Save a re-transcription to history, linking to the original recording
#[tauri::command]
pub async fn save_retranscription(
    app: AppHandle,
    text: String,
    model: String,
    recording_file: String,
    source_recording_id: String,
    status: Option<TranscriptionStatus>,
) -> Result<String, String> {
    // Save transcription to store with current timestamp
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    let timestamp = chrono::Utc::now().to_rfc3339();
    let mut transcription_data = serde_json::json!({
        "text": text.clone(),
        "model": model,
        "timestamp": timestamp.clone(),
        "recording_file": recording_file.clone(),
        "source_recording_id": source_recording_id.clone(),
        "is_retranscription": true,
    });

    let effective_status = transcription_data
        .as_object_mut()
        .ok_or_else(|| "Failed to build retranscription payload".to_string())
        .map(|map| apply_retranscription_status(map, status))?;

    store.set(&timestamp, transcription_data.clone());

    store
        .save()
        .map_err(|e| format!("Failed to save retranscription: {}", e))?;

    // Emit the new transcription data to frontend for append-only update
    let _ = emit_to_window(&app, "main", "transcription-added", transcription_data);

    // Refresh tray menu (best-effort) so Recent Transcriptions stays updated
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!(
            "Failed to update tray menu after saving retranscription: {}",
            e
        );
    }

    log::info!(
        "Saved retranscription with {} characters (source: {}, status: {})",
        text.len(),
        source_recording_id,
        effective_status.as_str()
    );
    Ok(timestamp)
}

/// Update an existing transcription entry in place (for re-transcription)
#[tauri::command]
pub async fn update_transcription(
    app: AppHandle,
    timestamp: String,
    text: String,
    model: String,
    status: Option<TranscriptionStatus>,
) -> Result<(), String> {
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    // Get the existing entry
    let existing = store
        .get(&timestamp)
        .ok_or_else(|| format!("Transcription not found: {}", timestamp))?;

    // Preserve original fields, update text, model, and status.
    let mut updated = existing.clone();
    let effective_status = updated
        .as_object_mut()
        .ok_or_else(|| "Transcription entry is not an object".to_string())
        .map(|map| {
            map.insert("text".to_string(), serde_json::Value::String(text.clone()));
            map.insert(
                "model".to_string(),
                serde_json::Value::String(model.clone()),
            );
            let effective_status = apply_retranscription_status(map, status);
            sync_retranscription_failure_metadata(map, effective_status, &text);
            effective_status
        })?;

    store.set(&timestamp, updated.clone());

    store
        .save()
        .map_err(|e| format!("Failed to save updated transcription: {}", e))?;

    // Emit update event to frontend
    let _ = emit_to_window(
        &app,
        "main",
        "transcription-updated",
        serde_json::json!({
            "timestamp": timestamp,
            "text": text,
            "model": model,
            "status": transcription_status_value(effective_status)
        }),
    );

    // Refresh tray menu (best-effort)
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!(
            "Failed to update tray menu after updating transcription: {}",
            e
        );
    }

    log::info!(
        "Updated transcription {} with {} characters",
        timestamp,
        text.len()
    );
    Ok(())
}

/// Open the file explorer with the specified file selected
#[tauri::command]
pub async fn show_in_folder(path: String) -> Result<(), String> {
    let path = std::path::Path::new(&path);

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    #[cfg(target_os = "windows")]
    {
        // Use explorer.exe /select to open folder with file selected
        std::process::Command::new("explorer.exe")
            .args(["/select,", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        // Use open -R to reveal file in Finder
        std::process::Command::new("open")
            .args(["-R", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        // Try xdg-open on the parent directory
        if let Some(parent) = path.parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("Failed to open file manager: {}", e))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod diarization_tests {
    use super::{group_words_into_speaker_text, TranscriptionWord};

    fn word(text: &str, speaker: Option<&str>) -> TranscriptionWord {
        TranscriptionWord {
            text: text.to_string(),
            start_ms: None,
            end_ms: None,
            speaker_id: speaker.map(str::to_string),
            confidence: None,
        }
    }

    #[test]
    fn two_speakers_produces_two_paragraphs() {
        let words = vec![
            word("Hello", Some("Speaker 0")),
            word("world.", Some("Speaker 0")),
            word("Thanks.", Some("Speaker 1")),
        ];
        let result = group_words_into_speaker_text(&words);
        assert_eq!(result, "Speaker 0: Hello world.\n\nSpeaker 1: Thanks.");
    }

    #[test]
    fn single_speaker_produces_one_block() {
        let words = vec![
            word("Hello", Some("Speaker 0")),
            word("world.", Some("Speaker 0")),
        ];
        let result = group_words_into_speaker_text(&words);
        assert_eq!(result, "Speaker 0: Hello world.");
    }

    #[test]
    fn no_speaker_produces_single_block_without_prefix() {
        let words = vec![word("Hello", None), word("world.", None)];
        let result = group_words_into_speaker_text(&words);
        assert_eq!(result, "Hello world.");
    }

    #[test]
    fn empty_input_returns_empty_string() {
        assert_eq!(group_words_into_speaker_text(&[]), "");
    }

    #[test]
    fn words_without_speaker_continue_current_run() {
        let words = vec![
            word("Hello", Some("Speaker 0")),
            word("there", None), // no speaker → continue Speaker 0 run
            word("Goodbye.", Some("Speaker 1")),
        ];
        let result = group_words_into_speaker_text(&words);
        assert_eq!(result, "Speaker 0: Hello there\n\nSpeaker 1: Goodbye.");
    }

    #[test]
    fn three_speaker_switches() {
        let words = vec![
            word("A", Some("Speaker 0")),
            word("B", Some("Speaker 1")),
            word("C", Some("Speaker 0")),
        ];
        let result = group_words_into_speaker_text(&words);
        assert_eq!(result, "Speaker 0: A\n\nSpeaker 1: B\n\nSpeaker 0: C");
    }

    // Soniox tokens carry their own leading whitespace and punctuation.
    #[test]
    fn soniox_style_pre_spaced_tokens_no_double_space() {
        // Soniox emits tokens like "How", " are", " you", "?"
        let words = vec![
            word("How", Some("Speaker 0")),
            word(" are", Some("Speaker 0")),
            word(" you", Some("Speaker 0")),
            word("?", Some("Speaker 0")),
        ];
        let result = group_words_into_speaker_text(&words);
        assert_eq!(result, "Speaker 0: How are you?");
    }

    #[test]
    fn soniox_style_two_speakers_pre_spaced() {
        let words = vec![
            word("Hello", Some("Speaker 0")),
            word(" there", Some("Speaker 0")),
            word(".", Some("Speaker 0")),
            word("How", Some("Speaker 1")),
            word(" are", Some("Speaker 1")),
            word(" you", Some("Speaker 1")),
            word("?", Some("Speaker 1")),
        ];
        let result = group_words_into_speaker_text(&words);
        assert_eq!(result, "Speaker 0: Hello there.\n\nSpeaker 1: How are you?");
    }
}

#[cfg(test)]
mod failure_class_tests {
    use super::*;

    #[test]
    fn typed_failure_class_is_independent_of_display_text_and_detail() {
        use crate::transcription::error::TranscriptionError;
        for (code, class) in [
            (TranscriptionErrorCode::Unauthorized, "auth"),
            (TranscriptionErrorCode::TransportFailed, "transport"),
            (
                TranscriptionErrorCode::StorageLimitExceeded,
                "cloud_storage_limit",
            ),
            (
                TranscriptionErrorCode::ModelUnavailable,
                "model_unavailable",
            ),
            (
                TranscriptionErrorCode::EngineUnavailable,
                "model_unavailable",
            ),
            (TranscriptionErrorCode::Timeout, "timeout"),
            (TranscriptionErrorCode::EngineFailed, "engine_failed"),
        ] {
            let error = TranscriptionError::new(
                code,
                TranscriptionSource::DesktopRecording,
                "Display copy changed",
            )
            .with_detail("network model authentication timed out");
            let failure = desktop_failure_from_transcription_error(error);
            assert_eq!(transcription_failure_class(&failure), class);
        }
    }

    #[test]
    fn typed_failure_keeps_desktop_cancellation_and_history_messages() {
        use crate::transcription::error::TranscriptionError;
        for (code, message, retryable) in [
            (
                TranscriptionErrorCode::Cancelled,
                "Transcription cancelled",
                false,
            ),
            (
                TranscriptionErrorCode::Timeout,
                "Transcription timed out",
                true,
            ),
            (
                TranscriptionErrorCode::Unauthorized,
                "Display copy: provider detail",
                true,
            ),
        ] {
            let failure = desktop_failure_from_transcription_error(
                TranscriptionError::new(
                    code,
                    TranscriptionSource::DesktopRecording,
                    "Display copy",
                )
                .with_detail("provider detail"),
            );
            assert_eq!(failure.message(), message);
            assert_eq!(failure.is_retryable_failure(), retryable);
            assert_eq!(failure.error_kind(), "local");
        }
    }

    #[test]
    fn legacy_failures_keep_string_classification() {
        assert_eq!(
            transcription_failure_class(&TranscriptionFailure::local(
                "Transcription timed out".into()
            )),
            "timeout"
        );
        assert_eq!(
            transcription_failure_class(&TranscriptionFailure::local("network error".into())),
            "transport"
        );
    }
}
