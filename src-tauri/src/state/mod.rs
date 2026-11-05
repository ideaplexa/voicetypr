pub mod app_state;
pub(crate) mod capture_slot;
pub mod unified_state;

pub use app_state::{
    emit_to_all, emit_to_window, flush_pill_event_queue, get_recording_state,
    update_recording_state, AppState, QueuedPillEvent, RecordingMode, RecordingState,
};
