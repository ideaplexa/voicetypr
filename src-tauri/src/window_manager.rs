use crate::commands::settings::{
    DEFAULT_INDICATOR_OFFSET, MAX_INDICATOR_OFFSET, MIN_INDICATOR_OFFSET,
};
use crate::utils::logger::*;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub(crate) const PILL_WIDTH: f64 = crate::pill::geometry::CANVAS_WIDTH;
pub(crate) const PILL_HEIGHT: f64 = crate::pill::geometry::CANVAS_HEIGHT;

#[derive(Debug, Clone, Copy, PartialEq)]
struct DesktopArea {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl DesktopArea {
    const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WindowManager {
    app_handle: AppHandle,
    main_window: Arc<Mutex<Option<WebviewWindow>>>,
    pill_window: Arc<Mutex<Option<WebviewWindow>>>,
    pill_creation: Arc<tokio::sync::Mutex<()>>,
}

fn calculate_pill_position(
    position: &str,
    desktop_area: DesktopArea,
    edge_offset: f64,
) -> (f64, f64) {
    let pill_width = crate::pill::geometry::LEGACY_WIDTH;
    let pill_height = crate::pill::geometry::LEGACY_HEIGHT;

    // Horizontal position: left, center, or right
    let x = if position.ends_with("-left") {
        desktop_area.x + edge_offset
    } else if position.ends_with("-right") {
        desktop_area.x + desktop_area.width - pill_width - edge_offset
    } else {
        // center (default)
        desktop_area.x + (desktop_area.width - pill_width) / 2.0
    };

    // Vertical position: top or bottom
    let y = if position.starts_with("top-") {
        desktop_area.y + edge_offset
    } else {
        // bottom (default)
        desktop_area.y + desktop_area.height - pill_height - edge_offset
    };

    (x, y)
}

#[cfg(target_os = "macos")]
fn constrain_window_position(
    position: (f64, f64),
    window_size: (f64, f64),
    desktop_area: DesktopArea,
) -> (f64, f64) {
    let max_x = (desktop_area.x + desktop_area.width - window_size.0).max(desktop_area.x);
    let max_y = (desktop_area.y + desktop_area.height - window_size.1).max(desktop_area.y);
    (
        position.0.clamp(desktop_area.x, max_x),
        position.1.clamp(desktop_area.y, max_y),
    )
}

fn logical_desktop_area(
    physical_x: i32,
    physical_y: i32,
    physical_width: u32,
    physical_height: u32,
    scale_factor: f64,
) -> Option<DesktopArea> {
    if !scale_factor.is_finite()
        || scale_factor <= 0.0
        || physical_width == 0
        || physical_height == 0
    {
        return None;
    }

    Some(DesktopArea::new(
        physical_x as f64 / scale_factor,
        physical_y as f64 / scale_factor,
        physical_width as f64 / scale_factor,
        physical_height as f64 / scale_factor,
    ))
}

impl WindowManager {
    pub fn new(app_handle: AppHandle) -> Self {
        log_with_context(
            log::Level::Info,
            "Window manager initialization",
            &[("operation", "WINDOW_MANAGER_INIT"), ("stage", "startup")],
        );

        // Get reference to main window on creation
        let main_window = app_handle.get_webview_window("main");

        let main_available = main_window.is_some();
        log_with_context(
            log::Level::Debug,
            "Window manager setup",
            &[
                ("main_window_available", main_available.to_string().as_str()),
                ("pill_window_created", "false"),
            ],
        );

        let window_manager = Self {
            app_handle,
            main_window: Arc::new(Mutex::new(main_window)),
            pill_window: Arc::new(Mutex::new(None)),
            pill_creation: Arc::new(tokio::sync::Mutex::new(())),
        };

        log_with_context(
            log::Level::Info,
            "Window manager ready",
            &[("operation", "WINDOW_MANAGER_INIT"), ("result", "success")],
        );

        window_manager
    }

    /// Get the main window reference
    pub fn get_main_window(&self) -> Option<WebviewWindow> {
        match self.main_window.lock() {
            Ok(guard) => guard.clone(),
            Err(e) => {
                log::error!("Main window mutex is poisoned: {}", e);
                None
            }
        }
    }

    /// Get the pill window reference (validates window is still alive)
    pub fn get_pill_window(&self) -> Option<WebviewWindow> {
        let mut pill_guard = match self.pill_window.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Pill window mutex is poisoned: {}", e);
                return None;
            }
        };

        // Check if the window reference is still valid
        if let Some(ref window) = *pill_guard {
            // Verify the window still exists
            if window.is_closable().is_ok() {
                return Some(window.clone());
            } else {
                // Window is no longer valid, clear the reference
                log::debug!("Pill window reference is stale, clearing");
                *pill_guard = None;
            }
        }

        // Fallback: Check if pill window exists in Tauri but not in our cache
        if let Some(window) = self.app_handle.get_webview_window("pill") {
            if window.is_closable().is_ok() {
                log::debug!("Found pill window via app_handle fallback, caching it");
                *pill_guard = Some(window.clone());
                return Some(window);
            }
        }

        None
    }

    /// Check if pill window exists and is valid
    pub fn has_pill_window(&self) -> bool {
        self.get_pill_window().is_some()
    }

    /// Store the pill window reference
    pub fn set_pill_window(&self, window: WebviewWindow) {
        let mut pill_guard = match self.pill_window.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to lock pill window mutex for storing window: {}", e);
                return;
            }
        };
        *pill_guard = Some(window);
        log::debug!("Stored pill window reference");
    }

    /// Show the pill window, creating it if necessary (with retry logic)
    pub async fn show_pill_window(&self) -> Result<(), String> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_MS: u64 = 100;

        for attempt in 1..=MAX_RETRIES {
            match self.show_pill_window_internal().await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    crate::telemetry::native_error("show");
                    if attempt < MAX_RETRIES {
                        log::warn!(
                            "Failed to show pill window (attempt {}): {}. Retrying...",
                            attempt,
                            e
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                    } else {
                        return Err(format!(
                            "Failed to show pill window after {} attempts: {}",
                            MAX_RETRIES, e
                        ));
                    }
                }
            }
        }

        unreachable!()
    }

    /// Internal implementation of show_pill_window
    async fn show_pill_window_internal(&self) -> Result<(), String> {
        let _creation = self.pill_creation.lock().await;
        // Serialize creation, and drop the cache lock before dispatching UI work.
        if let Some(window) = self.get_pill_window() {
            let (x, y) = self.calculate_center_position();
            window
                .set_position(tauri::LogicalPosition::new(x, y))
                .map_err(|e| e.to_string())?;
            self.emit_pill_geometry(&window);
            crate::pill::native::show(&window)?;
            crate::pill::hit_test::start(&window);
            return Ok(());
        }

        // Close any orphaned pill window that might exist in Tauri but not in our cache
        // (This is outside the lock so the await is safe)
        if let Some(orphan) = self.app_handle.get_webview_window("pill") {
            log::debug!("Closing orphaned pill window before creating new one");
            let _ = orphan.close();
            // Small delay to ensure window is fully closed
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // Create new window
        log::info!("Creating new pill window (lazy-loaded on recording start)");

        // Always use fixed center-bottom position
        let (position_x, position_y) = self.calculate_center_position();
        log::info!(
            "Positioning pill at center-bottom: ({}, {})",
            position_x,
            position_y
        );

        let pill_builder = WebviewWindowBuilder::new(
            &self.app_handle,
            "pill",
            WebviewUrl::App("pill.html".into()),
        )
        .title("Recording")
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        // Debug builds can opt out so E2E runs can screenshot the island.
        .content_protected(
            !cfg!(debug_assertions) || std::env::var_os("VOICETYPR_E2E_CAPTURE_PILL").is_none(),
        )
        .decorations(false)
        .transparent(true)
        .shadow(false) // Disabled to fix Windows transparency issue
        .skip_taskbar(true)
        .inner_size(PILL_WIDTH, PILL_HEIGHT)
        .accept_first_mouse(true)
        .position(position_x, position_y)
        .visible(false) // Configure native safety before the first show
        .focusable(false)
        .focused(false); // Don't steal focus

        // Disable context menu only in production builds
        #[cfg(not(debug_assertions))]
        let pill_builder = pill_builder.initialization_script(
            "document.addEventListener('contextmenu', e => e.preventDefault());",
        );

        #[cfg(debug_assertions)]
        let pill_builder = pill_builder;

        let pill_window = pill_builder.build().map_err(|e| e.to_string())?;
        if let Err(error) = pill_window.set_ignore_cursor_events(true) {
            crate::telemetry::native_error("hit_testing");
            log::warn!("Failed to make pill window click-through: {}", error);
        }

        crate::pill::native::configure(&pill_window)?;
        self.emit_pill_geometry(&pill_window);
        crate::pill::native::show(&pill_window)?;
        crate::pill::hit_test::start(&pill_window);

        // Emit current recording state to the pill window
        let app_state = pill_window.app_handle().state::<crate::AppState>();
        let current_state = app_state.get_current_state();
        let _ = pill_window.emit(
            "recording-state-changed",
            serde_json::json!({
                "state": match current_state {
                    crate::RecordingState::Idle => "idle",
                    crate::RecordingState::Starting => "starting",
                    crate::RecordingState::Recording => "recording",
                    crate::RecordingState::Stopping => "stopping",
                    crate::RecordingState::Transcribing => "transcribing",
                    crate::RecordingState::Error => "error",
                },
                "error": null
            }),
        );

        self.set_pill_window(pill_window);

        log::info!(
            "Pill window created and shown at ({}, {})",
            position_x,
            position_y
        );

        // After creating the pill window, flush any queued critical events in the background
        let app_for_flush = self.app_handle.clone();
        tauri::async_runtime::spawn(async move {
            crate::flush_pill_event_queue(&app_for_flush).await;
        });

        Ok(())
    }

    /// Hide the pill window (don't close it) with retry logic
    pub async fn hide_pill_window(&self) -> Result<(), String> {
        if crate::commands::pill_feedback::pill_mode(&self.app_handle) == "never" {
            crate::commands::pill_feedback::release_feedback_ownership();
        }
        if crate::commands::pill_feedback::feedback_owned() {
            return Ok(());
        }
        crate::pill::hit_test::stop(&self.app_handle);
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_MS: u64 = 50;

        for attempt in 1..=MAX_RETRIES {
            // Get the window reference inside the retry loop to handle stale references
            let window = {
                match self.pill_window.lock() {
                    Ok(pill_guard) => pill_guard.clone(),
                    Err(e) => {
                        log::error!("Pill window mutex is poisoned during hide: {}", e);
                        break;
                    }
                }
            };

            if let Some(window) = window {
                // Verify window is still valid before trying to hide
                if window.is_closable().is_err() {
                    // Window is no longer valid, clear the reference
                    if let Ok(mut pill_guard) = self.pill_window.lock() {
                        *pill_guard = None;
                        log::debug!("Pill window reference is stale during hide, cleared");
                    }
                    return Ok(());
                }

                match window.hide() {
                    Ok(_) => {
                        log::info!("Pill window hidden");
                        return Ok(());
                    }
                    Err(e) => {
                        // Check if error is because window was closed
                        if !window.is_closable().unwrap_or(false) {
                            // Window was closed, clear the reference
                            if let Ok(mut pill_guard) = self.pill_window.lock() {
                                *pill_guard = None;
                                log::debug!("Pill window was closed during hide attempt");
                            }
                            return Ok(());
                        }

                        if attempt < MAX_RETRIES {
                            log::warn!(
                                "Failed to hide pill window (attempt {}): {}. Retrying...",
                                attempt,
                                e
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS))
                                .await;
                        } else {
                            return Err(format!(
                                "Failed to hide pill window after {} attempts: {}",
                                MAX_RETRIES, e
                            ));
                        }
                    }
                }
            } else {
                // No window to hide
                return Ok(());
            }
        }

        Ok(())
    }

    /// Close the pill window (actually destroy it)
    pub async fn close_pill_window(&self) -> Result<(), String> {
        crate::commands::pill_feedback::renderer_closed();
        crate::pill::hit_test::stop(&self.app_handle);
        // Take the window out of the mutex
        let window = {
            match self.pill_window.lock() {
                Ok(mut pill_guard) => pill_guard.take(),
                Err(e) => {
                    log::error!("Pill window mutex is poisoned during close: {}", e);
                    return Ok(());
                }
            }
        };

        if let Some(window) = window {
            // Hide first
            let _ = window.hide();

            // Then close
            window.close().map_err(|e| e.to_string())?;
            log::info!("Pill window closed");
        }

        Ok(())
    }

    /// Emit event to specific window
    pub fn emit_to_window(
        &self,
        window_id: &str,
        event: &str,
        payload: serde_json::Value,
    ) -> Result<(), String> {
        // Only log critical events
        if matches!(event, "recording-state-changed" | "transcription-complete") {
            log::debug!("emit_to_window: window='{}', event='{}'", window_id, event);
        }

        let window = match window_id {
            "main" => self.get_main_window(),
            "pill" => self.get_pill_window(),
            _ => None,
        };

        if let Some(window) = window {
            // Skip visibility check for performance

            // Check if window is visible or if it's a critical event
            let is_critical = matches!(event, "recording-state-changed" | "transcription-complete");

            // Check if window is visible or if it's a critical event

            match window.emit(event, payload.clone()) {
                Ok(_) => {}
                Err(e) => {
                    if is_critical {
                        log::error!(
                            "[FLOW] Failed to emit critical event '{}' to {} window: {}",
                            event,
                            window_id,
                            e
                        );
                        // For critical events, retry with app-wide emission
                        if let Err(e2) = self.app_handle.emit(event, payload) {
                            log::error!("Also failed app-wide emission: {}", e2);
                        }
                    } else {
                        log::debug!(
                            "Failed to emit '{}' event to {} window: {}",
                            event,
                            window_id,
                            e
                        );
                    }
                    return Err(e.to_string());
                }
            }
        } else {
            log::debug!(
                "Cannot emit '{}' event - {} window not found",
                event,
                window_id
            );
            // For critical events when window not found, try app-wide emission
            let is_critical = matches!(event, "recording-state-changed" | "transcription-complete");

            // Queue critical pill events so they can be delivered when the pill window is created
            if is_critical && window_id == "pill" {
                let app_state = self.app_handle.state::<crate::AppState>();
                app_state.queue_pill_event(event, payload.clone());
            }

            if is_critical {
                if let Err(e) = self.app_handle.emit(event, payload) {
                    log::error!("App-wide emission also failed: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Emit event to pill window only
    pub fn emit_to_pill(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        self.emit_to_window("pill", event, payload)
    }

    /// Emit event to main window only
    pub fn emit_to_main(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        self.emit_to_window("main", event, payload)
    }

    /// Check if pill window is visible
    pub fn is_pill_visible(&self) -> bool {
        let pill_guard = match self.pill_window.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!(
                    "Pill window mutex is poisoned during visibility check: {}",
                    e
                );
                return false;
            }
        };

        if let Some(ref window) = *pill_guard {
            // Check both that window is valid and visible
            if window.is_closable().is_ok() {
                window.is_visible().unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Get the current pill indicator position from settings
    fn get_pill_position_setting(&self) -> String {
        use tauri_plugin_store::StoreExt;
        if let Ok(store) = self.app_handle.store("settings") {
            store
                .get("pill_indicator_position")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "bottom-center".to_string())
        } else {
            "bottom-center".to_string()
        }
    }

    /// Get the current pill indicator offset from settings (in pixels)
    fn get_pill_offset_setting(&self) -> f64 {
        use tauri_plugin_store::StoreExt;
        if let Ok(store) = self.app_handle.store("settings") {
            store
                .get("pill_indicator_offset")
                .and_then(|v| v.as_u64())
                .map(|v| v.clamp(MIN_INDICATOR_OFFSET as u64, MAX_INDICATOR_OFFSET as u64) as f64)
                .unwrap_or(DEFAULT_INDICATOR_OFFSET as f64)
        } else {
            DEFAULT_INDICATOR_OFFSET as f64
        }
    }

    fn calculate_pill_canvas_position_for(&self, position: &str) -> (f64, f64) {
        let desktop_area = self.get_positioning_area();
        let edge_offset = self.get_pill_offset_setting();
        let pill_position = calculate_pill_position(position, desktop_area, edge_offset);

        #[cfg(target_os = "macos")]
        let pill_position = constrain_window_position(
            pill_position,
            (
                crate::pill::geometry::LEGACY_WIDTH,
                crate::pill::geometry::LEGACY_HEIGHT,
            ),
            desktop_area,
        );

        crate::pill::geometry::Geometry::new(position).canvas_origin(pill_position)
    }

    /// Work area of the app/cursor display captured at recording start.
    fn get_positioning_area(&self) -> DesktopArea {
        crate::pill::positioning::monitor(&self.app_handle)
            .and_then(|monitor| {
                let work = monitor.work_area();
                let pos = monitor.position();
                let size = monitor.size();
                let (x, y, width, height) = crate::pill::positioning::select_area(
                    (
                        work.position.x,
                        work.position.y,
                        work.size.width,
                        work.size.height,
                    ),
                    (pos.x, pos.y, size.width, size.height),
                );
                logical_desktop_area(x, y, width, height, monitor.scale_factor())
            })
            .unwrap_or_else(|| DesktopArea::new(0.0, 0.0, 1920.0, 1080.0))
    }

    fn emit_pill_geometry(&self, window: &WebviewWindow) {
        let _ = self.app_handle.emit_to(
            window.label(),
            "pill-geometry",
            crate::pill::geometry::Geometry::new(&self.get_pill_position_setting()),
        );
    }

    fn calculate_center_position(&self) -> (f64, f64) {
        let position = self.get_pill_position_setting();
        self.calculate_pill_canvas_position_for(&position)
    }

    /// Reposition pill window using the current placement setting.
    /// Called when monitor configuration changes (display connect/disconnect, resolution change).
    pub fn reposition_floating_windows(&self) {
        use tauri::LogicalPosition;

        let position = self.get_pill_position_setting();
        let (pill_x, pill_y) = self.calculate_pill_canvas_position_for(&position);

        // Reposition pill window
        if let Some(pill) = self.get_pill_window() {
            self.emit_pill_geometry(&pill);
            if let Err(e) = pill.set_position(LogicalPosition::new(pill_x, pill_y)) {
                crate::telemetry::native_error("positioning");
                log::warn!("Failed to reposition pill window: {}", e);
            } else {
                log::info!("Repositioned pill window to ({}, {})", pill_x, pill_y);
            }
        }
    }

    /// Reposition pill window to a specific position.
    /// Called when the pill_indicator_position setting changes.
    pub fn reposition_floating_windows_with_position(&self, position: &str) {
        use tauri::LogicalPosition;

        let (pill_x, pill_y) = self.calculate_pill_canvas_position_for(position);

        // Reposition pill window
        if let Some(pill) = self.get_pill_window() {
            self.emit_pill_geometry(&pill);
            if let Err(e) = pill.set_position(LogicalPosition::new(pill_x, pill_y)) {
                crate::telemetry::native_error("positioning");
                log::warn!("Failed to reposition pill window: {}", e);
            } else {
                log::info!(
                    "Repositioned pill window to ({}, {}) for position '{}'",
                    pill_x,
                    pill_y,
                    position
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{calculate_pill_position, logical_desktop_area, DesktopArea};

    fn full_screen() -> DesktopArea {
        DesktopArea::new(0.0, 0.0, 1920.0, 1080.0)
    }

    // Screen: 1920x1080, pill: 260x64, edge_offset: 10
    // x_left = 10, x_center = 830, x_right = 1650
    // y_top = 10, y_bottom = 1006

    #[test]
    fn taskbar_selection_keeps_ten_pixel_offset() {
        let full = (-1920, 0, 1920, 1080);
        for work in [(-1920, 0, 1920, 1040), (-1880, 0, 1880, 1080), full] {
            let selected = crate::pill::positioning::select_area(work, full);
            for scale in [1.0, 2.0] {
                let area =
                    logical_desktop_area(selected.0, selected.1, selected.2, selected.3, scale)
                        .unwrap();
                let position = calculate_pill_position("bottom-left", area, 10.0);
                assert_eq!(position.0, area.x + 10.0);
                assert_eq!(
                    position.1 + crate::pill::geometry::LEGACY_HEIGHT,
                    area.y + area.height - 10.0
                );
            }
        }
    }

    #[test]
    fn calculate_pill_position_top_left() {
        let (x, y) = calculate_pill_position("top-left", full_screen(), 10.0);
        assert_eq!(x, 10.0);
        assert_eq!(y, 10.0);
    }

    #[test]
    fn calculate_pill_position_top_center() {
        let (x, y) = calculate_pill_position("top-center", full_screen(), 10.0);
        assert_eq!(x, 830.0);
        assert_eq!(y, 10.0);
    }

    #[test]
    fn calculate_pill_position_top_right() {
        let (x, y) = calculate_pill_position("top-right", full_screen(), 10.0);
        assert_eq!(x, 1650.0);
        assert_eq!(y, 10.0);
    }

    #[test]
    fn calculate_pill_position_bottom_left() {
        let (x, y) = calculate_pill_position("bottom-left", full_screen(), 10.0);
        assert_eq!(x, 10.0);
        assert_eq!(y, 1006.0);
    }

    #[test]
    fn calculate_pill_position_bottom_center() {
        let (x, y) = calculate_pill_position("bottom-center", full_screen(), 10.0);
        assert_eq!(x, 830.0);
        assert_eq!(y, 1006.0);
    }

    #[test]
    fn calculate_pill_position_bottom_right() {
        let (x, y) = calculate_pill_position("bottom-right", full_screen(), 10.0);
        assert_eq!(x, 1650.0);
        assert_eq!(y, 1006.0);
    }

    #[test]
    fn calculate_pill_position_defaults_to_bottom_center() {
        let (x, y) = calculate_pill_position("unknown", full_screen(), 10.0);
        assert_eq!(x, 830.0);
        assert_eq!(y, 1006.0);
    }

    #[test]
    fn calculate_pill_position_with_custom_offset() {
        // Test with 50px offset
        let (x, y) = calculate_pill_position("bottom-left", full_screen(), 50.0);
        assert_eq!(x, 50.0);
        assert_eq!(y, 966.0); // 1080 - 64 - 50
    }

    #[test]
    fn bottom_pill_uses_visible_area_above_dock() {
        // 1024x768 display with a 25px menu bar and 47px bottom Dock.
        let visible_area = DesktopArea::new(0.0, 25.0, 1024.0, 696.0);
        assert_eq!(
            calculate_pill_position("bottom-center", visible_area, 10.0),
            (382.0, 647.0)
        );
    }

    #[test]
    fn pill_position_preserves_offset_on_nonzero_monitor_origin() {
        let visible_area = DesktopArea::new(-1440.0, 20.0, 1440.0, 840.0);
        assert_eq!(
            calculate_pill_position("bottom-right", visible_area, 50.0),
            (-310.0, 746.0)
        );
    }

    #[test]
    fn physical_work_area_converts_to_logical_global_coordinates() {
        assert_eq!(
            logical_desktop_area(-2880, 50, 2880, 1680, 2.0),
            Some(DesktopArea::new(-1440.0, 25.0, 1440.0, 840.0))
        );
    }

    #[test]
    fn invalid_work_area_is_rejected_for_full_monitor_fallback() {
        assert_eq!(logical_desktop_area(0, 0, 0, 1080, 1.0), None);
        assert_eq!(logical_desktop_area(0, 0, 1920, 1080, 0.0), None);
        assert_eq!(logical_desktop_area(0, 0, 1920, 1080, f64::NAN), None);
    }
}
