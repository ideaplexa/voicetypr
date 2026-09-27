//! Media control module for pausing/resuming system media during recording.
//!
//! Uses platform-specific APIs:
//! - macOS: layered — `media-remote` crate pause/play command, NX media-key
//!   event, then CoreAudio output-mute fallback; state via JXA now-playing
//! - Windows: `windows` crate (GlobalSystemMediaTransportControls)

mod controller;

pub use controller::MediaPauseController;
