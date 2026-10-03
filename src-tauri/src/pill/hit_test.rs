use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

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
pub struct PointerState {
    rects: Mutex<Vec<Rect>>,
    inside: AtomicBool,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}
#[tauri::command]
pub fn pill_set_hit_regions(window: WebviewWindow, rects: Vec<Rect>) -> Result<(), String> {
    if window.label() != "pill" {
        return Err("Pill window required".into());
    }
    if rects.len() > 32 || rects.iter().any(|r| !r.valid()) {
        return Err("Invalid hit regions".into());
    }
    *window.state::<PointerState>().rects.lock().unwrap() = rects;
    Ok(())
}
pub fn stop(app: &AppHandle) {
    reset_pointer(app);
    if let Some(task) = app.state::<PointerState>().task.lock().unwrap().take() {
        task.abort();
    }
}
fn reset_pointer(app: &AppHandle) {
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
            if !window.is_visible().unwrap_or(false) {
                reset_pointer(&task_app);
                break;
            }
            let point = match (
                task_app.cursor_position(),
                window.inner_position(),
                window.scale_factor(),
            ) {
                (Ok(cursor), Ok(origin), Ok(scale)) => {
                    // Tao reports the global macOS cursor using the primary
                    // display scale; window origins use their own display scale.
                    #[cfg(target_os = "macos")]
                    let cursor_scale = task_app
                        .primary_monitor()
                        .ok()
                        .flatten()
                        .map(|m| m.scale_factor())
                        .unwrap_or(scale);
                    #[cfg(not(target_os = "macos"))]
                    let cursor_scale = scale;
                    Some((
                        cursor.x / cursor_scale - origin.x as f64 / scale,
                        cursor.y / cursor_scale - origin.y as f64 / scale,
                    ))
                }
                _ => None,
            };
            let next = point.is_some_and(|(x, y)| {
                hit_test(
                    &task_app.state::<PointerState>().rects.lock().unwrap(),
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
fn pointer_moved(last: Option<(f64, f64)>, next: (f64, f64)) -> bool {
    last.is_none_or(|last| (last.0 - next.0).hypot(last.1 - next.1) > 1.0)
}
#[cfg(test)]
mod tests {
    use super::*;
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
