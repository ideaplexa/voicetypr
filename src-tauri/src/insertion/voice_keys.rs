//! The key presses behind voice commands, and what Voicetypr knows about the
//! last dictation it pasted, so "delete that" removes exactly it.
//!
//! "Delete that" trusts only Voicetypr's own record of the paste, never the
//! target app's accessibility tree. The record is valid while nothing that
//! could move the caret or change the text has happened since: no key-down
//! reached any app (counted by the trigger engine's event tap or hook), the
//! same app and window are still in front, and less than a minute has passed.
//! Mouse clicks are not observed (no existing hook sees them), so removal uses
//! the app's own undo, which takes back the paste wherever the caret is.
//! Only a count, a process id, a window id and a time are kept, in memory;
//! no text is stored or logged.

use crate::writing::VoiceCommand;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Key {
    Return,
    A,
    Z,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Modifier {
    None,
    Shift,
    /// ⌘ on macOS, Ctrl elsewhere.
    Primary,
}

type Press = (Key, Modifier);

/// Shift+Return, not Return: chat apps and web forms send on a bare Return.
const LINE_BREAK: Press = (Key::Return, Modifier::Shift);

/// After this, "delete that" no longer trusts the record.
const RECORD_LIFETIME: Duration = Duration::from_secs(60);

/// Why "delete that" did nothing. Shown in the pill.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// No dictation was pasted since the last command.
    NothingToDelete,
    /// A key was pressed since the dictation.
    TypedSince,
    /// Another app or window is in front.
    FocusChanged,
    /// The dictation is older than the record's lifetime.
    TooLate,
    /// The trigger engine is not watching keys, so typing cannot be ruled out.
    NotWatching,
}

pub(crate) enum CommandError {
    Skipped(SkipReason),
    Failed(String),
}

impl From<String> for CommandError {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Focus {
    process_id: u64,
    window_id: String,
}

struct LastInsertion {
    focus: Focus,
    key_downs: u64,
    at: Instant,
}

static LAST_INSERTION: Mutex<Option<LastInsertion>> = Mutex::new(None);

fn frontmost() -> Option<Focus> {
    active_win_pos_rs::get_active_window()
        .ok()
        .map(|window| Focus {
            process_id: window.process_id,
            window_id: window.window_id,
        })
}

/// Remember the dictation just pasted into the frontmost app.
pub(crate) fn remember_insertion() {
    let insertion = frontmost().map(|focus| LastInsertion {
        focus,
        key_downs: keytrigger::text_key_downs(),
        at: Instant::now(),
    });
    *LAST_INSERTION.lock().unwrap_or_else(|e| e.into_inner()) = insertion;
}

pub(crate) fn forget_insertion() {
    *LAST_INSERTION.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn check_record(
    last: Option<&LastInsertion>,
    watching: bool,
    key_downs: u64,
    now: Instant,
    focus: Option<&Focus>,
) -> Result<(), SkipReason> {
    let last = last.ok_or(SkipReason::NothingToDelete)?;
    if !watching {
        return Err(SkipReason::NotWatching);
    }
    if key_downs != last.key_downs {
        return Err(SkipReason::TypedSince);
    }
    if focus != Some(&last.focus) {
        return Err(SkipReason::FocusChanged);
    }
    if now.duration_since(last.at) > RECORD_LIFETIME {
        return Err(SkipReason::TooLate);
    }
    Ok(())
}

fn presses_for(command: VoiceCommand) -> Vec<Press> {
    match command {
        VoiceCommand::NewLine => vec![LINE_BREAK],
        VoiceCommand::NewParagraph => vec![LINE_BREAK, LINE_BREAK],
        // The app's own undo takes back the paste wherever the caret went;
        // Backspace would delete at a caret a click had moved.
        VoiceCommand::UndoThat | VoiceCommand::DeleteThat => vec![(Key::Z, Modifier::Primary)],
        VoiceCommand::SelectAll => vec![(Key::A, Modifier::Primary)],
        VoiceCommand::Send => vec![(Key::Return, Modifier::None)],
    }
}

/// Press the keys for a command in the frontmost app. `current` is checked
/// before every key, so a cancel or Secure Input stops a long sequence.
pub(crate) fn perform(
    command: VoiceCommand,
    watching: bool,
    current: &dyn Fn() -> Result<(), String>,
) -> Result<(), CommandError> {
    // Every command moves the caret or the text, so the record is only good
    // for the command that follows the dictation directly.
    let last = LAST_INSERTION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if command == VoiceCommand::DeleteThat {
        check_record(
            last.as_ref(),
            watching,
            keytrigger::text_key_downs(),
            Instant::now(),
            frontmost().as_ref(),
        )
        .map_err(CommandError::Skipped)?;
    }
    let key_downs = keytrigger::text_key_downs();
    for (key, modifier) in presses_for(command) {
        current()?;
        wait_for_modifiers_released(current)?;
        // Waiting for modifiers can take a while; recheck before the undo.
        if command == VoiceCommand::DeleteThat
            && (keytrigger::text_key_downs() != key_downs
                || frontmost().as_ref() != last.as_ref().map(|last| &last.focus))
        {
            return Err(CommandError::Failed(
                "Input changed while deleting".to_string(),
            ));
        }
        press(key, modifier)?;
    }
    Ok(())
}

/// macOS key events carry their own modifier flags, so a key the user still
/// holds cannot leak into them.
#[cfg(not(target_os = "windows"))]
fn wait_for_modifiers_released(_current: &dyn Fn() -> Result<(), String>) -> Result<(), String> {
    Ok(())
}

/// SendInput keeps the physical keyboard state: wait until the user has let
/// go of Shift, Ctrl, Alt and Windows, or Shift+Return would become
/// Ctrl+Shift+Return.
#[cfg(target_os = "windows")]
fn wait_for_modifiers_released(current: &dyn Fn() -> Result<(), String>) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    for _ in 0..50 {
        current()?;
        // SAFETY: GetAsyncKeyState takes a virtual key code and no pointers.
        let held = [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
            .iter()
            .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) } as u16 & 0x8000 != 0);
        if !held {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err("A modifier key is still held".to_string())
}

#[cfg(target_os = "macos")]
fn press(key: Key, modifier: Modifier) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    // kVK_Return, kVK_ANSI_A, kVK_ANSI_Z.
    let code: u16 = match key {
        Key::Return => 36,
        Key::A => 0,
        Key::Z => 6,
    };
    // The modifier is a flag on the key event itself, as for ⌘V in paste_mac.
    // Explicit flags also keep a hotkey modifier the user still holds out of it.
    let flags = match modifier {
        Modifier::None => CGEventFlags::empty(),
        Modifier::Shift => CGEventFlags::CGEventFlagShift,
        Modifier::Primary => CGEventFlags::CGEventFlagCommand,
    };
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create CGEventSource".to_string())?;
    let down = CGEvent::new_keyboard_event(source.clone(), code, true)
        .map_err(|_| "Failed to create key event".to_string())?;
    let up = CGEvent::new_keyboard_event(source, code, false)
        .map_err(|_| "Failed to create key event".to_string())?;
    // Both events exist before the key goes down, so a key-up always follows.
    for event in [down, up] {
        event.set_flags(flags);
        event.post(CGEventTapLocation::HID);
        thread::sleep(Duration::from_millis(3));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn press(key: Key, modifier: Modifier) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY, VK_A, VK_CONTROL, VK_RETURN, VK_SHIFT, VK_Z,
    };

    fn input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: keytrigger::INJECTED_SIGNATURE,
                },
            },
        }
    }

    let vk = match key {
        Key::Return => VK_RETURN,
        Key::A => VK_A,
        Key::Z => VK_Z,
    };
    let held = match modifier {
        Modifier::None => None,
        Modifier::Shift => Some(VK_SHIFT),
        Modifier::Primary => Some(VK_CONTROL),
    };
    // One SendInput batch, so the key is always processed with its modifier
    // held (see paste_windows).
    let mut inputs = Vec::with_capacity(4);
    inputs.extend(held.map(|held| input(held, false)));
    inputs.push(input(vk, false));
    inputs.push(input(vk, true));
    inputs.extend(held.map(|held| input(held, true)));
    let size = std::mem::size_of::<INPUT>() as i32;
    // SAFETY: the slice is valid INPUT and the size matches its type.
    let sent = unsafe { SendInput(&inputs, size) } as usize;
    if sent != inputs.len() {
        // Release whatever a partial batch left down; never repeat the key.
        let releases: Vec<INPUT> = [Some(vk), held]
            .into_iter()
            .flatten()
            .map(|key| input(key, true))
            .collect();
        // SAFETY: as above.
        unsafe { SendInput(&releases, size) };
        return Err("Key press was blocked".to_string());
    }
    thread::sleep(Duration::from_millis(3));
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn press(_key: Key, _modifier: Modifier) -> Result<(), String> {
    Err("Voice commands are not supported on this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(at: Instant) -> LastInsertion {
        LastInsertion {
            focus: Focus {
                process_id: 7,
                window_id: "1".into(),
            },
            key_downs: 40,
            at,
        }
    }

    #[test]
    fn delete_that_trusts_the_record_only_while_nothing_happened_since() {
        let at = Instant::now();
        let last = record(at);
        let same = Some(&last.focus);
        let other = Focus {
            process_id: 7,
            window_id: "2".into(),
        };
        let soon = at + Duration::from_secs(5);
        assert_eq!(check_record(Some(&last), true, 40, soon, same), Ok(()));
        assert_eq!(
            check_record(None, true, 40, soon, same),
            Err(SkipReason::NothingToDelete)
        );
        assert_eq!(
            check_record(Some(&last), true, 41, soon, same),
            Err(SkipReason::TypedSince)
        );
        assert_eq!(
            check_record(Some(&last), true, 40, soon, Some(&other)),
            Err(SkipReason::FocusChanged)
        );
        assert_eq!(
            check_record(Some(&last), true, 40, at + Duration::from_secs(61), same),
            Err(SkipReason::TooLate)
        );
        assert_eq!(
            check_record(Some(&last), false, 40, soon, same),
            Err(SkipReason::NotWatching)
        );
    }

    #[test]
    fn a_line_break_never_presses_a_bare_return_and_send_always_does() {
        assert_eq!(
            presses_for(VoiceCommand::NewLine),
            [(Key::Return, Modifier::Shift)]
        );
        assert_eq!(
            presses_for(VoiceCommand::NewParagraph),
            [(Key::Return, Modifier::Shift); 2]
        );
        assert_eq!(
            presses_for(VoiceCommand::Send),
            [(Key::Return, Modifier::None)]
        );
        assert_eq!(
            presses_for(VoiceCommand::DeleteThat),
            [(Key::Z, Modifier::Primary)]
        );
    }

    #[test]
    fn delete_that_with_no_dictation_presses_no_key() {
        forget_insertion();
        assert!(matches!(
            perform(VoiceCommand::DeleteThat, true, &|| Ok(())),
            Err(CommandError::Skipped(SkipReason::NothingToDelete))
        ));
    }
}
