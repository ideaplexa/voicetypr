//! Platform backends. Each implements [`crate::engine::KeyEventSource`] by
//! normalizing raw OS key events into [`crate::types::RawKeyEvent`]s.
//!
//! P1 wave A ships STUBS only; the real macOS `CGEventTap` and Windows
//! `WH_KEYBOARD_LL` loops land in wave B. Tests drive the matcher/engine via
//! `MockSource`, so the stubs only need to compile.

#[cfg(target_os = "macos")]
mod macos;
/// Pure, platform-neutral Windows virtual-key + consume-decision helpers.
/// Compiled on Windows (the `cfg(windows)` hook, its sole non-test caller) and
/// under `cfg(test)` on every target, so the consume logic unit-tests on macOS
/// even though the hook itself cannot compile there. See `vk` for why a bare
/// modifier VK is never consumed.
#[cfg(any(target_os = "windows", test))]
mod vk;
#[cfg(target_os = "windows")]
mod windows;

use crate::engine::KeyEventSource;

/// Keys whose first key-down the macOS tap swallowed. Their auto-repeats are
/// swallowed too: the app never saw the key-down, and a passed-through repeat
/// would type into it (holding ⌥ Space inserts non-breaking spaces). macOS
/// flags repeats natively; the Windows LL hook infers them from its down-set,
/// which a missed key-up (secure desktop) would turn into a swallowed press.
#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
pub(crate) struct ConsumedKeys(std::collections::HashSet<u32>);

#[cfg(any(target_os = "macos", test))]
impl ConsumedKeys {
    /// Whether to swallow this key event. `consume_first` is the backend's
    /// decision for a non-repeat key-down; key-ups always pass through.
    pub(crate) fn decide(
        &mut self,
        code: u32,
        down: bool,
        is_repeat: bool,
        consume_first: bool,
    ) -> bool {
        if !down {
            self.0.remove(&code);
            false
        } else if is_repeat {
            self.0.contains(&code)
        } else if consume_first {
            self.0.insert(code);
            true
        } else {
            self.0.remove(&code);
            false
        }
    }
}

/// Construct the event source for the current platform.
pub fn platform_source() -> Box<dyn KeyEventSource> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacEventTap::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WinKeyboardHook::new())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Box::new(crate::engine::MockSource::new(Vec::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::ConsumedKeys;

    #[test]
    fn repeats_follow_the_first_key_down() {
        let mut keys = ConsumedKeys::default();
        // Swallowed first press: its repeats are swallowed, the key-up passes.
        assert!(keys.decide(49, true, false, true));
        assert!(keys.decide(49, true, true, false));
        assert!(!keys.decide(49, false, false, false));
        // After the key-up a stray repeat is not swallowed.
        assert!(!keys.decide(49, true, true, false));
        // A first press that passed through keeps its repeats passing through.
        assert!(!keys.decide(49, true, false, false));
        assert!(!keys.decide(49, true, true, false));
    }
}
