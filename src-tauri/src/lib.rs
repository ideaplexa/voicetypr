use chrono::Local;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::{Emitter, Listener, Manager};
use tauri_plugin_log::{Builder as LogBuilder, RotationStrategy, Target, TargetKind};
use tauri_plugin_store::StoreExt;

// Import our logging utilities
use crate::utils::logger::*;

mod ai;
mod audio;
pub mod cli;
mod cloud_stt;
mod commands;
mod ffmpeg;
mod license;
mod media;
mod menu;
mod parakeet;
mod product_analytics;
pub mod provider_capabilities;
mod recognition;
mod recording;
mod release_channel;
mod remote;
mod secure_store;
mod simple_cache;
mod state;
mod state_machine;
mod telemetry;
pub mod transcription;
mod tray_status;
mod trigger;
mod utils;
mod whisper;
mod window_manager;
mod writing;

#[cfg(test)]
mod tests;

// Helper functions for macOS dock icon visibility
#[cfg(target_os = "macos")]
pub fn show_dock_icon(app: &tauri::AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    log::debug!("Dock icon shown (ActivationPolicy::Regular)");
}

#[cfg(target_os = "macos")]
pub fn hide_dock_icon(app: &tauri::AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    log::debug!("Dock icon hidden (ActivationPolicy::Accessory)");
}
#[cfg(target_os = "macos")]
fn align_main_window_controls(window: &tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2_app_kit::{NSView, NSWindow, NSWindowButton};
    const TITLEBAR_HEIGHT: f64 = 36.0;
    const TRAFFIC_LIGHT_LEFT: f64 = 12.0;
    const TRAFFIC_LIGHT_VERTICAL_OFFSET: f64 = 2.0;

    let Ok(ns_window) = window.ns_window() else {
        log::warn!("Could not access the main NSWindow to align window controls");
        return;
    };

    // SAFETY: Tauri owns this NSWindow for the lifetime of `window`, and setup,
    // show, and window callbacks all execute on the AppKit main thread.
    unsafe {
        let ns_window = &*(ns_window.cast::<NSWindow>());
        let Some(close) = ns_window.standardWindowButton(NSWindowButton::CloseButton) else {
            return;
        };
        let Some(minimize) = ns_window.standardWindowButton(NSWindowButton::MiniaturizeButton)
        else {
            return;
        };
        let Some(zoom) = ns_window.standardWindowButton(NSWindowButton::ZoomButton) else {
            return;
        };
        let Some(button_container) = close.superview() else {
            return;
        };
        let Some(titlebar_container) = button_container.superview() else {
            return;
        };

        let close_frame = NSView::frame(&close);
        let mut titlebar_frame = NSView::frame(&titlebar_container);
        titlebar_frame.size.height = TITLEBAR_HEIGHT;
        titlebar_frame.origin.y = ns_window.frame().size.height - TITLEBAR_HEIGHT;
        let _: () = msg_send![&titlebar_container, setFrame: titlebar_frame];
        let mut button_container_frame = NSView::frame(&button_container);
        button_container_frame.origin.y = (TITLEBAR_HEIGHT - button_container_frame.size.height)
            / 2.0
            - TRAFFIC_LIGHT_VERTICAL_OFFSET;
        button_container.setFrameOrigin(button_container_frame.origin);

        let horizontal_step = NSView::frame(&minimize).origin.x - close_frame.origin.x;
        for (index, button) in [close, minimize, zoom].into_iter().enumerate() {
            let mut origin = NSView::frame(&button).origin;
            origin.x = TRAFFIC_LIGHT_LEFT + index as f64 * horizontal_step;
            button.setFrameOrigin(origin);
        }
    }
}

/// Show the main window and keep the macOS Dock icon in sync. Single entry point so
/// no caller forgets to reveal the Dock icon when the window becomes visible.
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        #[cfg(target_os = "macos")]
        align_main_window_controls(&window);
        let _ = window.set_focus();
        #[cfg(target_os = "macos")]
        show_dock_icon(app);
    }
}

/// Hide the main window and the macOS Dock icon — back to the menubar/tray.
fn hide_main_window(app: &tauri::AppHandle) -> bool {
    // If the system tray failed to create (e.g. the Windows shell notification
    // area wasn't ready at startup), hiding the window would leave the app
    // running with no way to bring it back. Keep the window visible instead.
    if app.tray_by_id("main").is_none() {
        log::warn!("No tray icon present; keeping main window visible instead of hiding to tray");
        return false;
    }
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("Main window is unavailable; could not hide it to the tray");
        return false;
    };
    if let Err(e) = window.hide() {
        log::error!("Failed to hide main window: {}", e);
        return false;
    }
    #[cfg(target_os = "macos")]
    hide_dock_icon(app);
    true
}

// Read the Windows taskbar theme (SystemUsesLightTheme) — deliberately distinct
// from the app theme (AppsUseLightTheme), because the tray icon lives on the
// taskbar and Windows lets the two differ (e.g. dark taskbar + light apps).
#[cfg(target_os = "windows")]
fn windows_taskbar_is_light() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("SystemUsesLightTheme"))
        .map(|v| v == 1)
        .unwrap_or(false)
}

// Pick the tray icon that contrasts with the current Windows taskbar theme:
// dark mark on a light taskbar, white mark on a dark taskbar. macOS uses a
// template icon that auto-adapts, so this is Windows-only.
#[cfg(target_os = "windows")]
fn apply_tray_theme_icon(app: &tauri::AppHandle) {
    if let Some(tray) = app.tray_by_id("main") {
        let icon = if windows_taskbar_is_light() {
            tauri::include_image!("icons/tray-light.png")
        } else {
            tauri::include_image!("icons/tray.png")
        };
        if let Err(e) = tray.set_icon(Some(icon)) {
            log::warn!("Failed to set theme-adaptive tray icon: {}", e);
        }
        let _ = tray.set_icon_as_template(false);
    }
}

#[cfg(target_os = "macos")]
const APP_QUIT_TO_TRAY_ID: &str = "app-quit-to-tray";
#[cfg(target_os = "macos")]
const HELP_CHECK_UPDATES_ID: &str = "help-check-updates";
#[cfg(target_os = "macos")]
const HELP_REPORT_ISSUE_ID: &str = "help-report-issue";
#[cfg(target_os = "macos")]
const HELP_RELEASE_NOTES_ID: &str = "help-release-notes";

/// Custom macOS app menu. Mirrors Tauri's default but: (1) the app-menu Quit (Cmd+Q)
/// is a normal item that hides to the tray instead of the predefined Quit (which maps
/// to native `terminate:` and can't be intercepted) — true quit stays on the tray menu
/// and Dock -> Quit; (2) About/Hide labels are branded "Voicetypr" with Ideaplexa LLC
/// metadata; (3) a Help submenu links updates, issues, and release notes.
#[cfg(target_os = "macos")]
fn build_app_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{AboutMetadataBuilder, Menu, MenuItem, PredefinedMenuItem, Submenu};

    let about_metadata = AboutMetadataBuilder::new()
        .name(Some("Voicetypr"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .copyright(Some("© Ideaplexa LLC"))
        .authors(Some(vec!["Ideaplexa LLC".to_string()]))
        .website(Some("https://voicetypr.com"))
        .website_label(Some("voicetypr.com"))
        .build();

    let quit_to_tray = MenuItem::with_id(
        app,
        APP_QUIT_TO_TRAY_ID,
        "Quit Voicetypr",
        true,
        Some("CmdOrCtrl+Q"),
    )?;

    let app_menu = Submenu::with_items(
        app,
        "Voicetypr",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About Voicetypr"), Some(about_metadata))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some("Hide Voicetypr"))?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &quit_to_tray,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let check_updates = MenuItem::with_id(
        app,
        HELP_CHECK_UPDATES_ID,
        "Check for Updates…",
        true,
        None::<&str>,
    )?;
    let report_issue = MenuItem::with_id(
        app,
        HELP_REPORT_ISSUE_ID,
        "Report an Issue",
        true,
        None::<&str>,
    )?;
    let release_notes = MenuItem::with_id(
        app,
        HELP_RELEASE_NOTES_ID,
        "Release Notes",
        true,
        None::<&str>,
    )?;

    let help_menu = Submenu::with_items(
        app,
        "Help",
        true,
        &[
            &check_updates,
            &PredefinedMenuItem::separator(app)?,
            &report_issue,
            &release_notes,
        ],
    )?;

    Menu::with_items(app, &[&app_menu, &edit_menu, &help_menu])
}

use audio::recorder::AudioRecorder;
use commands::remote::load_remote_settings;
use commands::telemetry::{
    defer_privacy_consent_for_session, get_product_analytics_status, get_telemetry_status,
    record_onboarding_completed, report_frontend_error, set_product_analytics_consent,
    set_telemetry_consent,
};
use commands::{
    ai::{
        cache_ai_api_key, clear_ai_api_key_cache, disable_ai_enhancement, get_ai_settings,
        get_ai_settings_for_provider, get_enhancement_options, get_openai_config,
        get_writing_settings, list_ai_providers, list_provider_models, probe_agent_cli,
        set_openai_config, test_openai_endpoint, update_agent_cli_fast_mode,
        update_agent_cli_reasoning, update_ai_settings, update_enhancement_options,
        update_writing_settings, validate_ai_api_key,
    },
    audio::*,
    cli_tool::{cli_tool_status, install_cli_tool, repair_cli_tool, uninstall_cli_tool},
    clipboard::{copy_image_to_clipboard, save_image_to_file},
    debug::{debug_transcription_flow, test_transcription_event},
    device::get_device_id,
    distribution::get_distribution_info,
    keyring::{keyring_delete, keyring_get, keyring_has, keyring_set},
    license::*,
    logs::{clear_old_logs, get_latest_log_for_bug_report, get_log_directory, open_logs_folder},
    model::{
        cancel_download, delete_model, download_model, download_parakeet_vocabulary_model,
        get_model_status, get_parakeet_vocabulary_status, list_downloaded_models, preload_model,
        set_cloud_stt_model, verify_model,
    },
    permissions::{
        check_accessibility_permission, check_microphone_permission, open_accessibility_settings,
        open_microphone_settings, request_accessibility_permission, request_microphone_permission,
        test_automation_permission,
    },
    remote::{
        add_remote_server, check_remote_server_status, discover_remote_servers,
        get_active_remote_server, get_firewall_status, get_local_ips, get_local_machine_id,
        get_remote_transcription_control, get_sharing_status, list_remote_servers,
        open_firewall_settings, refresh_active_remote_server_status, refresh_remote_servers,
        remove_remote_server, set_active_remote_server, start_sharing, stop_sharing,
        test_remote_connection, test_remote_server, transcribe_remote,
        update_remote_model_control_enabled, update_remote_server,
        update_remote_transcription_control,
    },
    reset::reset_app_data,
    settings::*,
    shortcuts::{get_shortcut_settings, list_shortcut_actions, update_shortcut_settings},
    stt::{
        cleanup_soniox_storage, clear_stt_key_cache, get_soniox_storage_counts, validate_stt_key,
    },
    system_info::get_system_specs,
    text::*,
    updater::{check_for_app_update, install_app_update},
    utils::{export_transcriptions, get_application_icon, save_transcript_file},
    window::*,
};
use remote::lifecycle::RemoteServerManager;
use whisper::cache::TranscriberCache;
use window_manager::WindowManager;

use menu::build_tray_menu;
pub use recognition::{
    auto_select_model_if_needed, get_recognition_availability_snapshot,
    recognition_availability_snapshot, RecognitionAvailabilitySnapshot,
};
pub use state::{
    emit_to_all, emit_to_window, flush_pill_event_queue, get_recording_state,
    update_recording_state, AppState, QueuedPillEvent, RecordingMode, RecordingState,
};

/// Shared log filter predicate applied to every tauri_plugin_log target.
/// Drops noisy third-party logs:
/// - Audio crates (always): whisper_rs, cpal, rubato, hound, audio::level_meter
/// - HTTP/transport crates below Warn: h2, hyper, hyper_util, reqwest, tower
fn log_filter_predicate(metadata: &log::Metadata) -> bool {
    let target = metadata.target();
    // Always drop low-level audio processing noise
    if target.contains("whisper_rs")
        || target.contains("audio::level_meter")
        || target.contains("cpal")
        || target.contains("rubato")
        || target.contains("hound")
    {
        return false;
    }
    // Drop HTTP/transport framing noise at Info/Debug/Trace; keep Warn and Error
    let is_http_transport = target == "h2"
        || target.starts_with("h2::")
        || target.starts_with("hyper")
        || target.starts_with("hyper_util")
        || target.starts_with("reqwest")
        || target.starts_with("tower");
    !(is_http_transport && metadata.level() > log::Level::Warn)
}

// Setup logging with daily rotation
fn setup_logging() -> tauri_plugin_log::Builder {
    let today = Local::now().format("%Y-%m-%d").to_string();

    // Release builds keep stdout + the FILE sink at Info (no DEBUG firehose
    // on disk) while the global level stays Debug: the in-memory ring target
    // captures DEBUG lines and every bug report attaches a redacted dump
    // (plan 060). Debug builds log Debug everywhere as before.
    let file_sink_max = if cfg!(debug_assertions) {
        log::Level::Trace
    } else {
        log::Level::Info
    };

    LogBuilder::default()
        .targets([
            Target::new(TargetKind::Stdout)
                .filter(log_filter_predicate)
                .filter(move |m| m.level() <= file_sink_max),
            Target::new(TargetKind::LogDir {
                file_name: Some(format!("voicetypr-{}", today)),
            })
            .filter(log_filter_predicate)
            .filter(move |m| m.level() <= file_sink_max),
            Target::new(TargetKind::Dispatch(crate::utils::ring_log::ring_dispatch()))
                .filter(log_filter_predicate),
        ])
        .rotation_strategy(RotationStrategy::KeepAll)
        .max_file_size(10_000_000) // 10MB per file
        .level(log::LevelFilter::Debug)
}

type TrayBuilder = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

struct TrayRecovery {
    builder: TrayBuilder,
}

fn current_tray_status(app: &tauri::AppHandle) -> tray_status::TrayStatus {
    let state = app.state::<tray_status::TrayStatusState>();
    if app.tray_by_id("main").is_some() {
        state.record_present()
    } else {
        state.snapshot()
    }
}

fn attempt_tray_creation(
    app: &tauri::AppHandle,
    builder: &TrayBuilder,
    source: &'static str,
) -> tray_status::TrayStatus {
    let state = app.state::<tray_status::TrayStatusState>();
    if app.tray_by_id("main").is_some() {
        return state.record_present();
    }

    let status = match builder() {
        Ok(()) => {
            let status = state.record_success();
            log::info!(
                "TRAY_CREATION | source={} | attempt={} | result=success",
                source,
                status.attempts
            );
            #[cfg(target_os = "windows")]
            apply_tray_theme_icon(app);
            status
        }
        Err(error) => {
            let status = state.record_failure(&error);
            log::warn!(
                "TRAY_CREATION | source={} | attempt={} | result=failure | error={}",
                source,
                status.attempts,
                error
            );
            status
        }
    };

    let _ = app.emit("tray-status-changed", &status);
    status
}

async fn attempt_tray_creation_on_main_thread(
    app: tauri::AppHandle,
    builder: TrayBuilder,
    source: &'static str,
) -> Result<tray_status::TrayStatus, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let attempt_app = app.clone();
    app.run_on_main_thread(move || {
        let status = attempt_tray_creation(&attempt_app, &builder, source);
        let _ = sender.send(status);
    })
    .map_err(|error| format!("Failed to schedule tray creation: {error}"))?;

    receiver
        .await
        .map_err(|_| "Tray creation task ended before returning a result".to_string())
}

fn schedule_deferred_tray_recovery(app: tauri::AppHandle, builder: TrayBuilder) {
    tauri::async_runtime::spawn(async move {
        for delay_secs in tray_status::DEFERRED_TRAY_RETRY_DELAYS_SECS {
            tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;

            if app.tray_by_id("main").is_some() {
                let status = app.state::<tray_status::TrayStatusState>().record_present();
                let _ = app.emit("tray-status-changed", &status);
                return;
            }

            match attempt_tray_creation_on_main_thread(
                app.clone(),
                Arc::clone(&builder),
                "deferred",
            )
            .await
            {
                Ok(status) if status.available => return,
                Ok(_) => {}
                Err(error) => {
                    log::error!("Deferred tray recovery could not run: {}", error);
                    return;
                }
            }
        }

        let status = current_tray_status(&app);
        log::error!(
            "Tray icon remains unavailable after {} attempts; keeping the main window visible",
            status.attempts
        );
    });
}

#[tauri::command]
fn get_tray_status(app: tauri::AppHandle) -> tray_status::TrayStatus {
    current_tray_status(&app)
}

#[tauri::command]
async fn retry_tray_creation(app: tauri::AppHandle) -> Result<tray_status::TrayStatus, String> {
    if app.tray_by_id("main").is_some() {
        return Ok(current_tray_status(&app));
    }

    let builder = Arc::clone(&app.state::<TrayRecovery>().builder);
    attempt_tray_creation_on_main_thread(app, builder, "manual").await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let app_start = Instant::now();
    let app_version = env!("CARGO_PKG_VERSION");

    // Log application startup
    log_lifecycle_event("APPLICATION_START", Some(app_version), None);

    #[cfg(debug_assertions)]
    {
        // Load .env file if it exists (development builds only)
        log_start("ENV_FILE_LOAD");
        match dotenv::dotenv() {
            Ok(path) => {
                log_file_operation("LOAD", &format!("{:?}", path), true, None, None);
            }
            Err(e) => {
                log::info!("📄 No .env file found or error loading it: {}", e);
            }
        }
    }

    // Initialize encryption key for secure storage
    log_start("ENCRYPTION_INIT");
    log_with_context(
        log::Level::Debug,
        "Initializing encryption",
        &[("component", "secure_store")],
    );

    #[cfg(not(target_os = "windows"))]
    if let Err(e) = secure_store::initialize_encryption_key() {
        log_failed(
            "ENCRYPTION_INIT",
            &format!("Failed to initialize encryption: {}", e),
        );
        eprintln!("Failed to initialize encryption: {}", e);
    } else {
        log::info!("✅ Encryption initialized successfully");
    }

    // Initialize independent diagnostics and product-analytics clients from
    // their own persisted choices. Only new product analytics is held behind
    // the one-time acknowledgement gate.
    let app_context = tauri::generate_context!();
    let analytics_consent =
        product_analytics::read_consent(app_context.config().identifier.as_str());
    let (telemetry_enabled, telemetry_install_id) =
        telemetry::read_consent(app_context.config().identifier.as_str());
    let _sentry_guard = telemetry::init(telemetry_enabled, telemetry_install_id);
    product_analytics::init(analytics_consent);

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(setup_logging().build())
        // Replaced tauri-plugin-cache with simple_store-backed cache
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // When a second instance is launched, bring the existing window to focus
            show_main_window(app);
        }))
        .plugin({
            #[cfg(target_os = "macos")]
            let autostart = tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None::<Vec<&str>>,
            );

            #[cfg(not(target_os = "macos"))]
            let autostart = tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent, // This param is ignored on non-macOS
                None::<Vec<&str>>,
            );

            autostart
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init());

    // Add NSPanel plugin on macOS
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .plugin(tauri_nspanel::init())
            .plugin(tauri_plugin_macos_permissions::init())
            .menu(build_app_menu)
            .on_menu_event(|app, event| {
                use tauri_plugin_opener::OpenerExt;
                let id = event.id();
                if id == APP_QUIT_TO_TRAY_ID {
                    // Cmd+Q / app-menu Quit: hide to tray instead of terminating.
                    hide_main_window(app);
                } else if id == HELP_CHECK_UPDATES_ID {
                    let _ = app.emit("tray-check-updates", ());
                } else if id == HELP_REPORT_ISSUE_ID {
                    let _ = app.opener().open_url(
                        "https://github.com/ideaplexa/voicetypr/issues",
                        None::<&str>,
                    );
                } else if id == HELP_RELEASE_NOTES_ID {
                    let _ = app.opener().open_url(
                        "https://github.com/ideaplexa/voicetypr/releases",
                        None::<&str>,
                    );
                }
            });
    }

    builder
        .setup(move |app| {
            let setup_start = Instant::now();
            log::info!("🚀 App setup START - version: {}", app_version);
            // Windows identity persistence must run after the single-instance
            // plugin has excluded secondary processes, before any secure reads.
            #[cfg(target_os = "windows")]
            if let Err(error) = secure_store::windows_identity::initialize(&app.path().app_data_dir()?) {
                log::error!("Windows identity initialization failed: {}", error);
                // Keep the UI available for recovery. Secure/API calls fail
                // closed rather than silently choosing a different identity.
            }
            let distribution_info = commands::distribution::get_distribution_info();
            log::info!(
                "Distribution channel: channel={}, store_install={}, package_family_name={:?}",
                distribution_info.channel,
                distribution_info.is_store_install,
                distribution_info.package_family_name
            );

            // Keyring is now used instead of Stronghold for API keys
            // Much faster and uses OS-native secure storage
            log::info!("🔐 Using OS-native keyring for secure API key storage");

            // Set up panic handler to catch crashes
            log_start("PANIC_HANDLER_SETUP");
            log_with_context(log::Level::Debug, "Setting up panic handler", &[
                ("component", "panic_handler")
            ]);

            // Chain the previous panic hook instead of replacing it. Telemetry
            // init installed sentry's panic hook (via the `panic` feature) to
            // capture release panics as events; overwriting it here would
            // silently drop panic capture. Run our local diagnostics first, then
            // forward to the prior hook so Sentry still records the event.
            let prev_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |panic_info| {
                let location = panic_info.location()
                    .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                    .unwrap_or_else(|| "unknown location".to_string());

                let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic payload".to_string()
                };

                log::error!("💥 CRITICAL PANIC at {}: {}", location, message);
                log_failed("PANIC", "Application panic occurred");
                log_with_context(log::Level::Error, "Panic details", &[
                    ("panic_location", &location),
                    ("panic_message", &message),
                    ("severity", "critical")
                ]);
                eprintln!("Application panic at {}: {}", location, message);

                // Try to save panic info to a crash file for debugging
                if let Ok(home_dir) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
                    let crash_file = std::path::Path::new(&home_dir).join(".voicetypr_crash.log");
                    let _ = std::fs::write(&crash_file, format!(
                        "Panic at {}: {}\nFull info: {:?}\nTime: {:?}",
                        location, message, panic_info, chrono::Local::now()
                    ));
                }
                // Forward to the prior (Sentry) hook so panics are still captured.
                prev_hook(panic_info);
            }));

            log::info!("✅ Panic handler configured");

            // Clean up old logs on startup (keep last 30 days)
            log_start("LOG_CLEANUP");
            log_with_context(log::Level::Debug, "Cleaning up old logs", &[
                ("retention_days", "30")
            ]);

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let cleanup_start = Instant::now();
                match commands::logs::clear_old_logs(app_handle, 30).await {
                    Ok(deleted) => {
                        log_complete("LOG_CLEANUP", cleanup_start.elapsed().as_millis() as u64);
                        log_with_context(log::Level::Debug, "Log cleanup complete", &[
                            ("files_deleted", deleted.to_string().as_str())
                        ]);
                        if deleted > 0 {
                            log::info!("🧹 Cleaned up {} old log files", deleted);
                        }
                    }
                    Err(e) => {
                        log_failed("LOG_CLEANUP", &e);
                        log_with_context(log::Level::Debug, "Log cleanup failed", &[
                            ("retention_days", "30")
                        ]);
                        log::warn!("Failed to clean up old logs: {}", e);
                    }
                }
            });

            // Accessory by default: Voicetypr is a background/menubar app, so it shows
            // no Dock icon until a window is open (show_dock_icon -> Regular). Hiding the
            // window returns to Accessory, mirroring the Windows close-to-tray behaviour.
            // The recording pill is a non-activating NSPanel.
            #[cfg(target_os = "macos")]
            {
                log_start("MACOS_SETUP");
                log_with_context(log::Level::Debug, "Setting up macOS policy", &[
                    ("policy", "Accessory")
                ]);

                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    let main_thread_handle = app_handle.clone();
                    let _ = app_handle.run_on_main_thread(move || {
                        if let Some(window) = main_thread_handle.get_webview_window("main") {
                            align_main_window_controls(&window);
                        }
                    });
                });
                log::info!("🍎 Set macOS activation policy to Accessory");

            }

            // Force a fresh online check while preserving the last successful
            // validation timestamp that anchors paid offline grace.
            {
                use crate::simple_cache;
                let _ = simple_cache::remove(app.app_handle(), "license_status");
            }

            // Initialize whisper manager
            let models_dir = app.path().app_data_dir()?.join("models");
            log::info!("🗂️  Models directory: {:?}", models_dir);

            log_start("WHISPER_MANAGER_INIT");
            log_with_context(log::Level::Debug, "Initializing Whisper manager", &[
                ("models_dir", format!("{:?}", models_dir).as_str())
            ]);

            // Ensure the models directory exists
            match std::fs::create_dir_all(&models_dir) {
                Ok(_) => {
                    log_file_operation("CREATE_DIR", &format!("{:?}", models_dir), true, None, None);
                }
                Err(e) => {
                    let error_msg = format!("Failed to create models directory: {}", e);
                    log_file_operation("CREATE_DIR", &format!("{:?}", models_dir), false, None, Some(&e.to_string()));
                    return Err(Box::new(std::io::Error::other(error_msg)));
                }
            }

            let whisper_manager = whisper::manager::WhisperManager::new(models_dir.clone());
            app.manage(AsyncRwLock::new(whisper_manager));

            log::info!("✅ Whisper manager initialized and managed");

            // Initialize Parakeet manager and cache directory
            let parakeet_dir = models_dir.join("parakeet");
            if let Err(e) = std::fs::create_dir_all(&parakeet_dir) {
                let error_msg = format!("Failed to create parakeet models directory: {}", e);
                log_file_operation("CREATE_DIR", &format!("{:?}", parakeet_dir), false, None, Some(&e.to_string()));
                return Err(Box::new(std::io::Error::other(error_msg)));
            }

            log_file_operation("CREATE_DIR", &format!("{:?}", parakeet_dir), true, None, None);
            let parakeet_manager = parakeet::ParakeetManager::new(parakeet_dir);
            app.manage(parakeet_manager);
            log::info!("🦜 Parakeet manager initialized");

            // Manage active downloads for cancellation
            app.manage(Arc::new(Mutex::new(HashMap::<String, Arc<AtomicBool>>::new())));

            // Initialize transcriber cache for keeping models in memory
            // Cache size is 1: only the current model (1-3GB RAM)
            // When user switches models, old one is unloaded immediately
            app.manage(AsyncMutex::new(TranscriberCache::new()));
            app.manage(crate::whisper::gpu_sidecar::GpuSidecarClient::new());
            log::info!("GPU sidecar client initialized");

            // Initialize remote transcription state
            app.manage(AsyncMutex::new(RemoteServerManager::new()));
            // Load saved remote settings from store (persists connections across restarts)
            let remote_settings = load_remote_settings(app.handle());
            let connection_count = remote_settings.saved_connections.len();
            let active_id = remote_settings.active_connection_id.clone();
            let sharing_was_enabled = remote_settings.server_config.enabled;
            log::info!(
                "🌐 [STARTUP] Remote settings loaded: {} connections, active_connection_id={:?}, sharing_enabled={}",
                connection_count,
                active_id,
                sharing_was_enabled
            );
            app.manage(AsyncMutex::new(remote_settings));
            log::info!("🌐 Remote transcription state initialized ({} saved connections)", connection_count);

            // Auto-start network sharing if it was enabled before app closed
            // BUT only if no remote server is active (can't share and use remote at same time)
            if sharing_was_enabled && active_id.is_none() {
                let app_handle_for_sharing = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

                    log::info!("🌐 [STARTUP] Auto-starting network sharing (was enabled before shutdown)");

                    let server_manager = app_handle_for_sharing.state::<AsyncMutex<crate::remote::lifecycle::RemoteServerManager>>();
                    let whisper_manager = app_handle_for_sharing.state::<AsyncRwLock<crate::whisper::manager::WhisperManager>>();
                    let remote_state = app_handle_for_sharing.state::<AsyncMutex<crate::remote::settings::RemoteSettings>>();

                    let refreshed_remote_settings = {
                        let settings = remote_state.lock().await;
                        settings.clone()
                    };

                    if !refreshed_remote_settings.server_config.enabled {
                        log::info!("🌐 [STARTUP] Skipping network sharing auto-start: sharing was disabled during startup delay");
                        return;
                    }

                    if refreshed_remote_settings.active_connection_id.is_some() {
                        log::info!(
                            "🌐 [STARTUP] Skipping network sharing auto-start: remote server became active during startup delay (id={:?})",
                            refreshed_remote_settings.active_connection_id
                        );
                        return;
                    }

                    let restore_port = refreshed_remote_settings.server_config.port;
                    let restore_password = refreshed_remote_settings.server_config.password.clone();

                    let result = async {
                        let server_name = hostname::get()
                            .ok()
                            .and_then(|h| h.into_string().ok())
                            .unwrap_or_else(|| "Voicetypr Server".to_string());

                        let store = app_handle_for_sharing
                            .store("settings")
                            .map_err(|e| format!("Failed to access store: {}", e))?;

                        let stored_model = store
                            .get("current_model")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                            .unwrap_or_default();

                        let stored_engine = store
                            .get("current_model_engine")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                            .unwrap_or_else(|| "whisper".to_string());

                        let model_name = if stored_model.is_empty() {
                            let wm = whisper_manager.read().await;
                            wm.get_first_downloaded_model()
                                .ok_or("No model downloaded")?
                        } else {
                            stored_model
                        };

                        let (model_path, validated_engine) = match crate::commands::remote::resolve_shareable_model_config(
                            &app_handle_for_sharing,
                            &model_name,
                            &stored_engine,
                        ).await {
                            Ok(config) => config,
                            Err(e) => {
                                let mut settings = remote_state.lock().await;
                                settings.server_config.enabled = false;
                                settings.sharing_was_active = false;
                                let _ = crate::commands::remote::save_remote_settings(&app_handle_for_sharing, &settings);
                                return Err(e);
                            }
                        };

                        let mut manager = server_manager.lock().await;
                        manager
                            .start(
                                restore_port,
                                restore_password,
                                server_name,
                                model_path,
                                model_name,
                                validated_engine,
                                Some(app_handle_for_sharing.clone()),
                            )
                            .await?;

                        Ok::<(), String>(())
                    }.await;

                    match result {
                        Ok(()) => log::info!("🌐 [STARTUP] Network sharing auto-started successfully on port {}", restore_port),
                        Err(e) => log::warn!("🌐 [STARTUP] Failed to auto-start network sharing: {}", e),
                    }
                });
            } else if sharing_was_enabled && active_id.is_some() {
                log::info!(
                    "🌐 [STARTUP] Skipping network sharing auto-start: remote server is active (id={:?})",
                    active_id
                );
            }

            // Initialize unified application state
            app.manage(AppState::new());
            log::info!("🧠 App state managed and ready");

            // Initialize window manager after app state is managed
            let app_state = app.state::<AppState>();
            let window_manager = WindowManager::new(app.app_handle().clone());
            app_state.set_window_manager(window_manager);

            migrate_ai_settings_before_key_cache(&app.handle().clone());

            // Warm AI API key cache from secure store BEFORE startup checks run.
            // This ensures persisted credentials are visible to perform_startup_checks()
            // without depending on frontend React mount timing.
            crate::commands::ai::warm_ai_key_cache_from_secure_store(&app.handle().clone());


            // Run comprehensive startup checks after state/window manager are ready
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                perform_startup_checks(app_handle).await;
            });

            // Show pill on startup if pill_indicator_mode is "always"
            let app_handle_for_pill = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Wait for frontend to be ready (Vite in dev mode, bundled files in prod)
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

                // Check if pill_indicator_mode setting is "always"
                let pill_mode = if let Ok(store) = app_handle_for_pill.store("settings") {
                    let stored_mode = store
                        .get("pill_indicator_mode")
                        .and_then(|v| v.as_str().map(|s| s.to_string()));
                    let legacy_show = store.get("show_pill_indicator").and_then(|v| v.as_bool());
                    let resolved = crate::commands::settings::resolve_pill_indicator_mode(
                        stored_mode.clone(),
                        legacy_show,
                        crate::commands::settings::Settings::default().pill_indicator_mode,
                    );
                    log::info!(
                        "Startup: pill_indicator_mode resolved='{}' stored={:?} legacy_show={:?}",
                        resolved,
                        stored_mode,
                        legacy_show
                    );
                    resolved
                } else {
                    let default_mode = crate::commands::settings::Settings::default().pill_indicator_mode;
                    log::info!(
                        "Startup: pill_indicator_mode using default='{}' (settings store unavailable)",
                        default_mode
                    );
                    default_mode
                };

                log::info!(
                    "Startup: pill_indicator_mode='{}', will show pill={}",
                    pill_mode,
                    pill_mode == "always"
                );

                // Only show pill on startup if mode is "always"
                if pill_mode == "always" {
                    log::info!("Startup: Showing pill because mode is 'always'");
                    if let Err(e) = crate::commands::window::show_pill_widget(app_handle_for_pill).await {
                        log::warn!("Failed to show pill on startup: {}", e);
                    }
                }
            });

            // Clean up old logs on startup (keep only today's log)
            let app_handle_for_logs = app.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                match clear_old_logs(app_handle_for_logs, 1).await {
                    Ok(deleted_count) => {
                        if deleted_count > 0 {
                            log::info!("Cleaned up {} old log files (keeping only today)", deleted_count);
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to clean up old logs: {}. App will continue normally.", e);
                    }
                }
            });

            // Pill position is loaded from settings when needed, no duplicate state

            // Initialize recorder state (kept separate for backwards compatibility)
            app.manage(RecorderState(Mutex::new(AudioRecorder::new())));

            let recorder_watchdog =
                audio::recorder_watchdog::RecorderWatchdog::new(app.app_handle().clone());
            recorder_watchdog.start();
            app.manage(recorder_watchdog);

            // Create device watcher in deferred state - will be started after mic permission granted
            // This prevents early mic permission prompts from CPAL's input_devices() enumeration
            app.manage(audio::device_watcher::DeviceWatcher::new(app.app_handle().clone()));

            // For returning users (onboarding already complete + mic permission granted),
            // start the device watcher automatically
            let app_handle_for_watcher = app.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                audio::device_watcher::try_start_device_watcher_if_ready(&app_handle_for_watcher).await;
            });

            // Create display watcher to reposition pill/toast on monitor changes
            let display_watcher = utils::display_watcher::DisplayWatcher::new(app.app_handle().clone());
            display_watcher.start();
            app.manage(display_watcher);

            app.manage(tray_status::TrayStatusState::default());

            // Create tray icon
            use tauri::tray::{TrayIconBuilder, TrayIconEvent};

            // Build the tray menu using our helper function
            // Note: We need to block here since setup is sync
            let tray_app = app.app_handle().clone();
            let tray_builder: TrayBuilder = Arc::new(move || -> Result<(), String> {
            let menu = tauri::async_runtime::block_on(build_tray_menu(&tray_app))
                .map_err(|error| error.to_string())?;


            // Bare-mark template icon for the menubar (no background; adapts to light/dark).
            let tray_icon = tauri::include_image!("icons/tray.png");

            let _tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("Voicetypr")
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    log::info!("Tray menu event: {:?}", event.id);
                    let event_id = event.id.as_ref().to_string();

                    if event_id == "dashboard" {
                        show_main_window(app);
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("navigate-to-overview", ());
                        }
                    } else if event_id == "quit" {
                        app.exit(0);
                    } else if event_id == "check_updates" {
                        let _ = app.emit("tray-check-updates", ());
                    } else if event_id.starts_with("model_") {
                        // Handle model selection
                        let model_name = match event_id.strip_prefix("model_") {
                            Some(name) => name.to_string(),
                            None => {
                                log::warn!("Invalid model event_id format: {}", event_id);
                                return; // Skip processing invalid model events
                            }
                        };
                        let app_handle = app.app_handle().clone();

                        tauri::async_runtime::spawn(async move {
                            match crate::commands::settings::set_model_from_tray(app_handle.clone(), model_name.clone()).await {
                                Ok(_) => {
                                    log::info!("Model changed from tray to: {}", model_name);
                                }
                                Err(e) => {
                                    log::error!("Failed to set model from tray: {}", e);
                                    // Emit error event so UI can show notification
                                    let _ = app_handle.emit("tray-action-error", &format!("Failed to change model: {}", e));
                                }
                            }
                        });
                    } else if event_id == "microphone_default" {
                        // Handle default microphone selection
                        let app_handle = app.app_handle().clone();

                        tauri::async_runtime::spawn(async move {
                            match crate::commands::settings::set_audio_device(app_handle.clone(), None).await {
                                Ok(_) => {
                                    log::info!("Microphone changed from tray to: System Default");
                                }
                                Err(e) => {
                                    log::error!("Failed to set default microphone from tray: {}", e);
                                    let _ = app_handle.emit("tray-action-error", &format!("Failed to change microphone: {}", e));
                                }
                            }
                        });
                    } else if event_id.starts_with("microphone_") {
                        // Handle specific microphone selection
                        let device_name = match event_id.strip_prefix("microphone_") {
                            Some(name) if name != "default" => Some(name.to_string()),
                            _ => {
                                // Already handled by microphone_default case above
                                return;
                            }
                        };
                        let app_handle = app.app_handle().clone();

                        tauri::async_runtime::spawn(async move {
                            match crate::commands::settings::set_audio_device(app_handle.clone(), device_name.clone()).await {
                                Ok(_) => {
                                    log::info!("Microphone changed from tray to: {:?}", device_name);
                                }
                                Err(e) => {
                                    log::error!("Failed to set microphone from tray: {}", e);
                                    let _ = app_handle.emit("tray-action-error", &format!("Failed to change microphone: {}", e));
                                }
                            }
                        });
                    }
                    else if event_id == "polish_on" || event_id == "polish_off" {
                        let app_handle = app.app_handle().clone();
                        let desired_enabled = event_id == "polish_on";
                        tauri::async_runtime::spawn(async move {
                            let current_enabled = app_handle
                                .store("settings")
                                .ok()
                                .and_then(|store| store.get("ai_enabled"))
                                .and_then(|value| value.as_bool())
                                .unwrap_or(false);

                            if current_enabled != desired_enabled {
                                match crate::commands::shortcuts::toggle_ai_formatting(app_handle.clone()).await {
                                    Ok(()) => {
                                        log::info!("Polish toggled from tray to requested state: {}", desired_enabled);
                                    }
                                    Err(e) => {
                                        log::error!("Failed to toggle Polish from tray: {}", e);
                                        let _ = app_handle.emit("tray-action-error", &format!("Failed to change Polish: {}", e));
                                    }
                                }
                            }

                            if let Err(e) = crate::commands::settings::update_tray_menu(app_handle.clone()).await {
                                log::warn!("Failed to refresh tray after Polish change: {}", e);
                            }
                        });
                    }
                    else if event_id == "copy_last_transcription" {
                        let app_handle = app.app_handle().clone();
                        tauri::async_runtime::spawn(async move {
                            match app_handle.store("transcriptions") {
                                Ok(store) => {
                                    let mut entries: Vec<(String, serde_json::Value)> = Vec::new();
                                    for key in store.keys() {
                                        if let Some(value) = store.get(&key) {
                                            entries.push((key.to_string(), value));
                                        }
                                    }

                                    if let Some(timestamp) = crate::menu::latest_copyable_transcription_id(&entries) {
                                        if let Some(text) = store
                                            .get(&timestamp)
                                            .and_then(|value| value.get("text").and_then(|text| text.as_str().map(str::to_string)))
                                        {
                                            if let Err(error) = crate::commands::text::copy_text_to_clipboard(text).await {
                                                log::error!("Failed to copy last transcription: {}", error);
                                                let _ = app_handle.emit("tray-action-error", &format!("Failed to copy: {}", error));
                                            } else {
                                                log::info!("Copied last transcription to clipboard");
                                            }
                                        }
                                    }
                                }
                                Err(error) => {
                                    log::error!("Failed to open transcriptions store: {}", error);
                                }
                            }
                        });
                    }
                    // Recent transcriptions copy handler
                    else if let Some(ts) = event_id.strip_prefix("recent_copy_") {
                        let ts_owned = ts.to_string();
                        let app_handle = app.app_handle().clone();
                        tauri::async_runtime::spawn(async move {
                            // Read text by timestamp and copy
                            match app_handle.store("transcriptions") {
                                Ok(store) => {
                                    if let Some(val) = store.get(&ts_owned) {
                                        if let Some(text) = val.get("text").and_then(|v| v.as_str()) {
                                            if let Err(e) = crate::commands::text::copy_text_to_clipboard(text.to_string()).await {
                                                log::error!("Failed to copy recent transcription: {}", e);
                                                let _ = app_handle.emit("tray-action-error", &format!("Failed to copy: {}", e));
                                            } else {
                                                log::info!("Copied recent transcription to clipboard");
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to open transcriptions store: {}", e);
                                }
                            }
                        });
                    }
                    // Recording mode switchers
                    else if event_id == "recording_mode_toggle" || event_id == "recording_mode_push_to_talk" {
                        let app_handle = app.app_handle().clone();
                        let mode = if event_id.ends_with("push_to_talk") { "push_to_talk" } else { "toggle" };
                        tauri::async_runtime::spawn(async move {
                            match crate::commands::settings::get_settings(app_handle.clone()).await {
                                Ok(mut s) => {
                                    s.recording_mode = mode.to_string();
                                    match crate::commands::settings::save_settings(app_handle.clone(), s, None).await {
                                        Err(e) => {
                                            log::error!("Failed to save recording mode from tray: {}", e);
                                            let _ = app_handle.emit("tray-action-error", &format!("Failed to change recording mode: {}", e));
                                        }
                                        Ok(()) => {
                                            if let Err(e) = crate::commands::settings::update_tray_menu(app_handle.clone()).await {
                                                log::warn!("Failed to refresh tray after mode change: {}", e);
                                            }
                                            // Notify frontend so SettingsContext refreshes
                                            let _ = app_handle.emit("settings-changed", ());
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to get settings for mode change: {}", e);
                                }
                            }
                        });
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        show_main_window(app);
                    }
                })
                .build(&tray_app)
                .map_err(|error| error.to_string())?;
            Ok(())
            });
            app.manage(TrayRecovery {
                builder: Arc::clone(&tray_builder),
            });

            // Tray creation can fail transiently while the OS status area is
            // starting. Preserve the nonfatal startup behavior from PR #96,
            // then continue with bounded delayed recovery instead of requiring
            // the user to restart the entire application.
            let mut tray_status = current_tray_status(app.app_handle());
            for startup_attempt in 1..=tray_status::STARTUP_TRAY_ATTEMPTS {
                tray_status =
                    attempt_tray_creation(app.app_handle(), &tray_builder, "startup");
                if tray_status.available {
                    break;
                }
                if startup_attempt < tray_status::STARTUP_TRAY_ATTEMPTS {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                }
            }

            if !tray_status.available {
                log::error!(
                    "Tray icon creation failed after {} startup attempts; keeping the main window visible and scheduling recovery",
                    tray_status.attempts
                );
                show_main_window(app.app_handle());
                schedule_deferred_tray_recovery(
                    app.app_handle().clone(),
                    Arc::clone(&tray_builder),
                );
            }

            // Windows ignores macOS template icons, so a single white mark is
            // invisible on a light taskbar. Pick the icon that contrasts with the
            // current taskbar theme (refreshed on theme changes in on_window_event).
            #[cfg(target_os = "windows")]
            apply_tray_theme_icon(app.app_handle());

            // Load recording mode into AppState; shortcut routing itself is rebuilt below
            // from the persisted settings store and current recording state.
            log_start("HOTKEY_SETUP");
            let recording_mode_str = match app.store("settings") {
                Ok(store) => store
                    .get("recording_mode")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "toggle".to_string()),
                Err(e) => {
                    log_failed("SETTINGS_LOAD", &format!("Failed to load settings store: {}", e));
                    "toggle".to_string()
                }
            };

            let app_state = app.state::<AppState>();
            let recording_mode = match recording_mode_str.as_str() {
                "push_to_talk" => RecordingMode::PushToTalk,
                _ => RecordingMode::Toggle,
            };

            if let Ok(mut mode_guard) = app_state.recording_mode.lock() {
                *mode_guard = recording_mode;
                log::info!("Recording mode set to: {:?}", recording_mode);
            }

            crate::trigger::engine_host::start_engine(app.app_handle());
            crate::trigger::engine_host::rebuild_engine_bindings(app.app_handle());
            log_complete("HOTKEY_SETUP", 0);
            {
                let engine_app = app.app_handle().clone();
                app.listen("accessibility-granted", move |_event| {
                    crate::trigger::engine_host::start_engine(&engine_app);
                    crate::trigger::engine_host::rebuild_engine_bindings(&engine_app);
                });
            }

            // Preload current model if set (graceful degradation)
            // Use Tauri's async runtime which is available after setup
            if let Ok(store) = app.store("settings") {
                let current_model_engine = store
                    .get("current_model_engine")
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "whisper".to_string());
                if let Some(current_model) = store.get("current_model")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .filter(|s| !s.is_empty())
                    .filter(|_| current_model_engine == "whisper")
                {
                    let app_handle = app.app_handle().clone();
                    // Use tauri::async_runtime instead of tokio directly
                    tauri::async_runtime::spawn(async move {
                        log::info!("Attempting to preload model on startup: {}", current_model);

                        // Get model path from WhisperManager
                        let whisper_state = app_handle.state::<AsyncRwLock<whisper::manager::WhisperManager>>();
                        let model_path = {
                            let manager = whisper_state.read().await;
                            manager.get_model_path(&current_model)
                        };

                        if let Some(model_path) = model_path {
                            if crate::commands::audio::warm_whisper_gpu_sidecar_on_model_preload(
                                &app_handle,
                                &model_path,
                            )
                            .await
                            {
                                log::info!(
                                    "Successfully preloaded model '{}' in Vulkan sidecar",
                                    current_model
                                );
                            } else {
                                let preload_result = {
                                    let cache_state = app_handle.state::<AsyncMutex<TranscriberCache>>();
                                    let mut cache = cache_state.lock().await;
                                    cache.get_or_create(&model_path).map(|_| ())
                                };

                                match preload_result {
                                    Ok(()) => {
                                        log::info!("Successfully preloaded model '{}' into cache", current_model);
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to preload model '{}': {}. App will continue without preloading.",
                                                 current_model, e);
                                    }
                                }
                            }
                        } else {
                            log::warn!("Model '{}' not found in models directory, skipping preload", current_model);
                        }
                    });
                } else if current_model_engine == "whisper" {
                    log::info!("No Whisper model configured for preloading");
                } else {
                    log::debug!(
                        "Skipping startup Whisper preload for '{}' engine",
                        current_model_engine
                    );
                }
            }

            // Create pill (macOS) and toast (all platforms) windows at startup
            {
                use tauri::{WebviewUrl, WebviewWindowBuilder};

                // On macOS, use the same saved placement and monitor work area as
                // later repositioning so an always-visible pill starts clear of the Dock.
                #[cfg(target_os = "macos")]
                let ((pos_x, pos_y), (toast_x, toast_y)) = {
                    let app_state = app.state::<AppState>();
                    app_state
                        .get_window_manager()
                        .map(|manager| manager.current_floating_window_positions())
                        .unwrap_or_else(|| {
                            log::warn!("Window manager unavailable during floating window placement; using safe defaults");
                            ((600.0, 842.0), (520.0, 754.0))
                        })
                };

                // Preserve the established non-macOS startup behavior: primary
                // monitor, bottom-center, with the fixed 10px edge offset.
                #[cfg(not(target_os = "macos"))]
                let ((pos_x, pos_y), (toast_x, toast_y)) = {
                    let (screen_width, screen_height) =
                        crate::utils::monitor::catch_monitor_panic(|| {
                            let monitor = app.primary_monitor().ok().flatten()?;
                            let size = monitor.size();
                            let scale = monitor.scale_factor();
                            Some((size.width as f64 / scale, size.height as f64 / scale))
                        })
                        .flatten()
                        .unwrap_or((1440.0, 900.0));
                    let pill_x = (screen_width - crate::window_manager::PILL_WIDTH) / 2.0;
                    let pill_y = screen_height - crate::window_manager::PILL_HEIGHT - 10.0;
                    let toast_x = pill_x
                        + (crate::window_manager::PILL_WIDTH
                            - crate::window_manager::TOAST_WIDTH)
                            / 2.0;
                    let toast_y = pill_y
                        - crate::window_manager::TOAST_HEIGHT
                        - crate::window_manager::FLOATING_WINDOW_GAP;
                    ((pill_x, pill_y), (toast_x, toast_y))
                };

                // macOS: create pill window and convert to NSPanel
                #[cfg(target_os = "macos")]
                {
                    // Properties aligned with window_manager.rs for consistency.
                    let pill_builder = WebviewWindowBuilder::new(app, "pill", WebviewUrl::App("pill".into()))
                        .title("Recording")
                        .resizable(false)
                        .maximizable(false)
                        .minimizable(false)
                        .decorations(false)
                        .always_on_top(true)
                        .visible_on_all_workspaces(true)
                        .content_protected(true)
                        .skip_taskbar(true)
                        .transparent(true)
                        .shadow(false)  // Prevent window shadow on macOS
                        .inner_size(crate::window_manager::PILL_WIDTH, crate::window_manager::PILL_HEIGHT)
                        .position(pos_x, pos_y)
                        .visible(true)  // Always visible (controlled by show_pill_indicator setting)
                        .focused(false);  // Don't steal focus

                    // Disable context menu only in production builds
                    #[cfg(not(debug_assertions))]
                    let pill_builder = pill_builder.initialization_script("document.addEventListener('contextmenu', e => e.preventDefault());");

                    #[cfg(debug_assertions)]
                    let pill_builder = pill_builder;

                    let pill_window = pill_builder.build()?;
                    if let Err(error) = pill_window.set_ignore_cursor_events(true) {
                        log::warn!("Failed to make pill window click-through: {}", error);
                    }

                    // Convert to NSPanel to prevent focus stealing
                    use tauri_nspanel::WebviewWindowExt;
                    pill_window.to_panel().map_err(|e| format!("Failed to convert to NSPanel: {:?}", e))?;

                    // Store the pill window reference in WindowManager
                    let app_state = app.state::<AppState>();
                    if let Some(window_manager) = app_state.get_window_manager() {
                        window_manager.set_pill_window(pill_window);
                        log::info!("Created pill window as NSPanel and stored in WindowManager");
                    } else {
                        log::warn!("Could not store pill window reference - WindowManager not available");
                    }
                }

                // Create toast window for feedback messages - all platforms
                log::info!(
                    "Toast window position: ({}, {}) relative to pill at ({}, {})",
                    toast_x,
                    toast_y,
                    pos_x,
                    pos_y
                );

                let toast_builder = WebviewWindowBuilder::new(app, "toast", WebviewUrl::App("toast".into()))
                    .title("Feedback")
                    .resizable(false)
                    .decorations(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .transparent(true)
                    .shadow(false) // Prevent window shadow/outline on macOS
                    .inner_size(
                        crate::window_manager::TOAST_WIDTH,
                        crate::window_manager::TOAST_HEIGHT,
                    )
                    .position(toast_x, toast_y)
                    .visible(false); // Starts hidden

                #[cfg(not(debug_assertions))]
                let toast_builder = toast_builder.initialization_script("document.addEventListener('contextmenu', e => e.preventDefault());");

                #[cfg(debug_assertions)]
                let toast_builder = toast_builder;

                let toast_window = toast_builder.build()?;

                // macOS: Convert toast to NSPanel to match pill behavior
                #[cfg(target_os = "macos")]
                {
                    use tauri_nspanel::WebviewWindowExt;
                    toast_window.to_panel().map_err(|e| format!("Failed to convert toast to NSPanel: {:?}", e))?;
                }

                log::info!("Created toast window for feedback");
            }

            // Sync autostart state on startup using shared logic
            {
                let app_handle = app.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(store) = app_handle.store("settings") {
                        let saved_autostart = store.get("launch_at_startup")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        // set_autostart will make OS match the saved setting,
                        // then persist the actual result.
                        match set_autostart(app_handle.clone(), saved_autostart).await {
                            Ok(actual) => {
                                if actual != saved_autostart {
                                    log::info!(
                                        "Autostart synced: saved={}, actual={}",
                                        saved_autostart, actual
                                    );
                                }
                            }
                            Err(e) => {
                                log::warn!("Failed to sync autostart on startup: {}", e);
                            }
                        }
                    }
                });
            }

            // Hide main window on start (menu bar only)
            // Only a configured local/cloud model hides the main window immediately.
            // Remote-only sessions stay visible until startup checks verify the remote is available.
            let should_hide_main = if let Ok(store) = app.store("settings") {
                let has_local_or_cloud_model = store
                    .get("current_model")
                    .and_then(|v| v.as_str().map(|s| !s.is_empty()))
                    .unwrap_or(false);

                has_local_or_cloud_model && active_id.is_none()
            } else {
                false
            };

            if should_hide_main {
                if hide_main_window(app.app_handle()) {
                    log::info!("Main window hidden - menubar mode active");
                } else {
                    log::warn!("Main window remains visible because menubar mode is unavailable");
                }
            } else {
                log::info!("👋 First launch or no source configured - keeping main window visible");
                // Show dock icon when main window is visible
                #[cfg(target_os = "macos")]
                show_dock_icon(app.app_handle());
            }

            // Log setup completion
            log_performance("APP_SETUP_COMPLETE", setup_start.elapsed().as_millis() as u64, None);
            log::info!("🎉 App setup COMPLETED - Total time: {}ms", setup_start.elapsed().as_millis());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            cancel_recording,
            get_current_recording_state,
            debug_transcription_flow,
            test_transcription_event,
            save_transcription,
            get_audio_devices,
            get_current_audio_device,
            download_model,
            get_model_status,
            get_parakeet_vocabulary_status,
            download_parakeet_vocabulary_model,
            preload_model,
            verify_model,
            set_cloud_stt_model,
            transcribe_audio,
            transcribe_audio_file,
            diarize_audio_file,
            get_settings,
            save_settings,
            get_transcription_acceleration_status,
            test_transcription_acceleration,
            set_audio_device,
            validate_microphone_selection,
            set_global_shortcut,
            get_shortcut_settings,
            update_shortcut_settings,
            list_shortcut_actions,
            get_supported_languages,
            set_model_from_tray,
            update_tray_menu,
            get_tray_status,
            retry_tray_creation,
            insert_text,
            delete_model,
            list_downloaded_models,
            cancel_download,
            cleanup_old_transcriptions,
            get_recordings_directory,
            open_recordings_folder,
            check_recording_exists,
            get_recording_path,
            save_retranscription,
            update_transcription,
            show_in_folder,
            get_transcription_history,
            get_transcription_count,
            delete_transcription_entry,
            clear_all_transcriptions,
            export_transcriptions,
            save_transcript_file,
            get_application_icon,
            show_pill_widget,
            hide_pill_widget,
            close_pill_widget,
            recreate_pill_widget,
            hide_toast_window,
            focus_main_window,
            check_accessibility_permission,
            request_accessibility_permission,
            open_accessibility_settings,
            open_microphone_settings,
            check_microphone_permission,
            request_microphone_permission,
            test_automation_permission,
            check_license_status,
            revalidate_license,
            restore_license,
            activate_license,
            deactivate_license,
            open_purchase_page,
            invalidate_license_cache,
            reset_app_data,
            copy_image_to_clipboard,
            save_image_to_file,
            copy_text_to_clipboard,
            get_ai_settings,
            get_ai_settings_for_provider,
            cache_ai_api_key,
            validate_ai_api_key,
            set_openai_config,
            get_openai_config,
            test_openai_endpoint,
            clear_ai_api_key_cache,
            update_ai_settings,
            update_agent_cli_reasoning,
            update_agent_cli_fast_mode,
            disable_ai_enhancement,
            get_enhancement_options,
            update_enhancement_options,
            get_writing_settings,
            update_writing_settings,
            list_ai_providers,
            list_provider_models,
            probe_agent_cli,
            keyring_set,
            keyring_get,
            keyring_delete,
            keyring_has,
            validate_stt_key,
            clear_stt_key_cache,
            get_soniox_storage_counts,
            cleanup_soniox_storage,
            get_latest_log_for_bug_report,
            get_log_directory,
            open_logs_folder,
            get_autostart_status,
            set_autostart,
            get_device_id,
            get_distribution_info,
            check_for_app_update,
            install_app_update,
            get_system_specs,
            // CLI launcher (voicetypr on PATH)
            install_cli_tool,
            repair_cli_tool,
            uninstall_cli_tool,
            cli_tool_status,
            // Independent privacy controls: GlitchTip diagnostics and PostHog
            // product analytics.
            get_telemetry_status,
            set_telemetry_consent,
            get_product_analytics_status,
            set_product_analytics_consent,
            defer_privacy_consent_for_session,
            record_onboarding_completed,
            report_frontend_error,
            // Remote transcription commands
            refresh_active_remote_server_status,
            get_recognition_availability_snapshot,
            start_sharing,
            stop_sharing,
            get_sharing_status,
            update_remote_model_control_enabled,
            get_local_ips,
            get_local_machine_id,
            get_firewall_status,
            discover_remote_servers,
            open_firewall_settings,
            add_remote_server,
            remove_remote_server,
            update_remote_server,
            list_remote_servers,
            test_remote_connection,
            test_remote_server,
            set_active_remote_server,
            get_active_remote_server,
            transcribe_remote,
            refresh_remote_servers,
            check_remote_server_status,
            get_remote_transcription_control,
            update_remote_transcription_control,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Only hide the window instead of closing it (except for pill)
                if window.label() == "main" {
                    // Close-to-tray ONLY when a tray icon exists. If tray creation
                    // failed at startup, prevent_close + hide would strand the app as a
                    // background process with no way back -> let the close proceed (quit).
                    if window.app_handle().tray_by_id("main").is_some() {
                        api.prevent_close();
                        if hide_main_window(window.app_handle()) {
                            log::info!("Main window hidden instead of closed");
                        }
                    } else {
                        log::warn!("No tray icon; allowing main window to close (quit) instead of hiding");
                    }
                }
            }
            // Refresh the tray icon when the OS theme flips (Windows taskbar).
            #[cfg(target_os = "windows")]
            if let tauri::WindowEvent::ThemeChanged(_) = event {
                apply_tray_theme_icon(window.app_handle());
            }
        })
        .build(app_context)
        .map_err(|e| -> Box<dyn std::error::Error> {
            log_failed("APPLICATION_BUILD", &format!("Critical error building Tauri application: {}", e));
            log_with_context(log::Level::Error, "Application build failed", &[
                ("stage", "application_build"),
                ("total_startup_time_ms", app_start.elapsed().as_millis().to_string().as_str())
            ]);
            eprintln!("Voicetypr failed to start: {}", e);
            Box::new(e)
        })?
        .run(|app_handle, event| match event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen {
                has_visible_windows,
                ..
            } => {
                if !has_visible_windows {
                    show_main_window(app_handle);
                }
            }
            tauri::RunEvent::Exit => {
                // Never strand a system output device we muted: restore the
                // mute state we changed before the process goes away.
                #[cfg(target_os = "macos")]
                crate::commands::audio::cleanup_media_pause_on_exit();
                product_analytics::shutdown()
            }
            _ => {}
        });

    // Log successful application startup
    log_lifecycle_event("APPLICATION_READY", Some(app_version), None);

    Ok(())
}

fn migrate_ai_settings_before_key_cache(app: &tauri::AppHandle) {
    let Ok(store) = app.store("settings") else {
        log::warn!("AI settings migration skipped: settings store unavailable");
        return;
    };

    const MIGRATION_KEYS: [&str; 6] = [
        "ai_enabled",
        "ai_provider",
        "ai_model",
        "ai_models_by_provider",
        "ai_model_needs_reselection",
        "enhancement_options",
    ];
    let mut values = serde_json::Map::new();
    for key in MIGRATION_KEYS {
        if let Some(value) = store.get(key) {
            values.insert(key.to_string(), value.clone());
        }
    }
    let original_values = values.clone();

    if migrate_ai_settings_values(&mut values) {
        for (key, value) in values {
            store.set(key, value);
        }
        if let Err(error) = store.save() {
            for key in MIGRATION_KEYS {
                if let Some(value) = original_values.get(key) {
                    store.set(key, value.clone());
                } else {
                    store.delete(key);
                }
            }
            log::warn!("AI settings migration save failed: {}", error);
        } else {
            log::info!("AI settings migration applied");
        }
    }
}

fn migrate_ai_settings_values(values: &mut serde_json::Map<String, serde_json::Value>) -> bool {
    let mut changed = false;

    let provider = values
        .get("ai_provider")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let migrated_provider = if provider == "google" {
        changed = true;
        "gemini".to_string()
    } else {
        provider
    };
    if values
        .get("ai_provider")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        != migrated_provider
    {
        values.insert(
            "ai_provider".to_string(),
            serde_json::Value::String(migrated_provider.clone()),
        );
        changed = true;
    }

    if let Some(models) = values
        .get_mut("ai_models_by_provider")
        .and_then(|value| value.as_object_mut())
    {
        if let Some(google_model) = models.remove("google") {
            if !models.contains_key("gemini") {
                models.insert("gemini".to_string(), google_model);
            }
            changed = true;
        }
    }

    let current_model = values
        .get("ai_model")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let ai_enabled = values
        .get("ai_enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !current_model.is_empty()
        && !ai_model_is_valid_for_provider(&migrated_provider, &current_model)
    {
        values.insert(
            "ai_model".to_string(),
            serde_json::Value::String(String::new()),
        );
        if ai_enabled {
            values.insert(
                "ai_model_needs_reselection".to_string(),
                serde_json::Value::Bool(true),
            );
        }
        changed = true;
    }

    if let Some(stored_options) = values.get("enhancement_options").cloned() {
        let normalized = crate::ai::prompts::enhancement_options_for_ai_enabled(
            Some(&stored_options),
            ai_enabled,
        )
        .and_then(|options| {
            serde_json::to_value(options)
                .map_err(|error| format!("Failed to serialize Polish options: {error}"))
        });
        match normalized {
            Ok(normalized) if normalized != stored_options => {
                values.insert("enhancement_options".to_string(), normalized);
                changed = true;
            }
            Ok(_) => {}
            Err(error) => log::warn!("Polish settings migration skipped: {}", error),
        }
    }

    changed
}

fn ai_model_is_valid_for_provider(provider: &str, model: &str) -> bool {
    if provider == "custom" {
        return !model.trim().is_empty();
    }
    crate::ai::providers::recommended_models(provider)
        .iter()
        .any(|candidate| candidate.model_id == model)
}

/// Perform essential startup checks
async fn perform_startup_checks(app: tauri::AppHandle) {
    let checks_start = Instant::now();
    log_start("STARTUP_CHECKS");
    log_with_context(
        log::Level::Debug,
        "Running startup checks",
        &[("stage", "comprehensive_validation")],
    );

    if let Err(err) = crate::commands::license::check_license_status_internal(&app).await {
        log::warn!(
            "Failed to warm runtime license cache during startup checks: {}",
            err
        );
    }

    if let Some(remote_settings) =
        app.try_state::<AsyncMutex<crate::remote::settings::RemoteSettings>>()
    {
        if let Err(err) = crate::commands::remote::refresh_active_remote_server_status_impl(
            &app,
            &remote_settings,
        )
        .await
        {
            log::warn!(
                "Failed to refresh active remote status during startup checks: {}",
                err
            );
        }
    }

    let availability = crate::recognition::emit_recognition_availability(&app).await;

    log_model_operation(
        "AVAILABILITY_CHECK",
        "all",
        if availability.any_available() {
            "AVAILABLE"
        } else {
            "NONE_FOUND"
        },
        None,
    );

    if availability.any_available() {
        if let Err(e) = auto_select_model_if_needed(&app, &availability).await {
            log::warn!("Failed to auto-select default model: {}", e);
        }
        if availability.remote_available {
            let onboarding_completed = app
                .store("settings")
                .ok()
                .and_then(|store| store.get("onboarding_completed").and_then(|v| v.as_bool()))
                .unwrap_or(false);

            if onboarding_completed {
                hide_main_window(&app);
            }
        }
    }

    if !availability.any_available() {
        log::warn!("⚠️  No speech recognition engines are ready");
        let _ = app.emit("no-models-on-startup", ());
        show_main_window(&app);
    } else {
        log::info!("✅ At least one speech recognition engine is ready");
    }

    // Validate AI settings if enabled
    if let Ok(store) = app.store("settings") {
        let ai_enabled = store
            .get("ai_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if ai_enabled {
            let provider = store
                .get("ai_provider")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();

            let model = store
                .get("ai_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();

            // Check if API key is cached
            use crate::commands::ai::get_ai_settings;
            match get_ai_settings(app.clone()).await {
                Ok(settings) => {
                    if !settings.has_api_key {
                        log::warn!("AI enabled but no API key found for provider: {}", provider);
                        // Disable AI to prevent errors during recording
                        store.set("ai_enabled", serde_json::Value::Bool(false));
                        let _ = store.save();

                        // Notify frontend
                        let _ = emit_to_window(
                            &app,
                            "main",
                            "ai-disabled-no-key",
                            "AI enhancement disabled - no API key found",
                        );
                    } else if model.is_empty() {
                        log::warn!("AI enabled but no model selected");
                        store.set("ai_enabled", serde_json::Value::Bool(false));
                        let _ = store.save();

                        let _ = emit_to_window(
                            &app,
                            "main",
                            "ai-disabled-no-model",
                            "AI enhancement disabled - no model selected",
                        );
                    } else {
                        log::info!("AI enhancement ready: {} with {}", provider, model);
                    }
                }
                Err(e) => {
                    log::error!("Failed to check AI settings: {}", e);
                }
            }
        }
    }

    let mut autoload_parakeet_model: Option<String> = None;
    let mut selection_was_reset = false;

    // Pre-check recording settings
    if let Ok(store) = app.store("settings") {
        // Validate speech language setting and keep the legacy key in sync.
        let speech_language = store
            .get("speech_language")
            .or_else(|| store.get("language"))
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        if let Some(lang) = speech_language {
            use crate::whisper::languages::validate_language;
            let validated = validate_language(Some(&lang));
            if validated != lang.as_str() {
                log::warn!(
                    "Invalid speech language '{}' in settings, resetting to '{}'",
                    lang,
                    validated
                );
                store.set(
                    "speech_language",
                    serde_json::Value::String(validated.to_string()),
                );
                store.set("language", serde_json::Value::String(validated.to_string()));
                let _ = store.save();
            }
        }

        // Check current model is still available based on engine type
        let mut _model_available = false;
        if let Some(current_model) = store
            .get("current_model")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
        {
            if !current_model.is_empty() {
                // Get the engine type to determine which manager to check
                let engine = store
                    .get("current_model_engine")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "whisper".to_string());

                if engine == "parakeet" {
                    // Check ParakeetManager for Parakeet models
                    if let Some(parakeet_manager) = app.try_state::<parakeet::ParakeetManager>() {
                        let models = parakeet_manager.list_models();
                        // Is the selected id a known catalog entry (downloaded or not)?
                        let model_known = models.iter().any(|m| m.name == current_model);
                        if let Some(status) = models.iter().find(|m| m.name == current_model) {
                            _model_available = status.downloaded;
                            if status.downloaded {
                                autoload_parakeet_model = Some(current_model.clone());
                            }
                        }
                        if model_known {
                            // Registered model: keep the selection even when the
                            // on-disk file is missing, so the dashboard / Repair
                            // flow and `requires_setup` can recover it.
                            if !_model_available {
                                log::warn!(
                                    "Current Parakeet model '{}' is registered but not on disk; keeping selection",
                                    current_model
                                );
                            }
                        } else {
                            // Unknown id (e.g. removed from the registry): fall
                            // back to auto-select instead of being stuck on an
                            // unloadable model.
                            log::warn!(
                                "Current Parakeet model '{}' is not a known catalog entry; resetting selection",
                                current_model
                            );
                            store.set("current_model", serde_json::Value::String(String::new()));
                            store.set(
                                "current_model_engine",
                                serde_json::Value::String("whisper".to_string()),
                            );
                            let _ = store.save();
                            selection_was_reset = true;
                        }
                    }
                } else {
                    // Check WhisperManager for Whisper models (default)
                    if let Some(whisper_manager) =
                        app.try_state::<AsyncRwLock<whisper::manager::WhisperManager>>()
                    {
                        let guard = whisper_manager.read().await;
                        // Is the selected id a known registered Whisper model?
                        let model_known = guard.get_models_status().contains_key(&current_model);
                        let downloaded = guard.get_downloaded_model_names();
                        _model_available = downloaded.contains(&current_model);
                        if model_known {
                            // Registered model: keep the selection even when the
                            // on-disk file is missing, so the dashboard / Repair
                            // flow and `requires_setup` can recover it.
                            if !_model_available {
                                log::warn!(
                                    "Current Whisper model '{}' is registered but not on disk; keeping selection",
                                    current_model
                                );
                            }
                        } else {
                            // Unknown id (e.g. removed from the registry): fall
                            // back to auto-select instead of being stuck on an
                            // unloadable model.
                            log::warn!(
                                "Current Whisper model '{}' is not a known catalog entry; resetting selection",
                                current_model
                            );
                            store.set("current_model", serde_json::Value::String(String::new()));
                            let _ = store.save();
                            selection_was_reset = true;
                        }
                    }
                }
            }
        }
    }

    // A removed/unknown selection was cleared above. The earlier
    // `auto_select_model_if_needed` ran while the stale id was still set (so it
    // no-op'd); re-run it now that the selection is empty so a downloaded model
    // is chosen instead of leaving the user with no selection.
    if selection_was_reset && availability.any_available() {
        if let Err(e) = auto_select_model_if_needed(&app, &availability).await {
            log::warn!(
                "Failed to auto-select after clearing unknown model selection: {}",
                e
            );
        }
    }

    if let Some(model_name) = autoload_parakeet_model {
        if let Some(parakeet_manager) = app.try_state::<parakeet::ParakeetManager>() {
            match parakeet_manager.load_model(&app, &model_name).await {
                Ok(_) => {
                    log::info!("✅ Parakeet model '{}' autoloaded from cache", model_name);
                }
                Err(err) => {
                    log::warn!(
                        "Failed to autoload Parakeet model '{}': {}",
                        model_name,
                        err
                    );
                    let message = format!(
                        "Unable to load Parakeet model '{}'. Please re-download it.",
                        model_name
                    );
                    let _ = app.emit("parakeet-unavailable", message.clone());
                }
            }
        }
    }

    // Log startup checks completion
    log_complete("STARTUP_CHECKS", checks_start.elapsed().as_millis() as u64);
    log_with_context(
        log::Level::Debug,
        "Startup checks complete",
        &[("status", "all_checks_completed")],
    );
    log::info!(
        "✅ Startup checks COMPLETED in {}ms",
        checks_start.elapsed().as_millis()
    );
}

// AI settings migration tests live here because startup owns the migration call.
#[cfg(test)]
mod ai_settings_migration_tests {
    use super::*;
    use serde_json::json;

    fn values(
        provider: &str,
        model: &str,
        models: serde_json::Value,
    ) -> serde_json::Map<String, serde_json::Value> {
        values_with_enabled(true, provider, model, models)
    }

    fn values_with_enabled(
        enabled: bool,
        provider: &str,
        model: &str,
        models: serde_json::Value,
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut values = serde_json::Map::new();
        values.insert("ai_enabled".to_string(), json!(enabled));
        values.insert("ai_provider".to_string(), json!(provider));
        values.insert("ai_model".to_string(), json!(model));
        values.insert("ai_models_by_provider".to_string(), models);
        values
    }

    #[test]
    fn migrates_google_provider_and_model_memory_to_gemini() {
        let mut values = values(
            "google",
            "gemini-2.5-flash",
            json!({ "google": "gemini-2.5-flash" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(values["ai_provider"], json!("gemini"));
        assert_eq!(
            values["ai_models_by_provider"],
            json!({ "gemini": "gemini-2.5-flash" })
        );
        assert_eq!(values["ai_model"], json!("gemini-2.5-flash"));
        assert_eq!(values.get("ai_model_needs_reselection"), None);
    }

    #[test]
    fn migration_keeps_existing_gemini_model_memory() {
        let mut values = values(
            "google",
            "gemini-2.5-flash",
            json!({ "google": "gemini-3-flash-preview", "gemini": "gemini-2.5-flash" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(
            values["ai_models_by_provider"],
            json!({ "gemini": "gemini-2.5-flash" })
        );
        assert_eq!(values.get("ai_model_needs_reselection"), None);
    }

    #[test]
    fn migration_flags_enabled_invalid_model_for_reselection_without_substitution() {
        let mut values = values("google", "text-bison", json!({ "google": "text-bison" }));

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(values["ai_enabled"], json!(true));
        assert_eq!(values["ai_provider"], json!("gemini"));
        assert_eq!(values["ai_model"], json!(""));
        assert_eq!(values["ai_model_needs_reselection"], json!(true));
    }

    #[test]
    fn migration_does_not_flag_disabled_invalid_model() {
        let mut values = values_with_enabled(
            false,
            "google",
            "text-bison",
            json!({ "google": "text-bison" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(values["ai_enabled"], json!(false));
        assert_eq!(values["ai_provider"], json!("gemini"));
        assert_eq!(values["ai_model"], json!(""));
        assert_eq!(values.get("ai_model_needs_reselection"), None);
    }

    #[test]
    fn migration_does_not_flag_empty_model() {
        let mut values = values("google", "", json!({ "google": "" }));

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(values["ai_enabled"], json!(true));
        assert_eq!(values["ai_provider"], json!("gemini"));
        assert_eq!(values["ai_model"], json!(""));
        assert_eq!(values.get("ai_model_needs_reselection"), None);
    }

    #[test]
    fn migration_keeps_custom_free_text_model() {
        let mut values = values("custom", "local-model", json!({ "custom": "local-model" }));

        assert!(!migrate_ai_settings_values(&mut values));
        assert_eq!(values["ai_model"], json!("local-model"));
    }

    #[test]
    fn migration_normalizes_legacy_global_preset_for_enabled_polish() {
        let mut values = values_with_enabled(
            true,
            "custom",
            "local-model",
            json!({ "custom": "local-model" }),
        );
        values.insert(
            "enhancement_options".to_string(),
            json!({ "preset": "Writing" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(
            values["enhancement_options"],
            json!({ "preset": "CleanDictation" })
        );
    }

    #[test]
    fn migration_normalizes_legacy_global_preset_for_disabled_polish() {
        let mut values = values_with_enabled(
            false,
            "custom",
            "local-model",
            json!({ "custom": "local-model" }),
        );
        values.insert(
            "enhancement_options".to_string(),
            json!({ "preset": "Writing" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        assert_eq!(
            values["enhancement_options"],
            json!({ "preset": "PersonalDictation" })
        );
    }

    #[test]
    fn migration_is_idempotent_after_first_pass() {
        let mut values = values(
            "google",
            "gemini-2.5-flash",
            json!({ "google": "gemini-2.5-flash" }),
        );

        assert!(migrate_ai_settings_values(&mut values));
        let first = values.clone();
        assert!(!migrate_ai_settings_values(&mut values));
        assert_eq!(values, first);
    }
}
