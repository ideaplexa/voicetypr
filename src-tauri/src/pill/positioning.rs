//! Select the destination display before any recording-start awaits/UI work.
use std::sync::Mutex;
use tauri::{AppHandle, Monitor};
static START_MONITOR: Mutex<Option<Monitor>> = Mutex::new(None);

pub fn snapshot(app: &AppHandle) -> Option<Monitor> {
    live_monitor(app)
}
pub fn install(monitor: Option<Monitor>) {
    *START_MONITOR.lock().unwrap() = monitor;
}
pub fn monitor(app: &AppHandle) -> Option<Monitor> {
    if matches!(
        crate::get_recording_state(app),
        crate::RecordingState::Starting
            | crate::RecordingState::Recording
            | crate::RecordingState::Stopping
            | crate::RecordingState::Transcribing
    ) {
        if let Some(monitor) = START_MONITOR.lock().unwrap().clone() {
            return Some(monitor);
        }
    }
    live_monitor(app)
}
fn live_monitor(app: &AppHandle) -> Option<Monitor> {
    let monitors = crate::utils::monitor::catch_monitor_panic(|| app.available_monitors().ok())?
        .unwrap_or_default();
    // Win32 gives the foreground window's actual monitor, even when it spans displays.
    #[cfg(target_os = "windows")]
    let foreground = unsafe {
        use windows::Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::GetForegroundWindow};
        let hwnd = GetForegroundWindow();
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !hwnd.0.is_null()
            && GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL), &mut info).as_bool()
        {
            Some((
                (info.rcMonitor.left + info.rcMonitor.right) as f64 / 2.0,
                (info.rcMonitor.top + info.rcMonitor.bottom) as f64 / 2.0,
            ))
        } else {
            None
        }
    };
    #[cfg(not(target_os = "windows"))]
    let foreground = active_win_pos_rs::get_active_window()
        .ok()
        .filter(|w| w.position.width > 0.0 && w.position.height > 0.0)
        .map(|w| {
            (
                w.position.x + w.position.width / 2.0,
                w.position.y + w.position.height / 2.0,
            )
        });
    let contains = |m: &&Monitor, point: (f64, f64), logical: bool| {
        let scale = if logical { m.scale_factor() } else { 1.0 };
        let p = m.position();
        let s = m.size();
        point.0 >= p.x as f64 / scale
            && point.0 < (p.x as f64 + s.width as f64) / scale
            && point.1 >= p.y as f64 / scale
            && point.1 < (p.y as f64 + s.height as f64) / scale
    };
    foreground
        .and_then(|p| {
            monitors
                .iter()
                .find(|m| contains(m, p, cfg!(target_os = "macos")))
                .cloned()
        })
        .or_else(|| {
            app.cursor_position().ok().and_then(|p| {
                #[cfg(target_os = "macos")]
                let (point, logical) = {
                    let primary_scale = app.primary_monitor().ok().flatten()?.scale_factor();
                    ((p.x / primary_scale, p.y / primary_scale), true)
                };
                #[cfg(not(target_os = "macos"))]
                let (point, logical) = ((p.x, p.y), false);
                monitors
                    .iter()
                    .find(|m| contains(m, point, logical))
                    .cloned()
            })
        })
        .or_else(|| app.primary_monitor().ok().flatten())
}

/// Prefer a valid work area; auto-hidden taskbars legitimately equal full bounds.
pub fn select_area(work: (i32, i32, u32, u32), full: (i32, i32, u32, u32)) -> (i32, i32, u32, u32) {
    if work.2 > 0 && work.3 > 0 {
        work
    } else {
        full
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn taskbar_work_areas_and_fallback() {
        let full = (-1920, 0, 1920, 1080);
        assert_eq!(
            select_area((-1920, 0, 1920, 1040), full),
            (-1920, 0, 1920, 1040)
        );
        assert_eq!(
            select_area((-1880, 0, 1880, 1080), full),
            (-1880, 0, 1880, 1080)
        );
        assert_eq!(select_area(full, full), full);
        assert_eq!(select_area((0, 0, 0, 0), full), full);
    }
}
