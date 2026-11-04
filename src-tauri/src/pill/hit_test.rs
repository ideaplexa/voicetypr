#[cfg(any(target_os = "macos", test))]
use std::{
    sync::{atomic::AtomicU64, Arc},
    time::Instant,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

#[cfg(target_os = "macos")]
#[path = "cursor.rs"]
mod cursor;

#[cfg(any(target_os = "macos", test))]
const POINTER_QUERY_MAX_AGE: Duration = Duration::from_millis(200);

#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
struct PointerQuery {
    pending: AtomicBool,
    generation: AtomicU64,
    completed: Mutex<Option<(Instant, (f64, f64))>>,
}

#[cfg(any(target_os = "macos", test))]
struct PendingPointerQuery(Arc<PointerQuery>);

#[cfg(any(target_os = "macos", test))]
impl Drop for PendingPointerQuery {
    fn drop(&mut self) {
        self.0.pending.store(false, Ordering::Release);
    }
}

#[cfg(any(target_os = "macos", test))]
impl PointerQuery {
    fn invalidate(&self) {
        let mut completed = self.completed.lock().unwrap();
        self.generation.fetch_add(1, Ordering::AcqRel);
        *completed = None;
        // Do not clear pending: an old task's queued closure still owns that
        // slot until it runs, even if the pointer task was stopped/restarted.
    }

    fn reset_if_current(&self, generation: u64, reset: impl FnOnce()) {
        let mut completed = self.completed.lock().unwrap();
        if self.generation.load(Ordering::Acquire) != generation {
            return;
        }
        *completed = None;
        // Hold the invalidation lock through the hover reset, so a stopped
        // synchronous poll cannot reset a newly started task's pointer state.
        // Only stop/start invalidate the generation, never the poll itself.
        reset();
    }

    fn latest(&self, now: Instant) -> Option<(f64, f64)> {
        self.completed
            .lock()
            .unwrap()
            .filter(|(at, _)| now.duration_since(*at) <= POINTER_QUERY_MAX_AGE)
            .map(|(_, point)| point)
    }

    fn enqueue(
        self: &Arc<Self>,
        generation: u64,
        queued_at: Instant,
        dispatch: impl FnOnce(Box<dyn FnOnce() + Send>) -> bool,
        query: impl FnOnce() -> Option<(f64, f64)> + Send + 'static,
    ) {
        if self
            .pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let pending = PendingPointerQuery(self.clone());
        let _ = dispatch(Box::new(move || {
            // The closure owns the slot even if dispatch accepts but drops it
            // without running (e.g. during event-loop shutdown).
            let _pending = pending;
            let state = &_pending.0;
            // Main may have been blocked for seconds (e.g. alpha show mode).
            // Skip stale work before touching AppKit; never wait on a receiver.
            let point = if queued_at.elapsed() <= POINTER_QUERY_MAX_AGE
                && state.generation.load(Ordering::Acquire) == generation
            {
                crate::utils::monitor::catch_monitor_panic(query).flatten()
            } else {
                None
            };
            let mut completed = state.completed.lock().unwrap();
            if state.generation.load(Ordering::Acquire) == generation {
                *completed = point.map(|point| (Instant::now(), point));
            }
        }));
    }
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Rect {
    fn valid(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
            && self.width > 0.0
            && self.height > 0.0
    }
    fn contains(&self, x: f64, y: f64, margin: f64) -> bool {
        x >= self.x - margin
            && x <= self.x + self.width + margin
            && y >= self.y - margin
            && y <= self.y + self.height + margin
    }
}
pub fn hit_test(rects: &[Rect], x: f64, y: f64, was_inside: bool) -> bool {
    rects
        .iter()
        .any(|r| r.contains(x, y, if was_inside { 2.0 } else { 0.0 }))
}
#[derive(Default)]
struct Regions {
    click: Vec<Rect>,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Windows uses the CSS cursor.
    actions: Vec<Rect>,
}
#[derive(Default)]
pub struct PointerState {
    regions: Mutex<Regions>,
    inside: AtomicBool,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    #[cfg(target_os = "macos")]
    query: Arc<PointerQuery>,
    #[cfg(target_os = "macos")]
    cursor_owned: AtomicBool,
}
#[tauri::command]
pub fn pill_set_hit_regions(
    window: WebviewWindow,
    rects: Vec<Rect>,
    action_rects: Option<Vec<Rect>>,
) -> Result<(), String> {
    if window.label() != "pill" {
        return Err("Pill window required".into());
    }
    let actions = action_rects.unwrap_or_default();
    if rects.len() > 32 || actions.len() > 32 || rects.iter().chain(&actions).any(|r| !r.valid()) {
        return Err("Invalid hit regions".into());
    }
    *window.state::<PointerState>().regions.lock().unwrap() = Regions {
        click: rects,
        actions,
    };
    Ok(())
}
pub fn stop(app: &AppHandle) {
    reset_pointer(app);
    if let Some(task) = app.state::<PointerState>().task.lock().unwrap().take() {
        task.abort();
    }
}
fn reset_pointer(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    app.state::<PointerState>().query.invalidate();
    reset_hover(app);
}
fn reset_hover(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    cursor::reset(app);
    if app
        .state::<PointerState>()
        .inside
        .swap(false, Ordering::Relaxed)
    {
        let _ = app.emit_to("pill", "pill-pointer", serde_json::json!({"inside": false}));
    }
}
pub fn start(window: &WebviewWindow) {
    let app = window.app_handle().clone();
    let state = app.state::<PointerState>();
    let mut task_slot = state.task.lock().unwrap();
    if task_slot
        .as_ref()
        .is_some_and(|task| !task.inner().is_finished())
    {
        return;
    }
    if let Some(task) = task_slot.take() {
        task.abort();
    }
    let window = window.clone();
    let task_app = app.clone();
    reset_pointer(&app);
    #[cfg(target_os = "macos")]
    let (query, generation) = {
        let query = state.query.clone();
        let generation = query.generation.load(Ordering::Acquire);
        (query, generation)
    };
    let task = tauri::async_runtime::spawn(async move {
        let mut inside = false;
        let mut last_emitted = None;
        if window.set_ignore_cursor_events(true).is_err() {
            crate::telemetry::native_error("hit_testing");
        }
        let mut interval = tokio::time::interval(Duration::from_nanos(1_000_000_000 / 30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if !crate::pill::native::is_shown(&window) {
                #[cfg(target_os = "macos")]
                query.reset_if_current(generation, || reset_hover(&task_app));
                #[cfg(not(target_os = "macos"))]
                reset_hover(&task_app);
                break;
            }
            // Tao's cursor_position ALSO reads NSScreen to obtain its scale.
            // Consume the latest completed read without waiting on main. At
            // most one cursor/window query may be queued, including across
            // task restarts. Monitor enumeration stays cached for 500 ms.
            #[cfg(target_os = "macos")]
            let point = {
                let query_app = task_app.clone();
                let query_window = window.clone();
                let point = query.latest(Instant::now());
                query.enqueue(
                    generation,
                    Instant::now(),
                    |work| task_app.run_on_main_thread(work).is_ok(),
                    move || {
                        let point = pointer_position(&query_app, &query_window);
                        cursor::update(&query_app, &query_window, point, generation);
                        point
                    },
                );
                point
            };
            #[cfg(not(target_os = "macos"))]
            let point = pointer_position(&task_app, &window);
            let next = point.is_some_and(|(x, y)| {
                hit_test(
                    &task_app
                        .state::<PointerState>()
                        .regions
                        .lock()
                        .unwrap()
                        .click,
                    x,
                    y,
                    inside,
                )
            });
            let changed = next != inside;
            let applied = !changed || window.set_ignore_cursor_events(!next).is_ok();
            if changed && !applied {
                crate::telemetry::native_error("hit_testing");
            }
            if changed && applied {
                inside = next;
                task_app
                    .state::<PointerState>()
                    .inside
                    .store(inside, Ordering::Relaxed);
                if !inside {
                    last_emitted = None;
                    // One exit transition clears hover; no outside polling events.
                    let _ = task_app.emit_to(
                        "pill",
                        "pill-pointer",
                        serde_json::json!({"inside": false}),
                    );
                }
            }
            if inside {
                if let Some((x, y)) = point {
                    if pointer_moved(last_emitted, (x, y)) {
                        let _ = task_app.emit_to(
                            "pill",
                            "pill-pointer",
                            serde_json::json!({"inside": true, "x": x, "y": y}),
                        );
                        last_emitted = Some((x, y));
                    }
                }
            }
        }
    });
    *task_slot = Some(task);
}
fn pointer_position(app: &AppHandle, window: &WebviewWindow) -> Option<(f64, f64)> {
    crate::utils::monitor::catch_monitor_panic(|| {
        match (
            app.cursor_position(),
            window.inner_position(),
            window.scale_factor(),
        ) {
            (Ok(cursor), Ok(origin), Ok(scale)) => {
                // Global cursor coordinates use the primary display's scale;
                // the window origin uses its own display's scale.
                #[cfg(target_os = "macos")]
                let cursor_scale = crate::utils::monitor::macos_geometry(app)
                    .map(|geometry| geometry.primary.scale_factor())
                    .unwrap_or(scale);
                #[cfg(not(target_os = "macos"))]
                let cursor_scale = scale;
                Some((
                    cursor.x / cursor_scale - origin.x as f64 / scale,
                    cursor.y / cursor_scale - origin.y as f64 / scale,
                ))
            }
            _ => None,
        }
    })
    .flatten()
}
fn pointer_moved(last: Option<(f64, f64)>, next: (f64, f64)) -> bool {
    last.is_none_or(|last| (last.0 - next.0).hypot(last.1 - next.1) > 1.0)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_query_never_waits_or_queues_behind_itself() {
        let state = Arc::new(PointerQuery::default());
        let mut queued = Vec::new();
        for _ in 0..300 {
            state.enqueue(
                0,
                Instant::now(),
                |work| {
                    queued.push(work);
                    true
                },
                || Some((12.0, 34.0)),
            );
        }
        assert_eq!(queued.len(), 1);
        assert_eq!(state.latest(Instant::now()), None);
        assert!(state.pending.load(Ordering::Acquire));
        queued.pop().unwrap()();
        assert_eq!(state.latest(Instant::now()), Some((12.0, 34.0)));
        assert!(!state.pending.load(Ordering::Acquire));
        state.enqueue(
            0,
            Instant::now(),
            |work| {
                queued.push(work);
                true
            },
            || Some((56.0, 78.0)),
        );
        // A pending refresh does not discard the last completed result.
        assert_eq!(queued.len(), 1);
        assert_eq!(state.latest(Instant::now()), Some((12.0, 34.0)));
        queued.pop().unwrap()();
        assert_eq!(state.latest(Instant::now()), Some((56.0, 78.0)));
    }

    #[test]
    fn pointer_query_skips_stale_work_and_expires_old_results() {
        let state = Arc::new(PointerQuery::default());
        state.enqueue(
            0,
            Instant::now() - POINTER_QUERY_MAX_AGE - Duration::from_millis(1),
            |work| {
                work();
                true
            },
            || panic!("stale work must not query AppKit"),
        );
        assert!(!state.pending.load(Ordering::Acquire));
        assert_eq!(state.latest(Instant::now()), None);
        let now = Instant::now();
        *state.completed.lock().unwrap() = Some((now, (1.0, 2.0)));
        assert_eq!(state.latest(now + POINTER_QUERY_MAX_AGE), Some((1.0, 2.0)));
        assert_eq!(
            state.latest(now + POINTER_QUERY_MAX_AGE + Duration::from_millis(1)),
            None
        );
    }

    #[test]
    fn pointer_query_restart_keeps_one_slot_and_discards_the_old_generation() {
        let state = Arc::new(PointerQuery::default());
        let mut queued = None;
        state.enqueue(
            0,
            Instant::now(),
            |work| {
                queued = Some(work);
                true
            },
            || panic!("old task must not query AppKit after restart"),
        );
        state.invalidate();
        assert_eq!(state.generation.load(Ordering::Acquire), 1);
        state.enqueue(
            1,
            Instant::now(),
            |_| panic!("must not dispatch behind the stopped task"),
            || Some((3.0, 4.0)),
        );
        queued.unwrap()();
        assert_eq!(state.latest(Instant::now()), None);
        state.enqueue(
            1,
            Instant::now(),
            |work| {
                work();
                true
            },
            || Some((3.0, 4.0)),
        );
        assert_eq!(state.latest(Instant::now()), Some((3.0, 4.0)));
        state.invalidate();
        assert_eq!(state.latest(Instant::now()), None);
    }

    #[test]
    fn old_hidden_poll_cannot_reset_a_restarted_generation() {
        let state = Arc::new(PointerQuery::default());
        let generation = state.generation.load(Ordering::Acquire);
        let hidden_observed = Arc::new(std::sync::Barrier::new(2));
        let resume_reset = Arc::new(std::sync::Barrier::new(2));
        let old_poll = {
            let state = state.clone();
            let hidden_observed = hidden_observed.clone();
            let resume_reset = resume_reset.clone();
            std::thread::spawn(move || {
                hidden_observed.wait();
                resume_reset.wait();
                state.reset_if_current(generation, || {
                    panic!("a stopped poll must not reset the new task's hover");
                });
            })
        };
        hidden_observed.wait();
        state.invalidate(); // stop
        state.invalidate(); // start
        let generation = state.generation.load(Ordering::Acquire);
        state.enqueue(
            generation,
            Instant::now(),
            |work| {
                work();
                true
            },
            || Some((3.0, 4.0)),
        );
        resume_reset.wait();
        old_poll.join().unwrap();
        assert_eq!(state.generation.load(Ordering::Acquire), generation);
        assert_eq!(state.latest(Instant::now()), Some((3.0, 4.0)));
        let mut reset = false;
        state.reset_if_current(generation, || reset = true);
        assert!(reset);
        assert_eq!(state.generation.load(Ordering::Acquire), generation);
        assert_eq!(state.latest(Instant::now()), None);
        state.enqueue(
            generation,
            Instant::now(),
            |work| {
                work();
                true
            },
            || Some((5.0, 6.0)),
        );
        assert_eq!(state.latest(Instant::now()), Some((5.0, 6.0)));
    }

    #[test]
    fn accepted_but_dropped_pointer_query_releases_the_slot() {
        let state = Arc::new(PointerQuery::default());
        state.enqueue(
            0,
            Instant::now(),
            |work| {
                drop(work);
                true
            },
            || panic!("dropped work must not query AppKit"),
        );
        assert!(!state.pending.load(Ordering::Acquire));
        state.enqueue(
            0,
            Instant::now(),
            |work| {
                work();
                true
            },
            || Some((1.0, 2.0)),
        );
        assert!(!state.pending.load(Ordering::Acquire));
        assert_eq!(state.latest(Instant::now()), Some((1.0, 2.0)));
    }

    #[test]
    fn pointer_query_dispatch_failure_or_panic_releases_the_slot() {
        let state = Arc::new(PointerQuery::default());
        state.enqueue(0, Instant::now(), |_| false, || Some((1.0, 2.0)));
        assert!(!state.pending.load(Ordering::Acquire));
        state.enqueue(
            0,
            Instant::now(),
            |work| {
                work();
                true
            },
            || panic!("stale monitor handle"),
        );
        assert!(!state.pending.load(Ordering::Acquire));
        assert_eq!(state.latest(Instant::now()), None);
    }

    #[test]
    fn movement_threshold() {
        assert!(pointer_moved(None, (0.0, 0.0)));
        assert!(!pointer_moved(Some((0.0, 0.0)), (1.0, 0.0)));
        assert!(pointer_moved(Some((0.0, 0.0)), (1.01, 0.0)));
    }
    #[test]
    fn hit_regions_and_hysteresis() {
        let regions = [
            Rect {
                x: 10.0,
                y: 20.0,
                width: 12.0,
                height: 12.0,
            },
            Rect {
                x: 40.0,
                y: 50.0,
                width: 30.0,
                height: 10.0,
            },
        ];
        assert!(hit_test(&regions, 10.0, 20.0, false));
        assert!(hit_test(&regions, 70.0, 60.0, false));
        assert!(!hit_test(&regions, 9.0, 20.0, false));
        assert!(hit_test(&regions, 8.0, 18.0, true));
        assert!(!hit_test(&regions, 7.9, 20.0, true));
        assert!(!hit_test(&regions, 25.0, 30.0, true));
        assert!(!hit_test(&[], 10.0, 20.0, true));
        assert!(!Rect {
            x: f64::NAN,
            y: 0.0,
            width: 1.0,
            height: 1.0
        }
        .valid());
    }
}
