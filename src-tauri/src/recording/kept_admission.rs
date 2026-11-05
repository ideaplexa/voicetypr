//! Retry publication shares the start/cancel ownership gate.
use super::*;
use crate::commands::recording_runtime::RecordingRuntime;

pub(super) fn admit<R: RecordingRuntime>(
    app: &AppHandle<R>,
    id: &str,
    anyway: bool,
    context: Option<crate::writing::ContextHint>,
) -> Result<Lease, String> {
    let state = app.state::<crate::AppState>();
    state.pending_start.admit_retry(&state, || {
        retry_setup(state.stop_in_flight.clone(), || {
            let probes = probe_store_availability(None);
            let lease = {
                let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
                store.apply_availability(probes);
                store.lease(id, anyway, || {
                    state.clear_cancellation();
                    let generation = audio::begin_recording_generation();
                    let (cancel, cancelled) = tokio::sync::watch::channel(false);
                    RetryRegistration {
                        generation,
                        cancel,
                        cancelled,
                    }
                })?
            };
            if let Some(context) = context {
                state.set_recording_app_context(context);
            }
            if state.get_current_state() == crate::RecordingState::Error {
                crate::update_recording_state(app, crate::RecordingState::Idle, None);
            }
            for phase in [
                crate::RecordingState::Starting,
                crate::RecordingState::Recording,
                crate::RecordingState::Stopping,
                crate::RecordingState::Transcribing,
            ] {
                crate::update_recording_state(app, phase, None);
            }
            Ok(lease)
        })
    })
}

#[cfg(test)]
#[path = "kept_admission_tests.rs"]
mod tests;
