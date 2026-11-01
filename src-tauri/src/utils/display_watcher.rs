use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::AppState;

/// Watches for display/monitor configuration changes and repositions floating windows.
pub struct DisplayWatcher {
    started: Arc<AtomicBool>,
    app: AppHandle,
}

impl DisplayWatcher {
    pub fn new(app: AppHandle) -> Self {
        Self {
            started: Arc::new(AtomicBool::new(false)),
            app,
        }
    }

    /// Start watching for display changes.
    /// On macOS, uses Core Graphics display reconfiguration callback.
    pub fn start(&self) {
        if self.started.swap(true, Ordering::SeqCst) {
            log::debug!("DisplayWatcher already running, skipping start");
            return;
        }

        log::info!("Starting DisplayWatcher for monitor configuration changes");

        #[cfg(target_os = "macos")]
        self.start_macos();

        #[cfg(target_os = "windows")]
        self.start_windows();
    }

    #[cfg(target_os = "macos")]
    fn start_macos(&self) {
        use core_graphics::display::CGDisplayRegisterReconfigurationCallback;

        // Store app handle in a thread-safe way for the callback
        let app_handle = self.app.clone();

        // Register the callback
        unsafe extern "C" fn display_callback(
            _display: u32,
            flags: u32,
            user_info: *const std::ffi::c_void,
        ) {
            // CGDisplayChangeSummaryFlags::kCGDisplayBeginConfigurationFlag = 1
            // We only want to handle the end of configuration (when flag is not set)
            if flags & 1 != 0 {
                return; // This is the beginning of a change, wait for end
            }

            log::info!("Display configuration changed, repositioning windows");
            let app_ptr = user_info as *const AppHandle;
            if !app_ptr.is_null() {
                let app = &*app_ptr;
                crate::utils::monitor::invalidate_macos_geometry(app);
                let app_state = app.state::<AppState>();
                if let Some(window_manager) = app_state.get_window_manager() {
                    window_manager.reposition_floating_windows();
                }
            }
        }

        // Leak the app handle so it lives for the lifetime of the app
        let app_box = Box::new(app_handle);
        let app_ptr = Box::into_raw(app_box) as *const std::ffi::c_void;

        unsafe {
            let result = CGDisplayRegisterReconfigurationCallback(display_callback, app_ptr);
            if result == 0 {
                log::info!("Successfully registered display reconfiguration callback");
            } else {
                log::error!(
                    "Failed to register display reconfiguration callback: {}",
                    result
                );
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn start_windows(&self) {
        // On Windows, we use a polling approach since WM_DISPLAYCHANGE requires a window message loop
        // The polling checks for monitor count/resolution changes every 2 seconds
        use std::thread;
        use std::time::Duration;

        let app = self.app.clone();
        let started = self.started.clone();

        thread::spawn(move || {
            let mut last_monitor_info = get_windows_monitor_info(&app);

            while started.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_secs(2));

                let current_info = get_windows_monitor_info(&app);
                if current_info != last_monitor_info {
                    log::info!("Display configuration changed, repositioning windows");

                    let app_state = app.state::<AppState>();
                    if let Some(window_manager) = app_state.get_window_manager() {
                        window_manager.reposition_floating_windows();
                    }

                    last_monitor_info = current_info;
                }
            }
        });
    }
}

#[cfg(target_os = "windows")]
fn get_windows_monitor_info(app: &AppHandle) -> (usize, u32, u32) {
    // Returns (monitor_count, primary_width, primary_height)
    crate::utils::monitor::catch_monitor_panic(|| {
        let count = app.available_monitors().map(|m| m.len()).unwrap_or(0);
        if let Some(primary) = app.primary_monitor().ok().flatten() {
            let size = primary.size();
            (count, size.width, size.height)
        } else {
            (count, 0, 0)
        }
    })
    .unwrap_or((0, 0, 0))
}

impl Drop for DisplayWatcher {
    fn drop(&mut self) {
        self.started.store(false, Ordering::Relaxed);
        log::debug!("DisplayWatcher stopped");
    }
}
