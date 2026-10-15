//! Debug-only, content-free latency measurement (plan 082, F0).
//!
//! Active only when `VOICETYPR_PERF=1` is set on a debug build; `enabled()` is
//! `false` in release (the `cfg!` short-circuits), so every hook below returns
//! at its first line. Lines are `[PERF] take=<generation> ...` and carry numbers
//! only: no transcript, audio, path, clipboard or window title.
use crate::RecordingState;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const ENV_VAR: &str = "VOICETYPR_PERF";
/// A hotkey press older than this did not start the take that is beginning now.
const HOTKEY_FRESH_MS: u64 = 5_000;

static ENABLED: OnceLock<bool> = OnceLock::new();
static PENDING_HOTKEY_MS: AtomicU64 = AtomicU64::new(0);
static TAKE: Mutex<Take> = Mutex::new(Take::new());

/// Pure gate: the variable must be exactly `1` and the build must be a debug build.
pub fn enabled_from(var: Option<&str>, debug_build: bool) -> bool {
    debug_build && var == Some("1")
}

pub fn enabled() -> bool {
    cfg!(debug_assertions)
        && *ENABLED.get_or_init(|| {
            enabled_from(
                std::env::var(ENV_VAR).ok().as_deref(),
                cfg!(debug_assertions),
            )
        })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Unix-ms stamps for one take; `0` means "not reached".
#[derive(Clone, Copy)]
struct Take {
    generation: u64,
    hotkey_ms: u64,
    starting_ms: u64,
    recording_ms: u64,
    show_called_ms: u64,
    shown_ms: u64,
    island_ms: u64,
    /// Whether the pill window started this take hidden; `None` until the show ran.
    window_hidden: Option<bool>,
    timing_logged: bool,
    ended: bool,
}

impl Take {
    const fn new() -> Self {
        Self {
            generation: 0,
            hotkey_ms: 0,
            starting_ms: 0,
            recording_ms: 0,
            show_called_ms: 0,
            shown_ms: 0,
            island_ms: 0,
            window_hidden: None,
            timing_logged: false,
            ended: false,
        }
    }

    fn timing_line(&self) -> String {
        self.timing_line_for(crate::pill::native::show_mode())
    }

    /// `show_mode` lets an A/B run tell the hide/show and alpha-0 takes apart.
    fn timing_line_for(&self, mode: crate::pill::native::ShowMode) -> String {
        format!(
            "[PERF] take={} hotkey_ms={} starting_ms={} recording_ms={} pill_show_called_ms={} pill_shown_ms={} show_mode={}",
            self.generation,
            stamp(self.hotkey_ms),
            stamp(self.starting_ms),
            stamp(self.recording_ms),
            stamp(self.show_called_ms),
            stamp(self.shown_ms),
            mode.as_str(),
        )
    }
}

fn stamp(value: u64) -> String {
    if value == 0 {
        "-".to_string()
    } else {
        value.to_string()
    }
}

/// Record the hotkey press. The generation does not exist yet, so the stamp is
/// parked until `begin_take` claims it.
pub fn hotkey_pressed() {
    if enabled() {
        PENDING_HOTKEY_MS.store(now_ms(), Ordering::Relaxed);
    }
}

/// Start measuring the take whose generation was just opened.
pub fn begin_take(generation: u64) {
    if !enabled() {
        return;
    }
    let now = now_ms();
    let pending = PENDING_HOTKEY_MS.swap(0, Ordering::Relaxed);
    let hotkey_ms = if pending != 0 && now.saturating_sub(pending) <= HOTKEY_FRESH_MS {
        pending
    } else {
        now
    };
    if let Ok(mut take) = TAKE.lock() {
        *take = Take {
            generation,
            hotkey_ms,
            ..Take::new()
        };
    }
}

#[derive(Clone, Copy)]
pub enum Stage {
    PillShowCalled,
    PillShown,
}

/// Stamp a start-path stage; the timing line is logged once the pill is shown.
pub fn mark(generation: u64, stage: Stage) {
    if !enabled() {
        return;
    }
    let now = now_ms();
    let line = {
        let Ok(mut take) = TAKE.lock() else { return };
        if take.generation != generation {
            return;
        }
        match stage {
            Stage::PillShowCalled => {
                take.show_called_ms = now;
                None
            }
            Stage::PillShown => {
                take.shown_ms = now;
                if take.timing_logged {
                    None
                } else {
                    take.timing_logged = true;
                    Some(take.timing_line())
                }
            }
        }
    };
    if let Some(line) = line {
        log::info!("{line}");
    }
}

/// Called from `update_recording_state` after the transition settled.
pub fn on_state(state: RecordingState) {
    if !enabled() {
        return;
    }
    let now = now_ms();
    let (generation, timing) = {
        let Ok(mut take) = TAKE.lock() else { return };
        if take.generation == 0 || take.ended {
            return;
        }
        match state {
            RecordingState::Starting => {
                if take.starting_ms == 0 {
                    take.starting_ms = now;
                }
                return;
            }
            RecordingState::Recording => {
                if take.recording_ms == 0 {
                    take.recording_ms = now;
                }
                return;
            }
            RecordingState::Stopping
            | RecordingState::Transcribing
            | RecordingState::Idle
            | RecordingState::Error => {}
        }
        take.ended = true;
        let timing = if take.timing_logged {
            None
        } else {
            take.timing_logged = true;
            Some(take.timing_line())
        };
        (take.generation, timing)
    };
    if let Some(line) = timing {
        log::info!("{line}");
    }
    match rss_mb() {
        Some(mb) => log::info!("[PERF] take={generation} rss_mb={mb:.1}"),
        None => log::info!("[PERF] take={generation} rss_mb=-"),
    }
}

/// Record whether the pill window was hidden when this take asked to show it.
pub fn note_window_hidden(generation: u64, hidden: bool) {
    if !enabled() {
        return;
    }
    if let Ok(mut take) = TAKE.lock() {
        if take.generation == generation {
            take.window_hidden = Some(hidden);
        }
    }
}

/// Log key for the first visible frame. Only a take that started from a hidden
/// window measures hotkey -> island; a window left up by the previous take is a
/// separate field so the two are never mixed.
fn island_field(window_hidden: Option<bool>) -> &'static str {
    if window_hidden == Some(false) {
        "island_already_visible_ms"
    } else {
        "island_visible_ms"
    }
}

/// The pill reports the unix ms of the first frame showing the recording state.
pub fn island_visible(unix_ms: f64) {
    if !enabled() || !unix_ms.is_finite() || unix_ms <= 0.0 {
        return;
    }
    let ms = unix_ms.round() as u64;
    let (generation, field) = {
        let Ok(mut take) = TAKE.lock() else { return };
        if take.generation == 0 || take.island_ms != 0 {
            return;
        }
        take.island_ms = ms;
        (take.generation, island_field(take.window_hidden))
    };
    log::info!("[PERF] take={generation} {field}={ms}");
}

/// Numbers the pill sends at take end. Field names are the wire format.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct PillSummary {
    pub generation: u64,
    pub frames: u64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    pub over16: u64,
    pub over33: u64,
    pub over50: u64,
    pub level_events: u64,
    pub level_gap_p95_ms: f64,
    pub pill_dom_nodes: u64,
    pub stack_items: u64,
    pub longtasks: Option<u64>,
}

impl PillSummary {
    pub fn line(&self) -> String {
        format!(
            "[PERF] take={} frames={} p50_ms={:.1} p95_ms={:.1} max_ms={:.1} over16={} over33={} over50={} level_events={} level_gap_p95_ms={:.1} pill_dom_nodes={} stack_items={} longtasks={}",
            self.generation,
            self.frames,
            self.p50_ms,
            self.p95_ms,
            self.max_ms,
            self.over16,
            self.over33,
            self.over50,
            self.level_events,
            self.level_gap_p95_ms,
            self.pill_dom_nodes,
            self.stack_items,
            self.longtasks.map_or_else(|| "-".to_string(), |n| n.to_string()),
        )
    }
}

/// Resident memory of this process in MiB (sysinfo is cfg-safe on macOS and Windows).
fn rss_mb() -> Option<f64> {
    use sysinfo::{get_current_pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = get_current_pid().ok()?;
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    system
        .process(pid)
        .map(|process| process.memory() as f64 / (1024.0 * 1024.0))
}

/// Lets the pill ask once at startup whether to measure. Always `false` in release.
#[tauri::command]
pub fn perf_enabled() -> bool {
    enabled()
}

#[tauri::command]
pub fn perf_island_visible(unix_ms: f64) {
    island_visible(unix_ms);
}

#[tauri::command]
pub fn perf_pill_summary(summary: PillSummary) {
    if enabled() {
        log::info!("{}", summary.line());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_needs_exact_flag_and_a_debug_build() {
        assert!(enabled_from(Some("1"), true));
        assert!(!enabled_from(Some("1"), false));
        assert!(!enabled_from(Some("0"), true));
        assert!(!enabled_from(Some("true"), true));
        assert!(!enabled_from(None, true));
    }

    #[test]
    fn island_metric_only_counts_takes_that_started_hidden() {
        assert_eq!(island_field(Some(true)), "island_visible_ms");
        assert_eq!(island_field(None), "island_visible_ms");
        assert_eq!(island_field(Some(false)), "island_already_visible_ms");
    }

    #[test]
    fn timing_line_marks_unreached_stages() {
        let take = Take {
            generation: 7,
            hotkey_ms: 1000,
            starting_ms: 1004,
            recording_ms: 1120,
            ..Take::new()
        };
        assert_eq!(
            take.timing_line_for(crate::pill::native::ShowMode::Hide),
            "[PERF] take=7 hotkey_ms=1000 starting_ms=1004 recording_ms=1120 pill_show_called_ms=- pill_shown_ms=- show_mode=hide"
        );
        assert!(take
            .timing_line_for(crate::pill::native::ShowMode::Alpha)
            .ends_with("show_mode=alpha"));
    }

    #[test]
    fn summary_line_is_content_free_numbers() {
        let summary = PillSummary {
            generation: 3,
            frames: 600,
            p50_ms: 16.7,
            p95_ms: 18.04,
            max_ms: 55.0,
            over16: 12,
            over33: 2,
            over50: 1,
            level_events: 590,
            level_gap_p95_ms: 20.0,
            pill_dom_nodes: 210,
            stack_items: 0,
            longtasks: None,
        };
        assert_eq!(
            summary.line(),
            "[PERF] take=3 frames=600 p50_ms=16.7 p95_ms=18.0 max_ms=55.0 over16=12 over33=2 over50=1 level_events=590 level_gap_p95_ms=20.0 pill_dom_nodes=210 stack_items=0 longtasks=-"
        );
    }

    #[test]
    fn summary_deserializes_from_the_pill_payload() {
        let summary: PillSummary = serde_json::from_str(
            r#"{"generation":1,"frames":2,"p50_ms":1.0,"p95_ms":2.0,"max_ms":3.0,"over16":0,"over33":0,"over50":0,"level_events":4,"level_gap_p95_ms":5.0,"pill_dom_nodes":6,"stack_items":0,"longtasks":null}"#,
        )
        .unwrap();
        assert_eq!(summary.longtasks, None);
        assert_eq!(summary.level_events, 4);
    }
}
