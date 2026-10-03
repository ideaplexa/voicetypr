use crate::recording::island::{BlockedKind, IslandAction};
use std::{sync::Mutex, time::Instant};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
#[derive(Default)]
struct Status {
    started: Option<Instant>,
    blocked: Option<(BlockedKind, IslandAction)>,
    revision: u64,
}
static STATUS: Mutex<Status> = Mutex::new(Status {
    started: None,
    blocked: None,
    revision: 0,
});
static STATUS_ITEM: Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>> = Mutex::new(None);
pub fn install(menu: &tauri::menu::Menu<tauri::Wry>) {
    *STATUS_ITEM.lock().unwrap_or_else(|e| e.into_inner()) =
        menu.get("status").and_then(|i| i.as_menuitem().cloned());
}
pub fn refresh(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::commands::settings::update_tray_menu(app).await;
    });
}
pub fn blocked(app: &AppHandle, kind: BlockedKind, action: IslandAction) {
    STATUS.lock().unwrap_or_else(|e| e.into_inner()).blocked = Some((kind, action));
    refresh(app);
}
pub fn fix_action() -> Option<IslandAction> {
    STATUS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .blocked
        .map(|(_, a)| a)
}
pub fn clear_blocked() {
    STATUS.lock().unwrap_or_else(|e| e.into_inner()).blocked = None;
}
pub fn snapshot() -> (Option<u64>, Option<String>) {
    let status = STATUS.lock().unwrap_or_else(|e| e.into_inner());
    (
        status.started.map(|t| t.elapsed().as_secs()),
        status.blocked.map(|(kind, _)| problem(kind).into()),
    )
}
fn problem(kind: BlockedKind) -> &'static str {
    match kind {
        BlockedKind::LicenseCheckFailed => "License check failed",
        BlockedKind::LicenseVerifyRequired => "License verification required",
        BlockedKind::TrialEnded => "Trial ended",
        BlockedKind::NoEngine => "No engine available",
        BlockedKind::CloudKeyMissing => "Cloud key missing",
        BlockedKind::CloudKeyRejected => "Cloud key rejected",
        BlockedKind::SonioxStorageFull => "Soniox storage full",
        BlockedKind::MicPermissionDenied => "Microphone permission needed",
        BlockedKind::MicMissing => "Microphone missing",
        BlockedKind::MicBusy => "Microphone busy",
        BlockedKind::AccessibilityOff => "Accessibility permission needed",
        BlockedKind::StartingUp => "Starting up",
    }
}
pub fn recording_changed(app: &AppHandle, state: crate::RecordingState) {
    let (changed, revision) = {
        let mut status = STATUS.lock().unwrap_or_else(|e| e.into_inner());
        if state == crate::RecordingState::Starting {
            status.blocked = None;
        }
        let recording = state == crate::RecordingState::Recording;
        if recording == status.started.is_some() {
            return;
        }
        status.revision += 1;
        status.started = recording.then(Instant::now);
        if recording {
            status.blocked = None;
        }
        (recording, status.revision)
    };
    refresh(app);
    if changed {
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let status = STATUS.lock().unwrap_or_else(|e| e.into_inner());
                if status.revision != revision {
                    return;
                }
                let Some(started) = status.started else {
                    return;
                };
                let seconds = started.elapsed().as_secs();
                if let Some(item) = STATUS_ITEM
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                {
                    let _ = item.set_text(super::model::recording_label(seconds));
                }
            }
        });
    }
}
pub fn shortcut_text(app: &AppHandle) -> String {
    let primary = crate::commands::shortcuts::get_effective_primary_shortcut(app.clone()).ok();
    let ptt = app.store("settings").ok().and_then(|store| {
        let hold = store
            .get("recording_mode")
            .and_then(|v| v.as_str().map(str::to_owned))
            .as_deref()
            == Some("push_to_talk");
        let separate = store
            .get("use_different_ptt_key")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        (hold && separate)
            .then(|| {
                store
                    .get("ptt_hotkey")
                    .and_then(|v| v.as_str().map(str::to_owned))
            })
            .flatten()
            .filter(|s| !s.trim().is_empty())
    });
    let raw = ptt
        .or_else(|| {
            primary.and_then(|p| {
                p.hotkey.or_else(|| {
                    p.binding.map(|b| {
                        if !b.shortcut.is_empty() {
                            b.shortcut
                        } else if let Some(modifier) = b.modifier {
                            format!("{:?} {:?}", modifier.side, modifier.modifier)
                        } else {
                            "Recording shortcut".into()
                        }
                    })
                })
            })
        })
        .unwrap_or_else(|| {
            if cfg!(target_os = "windows") {
                "Ctrl+Alt+Space"
            } else {
                "Alt+Space"
            }
            .into()
        });
    if cfg!(target_os = "macos") {
        raw.replace("CommandOrControl", "⌘")
            .replace("CmdOrCtrl", "⌘")
            .replace("Command", "⌘")
            .replace("Meta", "⌘")
            .replace("Control", "⌃")
            .replace("Ctrl", "⌃")
            .replace("Alt", "⌥")
            .replace("Shift", "⇧")
            .replace('+', "")
    } else {
        raw.replace("CommandOrControl", "Ctrl")
            .replace("CmdOrCtrl", "Ctrl")
            .replace("Control", "Ctrl")
    }
}
