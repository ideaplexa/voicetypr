//! macOS backend: an active (intercepting) `CGEventTap` on a dedicated CFRunLoop thread.
//!
//! - Default (intercepting) tap: the callback returns `None` to CONSUME a key
//!   when the consume predicate matches; otherwise it returns the event to pass through.
//! - `FlagsChanged` carries the changed modifier's keycode plus the post-change
//!   aggregate flag mask. The keycode gives the side; the modifier's type bit in
//!   the event flags gives authoritative down/up (`CGEventSourceKeyState` lags
//!   the in-flight event inside a tap, inverting tap order).
//! - Re-enable on `TapDisabledByTimeout/ByUserInput` directly in the callback via
//!   a captured `CFMachPortRef` cell (the callback runs on the run-loop thread),
//!   and signal the dispatcher to reset (events were missed).
//! - `request_stop` calls `CFRunLoopStop` on the retained (Send) `CFRunLoop`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use arc_swap::ArcSwap;

use core_foundation::base::TCFType;
use core_foundation::mach_port::CFMachPortRef;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CGKeyCode, EventField, KeyCode,
};
use parking_lot::Mutex;

use crate::engine::{ConsumeSet, Control, KeyEventSource, Msg, ReadySignal};
use crate::types::{KeySpec, ModSet, Modifier, NamedKey, RawKeyEvent, Side};

extern "C" {
    /// Enable/disable an event tap. Declared here because `core-graphics` keeps
    /// it private; we need it to re-enable from inside the tap callback.
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

pub struct MacEventTap {
    /// Retained so `request_stop` can stop the loop from another thread
    /// (`CFRunLoop` is `Send`/`Sync` in the core-foundation bindings).
    runloop: Mutex<Option<CFRunLoop>>,
}

impl MacEventTap {
    pub fn new() -> Self {
        Self {
            runloop: Mutex::new(None),
        }
    }
}

impl KeyEventSource for MacEventTap {
    fn run(&self, tx: Sender<Msg>, ready: ReadySignal, consume: Arc<ArcSwap<ConsumeSet>>) {
        // Shared with the callback (same thread, populated after creation): the
        // tap's mach port, for re-enabling the tap from within its own callback.
        let port_cell: Rc<RefCell<Option<CFMachPortRef>>> = Rc::new(RefCell::new(None));
        let port_cb = Rc::clone(&port_cell);
        let tx_cb = tx.clone();
        let own_pid = std::process::id() as i64;
        let consume_cb = Arc::clone(&consume);
        let consumed_keys = RefCell::new(super::ConsumedKeys::default());

        let tap = CGEventTap::new(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
            ],
            move |_proxy, etype, event| {
                // Ignore our OWN synthetic keystrokes (e.g. the post-transcription
                // paste): feeding them to the matcher could re-trigger a ModifierHold
                // hotkey (the paste presses the same modifier), spuriously satisfy a
                // DoubleTap/Chord bound to that modifier, or wedge state if a
                // synthetic key-up is dropped/coalesced. PID-scoped, so real user
                // input (source pid 0) and other apps pass through untouched.
                //
                // Gated on key event types: TapDisabled callbacks are out-of-band
                // and have no key behind them, so we must not read event fields for
                // them. LIMITATION: this only catches rdev-posted CGEvents; the
                // text.rs osascript/AppleScript paste fallback runs in a different
                // process and is NOT covered here.
                let is_key_event = matches!(
                    etype,
                    CGEventType::KeyDown | CGEventType::KeyUp | CGEventType::FlagsChanged
                );
                if is_key_event
                    && event.get_integer_value_field(EventField::EVENT_SOURCE_UNIX_PROCESS_ID)
                        == own_pid
                {
                    // Pass-through: deliver our own paste to the app; we only
                    // skip feeding the matcher (above). Do NOT consume it.
                    return Some(event.clone());
                }
                match etype {
                    CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                        if let Some(port) = *port_cb.borrow() {
                            unsafe { CGEventTapEnable(port, true) };
                        }
                        // Events were dropped while disabled: reset matcher state.
                        let _ = tx_cb.send(Msg::Control(Control::ReEnable));
                        // Out-of-band lifecycle callback — no key event is behind
                        // it, so the return value is a harmless no-op; returning
                        // None is the historic choice.
                        return None;
                    }
                    CGEventType::KeyDown | CGEventType::KeyUp => {
                        let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE)
                            as CGKeyCode;
                        let is_repeat = event
                            .get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT)
                            != 0;
                        let (key, side) = map_keycode(code);
                        let down = matches!(etype, CGEventType::KeyDown);
                        // Always feed the matcher: it fires/releases triggers on
                        // the event whether or not we also consume it for the app.
                        let _ = tx_cb.send(Msg::Raw(RawKeyEvent {
                            key,
                            side,
                            down,
                            is_repeat,
                        }));
                        // Consume a non-repeat key-down (never a modifier, never
                        // a key-up) whose exact (mods, key) is a registered
                        // combo/single, plus that key's auto-repeats, so neither
                        // reaches the focused application. Key-ups, other
                        // repeats and unbound keys fall through to pass-through.
                        let consume_first = down
                            && !is_repeat
                            && side.is_none()
                            && consume_cb
                                .load()
                                .consumes(key, flags_to_modset(event.get_flags()));
                        if consumed_keys.borrow_mut().decide(
                            u32::from(code),
                            down,
                            is_repeat,
                            consume_first,
                        ) {
                            // Consumed: do NOT forward the event to apps.
                            return None;
                        }
                    }
                    CGEventType::FlagsChanged => {
                        let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE)
                            as CGKeyCode;
                        let (key, side) = map_keycode(code);
                        // Down/up from the event's own post-change flag mask: the
                        // modifier's type bit is set iff it is now physically
                        // down. CGEventSourceKeyState lags the in-flight event in
                        // a tap callback (reports the pre-event state), which
                        // inverts tap order and stops IsolatedTap from ever
                        // firing. The keycode still supplies the side.
                        let down = modifier_flag(code)
                            .map(|mask| event.get_flags().contains(mask))
                            .unwrap_or(false);
                        if side.is_some() {
                            let _ = tx_cb.send(Msg::Raw(RawKeyEvent {
                                key,
                                side,
                                down,
                                is_repeat: false,
                            }));
                        }
                        // Modifiers are NEVER consumed: pass through unchanged.
                    }
                    _ => {}
                }
                // Default pass-through for every non-consumed event (key-ups,
                // modifiers, auto-repeats, and unbound keys).
                Some(event.clone())
            },
        );

        let tap = match tap {
            Ok(tap) => tap,
            Err(()) => {
                log::error!(
                    "keytrigger: CGEventTapCreate failed (Accessibility permission not granted?)"
                );
                ready.err("CGEventTapCreate failed (Accessibility permission not granted)");
                return;
            }
        };

        *port_cell.borrow_mut() = Some(tap.mach_port.as_concrete_TypeRef());

        let source = match tap.mach_port.create_runloop_source(0) {
            Ok(source) => source,
            Err(()) => {
                log::error!("keytrigger: failed to create CFRunLoop source for event tap");
                ready.err("failed to create CFRunLoop source for event tap");
                return;
            }
        };

        let current = CFRunLoop::get_current();
        let mode = unsafe { kCFRunLoopCommonModes };
        current.add_source(&source, mode);
        tap.enable();

        // Ordering invariant (mirrors windows.rs): the run loop handle MUST be
        // published to `self.runloop` BEFORE `ready.ok()` signals readiness, so
        // `request_stop` (which reads `self.runloop` and calls `rl.stop()`) is
        // never a no-op once readiness has been signaled. An early
        // `CFRunLoopStop` posted between here and `run_current()` is safe: it is
        // queued on the run loop and processed at startup, causing
        // `run_current()` to return immediately rather than block.
        *self.runloop.lock() = Some(current);
        ready.ok();

        // Blocks until CFRunLoopStop (from request_stop).
        CFRunLoop::run_current();

        *self.runloop.lock() = None;
    }

    fn request_stop(&self) {
        if let Some(rl) = self.runloop.lock().as_ref() {
            rl.stop();
        }
    }
}

/// Map a macOS virtual keycode to a [`KeySpec`] and (for side modifiers) a
/// [`Side`]. Unmapped keys fall back to [`KeySpec::Raw`].
fn map_keycode(code: CGKeyCode) -> (KeySpec, Option<Side>) {
    // Side-specific modifiers (the critical path for ModifierHold).
    match code {
        KeyCode::COMMAND => return (KeySpec::Named(NamedKey::MetaLeft), Some(Side::Left)),
        KeyCode::RIGHT_COMMAND => return (KeySpec::Named(NamedKey::MetaRight), Some(Side::Right)),
        KeyCode::SHIFT => return (KeySpec::Named(NamedKey::ShiftLeft), Some(Side::Left)),
        KeyCode::RIGHT_SHIFT => return (KeySpec::Named(NamedKey::ShiftRight), Some(Side::Right)),
        KeyCode::OPTION => return (KeySpec::Named(NamedKey::AltLeft), Some(Side::Left)),
        KeyCode::RIGHT_OPTION => return (KeySpec::Named(NamedKey::AltRight), Some(Side::Right)),
        KeyCode::CONTROL => return (KeySpec::Named(NamedKey::ControlLeft), Some(Side::Left)),
        KeyCode::RIGHT_CONTROL => {
            return (KeySpec::Named(NamedKey::ControlRight), Some(Side::Right))
        }
        _ => {}
    }

    let named = match code {
        KeyCode::CAPS_LOCK => Some(NamedKey::CapsLock),
        KeyCode::FUNCTION => Some(NamedKey::Fn),
        KeyCode::SPACE => Some(NamedKey::Space),
        KeyCode::RETURN => Some(NamedKey::Enter),
        KeyCode::TAB => Some(NamedKey::Tab),
        KeyCode::ESCAPE => Some(NamedKey::Escape),
        KeyCode::DELETE => Some(NamedKey::Backspace),
        KeyCode::FORWARD_DELETE => Some(NamedKey::Delete),
        KeyCode::HOME => Some(NamedKey::Home),
        KeyCode::END => Some(NamedKey::End),
        KeyCode::PAGE_UP => Some(NamedKey::PageUp),
        KeyCode::PAGE_DOWN => Some(NamedKey::PageDown),
        KeyCode::LEFT_ARROW => Some(NamedKey::ArrowLeft),
        KeyCode::RIGHT_ARROW => Some(NamedKey::ArrowRight),
        KeyCode::DOWN_ARROW => Some(NamedKey::ArrowDown),
        KeyCode::UP_ARROW => Some(NamedKey::ArrowUp),
        KeyCode::F1 => Some(NamedKey::F1),
        KeyCode::F2 => Some(NamedKey::F2),
        KeyCode::F3 => Some(NamedKey::F3),
        KeyCode::F4 => Some(NamedKey::F4),
        KeyCode::F5 => Some(NamedKey::F5),
        KeyCode::F6 => Some(NamedKey::F6),
        KeyCode::F7 => Some(NamedKey::F7),
        KeyCode::F8 => Some(NamedKey::F8),
        KeyCode::F9 => Some(NamedKey::F9),
        KeyCode::F10 => Some(NamedKey::F10),
        KeyCode::F11 => Some(NamedKey::F11),
        KeyCode::F12 => Some(NamedKey::F12),
        KeyCode::F13 => Some(NamedKey::F13),
        KeyCode::F14 => Some(NamedKey::F14),
        KeyCode::F15 => Some(NamedKey::F15),
        KeyCode::F16 => Some(NamedKey::F16),
        KeyCode::F17 => Some(NamedKey::F17),
        KeyCode::F18 => Some(NamedKey::F18),
        KeyCode::F19 => Some(NamedKey::F19),
        KeyCode::F20 => Some(NamedKey::F20),
        // Letters (kVK_ANSI_*).
        0x00 => Some(NamedKey::A),
        0x0B => Some(NamedKey::B),
        0x08 => Some(NamedKey::C),
        0x02 => Some(NamedKey::D),
        0x0E => Some(NamedKey::E),
        0x03 => Some(NamedKey::F),
        0x05 => Some(NamedKey::G),
        0x04 => Some(NamedKey::H),
        0x22 => Some(NamedKey::I),
        0x26 => Some(NamedKey::J),
        0x28 => Some(NamedKey::K),
        0x25 => Some(NamedKey::L),
        0x2E => Some(NamedKey::M),
        0x2D => Some(NamedKey::N),
        0x1F => Some(NamedKey::O),
        0x23 => Some(NamedKey::P),
        0x0C => Some(NamedKey::Q),
        0x0F => Some(NamedKey::R),
        0x01 => Some(NamedKey::S),
        0x11 => Some(NamedKey::T),
        0x20 => Some(NamedKey::U),
        0x09 => Some(NamedKey::V),
        0x0D => Some(NamedKey::W),
        0x07 => Some(NamedKey::X),
        0x10 => Some(NamedKey::Y),
        0x06 => Some(NamedKey::Z),
        // Digit row.
        0x1D => Some(NamedKey::Digit0),
        0x12 => Some(NamedKey::Digit1),
        0x13 => Some(NamedKey::Digit2),
        0x14 => Some(NamedKey::Digit3),
        0x15 => Some(NamedKey::Digit4),
        0x17 => Some(NamedKey::Digit5),
        0x16 => Some(NamedKey::Digit6),
        0x1A => Some(NamedKey::Digit7),
        0x1C => Some(NamedKey::Digit8),
        0x19 => Some(NamedKey::Digit9),
        // Numpad.
        0x52 => Some(NamedKey::Numpad0),
        0x53 => Some(NamedKey::Numpad1),
        0x54 => Some(NamedKey::Numpad2),
        0x55 => Some(NamedKey::Numpad3),
        0x56 => Some(NamedKey::Numpad4),
        0x57 => Some(NamedKey::Numpad5),
        0x58 => Some(NamedKey::Numpad6),
        0x59 => Some(NamedKey::Numpad7),
        0x5B => Some(NamedKey::Numpad8),
        0x5C => Some(NamedKey::Numpad9),
        0x4C => Some(NamedKey::NumpadEnter),
        0x45 => Some(NamedKey::NumpadAdd),
        0x4E => Some(NamedKey::NumpadSubtract),
        0x43 => Some(NamedKey::NumpadMultiply),
        0x4B => Some(NamedKey::NumpadDivide),
        0x41 => Some(NamedKey::NumpadDecimal),
        0x51 => Some(NamedKey::NumpadEqual),
        // Punctuation / OEM keys (US ANSI virtual keycodes).
        0x32 => Some(NamedKey::Backquote),
        0x1B => Some(NamedKey::Minus),
        0x18 => Some(NamedKey::Equal),
        0x21 => Some(NamedKey::BracketLeft),
        0x1E => Some(NamedKey::BracketRight),
        0x2A => Some(NamedKey::Backslash),
        0x2B => Some(NamedKey::Comma),
        0x2C => Some(NamedKey::Slash),
        0x2F => Some(NamedKey::Period),
        0x29 => Some(NamedKey::Semicolon),
        0x27 => Some(NamedKey::Quote),
        _ => None,
    };

    match named {
        Some(n) => (KeySpec::Named(n), None),
        None => (KeySpec::Raw(code as u32), None),
    }
}

/// Device-independent modifier-type flag for a side-specific modifier keycode.
/// FlagsChanged events carry the post-change flag mask, so testing this bit
/// yields authoritative down/up for the modifier that changed (the keycode still
/// supplies the side). Returns `None` for non-modifier and toggle keys, which
/// the FlagsChanged forwarder then drops.
fn modifier_flag(code: CGKeyCode) -> Option<CGEventFlags> {
    match code {
        KeyCode::COMMAND | KeyCode::RIGHT_COMMAND => Some(CGEventFlags::CGEventFlagCommand),
        KeyCode::SHIFT | KeyCode::RIGHT_SHIFT => Some(CGEventFlags::CGEventFlagShift),
        KeyCode::OPTION | KeyCode::RIGHT_OPTION => Some(CGEventFlags::CGEventFlagAlternate),
        KeyCode::CONTROL | KeyCode::RIGHT_CONTROL => Some(CGEventFlags::CGEventFlagControl),
        _ => None,
    }
}

/// Translate a `CGEvent`'s aggregate modifier flags into a side-agnostic
/// [`ModSet`] for the consume predicate. Only the four logical modifiers map;
/// Caps Lock / Fn and other flags are ignored (they never count toward a combo).
fn flags_to_modset(flags: CGEventFlags) -> ModSet {
    let mut s = ModSet::empty();
    if flags.contains(CGEventFlags::CGEventFlagCommand) {
        s.insert(Modifier::Meta);
    }
    if flags.contains(CGEventFlags::CGEventFlagControl) {
        s.insert(Modifier::Control);
    }
    if flags.contains(CGEventFlags::CGEventFlagAlternate) {
        s.insert(Modifier::Alt);
    }
    if flags.contains(CGEventFlags::CGEventFlagShift) {
        s.insert(Modifier::Shift);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::map_keycode;
    use crate::types::Side;
    use core_graphics::event::KeyCode;

    #[test]
    fn side_specific_modifiers_carry_a_side() {
        // These are the only FlagsChanged keys the matcher should ever see.
        assert_eq!(map_keycode(KeyCode::CONTROL).1, Some(Side::Left));
        assert_eq!(map_keycode(KeyCode::RIGHT_CONTROL).1, Some(Side::Right));
        assert_eq!(map_keycode(KeyCode::COMMAND).1, Some(Side::Left));
        assert_eq!(map_keycode(KeyCode::RIGHT_OPTION).1, Some(Side::Right));
        assert_eq!(map_keycode(KeyCode::SHIFT).1, Some(Side::Left));
    }

    #[test]
    fn toggle_and_phantom_modifiers_have_no_side() {
        // Caps Lock and fn must report no side so the FlagsChanged forwarder
        // drops them: Caps Lock's sticky "on" state would otherwise live in the
        // matcher's keys_down forever and permanently block IsolatedTap.
        assert_eq!(map_keycode(KeyCode::CAPS_LOCK).1, None);
        assert_eq!(map_keycode(KeyCode::FUNCTION).1, None);
    }

    #[test]
    fn modifier_flag_maps_side_modifiers_and_drops_toggles() {
        use super::modifier_flag;
        use core_graphics::event::CGEventFlags;
        assert_eq!(
            modifier_flag(KeyCode::CONTROL),
            Some(CGEventFlags::CGEventFlagControl)
        );
        assert_eq!(
            modifier_flag(KeyCode::RIGHT_COMMAND),
            Some(CGEventFlags::CGEventFlagCommand)
        );
        assert_eq!(
            modifier_flag(KeyCode::OPTION),
            Some(CGEventFlags::CGEventFlagAlternate)
        );
        assert_eq!(
            modifier_flag(KeyCode::RIGHT_SHIFT),
            Some(CGEventFlags::CGEventFlagShift)
        );
        assert_eq!(modifier_flag(KeyCode::CAPS_LOCK), None);
        assert_eq!(modifier_flag(KeyCode::FUNCTION), None);
    }
}
