//! Ownership of a start while validation is awaiting. This request generation
//! precedes the recording generation: no mic or take exists until commit.
use super::start_source::StartSource;
use crate::{commands::audio, AppState, RecordingState};
use std::{future::Future, sync::Mutex};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(crate) struct StartRequests(Mutex<Requests>);

#[derive(Default)]
struct Requests {
    generation: u64,
    pending: Option<(u64, CancellationToken)>,
}

pub(crate) struct StartRequest<'a> {
    owner: &'a StartRequests,
    generation: u64,
    cancellation: CancellationToken,
}

impl StartRequests {
    pub(crate) fn claim(
        &self,
        read_state: impl FnOnce() -> RecordingState,
    ) -> Option<StartRequest<'_>> {
        let mut requests = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if requests.pending.is_some()
            || matches!(
                read_state(),
                RecordingState::Starting | RecordingState::Recording
            )
        {
            return None;
        }
        requests.generation = requests.generation.wrapping_add(1);
        let generation = requests.generation;
        let cancellation = CancellationToken::new();
        requests.pending = Some((generation, cancellation.clone()));
        Some(StartRequest {
            owner: self,
            generation,
            cancellation,
        })
    }

    /// Serialized with commit. Either validation loses its request, or Starting
    /// is already published and the caller must use the existing stop/cancel path.
    pub(crate) fn cancel(&self) -> bool {
        self.cancel_with(|| ()).is_some()
    }

    /// There is no take to clean up while an Idle/Error start is validating.
    /// Set cancellation under the ownership lock and let the command return
    /// without asynchronous cleanup that could reset a newer request to Idle.
    pub(crate) fn cancel_before_recording(&self, state: &AppState) -> bool {
        self.cancel_with(|| {
            state.request_cancellation();
            matches!(
                state.get_current_state(),
                RecordingState::Idle | RecordingState::Error
            )
        })
        .unwrap_or(false)
    }

    fn cancel_with<T>(&self, work: impl FnOnce() -> T) -> Option<T> {
        let mut requests = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let (_, cancellation) = requests.pending.take()?;
        let result = work();
        cancellation.cancel();
        Some(result)
    }
}

impl StartRequest<'_> {
    pub(crate) async fn validate<T>(&self, validation: impl Future<Output = T>) -> Option<T> {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => None,
            result = validation => Some(result),
        }
    }

    fn commit<T>(&self, work: impl FnOnce() -> T) -> Option<T> {
        let mut requests = self.owner.0.lock().unwrap_or_else(|e| e.into_inner());
        if requests
            .pending
            .as_ref()
            .is_none_or(|(g, _)| *g != self.generation)
        {
            return None;
        }
        requests.pending = None;
        // Never await here. Cancel cannot invalidate ownership between this
        // check and clearing stale flags/publishing Starting.
        Some(work())
    }

    pub(crate) fn publish(self, app: &AppHandle, source: StartSource) -> Result<bool, String> {
        self.commit(|| {
            let state = app.state::<AppState>();
            if matches!(
                state.get_current_state(),
                RecordingState::Starting | RecordingState::Recording
            ) {
                return Ok(false);
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
            state.capture_recording_app_context(crate::writing::capture_active_app_context);
            crate::update_recording_state(app, RecordingState::Starting, None);
            if state.get_current_state() != RecordingState::Starting {
                return Err("Cannot start recording in current state".to_string());
            }
            Ok(true)
        })
        .unwrap_or(Ok(false))
    }
}

impl Drop for StartRequest<'_> {
    fn drop(&mut self) {
        let mut requests = self.owner.0.lock().unwrap_or_else(|e| e.into_inner());
        if requests
            .pending
            .as_ref()
            .is_some_and(|(g, _)| *g == self.generation)
        {
            requests.pending = None;
        }
    }
}

#[cfg(test)]
mod tests;
