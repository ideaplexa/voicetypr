//! The key presses behind voice commands, and where the last dictation landed
//! so "delete that" removes exactly it. Only counts, a process id and an
//! accessibility element are kept, in memory; no text is stored or logged.
//!
//! "Delete that" runs only where the dictation can be proven to sit right
//! before the caret: macOS text fields that report their caret through
//! accessibility. Elsewhere (Windows today, some web and Electron fields) it
//! presses nothing.

use crate::writing::VoiceCommand;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Key {
    Return,
    Backspace,
    Left,
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

/// Longer dictations are not selected key by key; "delete that" refuses them.
const MAX_DELETE_STEPS: usize = 1000;

/// How often a long deletion rechecks that the same app is still in front.
const FOCUS_CHECK_EVERY: usize = 32;

/// Where the next paste will land, read just before it.
pub(crate) struct Target {
    process_id: Option<u64>,
    #[cfg(target_os = "macos")]
    focused: Option<super::leading_space::FocusedText>,
}

struct LastInsertion {
    process_id: u64,
    /// Caret steps (grapheme clusters) the dictation spans.
    steps: usize,
    /// The field it went into, where the paste left the caret, and its
    /// length, all in that field's UTF-16 units.
    #[cfg(target_os = "macos")]
    landing: Option<(super::leading_space::FocusedText, isize, isize)>,
}

static LAST_INSERTION: Mutex<Option<LastInsertion>> = Mutex::new(None);

fn frontmost_process_id() -> Option<u64> {
    active_win_pos_rs::get_active_window()
        .ok()
        .map(|window| window.process_id)
}

pub(crate) fn before_paste() -> Target {
    Target {
        process_id: frontmost_process_id(),
        #[cfg(target_os = "macos")]
        focused: super::leading_space::focused_text(),
    }
}

/// Remember the dictation just pasted at `target`.
pub(crate) fn remember_insertion(target: Target, text: &str) {
    let steps = text.replace("\r\n", "\n").graphemes(true).count();
    let insertion = target.process_id.map(|process_id| LastInsertion {
        process_id,
        steps,
        #[cfg(target_os = "macos")]
        landing: target.focused.map(|focused| {
            let length = text.encode_utf16().count() as isize;
            let caret = focused.location + length;
            (focused, caret, length)
        }),
    });
    *LAST_INSERTION.lock().unwrap_or_else(|e| e.into_inner()) = insertion;
}

pub(crate) fn forget_insertion() {
    *LAST_INSERTION.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Caret steps to select back over the last dictation, only when it is
/// provably still right before the caret in the same place.
fn deletable_steps(last: &LastInsertion) -> Option<usize> {
    if last.steps == 0
        || last.steps > MAX_DELETE_STEPS
        || frontmost_process_id() != Some(last.process_id)
    {
        return None;
    }
    // The same focused element, the caret where the paste left it, nothing
    // selected. A field that hides its caret cannot be verified.
    #[cfg(target_os = "macos")]
    {
        let (field, caret, _) = last.landing.as_ref()?;
        let now = super::leading_space::focused_text()?;
        (now.same_element(field) && now.location == *caret && now.length == 0).then_some(last.steps)
    }
    #[cfg(not(target_os = "macos"))]
    None
}

/// Right before Backspace: exactly the dictation is selected in its field.
#[cfg(target_os = "macos")]
fn exactly_dictation_selected(last: &LastInsertion) -> bool {
    let Some((field, caret, length)) = last.landing.as_ref() else {
        return false;
    };
    // The app applies the last Shift+Left on its own time; give it a moment.
    (0..10).any(|attempt| {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(30));
        }
        super::leading_space::focused_text().is_some_and(|now| {
            now.same_element(field) && now.location == caret - length && now.length == *length
        })
    })
}

#[cfg(not(target_os = "macos"))]
fn exactly_dictation_selected(_last: &LastInsertion) -> bool {
    false
}

fn presses_for(command: VoiceCommand, delete_steps: usize) -> Vec<Press> {
    match command {
        VoiceCommand::NewLine => vec![LINE_BREAK],
        VoiceCommand::NewParagraph => vec![LINE_BREAK, LINE_BREAK],
        VoiceCommand::UndoThat => vec![(Key::Z, Modifier::Primary)],
        VoiceCommand::SelectAll => vec![(Key::A, Modifier::Primary)],
        VoiceCommand::Send => vec![(Key::Return, Modifier::None)],
        // Select the last dictation backwards, then delete the selection, so
        // the app sees one deletion it can undo.
        VoiceCommand::DeleteThat => {
            let mut presses = vec![(Key::Left, Modifier::Shift); delete_steps];
            presses.push((Key::Backspace, Modifier::None));
            presses
        }
    }
}

/// Press the keys for a command in the frontmost app. `current` is checked
/// before every key, so a cancel or Secure Input stops a long deletion.
pub(crate) fn perform(
    command: VoiceCommand,
    current: &dyn Fn() -> Result<(), String>,
) -> Result<(), String> {
    // Every command moves the caret or the text, so the remembered dictation
    // is only good for the command that follows it directly.
    let last = LAST_INSERTION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    let delete_steps = if command == VoiceCommand::DeleteThat {
        last.as_ref()
            .and_then(deletable_steps)
            .ok_or_else(|| "No dictation right before the caret to delete".to_string())?
    } else {
        0
    };
    let presses = presses_for(command, delete_steps);
    let last_index = presses.len() - 1;
    for (index, (key, modifier)) in presses.into_iter().enumerate() {
        current()?;
        wait_for_modifiers_released(current)?;
        if command == VoiceCommand::DeleteThat {
            let last = last.as_ref().expect("checked by deletable_steps");
            if index > 0
                && index % FOCUS_CHECK_EVERY == 0
                && frontmost_process_id() != Some(last.process_id)
            {
                return Err("The app in front changed".to_string());
            }
            // The selection stays visible if this fails; nothing is deleted.
            if index == last_index && !exactly_dictation_selected(last) {
                return Err("The selection is not the last dictation".to_string());
            }
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
/// go of Shift, Ctrl, Alt and Windows, or Ctrl+Shift+Left would select words.
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

    // kVK_Return, kVK_Delete, kVK_LeftArrow, kVK_ANSI_A, kVK_ANSI_Z.
    let code: u16 = match key {
        Key::Return => 36,
        Key::Backspace => 51,
        Key::Left => 123,
        Key::A => 0,
        Key::Z => 6,
    };
    // The modifier is a flag on the key event itself, as for ⌘V in paste_mac.
    // Explicit flags also keep a hotkey modifier the user still holds out of it.
    let mut flags = match modifier {
        Modifier::None => CGEventFlags::empty(),
        Modifier::Shift => CGEventFlags::CGEventFlagShift,
        Modifier::Primary => CGEventFlags::CGEventFlagCommand,
    };
    if key == Key::Left {
        // Real arrow keys carry these; some apps ignore an arrow without them.
        flags |= CGEventFlags::CGEventFlagNumericPad | CGEventFlags::CGEventFlagSecondaryFn;
    }
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
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_A, VK_BACK, VK_CONTROL, VK_LEFT,
        VK_RETURN, VK_SHIFT, VK_Z,
    };

    fn input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        let mut flags = KEYBD_EVENT_FLAGS(0);
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        // Without the extended flag, Shift+Left is read as the numeric keypad's
        // 4 and Windows drops the Shift.
        if vk == VK_LEFT {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: keytrigger::INJECTED_SIGNATURE,
                },
            },
        }
    }

    let vk = match key {
        Key::Return => VK_RETURN,
        Key::Backspace => VK_BACK,
        Key::Left => VK_LEFT,
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

    #[test]
    fn a_line_break_never_presses_a_bare_return_and_send_always_does() {
        assert_eq!(
            presses_for(VoiceCommand::NewLine, 0),
            [(Key::Return, Modifier::Shift)]
        );
        assert_eq!(
            presses_for(VoiceCommand::NewParagraph, 9),
            [(Key::Return, Modifier::Shift); 2]
        );
        assert_eq!(
            presses_for(VoiceCommand::Send, 0),
            [(Key::Return, Modifier::None)]
        );
    }

    #[test]
    fn delete_that_selects_exactly_the_last_dictation_then_deletes_it() {
        let presses = presses_for(VoiceCommand::DeleteThat, 3);
        assert_eq!(presses[..3], [(Key::Left, Modifier::Shift); 3]);
        assert_eq!(presses[3..], [(Key::Backspace, Modifier::None)]);
    }

    #[test]
    fn delete_that_with_no_dictation_to_remove_presses_no_key() {
        forget_insertion();
        assert!(perform(VoiceCommand::DeleteThat, &|| Ok(())).is_err());
    }
}
