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

    /// There is no take to clean up while an Idle/Error start is validating.
    /// Set cancellation under the ownership lock and let the command return
    /// without asynchronous cleanup that could reset a newer request to Idle.
    pub(crate) fn cancel_before_recording(&self, state: &AppState) -> bool {
        self.cancel_if(
            |_| true,
            || {
                state.request_cancellation();
                matches!(
                    state.get_current_state(),
                    RecordingState::Idle | RecordingState::Error
                )
            },
        )
        .unwrap_or(false)
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

    pub(crate) fn publish(
        self,
        app: &AppHandle,
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
            super::kept::cancel_active_retry(app);
            state
                .pointer_recording
                .store(source.is_toggle(), std::sync::atomic::Ordering::SeqCst);
            let generation = audio::begin_recording_generation();
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
            state.capture_recording_app_context(crate::writing::capture_active_app_context);
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
