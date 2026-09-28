mod model_selection;

pub use model_selection::{
    auto_select_model_if_needed, emit_recognition_availability,
    get_recognition_availability_snapshot, recognition_availability_snapshot, selected_model_error,
    RecognitionAvailabilitySnapshot,
};
