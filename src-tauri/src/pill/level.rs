//! Display level for the island wave and the Recording mic meter (capture is untouched).
//!
//! The level thread turns the 60 Hz linear peak into [0, 1] with `LevelMapper`: a speech-tuned
//! dB scale (about -50 dBFS is 0, about -12 dBFS is 1, soft knee near the top) plus a gentle
//! display auto-gain that follows the take's recent peak. Silence and room noise stay flat.
//! The mapper lives here, never in the real-time audio callback.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::{Duration, Instant},
};
use tauri::Emitter;
/// True while the main window's Recording screen mic meter is mounted.
static MAIN_METER_ACTIVE: AtomicBool = AtomicBool::new(false);
static MAIN_WINDOW_VISIBLE: AtomicBool = AtomicBool::new(true);

/// Native hide/show is authoritative: WKWebView need not update document.visibilityState.
pub fn main_window_visibility_changed(window: &tauri::WebviewWindow, visible: bool) {
    MAIN_WINDOW_VISIBLE.store(visible, Ordering::Relaxed);
    let _ = window.emit("main-window-visibility-changed", visible);
}

fn should_emit_main_meter(active: bool, visible: bool) -> bool {
    active && visible
}

/// Clear the flag when the main page (re)loads, since React cleanup is skipped then.
pub fn reset_main_meter() {
    MAIN_METER_ACTIVE.store(false, Ordering::Relaxed);
}

#[tauri::command]
pub fn set_audio_meter_active(window: tauri::WebviewWindow, active: bool) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Main window required".into());
    }
    MAIN_METER_ACTIVE.store(active, Ordering::Relaxed);
    Ok(())
}

const TICK_SECONDS: f64 = 1.0 / 60.0;
/// dBFS that maps to 0 and to 1 (before the knee).
const FLOOR_DB: f64 = -50.0;
const CEIL_DB: f64 = -12.0;
/// The scaled value where the soft knee starts compressing toward 1.
const KNEE: f64 = 0.85;
/// The follower of the take's recent peak: quick to rise, slow to let go.
const ATTACK_SECONDS: f64 = 0.1;
const RELEASE_SECONDS: f64 = 4.0;
/// Auto-gain lifts the followed peak toward this level, by at most `MAX_GAIN_DB`.
const TARGET_DB: f64 = -18.0;
const MAX_GAIN_DB: f64 = 15.0;
/// Below `GATE_DB` the followed peak is room noise or silence and gets no gain; the gain
/// fades in over the next `GATE_RAMP_DB` so a noisy room is not lifted into full bars.
const GATE_DB: f64 = -52.0;
const GATE_RAMP_DB: f64 = 10.0;
/// Where the follower starts each take (below the gate, so a take begins without gain).
const START_DB: f64 = -60.0;

/// Peak level -> display level, one per take. Pure arithmetic, no allocation.
pub struct LevelMapper {
    follow_db: f64,
}
impl LevelMapper {
    pub fn new() -> Self {
        Self {
            follow_db: START_DB,
        }
    }
    /// `peak` is the linear peak (0..=1) of the last `dt` seconds. Returns 0..=1.
    pub fn map(&mut self, peak: f64, dt: f64) -> f64 {
        let peak = if peak.is_finite() {
            peak.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let db = 20.0 * peak.max(1e-6).log10();
        let tau = if db > self.follow_db {
            ATTACK_SECONDS
        } else {
            RELEASE_SECONDS
        };
        self.follow_db += (db - self.follow_db) * (1.0 - (-dt / tau).exp());
        let ramp = ((self.follow_db - GATE_DB) / GATE_RAMP_DB).clamp(0.0, 1.0);
        // The pauses between words (room noise) get no lift, even while the reference is low.
        let now = ((db - GATE_DB) / GATE_RAMP_DB).clamp(0.0, 1.0);
        let gain = (TARGET_DB - self.follow_db).clamp(0.0, MAX_GAIN_DB) * ramp * now;
        soft_knee((db + gain - FLOOR_DB) / (CEIL_DB - FLOOR_DB))
    }
}
impl Default for LevelMapper {
    fn default() -> Self {
        Self::new()
    }
}
/// Identity up to `KNEE`, then a quadratic roll-off that reaches exactly 1 with zero slope.
fn soft_knee(x: f64) -> f64 {
    let top = 2.0 - KNEE;
    if x <= KNEE {
        x.max(0.0)
    } else if x >= top {
        1.0
    } else {
        x - (x - KNEE).powi(2) / (2.0 * (top - KNEE))
    }
}

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
        let mut mapper = LevelMapper::new();
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
                        level: mapper.map(peak, TICK_SECONDS),
                    };
                    // The pill draws the wave; the main window's Recording screen shows a mic meter.
                    let _ = app.emit_to("pill", "audio-level", level.clone());
                    if should_emit_main_meter(
                        MAIN_METER_ACTIVE.load(Ordering::Relaxed),
                        MAIN_WINDOW_VISIBLE.load(Ordering::Relaxed),
                    ) {
                        let _ = app.emit_to("main", "audio-level", level);
                    }
                    peak = 0.0;
                    deadline = Instant::now() + interval;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_main_window_never_receives_meter_events() {
        assert!(!should_emit_main_meter(true, false));
        assert!(!should_emit_main_meter(false, true));
        assert!(should_emit_main_meter(true, true));
    }

    #[test]
    fn page_reload_clears_meter_flag() {
        MAIN_METER_ACTIVE.store(true, Ordering::Relaxed);
        reset_main_meter();
        assert!(!MAIN_METER_ACTIVE.load(Ordering::Relaxed));
    }

    fn linear(db: f64) -> f64 {
        10f64.powf(db / 20.0)
    }
    /// Feed `seconds` of a constant peak and return the last mapped value.
    fn settle(mapper: &mut LevelMapper, db: f64, seconds: f64) -> f64 {
        let mut out = 0.0;
        for _ in 0..(seconds * 60.0) as usize {
            out = mapper.map(linear(db), TICK_SECONDS);
        }
        out
    }

    #[test]
    fn silence_reads_zero() {
        let mut mapper = LevelMapper::new();
        assert_eq!(settle(&mut mapper, -120.0, 3.0), 0.0);
        assert_eq!(mapper.map(0.0, TICK_SECONDS), 0.0);
        assert_eq!(mapper.map(f64::NAN, TICK_SECONDS), 0.0);
    }

    #[test]
    fn normal_speech_lands_mid_to_high() {
        let mut mapper = LevelMapper::new();
        let level = settle(&mut mapper, -20.0, 3.0);
        assert!((0.6..=1.0).contains(&level), "level {level}");
        // Typical 5-15 % of full scale is no longer stuck at a third of the bars.
        for db in [-26.0, -16.0] {
            let level = settle(&mut LevelMapper::new(), db, 3.0);
            assert!(level >= 0.6, "{db} dBFS gave {level}");
        }
    }

    #[test]
    fn full_scale_reaches_one_and_does_not_stick() {
        let mut mapper = LevelMapper::new();
        assert_eq!(settle(&mut mapper, 0.0, 1.0), 1.0);
        // The very next quiet tick drops: no hold or smoothing here (the pill smooths).
        assert!(mapper.map(linear(-30.0), TICK_SECONDS) < 0.9);
        assert_eq!(mapper.map(0.0, TICK_SECONDS), 0.0);
    }

    #[test]
    fn auto_gain_lifts_a_quiet_talker() {
        let plain = soft_knee((-35.0 - FLOOR_DB) / (CEIL_DB - FLOOR_DB));
        let lifted = settle(&mut LevelMapper::new(), -35.0, 3.0);
        assert!(lifted > plain + 0.25, "lifted {lifted} vs plain {plain}");
        assert!(lifted >= 0.7, "lifted {lifted}");
    }

    #[test]
    fn floor_keeps_room_noise_flat() {
        assert_eq!(settle(&mut LevelMapper::new(), -60.0, 5.0), 0.0);
        // Louder room noise gets almost no gain and never reaches the top of the meter.
        let noise = settle(&mut LevelMapper::new(), -50.0, 5.0);
        assert!(noise <= 0.1, "noise {noise}");
    }

    #[test]
    fn pauses_of_a_quiet_talker_stay_flat() {
        let mut mapper = LevelMapper::new();
        settle(&mut mapper, -35.0, 3.0);
        let pause = settle(&mut mapper, -58.0, 0.3);
        assert!(pause <= 0.05, "pause {pause}");
    }

    #[test]
    fn gain_releases_slowly_after_a_loud_phrase() {
        let mut mapper = LevelMapper::new();
        settle(&mut mapper, -8.0, 1.0);
        // Right after a loud phrase the reference is still high, so quiet speech is not boosted.
        let soon = settle(&mut mapper, -35.0, 0.2);
        let later = settle(&mut mapper, -35.0, 12.0);
        assert!(soon < 0.5 && later > soon, "soon {soon} later {later}");
    }

    #[test]
    fn knee_is_monotonic_and_bounded() {
        let mut last = 0.0;
        for i in 0..=200 {
            let y = soft_knee(i as f64 / 100.0);
            assert!((0.0..=1.0).contains(&y) && y >= last);
            last = y;
        }
        assert_eq!(soft_knee(-1.0), 0.0);
        assert_eq!(soft_knee(1.5), 1.0);
    }
}
