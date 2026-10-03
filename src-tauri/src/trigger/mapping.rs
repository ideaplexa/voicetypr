//! Map persisted `ShortcutBinding`s to `keytrigger::Trigger`s.

use std::time::Duration;

use keytrigger::{KeySpec, ModSet, Modifier, NamedKey, Side, TapKey, Trigger};

use crate::commands::key_normalizer::normalize_shortcut_keys;
use crate::commands::shortcuts::{
    ModifierKind, ShortcutAction, ShortcutBinding, ShortcutTrigger, SideKind, TriggerKind,
};

/// Max single-press duration that still counts as an isolated tap.
const ISOLATED_TAP_MS: u64 = 500;

/// True if this binding is handled by the native engine (by the native engine).
pub fn is_engine_kind(binding: &ShortcutBinding) -> bool {
    matches!(
        binding.trigger_kind,
        TriggerKind::Combo | TriggerKind::ModifierHold | TriggerKind::IsolatedTap
    )
}

/// Convert a binding to its `keytrigger::Trigger`, or `None` if required fields
/// are missing or a combo string fails to parse.
pub fn to_trigger(binding: &ShortcutBinding) -> Option<Trigger> {
    match binding.trigger_kind {
        TriggerKind::Combo => parse_combo(&binding.shortcut).ok(),
        TriggerKind::ModifierHold => {
            let spec = binding.modifier?;
            Some(Trigger::ModifierHold {
                modifier: modifier(spec.modifier),
                side: side(spec.side),
            })
        }
        TriggerKind::IsolatedTap => {
            let spec = binding.modifier?;
            Some(Trigger::IsolatedTap {
                key: TapKey::Mod(modifier(spec.modifier), side(spec.side)),
                within: Duration::from_millis(ISOLATED_TAP_MS),
            })
        }
    }
}

/// Returns `true` iff `bindings` contains at least one enabled `HoldToRecord`
/// engine-kind binding with a resolvable trigger.
///
/// Used at startup: an empty primary hotkey is only safe to skip when such a
/// binding exists and covers recording via the native keytrigger engine.
#[cfg(test)]
pub fn has_recording_engine_binding(bindings: &[ShortcutBinding]) -> bool {
    bindings.iter().any(|b| {
        b.enabled
            && b.action == ShortcutAction::HoldToRecord
            && is_engine_kind(b)
            && to_trigger(b).is_some()
    })
}

/// Validate an enabled engine-kind binding (bindings). Returns a user-facing error string on failure.
pub fn validate(binding: &ShortcutBinding) -> Result<(), String> {
    if binding.id.trim().is_empty() {
        return Err("Shortcut binding id is required".to_string());
    }
    match binding.trigger_kind {
        TriggerKind::Combo => {
            // Combo routes through the engine as ComboExact/SingleKey; reject it
            // here if the stored shortcut string cannot be parsed.
            parse_combo(&binding.shortcut)?;
        }
        TriggerKind::ModifierHold => {
            if binding.modifier.is_none() {
                return Err(format!(
                    "Hold-a-modifier binding '{}' requires a modifier",
                    binding.id
                ));
            }
            // The engine emits Pressed on modifier-down / Released on up; only the
            // Hold trigger + HoldToRecord action consume that (dispatch_action
            // otherwise silently no-ops).
            if binding.trigger != ShortcutTrigger::Hold
                || binding.action != ShortcutAction::HoldToRecord
            {
                return Err(
                    "Hold-a-modifier shortcuts can only be used for Hold-to-record".to_string(),
                );
            }
        }
        TriggerKind::IsolatedTap => {
            if binding.modifier.is_none() {
                return Err(format!(
                    "Tap-a-modifier binding '{}' requires a modifier",
                    binding.id
                ));
            }
            // Isolated single-tap is a one-shot press; it cannot drive a hold.
            if binding.trigger != ShortcutTrigger::Pressed {
                return Err("Tap-a-modifier shortcuts must use the press trigger".to_string());
            }
            if binding.action == ShortcutAction::HoldToRecord {
                return Err("Tap-a-modifier cannot be used for Hold-to-record".to_string());
            }
        }
    }
    Ok(())
}

fn modifier(kind: ModifierKind) -> Modifier {
    match kind {
        ModifierKind::Alt => Modifier::Alt,
        ModifierKind::Control => Modifier::Control,
        ModifierKind::Meta => Modifier::Meta,
        ModifierKind::Shift => Modifier::Shift,
    }
}

fn side(kind: SideKind) -> Side {
    match kind {
        SideKind::Left => Side::Left,
        SideKind::Right => Side::Right,
        SideKind::Either => Side::Either,
    }
}

/// Parse a normalized combo string (e.g. "CommandOrControl+Shift+Space") into a
/// `ComboExact` (>=1 modifier) or `SingleKey` (no modifier) trigger. Errors on an
/// unknown token or when there is not exactly one non-modifier key.
/// Two shortcut strings name the same physical combo (spelling, case and
/// modifier order ignored).
pub(crate) fn same_shortcut(a: &str, b: &str) -> bool {
    match (parse_combo(a), parse_combo(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => normalize_shortcut_keys(a).eq_ignore_ascii_case(&normalize_shortcut_keys(b)),
    }
}

pub(crate) fn parse_combo(shortcut: &str) -> Result<Trigger, String> {
    let normalized = normalize_shortcut_keys(shortcut);
    let mut mods = ModSet::empty();
    let mut key: Option<KeySpec> = None;
    for token in normalized.split('+').filter(|t| !t.is_empty()) {
        if let Some(m) = modifier_token(token) {
            mods.insert(m);
        } else if let Some(k) = key_token(token) {
            if key.is_some() {
                return Err(format!(
                    "Shortcut '{}' has more than one non-modifier key",
                    shortcut
                ));
            }
            key = Some(KeySpec::Named(k));
        } else {
            return Err(format!(
                "Unsupported key '{}' in shortcut '{}'",
                token, shortcut
            ));
        }
    }
    let key = key.ok_or_else(|| format!("Shortcut '{}' has no non-modifier key", shortcut))?;
    if mods.is_empty() {
        Ok(Trigger::SingleKey { key })
    } else {
        Ok(Trigger::ComboExact { mods, key })
    }
}

/// Normalized modifier token -> side-agnostic `Modifier`. `CommandOrControl`
/// resolves per-OS: Command (Meta) on macOS, Control elsewhere.
fn modifier_token(token: &str) -> Option<Modifier> {
    match token {
        "CommandOrControl" => Some(if cfg!(target_os = "macos") {
            Modifier::Meta
        } else {
            Modifier::Control
        }),
        "Control" => Some(Modifier::Control),
        "Alt" => Some(Modifier::Alt),
        "Shift" => Some(Modifier::Shift),
        _ => None,
    }
}

/// Normalized non-modifier key token -> keytrigger `NamedKey` (None if unknown).
fn key_token(token: &str) -> Option<NamedKey> {
    if token.len() == 1 {
        let c = token.chars().next()?;
        if c.is_ascii_uppercase() {
            return letter_named(c);
        }
        if c.is_ascii_digit() {
            return digit_named(c);
        }
    }
    let named = match token {
        "Space" => NamedKey::Space,
        "Enter" => NamedKey::Enter,
        "Tab" => NamedKey::Tab,
        "Backspace" => NamedKey::Backspace,
        "Escape" => NamedKey::Escape,
        "Delete" => NamedKey::Delete,
        "Insert" => NamedKey::Insert,
        "Up" => NamedKey::ArrowUp,
        "Down" => NamedKey::ArrowDown,
        "Left" => NamedKey::ArrowLeft,
        "Right" => NamedKey::ArrowRight,
        "Home" => NamedKey::Home,
        "End" => NamedKey::End,
        "PageUp" => NamedKey::PageUp,
        "PageDown" => NamedKey::PageDown,
        "Comma" => NamedKey::Comma,
        "Period" => NamedKey::Period,
        "Slash" => NamedKey::Slash,
        "Semicolon" => NamedKey::Semicolon,
        "Quote" => NamedKey::Quote,
        "BracketLeft" => NamedKey::BracketLeft,
        "BracketRight" => NamedKey::BracketRight,
        "Backslash" => NamedKey::Backslash,
        "Equal" => NamedKey::Equal,
        "Minus" => NamedKey::Minus,
        "Backquote" => NamedKey::Backquote,
        "NumpadAdd" => NamedKey::NumpadAdd,
        "NumpadSubtract" => NamedKey::NumpadSubtract,
        "NumpadMultiply" => NamedKey::NumpadMultiply,
        "NumpadDivide" => NamedKey::NumpadDivide,
        "NumpadDecimal" => NamedKey::NumpadDecimal,
        "NumpadEnter" => NamedKey::NumpadEnter,
        "NumpadEqual" => NamedKey::NumpadEqual,
        _ if token.starts_with("Numpad") => return numpad_named(token),
        _ if token.starts_with('F') => return fkey_named(token),
        _ => return None,
    };
    Some(named)
}

fn letter_named(c: char) -> Option<NamedKey> {
    const LETTERS: [NamedKey; 26] = [
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
    ];
    LETTERS.get((c as u8 - b'A') as usize).copied()
}

fn digit_named(c: char) -> Option<NamedKey> {
    const DIGITS: [NamedKey; 10] = [
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
    ];
    DIGITS.get((c as u8 - b'0') as usize).copied()
}

fn fkey_named(token: &str) -> Option<NamedKey> {
    const FKEYS: [NamedKey; 24] = [
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
    ];
    let n: usize = token.strip_prefix('F')?.parse().ok()?;
    n.checked_sub(1).and_then(|i| FKEYS.get(i).copied())
}

fn numpad_named(token: &str) -> Option<NamedKey> {
    const NUMPAD: [NamedKey; 10] = [
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
    ];
    let n: usize = token.strip_prefix("Numpad")?.parse().ok()?;
    NUMPAD.get(n).copied()
}

#[cfg(test)]
mod tests {
    use super::{
        has_recording_engine_binding, is_engine_kind, parse_combo, same_shortcut, to_trigger,
        validate,
    };

    #[test]
    fn same_shortcut_ignores_spelling_case_and_modifier_order() {
        // "Ctrl" means Cmd/Ctrl: the same physical combo only off macOS.
        assert_eq!(
            same_shortcut("Control+Space", "Ctrl+Space"),
            !cfg!(target_os = "macos")
        );
        assert!(same_shortcut("alt+space", "Alt+Space"));
        assert!(same_shortcut("Shift+Alt+K", "Alt+Shift+K"));
        assert!(!same_shortcut("Alt+Space", "Control+Space"));
    }
    use crate::commands::shortcuts::{
        ModifierKind, ModifierSpec, ShortcutAction, ShortcutBinding, ShortcutTrigger, SideKind,
        TriggerKind,
    };
    use keytrigger::{KeySpec, ModSet, Modifier, NamedKey, Side, Trigger};

    fn hold_binding() -> ShortcutBinding {
        ShortcutBinding {
            id: "x".to_string(),
            action: ShortcutAction::HoldToRecord,
            shortcut: String::new(),
            trigger: ShortcutTrigger::Hold,
            enabled: true,
            allow_risky_combo: false,
            trigger_kind: TriggerKind::ModifierHold,
            modifier: Some(ModifierSpec {
                modifier: ModifierKind::Alt,
                side: SideKind::Right,
            }),
        }
    }

    #[test]
    fn combo_maps_and_validates() {
        let mut b = hold_binding();
        b.trigger_kind = TriggerKind::Combo;
        b.shortcut = "CommandOrControl+Shift+Space".to_string();
        assert!(is_engine_kind(&b));
        assert!(validate(&b).is_ok());
        assert!(matches!(to_trigger(&b), Some(Trigger::ComboExact { .. })));
    }

    #[test]
    fn modifier_hold_maps_and_validates() {
        let b = hold_binding();
        assert!(is_engine_kind(&b));
        assert!(validate(&b).is_ok());
        assert!(matches!(
            to_trigger(&b),
            Some(Trigger::ModifierHold {
                modifier: Modifier::Alt,
                side: Side::Right
            })
        ));
    }

    #[test]
    fn modifier_hold_rejects_non_hold_record_action() {
        let mut b = hold_binding();
        b.action = ShortcutAction::ToggleRecording;
        assert!(validate(&b).is_err());
    }

    #[test]
    fn engine_kind_missing_modifier_rejected() {
        let mut b = hold_binding();
        b.modifier = None;
        assert!(validate(&b).is_err());
        assert!(to_trigger(&b).is_none());
    }

    #[test]
    fn modifier_hold_left_side_maps() {
        let mut b = hold_binding();
        b.modifier = Some(ModifierSpec {
            modifier: ModifierKind::Control,
            side: SideKind::Left,
        });
        assert!(is_engine_kind(&b));
        assert!(validate(&b).is_ok());
        assert!(matches!(
            to_trigger(&b),
            Some(Trigger::ModifierHold {
                modifier: Modifier::Control,
                side: Side::Left,
            })
        ));
    }

    #[test]
    fn modifier_hold_either_side_maps() {
        let mut b = hold_binding();
        b.modifier = Some(ModifierSpec {
            modifier: ModifierKind::Alt,
            side: SideKind::Either,
        });
        assert!(validate(&b).is_ok());
        assert!(matches!(
            to_trigger(&b),
            Some(Trigger::ModifierHold {
                modifier: Modifier::Alt,
                side: Side::Either,
            })
        ));
    }

    #[test]
    fn has_recording_engine_binding_enabled_hold() {
        let b = hold_binding();
        assert!(has_recording_engine_binding(&[b]));
    }

    #[test]
    fn has_recording_engine_binding_disabled_returns_false() {
        let mut b = hold_binding();
        b.enabled = false;
        assert!(!has_recording_engine_binding(&[b]));
    }

    #[test]
    fn has_recording_engine_binding_invalid_combo_returns_false() {
        let mut b = hold_binding();
        b.trigger_kind = TriggerKind::Combo;
        assert!(!has_recording_engine_binding(&[b]));
    }

    #[test]
    fn has_recording_engine_binding_toggle_action_returns_false() {
        let mut b = hold_binding();
        b.action = ShortcutAction::ToggleRecording;
        // ModifierHold + ToggleRecording is not a HoldToRecord binding
        assert!(!has_recording_engine_binding(&[b]));
    }

    #[test]
    fn has_recording_engine_binding_empty_slice_returns_false() {
        assert!(!has_recording_engine_binding(&[]));
    }

    #[test]
    fn isolated_tap_maps_and_validates() {
        let mut b = hold_binding();
        b.trigger_kind = TriggerKind::IsolatedTap;
        b.trigger = ShortcutTrigger::Pressed;
        b.action = ShortcutAction::ToggleRecording;
        b.modifier = Some(ModifierSpec {
            modifier: ModifierKind::Control,
            side: SideKind::Either,
        });
        assert!(is_engine_kind(&b));
        assert!(validate(&b).is_ok());
        assert!(matches!(to_trigger(&b), Some(Trigger::IsolatedTap { .. })));
    }

    #[test]
    fn isolated_tap_rejects_hold_to_record() {
        let mut b = hold_binding();
        b.trigger_kind = TriggerKind::IsolatedTap;
        b.trigger = ShortcutTrigger::Pressed;
        b.action = ShortcutAction::HoldToRecord;
        assert!(validate(&b).is_err());
    }

    #[test]
    fn isolated_tap_requires_pressed_trigger() {
        let mut b = hold_binding();
        b.trigger_kind = TriggerKind::IsolatedTap;
        b.trigger = ShortcutTrigger::Hold;
        b.action = ShortcutAction::ToggleRecording;
        assert!(validate(&b).is_err());
    }

    #[test]
    fn isolated_tap_missing_modifier_rejected() {
        let mut b = hold_binding();
        b.trigger_kind = TriggerKind::IsolatedTap;
        b.trigger = ShortcutTrigger::Pressed;
        b.action = ShortcutAction::ToggleRecording;
        b.modifier = None;
        assert!(validate(&b).is_err());
        assert!(to_trigger(&b).is_none());
    }

    #[test]
    fn parse_combo_maps_combos_single_keys_and_special_keys() {
        // Combo with >=1 modifier -> ComboExact with EXACT mods (per-OS CommandOrControl).
        let expected_mods = if cfg!(target_os = "macos") {
            ModSet::empty().with(Modifier::Meta).with(Modifier::Shift)
        } else {
            ModSet::empty()
                .with(Modifier::Control)
                .with(Modifier::Shift)
        };
        assert_eq!(
            parse_combo("CommandOrControl+Shift+Space"),
            Ok(Trigger::ComboExact {
                mods: expected_mods,
                key: KeySpec::Named(NamedKey::Space),
            })
        );
        // Lone non-modifier key -> SingleKey.
        assert_eq!(
            parse_combo("F8"),
            Ok(Trigger::SingleKey {
                key: KeySpec::Named(NamedKey::F8),
            })
        );
        // NumpadEqual is picker-emittable AND fires on both backends, so it must parse.
        // CapsLock is NOT engine-representable on macOS (it arrives as flagsChanged with
        // no key-down), so it must error clearly at save instead of becoming a dead hotkey.
        assert!(parse_combo("Alt+CapsLock").is_err());
        assert!(parse_combo("CapsLock").is_err());
        assert!(matches!(
            parse_combo("Control+NumpadEqual"),
            Ok(Trigger::ComboExact {
                key: KeySpec::Named(NamedKey::NumpadEqual),
                ..
            })
        ));
        // Untappable / unknown keys fail clearly (not silently): a modifier-only
        // string has no key, and media keys are not engine-representable.
        assert!(parse_combo("Shift").is_err());
        assert!(parse_combo("Alt+AudioVolumeUp").is_err());
    }
}
