#[cfg(target_os = "macos")]
use tokio::time::{sleep, Duration};

use crate::audio::device_watcher::try_start_device_watcher_if_ready;

#[tauri::command]
pub async fn check_accessibility_permission(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::check_accessibility_permission;

        log::info!("Checking accessibility permissions for keyboard simulation");

        // Try checking with a small delay to handle macOS timing issues
        let mut attempts = 0;
        const MAX_ATTEMPTS: u8 = 3;

        loop {
            attempts += 1;

            // Check permission
            let has_permission = check_accessibility_permission().await;

            // If we got a definitive result or reached max attempts, return
            if has_permission || attempts >= MAX_ATTEMPTS {
                if has_permission {
                    log::info!("Accessibility permission is authorized");
                    // A grant made in System Settings is first observed by this
                    // polling command, not by the request command. Reuse the
                    // existing event so backend dependants (including the
                    // trigger engine) recover without an app restart.
                    emit_accessibility_granted(&app);
                } else {
                    log::warn!(
                        "Accessibility permission is not authorized after {} attempts",
                        attempts
                    );
                }
                return Ok(has_permission);
            }

            // Wait a bit before retry
            log::debug!(
                "Permission check returned false, retrying... (attempt {}/{})",
                attempts,
                MAX_ATTEMPTS
            );
            sleep(Duration::from_millis(200)).await;
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        // On non-macOS platforms, we don't need special permissions
        Ok(true)
    }
}

#[cfg(target_os = "macos")]
fn emit_accessibility_granted(app: &tauri::AppHandle) {
    use tauri::Emitter;

    log::info!("Emitting accessibility-granted event");
    if let Err(error) = app.emit("accessibility-granted", ()) {
        log::warn!("Failed to emit accessibility-granted event: {}", error);
    }
}

#[tauri::command]
#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
pub async fn request_accessibility_permission(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::{
            check_accessibility_permission, request_accessibility_permission,
        };

        // First check if permission is already granted
        let already_granted = check_accessibility_permission().await;
        if already_granted {
            log::info!("Accessibility permission already granted");
            emit_accessibility_granted(&app);

            // Return true to indicate permission is already granted
            return Ok(true);
        }

        log::info!("Requesting accessibility permissions");
        request_accessibility_permission().await;

        // Wait a bit for macOS to process the request
        sleep(Duration::from_millis(500)).await;

        // Check the permission status after request and update readiness
        let has_permission = check_accessibility_permission().await;

        log::info!(
            "Accessibility permission check after request: {}",
            has_permission
        );

        // Emit appropriate event based on permission status
        if has_permission {
            emit_accessibility_granted(&app);
        } else {
            use tauri::Emitter;
            let _ = app.emit("accessibility-denied", ());
        }

        Ok(has_permission)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}

#[tauri::command]
pub async fn check_microphone_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::check_microphone_permission;

        log::info!("Checking microphone permissions");

        // Try checking with a small delay to handle macOS timing issues
        let mut attempts = 0;
        const MAX_ATTEMPTS: u8 = 3;

        loop {
            attempts += 1;

            // Check permission
            let has_permission = check_microphone_permission().await;

            // If we got a definitive result or reached max attempts, return
            if has_permission || attempts >= MAX_ATTEMPTS {
                if has_permission {
                    log::info!("Microphone permission is authorized");
                } else {
                    log::warn!(
                        "Microphone permission is not authorized after {} attempts",
                        attempts
                    );
                }
                return Ok(has_permission);
            }

            // Wait a bit before retry
            log::debug!(
                "Permission check returned false, retrying... (attempt {}/{})",
                attempts,
                MAX_ATTEMPTS
            );
            sleep(Duration::from_millis(200)).await;
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On non-macOS platforms, we assume permission is granted
        Ok(true)
    }
}

#[tauri::command]
pub async fn request_microphone_permission(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::{
            check_microphone_permission, request_microphone_permission,
        };

        // First check if permission is already granted
        let already_granted = check_microphone_permission().await;
        if already_granted {
            log::info!("Microphone permission already granted");

            // Emit microphone-granted event for UI update
            log::info!("Emitting microphone-granted event");
            use tauri::Emitter;
            let _ = app.emit("microphone-granted", ());

            // Try to start device watcher if onboarding is complete
            try_start_device_watcher_if_ready(&app).await;

            return Ok(true);
        }

        log::info!("Requesting microphone permissions");

        // Request permission - this will show the system dialog
        let _ = request_microphone_permission().await;

        // Wait a bit for macOS to process
        sleep(Duration::from_millis(500)).await;

        // After requesting, check the actual permission status
        let has_permission = check_microphone_permission().await;

        if has_permission {
            log::info!("Microphone permission granted");
        } else {
            log::warn!("Microphone permission denied");
        }

        // Emit appropriate event based on permission status
        use tauri::Emitter;
        if has_permission {
            let _ = app.emit("microphone-granted", ());

            // Try to start device watcher if onboarding is complete
            try_start_device_watcher_if_ready(&app).await;
        } else {
            let _ = app.emit("microphone-denied", ());
        }

        Ok(has_permission)
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On non-macOS, try to start device watcher (permission always granted)
        try_start_device_watcher_if_ready(&app).await;
        Ok(true)
    }
}

#[tauri::command]
pub async fn test_automation_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        log::info!("Testing automation permission by simulating Cmd+V");

        // Try to simulate Cmd+V which will trigger the System Events permission dialog
        // This is exactly what happens during actual paste operation
        let script = r#"
            tell application "System Events"
                -- Simulate Cmd+V (paste)
                keystroke "v" using command down
                return "success"
            end tell
        "#;

        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| format!("Failed to run AppleScript: {}", e))?;

        if output.status.success() {
            log::info!("Automation permission granted - Cmd+V simulation succeeded");
            Ok(true)
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            if error.contains("not allowed assistive access") || error.contains("1743") {
                log::warn!("Automation permission denied by user: {}", error);

                Ok(false)
            } else {
                log::error!("AppleScript failed with unexpected error: {}", error);

                Err(format!("AppleScript error: {}", error))
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}

/// Open the system accessibility settings
#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        // Open Privacy & Security > Accessibility
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();

        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        // Windows doesn't have the same accessibility permission model.
        // Launch the Settings URI through the shell; it is not an executable path.
        Command::new("cmd")
            .args(["/C", "start", "", "ms-settings:easeofaccess"])
            .spawn()
            .map_err(|e| format!("Failed to open accessibility settings: {}", e))?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(())
    }
}

/// Open the system microphone (Privacy & Security) settings
#[tauri::command]
pub fn open_microphone_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        // Open Privacy & Security > Microphone
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            .spawn();

        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("cmd")
            .args(["/C", "start", "", "ms-settings:privacy-microphone"])
            .spawn()
            .map_err(|e| format!("Failed to open microphone settings: {}", e))?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(())
    }
}
