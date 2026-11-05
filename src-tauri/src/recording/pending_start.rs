//! One request owns cancellable validation; only publication creates a take.
//! Request identity is not a second recording generation. The input-route epoch
//! is separate: it also rejects queued hotkey events across Secure Input edges.
use super::{
    secure_input::StartPermit,
    start_source::{ptt_key_released, StartSource},
};
use crate::audio::recorder::CaptureInterrupt;
use crate::{commands::audio, AppState, RecordingState};
use std::{
    future::Future,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(crate) struct StartRequests(Mutex<Option<Arc<Pending>>>);

struct Pending {
    source: StartSource,
    permit: StartPermit,
    cancellation: CancellationToken,
}

pub(crate) struct StartRequest<'a> {
    owner: &'a StartRequests,
    pending: Arc<Pending>,
}

impl StartRequests {
    pub(crate) fn claim(
        &self,
        state: &AppState,
        source: StartSource,
        permit: StartPermit,
    ) -> Option<StartRequest<'_>> {
        let mut requests = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if requests.is_some()
            || !permit.is_current_state(state)
            || matches!(
                state.get_current_state(),
                RecordingState::Starting | RecordingState::Recording
            )
        {
            return None;
        }
        let pending = Arc::new(Pending {
            source,
            permit,
            cancellation: CancellationToken::new(),
        });
        *requests = Some(pending.clone());
        Some(StartRequest {
            owner: self,
            pending,
        })
    }

    /// Serialized with commit. Either validation loses its request, or Starting
    /// is already published and the caller must use the existing stop/cancel path.
    pub(crate) fn cancel(&self) -> bool {
        self.cancel_if(|_| true, || ()).is_some()
    }

    pub(crate) fn cancel_released_hold(&self, state: &AppState) -> bool {
        self.cancel_if(|pending| ptt_key_released(state, pending.source), || ())
            .is_some()
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn cancel_invalidated_input(&self, state: &AppState) {
        self.cancel_if(|pending| !pending.permit.is_current_state(state), || ());
    }

    /// Capture the cancellation target and latch it in the publication lock.
    pub(crate) fn begin_cancel(&self, state: &AppState) -> (u64, bool, RecordingState) {
        let mut requests = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let pending = requests.take();
        if let Some(pending) = &pending {
            pending.cancellation.cancel();
        }
        let generation = audio::current_recording_generation();
        let phase = state.get_current_state();
        state.cancel_capture(generation);
        let pending_only =
            pending.is_some() && matches!(phase, RecordingState::Idle | RecordingState::Error);
        (generation, pending_only, phase)
    }

    /// Retry is the latest action, but retires validation only after setup
    /// succeeds. Lease registration, generation advancement and state publication
    /// must all run inside `setup`, serialized with start/cancel publication.
    /// Lock order: publication -> kept Store -> feedback hide. Never await or
    /// synchronously hop to the UI thread in this critical section.
    pub(crate) fn admit_retry<T>(
        &self,
        state: &AppState,
        setup: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let mut requests = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !matches!(
            state.get_current_state(),
            RecordingState::Idle | RecordingState::Error
        ) {
            return Err("Dictation is busy".into());
        }
        let retry = setup()?;
        if let Some(pending) = requests.take() {
            pending.cancellation.cancel();
        }
        Ok(retry)
    }

    /// Synchronous start effects and replacement publication share this lock.
    /// Never await or synchronously hop to the UI thread inside `work`.
    pub(crate) fn with_active<T>(
        &self,
        state: &AppState,
        generation: u64,
        work: impl FnOnce() -> T,
    ) -> Option<T> {
        let _requests = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !state.capture_interrupts.is_active(generation)
            || state.is_cancellation_requested()
            || !matches!(
                state.get_current_state(),
                RecordingState::Starting | RecordingState::Recording
            )
        {
            return None;
        }
        Some(work())
    }

    /// Cleanup may run after interruption, but never against a replacement.
    pub(crate) fn with_current<T>(&self, generation: u64, work: impl FnOnce() -> T) -> Option<T> {
        let _requests = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if audio::recording_generation_is_stale(generation) {
            return None;
        }
        Some(work())
    }

    fn cancel_if<T>(
        &self,
        matches: impl FnOnce(&Pending) -> bool,
        work: impl FnOnce() -> T,
    ) -> Option<T> {
        let mut requests = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !matches(requests.as_ref()?) {
            return None;
        }
        let pending = requests.take()?;
        let result = work();
        pending.cancellation.cancel();
        Some(result)
    }
}

impl StartRequest<'_> {
    pub(crate) async fn validate<T>(&self, validation: impl Future<Output = T>) -> Option<T> {
        tokio::select! {
            biased;
            () = self.pending.cancellation.cancelled() => None,
            result = validation => Some(result),
        }
    }

    fn commit<T>(&self, work: impl FnOnce() -> T) -> Option<T> {
        let mut requests = self.owner.0.lock().unwrap_or_else(|e| e.into_inner());
        if requests
            .as_ref()
            .is_none_or(|pending| !Arc::ptr_eq(pending, &self.pending))
        {
            return None;
        }
        *requests = None;
        // Never await here. Cancel cannot invalidate ownership between this
        // check and clearing stale flags/publishing Starting.
        Some(work())
    }

    pub(crate) fn publish<R: crate::commands::recording_runtime::RecordingRuntime>(
        self,
        app: &AppHandle<R>,
    ) -> Result<Option<(u64, CaptureInterrupt)>, String> {
        self.commit(|| {
            let state = app.state::<AppState>();
            let source = self.pending.source;
            if !self.pending.permit.is_current_state(&state)
                || ptt_key_released(&state, source)
                || matches!(
                    state.get_current_state(),
                    RecordingState::Starting | RecordingState::Recording
                )
            {
                return Ok(None);
            }
            if state.get_current_state() == RecordingState::Error {
                crate::update_recording_state(app, RecordingState::Idle, Some("recover".into()));
            }
            super::kept::cancel_active_retry_owned(app);
            state
                .pointer_recording
                .store(source.is_toggle(), std::sync::atomic::Ordering::SeqCst);
            let generation = audio::begin_recording_generation();
            state.current_recording_path.begin(generation);
            state.capture_files.begin(generation);
            crate::perf::begin_take(generation);
            let hold = !source.is_toggle()
                && state
                    .recording_mode
                    .lock()
                    .map(|m| *m == crate::RecordingMode::PushToTalk)
                    .unwrap_or(false);
            crate::observability::update(
                generation,
                "mode",
                serde_json::json!(if hold { "hold" } else { "toggle" }),
            );
            crate::observability::update(
                generation,
                "start_source",
                serde_json::json!(source.telemetry_label()),
            );
            // Only this owned request can clear cancellation from an older take.
            state.clear_cancellation();
            audio::clear_pending_stop_after_start(&state);
            let interrupt = state.capture_interrupts.begin(generation);
            if R::native(app).is_some() {
                state.capture_recording_app_context(crate::writing::capture_active_app_context);
            }
            crate::update_recording_state(app, RecordingState::Starting, None);
            if state.get_current_state() != RecordingState::Starting {
                return Err("Cannot start recording in current state".to_string());
            }
            Ok(Some((generation, interrupt)))
        })
        .unwrap_or(Ok(None))
    }
}

impl Drop for StartRequest<'_> {
    fn drop(&mut self) {
        let mut requests = self.owner.0.lock().unwrap_or_else(|e| e.into_inner());
        if requests
            .as_ref()
            .is_some_and(|pending| Arc::ptr_eq(pending, &self.pending))
        {
            *requests = None;
        }
    }
}

#[cfg(test)]
mod tests;
