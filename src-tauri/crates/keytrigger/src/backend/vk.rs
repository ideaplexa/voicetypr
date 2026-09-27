//! Pure, platform-neutral helpers for the Windows `WH_KEYBOARD_LL` consume
//! decision.
//!
//! The Windows backend funnels EVERY key — modifiers included — through one
//! low-level hook, so the "swallow this key?" decision must explicitly avoid
//! consuming bare modifiers. macOS needs no such guard: modifier transitions
//! arrive as `FlagsChanged` events on a branch that ALWAYS passes through
//! (`backend/macos.rs`). To keep that invariant unit-testable on macOS (the
//! `cfg(windows)` hook itself cannot compile here), the consume decision and
//! the raw-VK modifier predicate live in this ungated module, and the
//! `cfg(windows)` hook is a thin caller.

use crate::engine::ConsumeSet;
use crate::types::{KeySpec, ModSet, NamedKey, Side};

const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_MENU: u32 = 0x12;
const VK_LSHIFT: u32 = 0xA0;
const VK_RSHIFT: u32 = 0xA1;
const VK_LCONTROL: u32 = 0xA2;
const VK_RCONTROL: u32 = 0xA3;
const VK_LMENU: u32 = 0xA4;
const VK_RMENU: u32 = 0xA5;
const LLKHF_EXTENDED: u32 = 0x01;
const RIGHT_SHIFT_SCAN_CODE: u32 = 0x36;

/// Whether a Windows virtual-key code is a modifier: the side-agnostic base
/// codes (`VK_SHIFT`/`VK_CONTROL`/`VK_MENU`) plus the left/right and Windows
/// (Meta) codes the low-level hook reports distinctly.
///
/// - `0x10` `VK_SHIFT`, `0x11` `VK_CONTROL`, `0x12` `VK_MENU` (Alt)
/// - `0x5B` `VK_LWIN`, `0x5C` `VK_RWIN`
/// - `0xA0..=0xA5` `VK_LSHIFT`/`VK_RSHIFT`/`VK_LCONTROL`/`VK_RCONTROL`/
///   `VK_LMENU`/`VK_RMENU`
pub(crate) fn is_modifier_vk(vk: u32) -> bool {
    matches!(
        vk,
        VK_SHIFT
            | VK_CONTROL
            | VK_MENU
            | 0x5B
            | 0x5C
            | VK_LSHIFT
            | VK_RSHIFT
            | VK_LCONTROL
            | VK_RCONTROL
            | VK_LMENU
            | VK_RMENU
    )
}

/// Normalize Windows modifier VKs to side-specific [`NamedKey`] values.
///
/// Low-level hooks can report either side-specific modifier VKs (`0xA0..=0xA5`)
/// or generic `VK_SHIFT`/`VK_CONTROL`/`VK_MENU` (`0x10..=0x12`). Generic Control
/// and Alt use `LLKHF_EXTENDED` for the right-hand key; generic Shift uses scan
/// code `0x36` for right Shift. If those hints are absent or indeterminate, the
/// mapping defaults to the left side so a modifier event never falls through as
/// `KeySpec::Raw` and bypasses the matcher.
pub(crate) fn map_modifier_vk(
    vk: u32,
    scan_code: u32,
    flags: u32,
) -> Option<(KeySpec, Option<Side>)> {
    let extended = (flags & LLKHF_EXTENDED) != 0;
    let mapped = match vk {
        VK_LSHIFT => (NamedKey::ShiftLeft, Side::Left),
        VK_RSHIFT => (NamedKey::ShiftRight, Side::Right),
        VK_LCONTROL => (NamedKey::ControlLeft, Side::Left),
        VK_RCONTROL => (NamedKey::ControlRight, Side::Right),
        VK_LMENU => (NamedKey::AltLeft, Side::Left),
        VK_RMENU => (NamedKey::AltRight, Side::Right),
        VK_SHIFT if scan_code == RIGHT_SHIFT_SCAN_CODE => (NamedKey::ShiftRight, Side::Right),
        VK_SHIFT => (NamedKey::ShiftLeft, Side::Left),
        VK_CONTROL if extended => (NamedKey::ControlRight, Side::Right),
        VK_CONTROL => (NamedKey::ControlLeft, Side::Left),
        VK_MENU if extended => (NamedKey::AltRight, Side::Right),
        VK_MENU => (NamedKey::AltLeft, Side::Left),
        _ => return None,
    };
    Some((KeySpec::Named(mapped.0), Some(mapped.1)))
}

/// Pure consume decision for a single key event.
///
/// Returns `true` (swallow via `LRESULT(1)`) ONLY for a non-repeat, non-modifier
/// key-DOWN whose exact `(mods, key)` is registered in `consume`. Key-ups,
/// auto-repeats, and — critically — ANY modifier virtual-key always pass through
/// (`false`). The raw event is still forwarded to the matcher upstream of this
/// call, so a modifier-hold / isolated-tap / double-tap trigger still observes
/// modifier presses even though they are never consumed.
///
/// Consuming a bare modifier (Control/Shift/Alt/Win) would globally swallow it
/// and break system shortcuts like Ctrl+C/Ctrl+V. `is_modifier_vk` is the
/// primary guard; `side.is_none()` (modifier events carry a side) is a
/// redundant secondary guard kept for robustness — it does NOT alone suffice,
/// because the generic `VK_SHIFT`/`VK_CONTROL`/`VK_MENU` codes (0x10/0x11/0x12)
/// are not side-specific and fall through `map_vk` with `side = None`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn should_consume_keydown(
    vk: u32,
    key: KeySpec,
    side: Option<Side>,
    down: bool,
    is_repeat: bool,
    mods: ModSet,
    consume: &ConsumeSet,
) -> bool {
    down && !is_repeat && !is_modifier_vk(vk) && side.is_none() && consume.consumes(key, mods)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Modifier, NamedKey};

    /// A consume set containing exactly one bare single key, bypassing
    /// `ConsumeSet::from_bindings` (which now filters bare modifiers) so we can
    /// prove the VK-level guard saves us even if a bare-modifier `SingleKey`
    /// somehow entered the live consume set.
    fn single(key: NamedKey) -> ConsumeSet {
        ConsumeSet {
            singles: vec![KeySpec::Named(key)],
            combos: Vec::new(),
        }
    }

    #[test]
    fn modifier_vks_are_recognized() {
        // side-agnostic base codes
        for vk in [0x10u32, 0x11, 0x12] {
            assert!(is_modifier_vk(vk), "0x{:X} should be a modifier", vk);
        }
        // Windows / Meta keys
        for vk in [0x5Bu32, 0x5C] {
            assert!(is_modifier_vk(vk), "0x{:X} should be a modifier", vk);
        }
        // side-specific L/R modifiers
        for vk in [0xA0u32, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5] {
            assert!(is_modifier_vk(vk), "0x{:X} should be a modifier", vk);
        }
    }

    #[test]
    fn generic_modifier_vks_map_to_side_specific_keys() {
        assert_eq!(
            map_modifier_vk(VK_SHIFT, 0x2A, 0),
            Some((KeySpec::Named(NamedKey::ShiftLeft), Some(Side::Left))),
        );
        assert_eq!(
            map_modifier_vk(VK_SHIFT, RIGHT_SHIFT_SCAN_CODE, 0),
            Some((KeySpec::Named(NamedKey::ShiftRight), Some(Side::Right))),
        );
        assert_eq!(
            map_modifier_vk(VK_CONTROL, 0x1D, 0),
            Some((KeySpec::Named(NamedKey::ControlLeft), Some(Side::Left))),
        );
        assert_eq!(
            map_modifier_vk(VK_CONTROL, 0x1D, LLKHF_EXTENDED),
            Some((KeySpec::Named(NamedKey::ControlRight), Some(Side::Right))),
        );
        assert_eq!(
            map_modifier_vk(VK_MENU, 0x38, 0),
            Some((KeySpec::Named(NamedKey::AltLeft), Some(Side::Left))),
        );
        assert_eq!(
            map_modifier_vk(VK_MENU, 0x38, LLKHF_EXTENDED),
            Some((KeySpec::Named(NamedKey::AltRight), Some(Side::Right))),
        );
    }

    #[test]
    fn side_specific_modifier_vks_still_map_directly() {
        assert_eq!(
            map_modifier_vk(VK_LSHIFT, 0, 0),
            Some((KeySpec::Named(NamedKey::ShiftLeft), Some(Side::Left))),
        );
        assert_eq!(
            map_modifier_vk(VK_RSHIFT, 0, 0),
            Some((KeySpec::Named(NamedKey::ShiftRight), Some(Side::Right))),
        );
        assert_eq!(
            map_modifier_vk(VK_LCONTROL, 0, 0),
            Some((KeySpec::Named(NamedKey::ControlLeft), Some(Side::Left))),
        );
        assert_eq!(
            map_modifier_vk(VK_RCONTROL, 0, 0),
            Some((KeySpec::Named(NamedKey::ControlRight), Some(Side::Right))),
        );
        assert_eq!(
            map_modifier_vk(VK_LMENU, 0, 0),
            Some((KeySpec::Named(NamedKey::AltLeft), Some(Side::Left))),
        );
        assert_eq!(
            map_modifier_vk(VK_RMENU, 0, 0),
            Some((KeySpec::Named(NamedKey::AltRight), Some(Side::Right))),
        );
    }

    #[test]
    fn non_modifier_vks_are_not_recognized() {
        for vk in [
            0x41u32, // 'A'
            0x30,    // '0'
            0x77,    // VK_F8
            0x1B,    // VK_ESCAPE
            0x0D,    // VK_RETURN
            0x20,    // VK_SPACE
        ] {
            assert!(!is_modifier_vk(vk), "0x{:X} should NOT be a modifier", vk);
        }
    }

    #[test]
    fn modifier_vks_are_never_consumed_even_when_bound() {
        // The generic base codes (0x10/0x11/0x12) are the ones `side.is_none()`
        // alone would miss: prove the VK guard catches them too.
        for vk in [0x10u32, 0x11, 0x12] {
            assert!(
                !should_consume_keydown(
                    vk,
                    KeySpec::Raw(vk),
                    None,
                    true,
                    false,
                    ModSet::empty(),
                    &ConsumeSet {
                        singles: vec![KeySpec::Raw(vk)],
                        combos: Vec::new(),
                    },
                ),
                "generic modifier 0x{:X} must NEVER be consumed",
                vk
            );
        }
        // Every side-specific modifier family, each with a matching bare-modifier
        // SingleKey bound (the worst case the guard must defeat).
        for (vk, key, side) in [
            (0xA0u32, NamedKey::ShiftLeft, Side::Left),
            (0xA1, NamedKey::ShiftRight, Side::Right),
            (0xA2, NamedKey::ControlLeft, Side::Left),
            (0xA3, NamedKey::ControlRight, Side::Right),
            (0xA4, NamedKey::AltLeft, Side::Left),
            (0xA5, NamedKey::AltRight, Side::Right),
            (0x5B, NamedKey::MetaLeft, Side::Left),
            (0x5C, NamedKey::MetaRight, Side::Right),
        ] {
            assert!(
                !should_consume_keydown(
                    vk,
                    KeySpec::Named(key),
                    Some(side),
                    true,
                    false,
                    ModSet::empty(),
                    &single(key),
                ),
                "modifier 0x{:X} must NEVER be consumed even when bound",
                vk
            );
        }
    }

    #[test]
    fn non_modifier_single_keys_still_consume_when_bound() {
        // VK_F8 (0x77), bound as a SingleKey with no mods held → consume.
        assert!(should_consume_keydown(
            0x77,
            KeySpec::Named(NamedKey::F8),
            None,
            true,
            false,
            ModSet::empty(),
            &single(NamedKey::F8),
        ));
        // VK_ESCAPE (0x1B) → consume.
        assert!(should_consume_keydown(
            0x1B,
            KeySpec::Named(NamedKey::Escape),
            None,
            true,
            false,
            ModSet::empty(),
            &single(NamedKey::Escape),
        ));
    }

    #[test]
    fn keyups_and_repeats_never_consume() {
        let set = single(NamedKey::F8);
        // key-up (down = false) → pass through.
        assert!(!should_consume_keydown(
            0x77,
            KeySpec::Named(NamedKey::F8),
            None,
            false,
            false,
            ModSet::empty(),
            &set,
        ));
        // auto-repeat → pass through.
        assert!(!should_consume_keydown(
            0x77,
            KeySpec::Named(NamedKey::F8),
            None,
            true,
            true,
            ModSet::empty(),
            &set,
        ));
    }

    #[test]
    fn single_key_with_modifier_held_does_not_consume() {
        // A bound single key pressed while a modifier is held passes through
        // (mirrors `ConsumeSet::consumes`: singles require zero mods).
        assert!(!should_consume_keydown(
            0x77,
            KeySpec::Named(NamedKey::F8),
            None,
            true,
            false,
            ModSet::empty().with(Modifier::Shift),
            &single(NamedKey::F8),
        ));
    }
}
