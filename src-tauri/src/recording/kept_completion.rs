//! Phase/flag writes share start/cancel publication ownership.
use super::*;
use crate::commands::recording_runtime::RecordingRuntime;

fn with_current<R: RecordingRuntime, T>(
    app: &AppHandle<R>,
    generation: u64,
    work: impl FnOnce(&mut Store) -> T,
) -> Option<T> {
    let state = app.state::<crate::AppState>();
    state
        .pending_start
        .with_current(generation, || {
            // publication -> Store -> feedback. Also exclude shutdown detachment
            // until the final state/card write completes; no I/O await/UI hop.
            let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
            (!store.closed).then(|| work(&mut store))
        })
        .flatten()
}

pub(super) fn publish<R: RecordingRuntime, T>(
    app: &AppHandle<R>,
    generation: u64,
    settle: impl FnOnce(&mut Store) -> T,
) -> Option<T> {
    with_current(app, generation, |store| {
        #[cfg(test)]
        super::lifecycle_tests::before_idle(app);
        crate::update_recording_state(app, crate::RecordingState::Idle, None);
        settle(store)
    })
}

fn cancel_phase<R: RecordingRuntime>(app: &AppHandle<R>) {
    app.state::<crate::AppState>().request_cancellation();
    if crate::get_recording_state(app) == crate::RecordingState::Transcribing {
        crate::update_recording_state(app, crate::RecordingState::Idle, None);
    }
}

pub(super) fn cancelled<R: RecordingRuntime>(app: &AppHandle<R>, generation: Option<u64>) {
    if let Some(generation) = generation {
        with_current(app, generation, |_| cancel_phase(app));
    }
}

/// StartRequest already owns the publication lock. Do not reacquire it.
pub(super) fn cancelled_owned<R: RecordingRuntime>(app: &AppHandle<R>, generation: Option<u64>) {
    let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    if !store.closed && generation.is_some_and(|g| !audio::recording_generation_is_stale(g)) {
        cancel_phase(app);
    }
}
