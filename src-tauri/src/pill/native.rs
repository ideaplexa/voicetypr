//! Non-activating native configuration, shared by the pill and temporary toast.
#[cfg(target_os = "macos")]
use tauri::Manager;
use tauri::{Runtime, WebviewWindow};

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
        let panel = window.to_panel().map_err(|e| e.to_string())?;
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
        panel.set_collection_behaviour(
            NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary,
        );
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

fn show_native<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        // Never call RawNSPanel::show(): it explicitly makes the panel key.
        window
            .app_handle()
            .get_webview_panel(window.label())
            .map_err(|e| format!("{e:?}"))?
            .order_front_regardless();
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
    on_main(window, show_native)
}
