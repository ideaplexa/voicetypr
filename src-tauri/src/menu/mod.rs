pub(crate) mod actions;
mod languages;
mod model;
pub(crate) mod quick;
pub(crate) mod runtime;
pub(crate) mod tray;

pub(crate) use tray::latest_copyable_transcription_id;
pub use tray::{build_tray_menu, should_include_remote_connection_in_tray};

#[cfg(test)]
pub use tray::{format_tray_model_label, format_tray_polish_label, should_mark_model_selected};
pub(crate) mod refresh;
