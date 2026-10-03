//! The wave uses sqrt(peak), a perceptual amplitude curve, clamped to [0, 1].
use std::{
    sync::mpsc::{Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};
use tauri::Emitter;
#[derive(Clone, serde::Serialize)]
struct Level {
    generation: u64,
    level: f64,
}
pub fn spawn(app: tauri::AppHandle, receiver: Receiver<f64>, generation: u64) {
    std::thread::spawn(move || {
        let interval = Duration::from_nanos(1_000_000_000 / 60);
        let mut deadline = Instant::now() + interval;
        let mut peak = 0.0_f64;
        loop {
            if crate::commands::audio::recording_generation_is_stale(generation) {
                break;
            }
            match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(level) => peak = peak.max(level),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    if crate::get_recording_state(&app) != crate::RecordingState::Recording {
                        break;
                    }
                    // Drain queued callbacks before resetting the event's peak.
                    for level in receiver.try_iter() {
                        peak = peak.max(level);
                    }
                    let level = Level {
                        generation,
                        level: peak.clamp(0.0, 1.0).sqrt(),
                    };
                    // The pill draws the wave; the main window's Recording screen shows a mic meter.
                    let _ = app.emit_to("pill", "audio-level", level.clone());
                    let _ = app.emit_to("main", "audio-level", level);
                    peak = 0.0;
                    deadline = Instant::now() + interval;
                }
            }
        }
    });
}
