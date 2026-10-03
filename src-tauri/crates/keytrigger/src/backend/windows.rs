//! Windows backend: a low-level keyboard hook (`WH_KEYBOARD_LL`) installed and
//! pumped on ONE dedicated thread (LL hook callbacks are delivered to the
//! installing thread's message loop, so install + `GetMessageW` must share a
//! thread).
//!
//! - Force the message queue with `PeekMessageW(PM_NOREMOVE)` and record the
//!   thread id before signalling ready, so `request_stop`'s `PostThreadMessageW`
//!   is never dropped.
//! - The C hook callback cannot capture Rust state, so per-thread state (the
//!   event `Sender` and the down-key set for repeat detection) lives in a
//!   `thread_local` set on the pump thread.
//! - There is NO timeout-disabled notification on Windows (unlike macOS); the
//!   tiny callback keeps us under `LowLevelHooksTimeout`. A host-side health
//!   check can restart the thread if needed.
//!
//! NOTE: cfg(windows)-only; verified by CI compilation + manual smoke (this repo
//! cannot run Windows runtime tests on macOS).

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use arc_swap::ArcSwap;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, PM_NOREMOVE,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

use super::vk::{map_modifier_vk, should_consume_keydown};
use crate::engine::{ConsumeSet, KeyEventSource, Msg, ReadySignal};
use crate::types::{KeySpec, ModSet, Modifier, NamedKey, RawKeyEvent, Side};

struct HookState {
    tx: Sender<Msg>,
    /// Currently-down virtual-key codes, for autorepeat detection (LL hooks do
    /// not flag repeats).
    down: HashSet<u32>,
    /// Lock-free consume set shared with the matcher's binding set; the hook
    /// consults it (no lock) to decide whether to swallow a key-down.
    consume: Arc<ArcSwap<ConsumeSet>>,
    /// Keys whose first key-down was swallowed; their repeats are swallowed too.
    consumed: super::ConsumedKeys,
}

thread_local! {
    static HOOK_STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

pub struct WinKeyboardHook {
    /// Pump-thread id, set once the queue exists; 0 until ready / after stop.
    thread_id: AtomicU32,
}

impl WinKeyboardHook {
    pub fn new() -> Self {
        Self {
            thread_id: AtomicU32::new(0),
        }
    }
}

impl KeyEventSource for WinKeyboardHook {
    fn run(&self, tx: Sender<Msg>, ready: ReadySignal, consume: Arc<ArcSwap<ConsumeSet>>) {
        let consume_for_hook = Arc::clone(&consume);
        HOOK_STATE.with(|s| {
            *s.borrow_mut() = Some(HookState {
                tx: tx.clone(),
                down: HashSet::new(),
                consume: consume_for_hook,
                consumed: super::ConsumedKeys::default(),
            });
        });

        // Force the thread message queue to exist before anyone posts to it.
        let mut msg = MSG::default();
        unsafe {
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        }
        self.thread_id
            .store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);

        let hook =
            match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) } {
                Ok(hook) => hook,
                Err(e) => {
                    log::error!("keytrigger: SetWindowsHookExW failed: {e}");
                    self.thread_id.store(0, Ordering::SeqCst);
                    HOOK_STATE.with(|s| *s.borrow_mut() = None);
                    ready.err(format!("SetWindowsHookExW failed: {e}"));
                    return;
                }
            };

        ready.ok();

        // Pump until WM_QUIT (posted by request_stop). The LL hook fires as a
        // side effect of message processing on this thread.
        loop {
            let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if result.0 <= 0 {
                break; // 0 = WM_QUIT, -1 = error
            }
        }

        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        }
        self.thread_id.store(0, Ordering::SeqCst);
        HOOK_STATE.with(|s| *s.borrow_mut() = None);
    }

    fn request_stop(&self) {
        let id = self.thread_id.load(Ordering::SeqCst);
        if id != 0 {
            unsafe {
                let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// Whether a hook event is one of Voicetypr's OWN synthetic keystrokes: injected
/// (`LLKHF_INJECTED`) AND carrying our [`crate::INJECTED_SIGNATURE`] in `dwExtraInfo`.
/// External tools' injected input (Stream Deck, AutoHotkey, PowerToys) sets the
/// injected flag but NOT our signature, so it returns false and is allowed through.
#[inline]
fn is_own_injection(flags: u32, extra_info: usize) -> bool {
    (flags & LLKHF_INJECTED.0) != 0 && extra_info == crate::INJECTED_SIGNATURE
}

unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // A panic unwinding out of an `extern "system"` (Win32) callback is UB
        // and aborts the whole process across the FFI boundary, so catch it and
        // fall through to the trailing CallNextHookEx. The closure returns whether
        // the key was consumed; a panic is treated as "not consumed" (pass through).
        let consumed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            // Ignore only Voicetypr's OWN synthetic keystrokes. The post-transcription
            // paste (commands::text::paste_windows) injects Ctrl+V via SendInput stamped
            // with crate::INJECTED_SIGNATURE in dwExtraInfo; forwarding it to the matcher
            // could re-trigger a ModifierHold on Ctrl (the paste modifier), spuriously
            // satisfy a DoubleTap/Chord, or wedge state if a synthetic key-up is missed. A
            // WH_KEYBOARD_LL hook cannot see the injecting pid, so we distinguish by
            // dwExtraInfo rather than a blanket LLKHF_INJECTED drop — that blanket drop
            // regressed external tools (Stream Deck, AutoHotkey, PowerToys) that fire the
            // hotkey via SendInput (which sets LLKHF_INJECTED). Their events carry a
            // different dwExtraInfo and pass through. Still chain CallNextHookEx below.
            if is_own_injection(kb.flags.0, kb.dwExtraInfo) {
                return false;
            }
            let message = wparam.0 as u32;
            let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
            let vk = kb.vkCode;
            HOOK_STATE.with(|s| {
                if let Some(state) = s.borrow_mut().as_mut() {
                    let is_repeat = if down {
                        !state.down.insert(vk)
                    } else {
                        state.down.remove(&vk);
                        false
                    };
                    let (key, side) = map_vk(vk, kb.scanCode, kb.flags.0);
                    let _ = state.tx.send(Msg::Raw(RawKeyEvent {
                        key,
                        side,
                        down,
                        is_repeat,
                    }));
                    // Forward EVERY event to the matcher for observation, but
                    // swallow (consume) only a non-repeat, non-modifier key-down
                    // whose exact (mods, key) is a registered combo/single, plus
                    // that key's auto-repeats (`ConsumedKeys`). A bare
                    // modifier VK is NEVER consumed — swallowing Control/Shift/Alt/
                    // Win would globally break shortcuts like Ctrl+C/V. macOS routes
                    // modifier changes through a FlagsChanged branch that always
                    // passes through; `should_consume_keydown` enforces the same
                    // invariant here (pure + unit-tested in `backend::vk`).
                    let consume_first = should_consume_keydown(
                        vk,
                        key,
                        side,
                        down,
                        is_repeat,
                        modset_from_down(&state.down),
                        &state.consume.load(),
                    );
                    // Repeats of a swallowed key are swallowed too.
                    state.consumed.decide(vk, down, is_repeat, consume_first)
                } else {
                    false
                }
            })
        }))
        .unwrap_or(false);
        if consumed {
            // Swallow: do not chain to the next hook, so the key never reaches
            // the focused application.
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Map a Windows virtual-key code to a [`KeySpec`] and (for side modifiers) a
/// [`Side`]. Unmapped keys fall back to [`KeySpec::Raw`].
fn map_vk(vk: u32, scan_code: u32, flags: u32) -> (KeySpec, Option<Side>) {
    // Side-specific modifiers (LL hook reports L/R distinctly).
    if let Some(mapped) = map_modifier_vk(vk, scan_code, flags) {
        return mapped;
    }
    match vk {
        0x5B => return (KeySpec::Named(NamedKey::MetaLeft), Some(Side::Left)),
        0x5C => return (KeySpec::Named(NamedKey::MetaRight), Some(Side::Right)),
        _ => {}
    }

    // Letters: VK 'A'..'Z' == ASCII 0x41..0x5A.
    if (0x41..=0x5A).contains(&vk) {
        let letter = [
            NamedKey::A,
            NamedKey::B,
            NamedKey::C,
            NamedKey::D,
            NamedKey::E,
            NamedKey::F,
            NamedKey::G,
            NamedKey::H,
            NamedKey::I,
            NamedKey::J,
            NamedKey::K,
            NamedKey::L,
            NamedKey::M,
            NamedKey::N,
            NamedKey::O,
            NamedKey::P,
            NamedKey::Q,
            NamedKey::R,
            NamedKey::S,
            NamedKey::T,
            NamedKey::U,
            NamedKey::V,
            NamedKey::W,
            NamedKey::X,
            NamedKey::Y,
            NamedKey::Z,
        ][(vk - 0x41) as usize];
        return (KeySpec::Named(letter), None);
    }
    // Top-row digits: VK '0'..'9' == 0x30..0x39.
    if (0x30..=0x39).contains(&vk) {
        let digit = [
            NamedKey::Digit0,
            NamedKey::Digit1,
            NamedKey::Digit2,
            NamedKey::Digit3,
            NamedKey::Digit4,
            NamedKey::Digit5,
            NamedKey::Digit6,
            NamedKey::Digit7,
            NamedKey::Digit8,
            NamedKey::Digit9,
        ][(vk - 0x30) as usize];
        return (KeySpec::Named(digit), None);
    }
    // Function keys: VK_F1 (0x70) .. VK_F24 (0x87).
    if (0x70..=0x87).contains(&vk) {
        let f = [
            NamedKey::F1,
            NamedKey::F2,
            NamedKey::F3,
            NamedKey::F4,
            NamedKey::F5,
            NamedKey::F6,
            NamedKey::F7,
            NamedKey::F8,
            NamedKey::F9,
            NamedKey::F10,
            NamedKey::F11,
            NamedKey::F12,
            NamedKey::F13,
            NamedKey::F14,
            NamedKey::F15,
            NamedKey::F16,
            NamedKey::F17,
            NamedKey::F18,
            NamedKey::F19,
            NamedKey::F20,
            NamedKey::F21,
            NamedKey::F22,
            NamedKey::F23,
            NamedKey::F24,
        ][(vk - 0x70) as usize];
        return (KeySpec::Named(f), None);
    }
    // Numpad: VK_NUMPAD0 (0x60) .. VK_NUMPAD9 (0x69).
    if (0x60..=0x69).contains(&vk) {
        let n = [
            NamedKey::Numpad0,
            NamedKey::Numpad1,
            NamedKey::Numpad2,
            NamedKey::Numpad3,
            NamedKey::Numpad4,
            NamedKey::Numpad5,
            NamedKey::Numpad6,
            NamedKey::Numpad7,
            NamedKey::Numpad8,
            NamedKey::Numpad9,
        ][(vk - 0x60) as usize];
        return (KeySpec::Named(n), None);
    }

    let named = match vk {
        0x20 => Some(NamedKey::Space),
        0x0D => Some(NamedKey::Enter),
        0x1B => Some(NamedKey::Escape),
        0x09 => Some(NamedKey::Tab),
        0x08 => Some(NamedKey::Backspace),
        0x2E => Some(NamedKey::Delete),
        0x2D => Some(NamedKey::Insert),
        0x24 => Some(NamedKey::Home),
        0x23 => Some(NamedKey::End),
        0x21 => Some(NamedKey::PageUp),
        0x22 => Some(NamedKey::PageDown),
        0x25 => Some(NamedKey::ArrowLeft),
        0x26 => Some(NamedKey::ArrowUp),
        0x27 => Some(NamedKey::ArrowRight),
        0x28 => Some(NamedKey::ArrowDown),
        0x14 => Some(NamedKey::CapsLock),
        0x6A => Some(NamedKey::NumpadMultiply),
        0x6B => Some(NamedKey::NumpadAdd),
        0x6D => Some(NamedKey::NumpadSubtract),
        0x6E => Some(NamedKey::NumpadDecimal),
        0x6F => Some(NamedKey::NumpadDivide),
        0x92 => Some(NamedKey::NumpadEqual), // VK_OEM_NEC_EQUAL ('=' on numpad)
        // Punctuation / OEM keys (US ANSI layout).
        0xBA => Some(NamedKey::Semicolon),
        0xBB => Some(NamedKey::Equal),
        0xBC => Some(NamedKey::Comma),
        0xBD => Some(NamedKey::Minus),
        0xBE => Some(NamedKey::Period),
        0xBF => Some(NamedKey::Slash),
        0xC0 => Some(NamedKey::Backquote),
        0xDB => Some(NamedKey::BracketLeft),
        0xDC => Some(NamedKey::Backslash),
        0xDD => Some(NamedKey::BracketRight),
        0xDE => Some(NamedKey::Quote),
        _ => None,
    };

    match named {
        Some(n) => (KeySpec::Named(n), None),
        None => (KeySpec::Raw(vk), None),
    }
}

/// Derive the side-agnostic [`ModSet`] currently held from the set of down
/// virtual-key codes (the LL hook reports side-specific modifier vks).
fn modset_from_down(down: &HashSet<u32>) -> ModSet {
    let mut m = ModSet::empty();
    if down.contains(&0x10) || down.contains(&0xA0) || down.contains(&0xA1) {
        m.insert(Modifier::Shift);
    }
    if down.contains(&0x11) || down.contains(&0xA2) || down.contains(&0xA3) {
        m.insert(Modifier::Control);
    }
    if down.contains(&0x12) || down.contains(&0xA4) || down.contains(&0xA5) {
        m.insert(Modifier::Alt);
    }
    if down.contains(&0x5B) || down.contains(&0x5C) {
        m.insert(Modifier::Meta);
    }
    m
}

#[cfg(test)]
mod injection_filter_tests {
    use super::*;

    #[test]
    fn own_injection_is_ignored() {
        // Injected flag + our signature => our own paste => ignored.
        assert!(is_own_injection(
            LLKHF_INJECTED.0,
            crate::INJECTED_SIGNATURE
        ));
    }

    #[test]
    fn external_injection_passes_through() {
        // Injected flag but NOT our signature (Stream Deck / AutoHotkey) => allowed.
        assert!(!is_own_injection(LLKHF_INJECTED.0, 0));
        assert!(!is_own_injection(LLKHF_INJECTED.0, 0xDEAD_BEEF));
    }

    #[test]
    fn physical_key_passes_through() {
        // No injected flag => physical key => allowed regardless of extra_info.
        assert!(!is_own_injection(0, 0));
        assert!(!is_own_injection(0, crate::INJECTED_SIGNATURE));
    }
}
