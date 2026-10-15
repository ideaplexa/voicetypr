//! Non-activating native configuration, for the island panel.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    OnceLock,
};
#[cfg(target_os = "macos")]
use tauri::Manager;
use tauri::{Runtime, WebviewWindow};

/// Debug-only A/B switch for how the idle island is concealed (plan 082 F0).
pub const SHOW_MODE_ENV: &str = "VOICETYPR_PILL_SHOW_MODE";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowMode {
    /// Default: `hide()` while idle, `orderFrontRegardless` on show.
    Hide,
    /// macOS debug experiment: stay ordered in at alpha 0 and click-through while idle.
    Alpha,
}

impl ShowMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hide => "hide",
            Self::Alpha => "alpha",
        }
    }
}

/// Pure gate: only a debug build with the exact value `alpha` leaves the default.
pub fn parse_show_mode(var: Option<&str>, debug_build: bool) -> ShowMode {
    if debug_build && var == Some("alpha") {
        ShowMode::Alpha
    } else {
        ShowMode::Hide
    }
}

pub fn show_mode() -> ShowMode {
    static MODE: OnceLock<ShowMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        // TODO(windows): a layered window at alpha 0 with WS_EX_TRANSPARENT; until
        // then Windows always uses the default hide/show.
        if !cfg!(target_os = "macos") {
            return ShowMode::Hide;
        }
        parse_show_mode(
            std::env::var(SHOW_MODE_ENV).ok().as_deref(),
            cfg!(debug_assertions),
        )
    })
}

/// Logical visibility in alpha mode, where the native window never leaves the screen list.
static ALPHA_SHOWN: AtomicBool = AtomicBool::new(false);

#[allow(deprecated)] // The pinned v2 API accepts cocoa collection-behavior flags.
fn configure_native<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // Pinned v2 has no PanelBuilder/config API. Convert using its real API,
        // then subclass RawNSPanel to override its hard-coded YES for key status.
        use tauri_nspanel::{
            cocoa::{
                appkit::NSWindowCollectionBehavior,
                base::{BOOL, NO},
            },
            objc::{
                declare::ClassDecl,
                runtime::{Class, Object, Sel},
            },
            WebviewWindowExt,
        };
        extern "C" fn never(_: &Object, _: Sel) -> BOOL {
            NO
        }
        let panel = window.to_panel().map_err(|e| {
            crate::telemetry::native_error("panel_conversion");
            e.to_string()
        })?;
        let subclass = Class::get("VoicetyprNonactivatingPanel").unwrap_or_else(|| {
            let mut cls = ClassDecl::new(
                "VoicetyprNonactivatingPanel",
                Class::get("RawNSPanel").expect("converted panel class"),
            )
            .expect("panel subclass");
            unsafe {
                cls.add_method(
                    Sel::register("canBecomeKeyWindow"),
                    never as extern "C" fn(&Object, Sel) -> BOOL,
                );
                cls.add_method(
                    Sel::register("canBecomeMainWindow"),
                    never as extern "C" fn(&Object, Sel) -> BOOL,
                );
            }
            cls.register()
        });
        unsafe {
            tauri_nspanel::raw_nspanel::object_setClass(
                window.ns_window().map_err(|e| e.to_string())? as _,
                subclass as *const _ as _,
            );
        }
        panel.set_style_mask(
            (objc2_app_kit::NSWindowStyleMask::Borderless
                | objc2_app_kit::NSWindowStyleMask::NonactivatingPanel)
                .bits() as i32,
        );
        panel.set_becomes_key_only_if_needed(true);
        panel.set_floating_panel(true);
        panel.set_level(objc2_app_kit::NSStatusWindowLevel as i32);
        let mut behaviour = NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary;
        if show_mode() == ShowMode::Alpha {
            // A panel that stays ordered in must not move with Spaces or join Cmd-Tab/Cmd-`.
            behaviour |= NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoresCycle;
        }
        panel.set_collection_behaviour(behaviour);
        panel.set_hides_on_deactivate(false);
        panel.set_has_shadow(false);
    }
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};
        let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                (style | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize)
                    & !(WS_EX_APPWINDOW.0 as isize),
            );
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    let _ = window; // Other platforms retain Tauri's non-focusable builder.
    Ok(())
}

/// Alpha mode idle state: ordered in, fully transparent and click-through.
fn conceal_native<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        let panel = window
            .app_handle()
            .get_webview_panel(window.label())
            .map_err(|e| format!("{e:?}"))?;
        panel.set_alpha_value(0.0);
        panel.set_ignore_mouse_events(true);
        if !panel.is_visible() {
            // Never RawNSPanel::show(): it makes the panel key.
            panel.order_front_regardless();
        }
    }
    let _ = window;
    Ok(())
}

fn show_native<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        // Never call RawNSPanel::show(): it explicitly makes the panel key.
        let panel = window
            .app_handle()
            .get_webview_panel(window.label())
            .map_err(|e| format!("{e:?}"))?;
        if show_mode() == ShowMode::Alpha {
            // The hit-region task restores the pointer-driven click-through state.
            panel.set_alpha_value(1.0);
            panel.set_ignore_mouse_events(true);
            if !panel.is_visible() {
                panel.order_front_regardless();
            }
            return Ok(());
        }
        panel.order_front_regardless();
    }
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};
        let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    window.show().map_err(|e| e.to_string())?;
    Ok(())
}

// AppKit and Win32 mutations always run on the UI thread. Tauri executes this
// closure inline when already on that thread, including during setup.
fn on_main<R: Runtime>(
    window: &WebviewWindow<R>,
    action: fn(&WebviewWindow<R>) -> Result<(), String>,
) -> Result<(), String> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let owned = window.clone();
    window
        .run_on_main_thread(move || {
            let _ = sender.send(action(&owned));
        })
        .map_err(|e| e.to_string())?;
    receiver.recv().map_err(|e| e.to_string())?
}
pub fn configure<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    on_main(window, configure_native)
}
pub fn show<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    on_main(window, show_native)?;
    ALPHA_SHOWN.store(true, Ordering::SeqCst);
    Ok(())
}
/// Idle state for a freshly configured window: alpha mode orders it in at alpha 0
/// (so the first show is only a repaint); the default mode leaves it hidden.
pub fn prepare_idle<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    ALPHA_SHOWN.store(false, Ordering::SeqCst);
    if show_mode() == ShowMode::Alpha {
        on_main(window, conceal_native)?;
    }
    Ok(())
}
/// Conceal the island: `hide()` by default, alpha 0 + click-through in alpha mode.
pub fn hide<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    ALPHA_SHOWN.store(false, Ordering::SeqCst);
    if show_mode() == ShowMode::Alpha {
        on_main(window, conceal_native)
    } else {
        window.hide().map_err(|e| e.to_string())
    }
}
/// Whether the island is shown to the user (not merely ordered in at alpha 0).
pub fn is_shown<R: Runtime>(window: &WebviewWindow<R>) -> bool {
    if show_mode() == ShowMode::Alpha {
        ALPHA_SHOWN.load(Ordering::SeqCst)
    } else {
        window.is_visible().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_mode_is_debug_only_and_exact() {
        assert_eq!(parse_show_mode(Some("alpha"), true), ShowMode::Alpha);
        assert_eq!(parse_show_mode(Some("alpha"), false), ShowMode::Hide);
        assert_eq!(parse_show_mode(Some("Alpha"), true), ShowMode::Hide);
        assert_eq!(parse_show_mode(Some("hide"), true), ShowMode::Hide);
        assert_eq!(parse_show_mode(None, true), ShowMode::Hide);
        assert_eq!(ShowMode::Hide.as_str(), "hide");
        assert_eq!(ShowMode::Alpha.as_str(), "alpha");
    }
}
