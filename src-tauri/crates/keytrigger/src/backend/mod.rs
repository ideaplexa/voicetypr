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
