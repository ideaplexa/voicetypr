use crate::commands::key_normalizer::is_typing_safe_single_key;
use crate::commands::shortcuts::{
    decode_shortcut_settings_value, hold_shortcut_transition, is_single_key_shortcut,
    next_ai_enabled, normalized_custom_shortcut_conflict, pressed_shortcut_should_run,
    should_normalize_preset_to_clean_on_enable, validate_shortcut_settings, CustomHoldTransition,
    ExistingShortcutStrings, ModifierKind, ModifierSpec, ShortcutAction, ShortcutBinding,
    ShortcutSettings, ShortcutTrigger, SideKind, TriggerKind, MAX_SINGLE_KEY_BINDINGS,
};
use keytrigger::KeyPhase;
use std::collections::HashSet;

fn binding(action: ShortcutAction, shortcut: &str) -> ShortcutBinding {
    ShortcutBinding {
        id: format!("{:?}-{}", action, shortcut),
        action,
        shortcut: shortcut.to_string(),
        trigger: ShortcutTrigger::Pressed,
        enabled: true,
        allow_risky_combo: false,
        trigger_kind: crate::commands::shortcuts::TriggerKind::Combo,
        modifier: None,
    }
}

#[test]
fn shortcut_settings_default_empty() {
    assert!(ShortcutSettings::default().bindings.is_empty());
}

#[test]
fn stored_retired_mode_shortcut_is_dropped_without_losing_valid_bindings() {
    let stored = serde_json::json!({
        "bindings": [
            {
                "id": "retired-writing",
                "action": "set_writing",
                "shortcut": "F5",
                "trigger": "pressed",
                "enabled": true,
                "allow_risky_combo": true
            },
            {
                "id": "recording-toggle",
                "action": "toggle_recording",
                "shortcut": "F1",
                "trigger": "pressed",
                "enabled": true,
                "allow_risky_combo": true
            },
            {
                "id": "copy-last",
                "action": "copy_last_transcription",
                "shortcut": "CommandOrControl+Shift+C",
                "trigger": "pressed",
                "enabled": true,
                "allow_risky_combo": false
            }
        ]
    });

    let loaded = decode_shortcut_settings_value(stored)
        .expect("retired shortcut action should not fail the whole settings load");

    assert!(loaded.dropped_unknown_actions);
    assert_eq!(loaded.settings.bindings.len(), 2);
    assert_eq!(loaded.settings.bindings[0].id, "recording-toggle");
    assert_eq!(
        loaded.settings.bindings[0].action,
        ShortcutAction::ToggleRecording
    );
    assert_eq!(loaded.settings.bindings[1].id, "copy-last");
    assert_eq!(
        loaded.settings.bindings[1].action,
        ShortcutAction::CopyLastTranscription
    );
}

#[test]
fn single_key_hold_to_record_requires_risky_flag() {
    let mut single_key = binding(ShortcutAction::HoldToRecord, "F1");
    single_key.trigger = ShortcutTrigger::Hold;
    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![single_key.clone()],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());

    single_key.allow_risky_combo = true;
    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![single_key],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_ok());

    let mut modifier_only = binding(ShortcutAction::HoldToRecord, "Alt");
    modifier_only.trigger = ShortcutTrigger::Hold;
    modifier_only.allow_risky_combo = true;
    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![modifier_only],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());
}

#[test]
fn hold_to_record_requires_hold_trigger() {
    let mut hold = binding(ShortcutAction::HoldToRecord, "CommandOrControl+Alt+H");
    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![hold.clone()],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());

    hold.trigger = ShortcutTrigger::Hold;
    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![hold],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_ok());
}

#[test]
fn non_hold_actions_reject_hold_trigger() {
    let mut toggle = binding(ShortcutAction::ToggleRecording, "CommandOrControl+Alt+T");
    toggle.trigger = ShortcutTrigger::Hold;

    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![toggle],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());
}

#[test]
fn duplicate_enabled_custom_binding_rejected() {
    let first = binding(
        ShortcutAction::CopyLastTranscription,
        "CommandOrControl+Alt+C",
    );
    let second = binding(ShortcutAction::PasteLastTranscription, "Cmd+Alt+C");

    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![first, second],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());
}

#[test]
fn duplicate_existing_hotkey_rejected() {
    let existing = ExistingShortcutStrings {
        primary_hotkey: Some("CommandOrControl+Shift+Space".to_string()),
        ptt_hotkey: Some("Alt+Space".to_string()),
    };

    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![binding(
                ShortcutAction::OpenDashboard,
                "CommandOrControl+Shift+Space",
            )],
        },
        &existing,
    )
    .is_err());

    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![binding(ShortcutAction::OpenDashboard, "Alt+Space")],
        },
        &existing,
    )
    .is_err());
}

#[test]
fn primary_and_ptt_must_differ() {
    // The recording hotkey and the push-to-talk hotkey must not resolve to the
    // same physical trigger -- engine_host would otherwise synthesize both a
    // `primary` and a `ptt` binding for one keypress.
    let same = ExistingShortcutStrings {
        primary_hotkey: Some("CommandOrControl+Shift+P".to_string()),
        ptt_hotkey: Some("CommandOrControl+Shift+P".to_string()),
    };
    assert!(validate_shortcut_settings(ShortcutSettings::default(), &same).is_err());

    // Distinct triggers are fine.
    let distinct = ExistingShortcutStrings {
        primary_hotkey: Some("CommandOrControl+Shift+P".to_string()),
        ptt_hotkey: Some("Alt+Space".to_string()),
    };
    assert!(validate_shortcut_settings(ShortcutSettings::default(), &distinct).is_ok());
}

#[cfg(target_os = "windows")]
#[test]
fn platform_equivalent_combos_dedup_on_windows() {
    // On Windows, parse_combo collapses CommandOrControl -> Control, so these two
    // string-distinct combos resolve to the SAME trigger and must be rejected as
    // duplicates -- a plain string compare would let both fire on one keypress.
    let bindings = vec![
        binding(ShortcutAction::CopyLastTranscription, "CommandOrControl+J"),
        binding(ShortcutAction::PasteLastTranscription, "Control+J"),
    ];
    assert!(validate_shortcut_settings(
        ShortcutSettings { bindings },
        &ExistingShortcutStrings::default(),
    )
    .is_err());
}

#[test]
fn escape_is_reserved_even_for_risky_hold_binding() {
    let mut escape = binding(ShortcutAction::HoldToRecord, "Escape");
    escape.trigger = ShortcutTrigger::Hold;
    escape.allow_risky_combo = true;

    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![escape],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());
}

#[test]
fn normalized_custom_shortcut_conflict_ignores_disabled_bindings() {
    let enabled = binding(ShortcutAction::OpenDashboard, "CommandOrControl+Alt+D");
    let mut disabled = binding(
        ShortcutAction::CopyLastTranscription,
        "CommandOrControl+Alt+C",
    );
    disabled.enabled = false;

    let settings = ShortcutSettings {
        bindings: vec![enabled.clone(), disabled],
    };

    assert_eq!(
        normalized_custom_shortcut_conflict("CommandOrControl+Alt+D", &settings),
        Some(enabled.id)
    );
    assert_eq!(
        normalized_custom_shortcut_conflict("CommandOrControl+Alt+C", &settings),
        None
    );
}

#[test]
fn pressed_trigger_dedupes_until_release() {
    let mut active = HashSet::new();

    assert!(pressed_shortcut_should_run(
        &mut active,
        "copy",
        KeyPhase::Pressed,
    ));
    assert!(!pressed_shortcut_should_run(
        &mut active,
        "copy",
        KeyPhase::Pressed,
    ));
    assert!(!pressed_shortcut_should_run(
        &mut active,
        "copy",
        KeyPhase::Released,
    ));
    assert!(pressed_shortcut_should_run(
        &mut active,
        "copy",
        KeyPhase::Pressed,
    ));
}

#[test]
fn multiple_hold_bindings_stop_only_after_last_release() {
    let mut active = HashSet::new();

    assert_eq!(
        hold_shortcut_transition(&mut active, "hold-a", KeyPhase::Pressed),
        CustomHoldTransition::Start
    );
    assert_eq!(
        hold_shortcut_transition(&mut active, "hold-b", KeyPhase::Pressed),
        CustomHoldTransition::Noop
    );
    assert_eq!(
        hold_shortcut_transition(&mut active, "hold-a", KeyPhase::Released),
        CustomHoldTransition::Noop
    );
    assert_eq!(
        hold_shortcut_transition(&mut active, "hold-b", KeyPhase::Released),
        CustomHoldTransition::Stop
    );
}

#[test]
fn disabled_invalid_binding_does_not_fail_validation() {
    let mut disabled = binding(ShortcutAction::OpenDashboard, "");
    disabled.enabled = false;

    let validated = validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![disabled],
        },
        &ExistingShortcutStrings::default(),
    )
    .expect("disabled invalid shortcut should not be registered or validated");

    assert_eq!(validated.bindings.len(), 1);
    assert!(!validated.bindings[0].enabled);
}

#[test]
fn next_ai_enabled_toggles_correctly() {
    // Disabling always works regardless of can_enable
    assert_eq!(next_ai_enabled(true, true), Some(false));
    assert_eq!(next_ai_enabled(true, false), Some(false));
    // Enabling only works when can_enable is true
    assert_eq!(next_ai_enabled(false, true), Some(true));
    // Enabling is refused without a usable AI setup
    assert_eq!(next_ai_enabled(false, false), None);
}

#[test]
fn enabling_polish_normalizes_any_non_clean_preset_to_clean() {
    // A non-AI preset would leave Polish "on but inert" — normalize to Clean.
    assert!(should_normalize_preset_to_clean_on_enable(Some(
        "PersonalDictation"
    )));
    // Legacy global reshaping presets must NOT survive a tray/shortcut enable;
    // normalize them even before the recording-path migration runs.
    assert!(should_normalize_preset_to_clean_on_enable(Some("Notes")));
    assert!(should_normalize_preset_to_clean_on_enable(Some("Writing")));
    assert!(should_normalize_preset_to_clean_on_enable(Some("Message")));
    assert!(should_normalize_preset_to_clean_on_enable(Some("Code")));
    // Legacy aliases resolve to a reshaping preset too — normalize.
    assert!(should_normalize_preset_to_clean_on_enable(Some("Coding")));
    // Already-Clean is left alone.
    assert!(!should_normalize_preset_to_clean_on_enable(Some(
        "CleanDictation"
    )));
    // "Default" and a missing preset both resolve to Clean when enabled.
    assert!(!should_normalize_preset_to_clean_on_enable(Some("Default")));
    assert!(!should_normalize_preset_to_clean_on_enable(None));
}

#[test]
fn single_key_safe_on_non_hold_action_validates() {
    // F1 on ToggleRecording with allow_risky_combo should pass
    let mut toggle_binding = binding(ShortcutAction::ToggleRecording, "F1");
    toggle_binding.allow_risky_combo = true;
    assert!(
        validate_shortcut_settings(
            ShortcutSettings {
                bindings: vec![toggle_binding]
            },
            &ExistingShortcutStrings::default(),
        )
        .is_ok(),
        "F1 on ToggleRecording with allow_risky_combo should validate"
    );

    // Home on CopyLastTranscription with allow_risky_combo should pass
    let mut copy_binding = binding(ShortcutAction::CopyLastTranscription, "Home");
    copy_binding.allow_risky_combo = true;
    assert!(
        validate_shortcut_settings(
            ShortcutSettings {
                bindings: vec![copy_binding]
            },
            &ExistingShortcutStrings::default(),
        )
        .is_ok(),
        "Home on CopyLastTranscription with allow_risky_combo should validate"
    );
}

#[test]
fn single_key_typing_key_rejected_for_any_action() {
    // Letter key rejected for HoldToRecord
    let mut hold_binding = binding(ShortcutAction::HoldToRecord, "E");
    hold_binding.trigger = ShortcutTrigger::Hold;
    hold_binding.allow_risky_combo = true;
    let err = validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![hold_binding],
        },
        &ExistingShortcutStrings::default(),
    )
    .unwrap_err();
    assert!(
        err.contains("function key") || err.contains("Single-key"),
        "Expected typing-key rejection for HoldToRecord, got: {err}"
    );

    // Letter key rejected for non-HoldToRecord action
    let mut toggle_binding = binding(ShortcutAction::ToggleRecording, "E");
    toggle_binding.allow_risky_combo = true;
    let err = validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![toggle_binding],
        },
        &ExistingShortcutStrings::default(),
    )
    .unwrap_err();
    assert!(
        err.contains("function key") || err.contains("Single-key"),
        "Expected typing-key rejection for ToggleRecording, got: {err}"
    );
}

#[test]
fn single_key_cap_allows_5_rejects_6() {
    fn safe_sk(action: ShortcutAction, fkey: &str) -> ShortcutBinding {
        ShortcutBinding {
            allow_risky_combo: true,
            ..binding(action, fkey)
        }
    }

    let five_bindings = vec![
        safe_sk(ShortcutAction::ToggleRecording, "F1"),
        safe_sk(ShortcutAction::CancelRecording, "F2"),
        safe_sk(ShortcutAction::CopyLastTranscription, "F3"),
        safe_sk(ShortcutAction::PasteLastTranscription, "F4"),
        safe_sk(ShortcutAction::ToggleAiFormatting, "F5"),
    ];
    assert_eq!(five_bindings.len(), MAX_SINGLE_KEY_BINDINGS);
    assert!(
        validate_shortcut_settings(
            ShortcutSettings {
                bindings: five_bindings.clone()
            },
            &ExistingShortcutStrings::default(),
        )
        .is_ok(),
        "Exactly {} single-key bindings should be accepted",
        MAX_SINGLE_KEY_BINDINGS
    );

    let mut six_bindings = five_bindings;
    six_bindings.push(safe_sk(ShortcutAction::OpenDashboard, "F6"));
    let err = validate_shortcut_settings(
        ShortcutSettings {
            bindings: six_bindings,
        },
        &ExistingShortcutStrings::default(),
    )
    .unwrap_err();
    assert!(
        err.contains("single-key"),
        "Expected single-key cap error for 6 bindings, got: {err}"
    );
}

#[test]
fn is_typing_safe_single_key_allowlist() {
    // Allowed: function keys
    assert!(is_typing_safe_single_key("F1"));
    assert!(is_typing_safe_single_key("F12"));
    assert!(is_typing_safe_single_key("F24"));
    // Allowed: numpad
    assert!(is_typing_safe_single_key("Numpad0"));
    assert!(is_typing_safe_single_key("Numpad9"));
    assert!(is_typing_safe_single_key("NumpadAdd"));
    assert!(is_typing_safe_single_key("NumpadSubtract"));
    // Allowed: navigation cluster
    assert!(is_typing_safe_single_key("Home"));
    assert!(is_typing_safe_single_key("End"));
    assert!(is_typing_safe_single_key("PageUp"));
    assert!(is_typing_safe_single_key("PageDown"));
    assert!(is_typing_safe_single_key("Insert"));
    assert!(is_typing_safe_single_key("Up"));
    assert!(is_typing_safe_single_key("Down"));
    assert!(is_typing_safe_single_key("Left"));
    assert!(is_typing_safe_single_key("Right"));

    // Rejected: letters, digits, typing keys
    assert!(!is_typing_safe_single_key("A"));
    assert!(!is_typing_safe_single_key("1"));
    assert!(!is_typing_safe_single_key("Space"));
    assert!(!is_typing_safe_single_key("Enter"));
    assert!(!is_typing_safe_single_key("Tab"));
    assert!(!is_typing_safe_single_key("Backspace"));
    assert!(!is_typing_safe_single_key("Delete"));
    assert!(!is_typing_safe_single_key("Escape"));
    assert!(!is_typing_safe_single_key("Alt"));
    assert!(!is_typing_safe_single_key("Shift"));
    // Rejected: out-of-range function keys
    assert!(!is_typing_safe_single_key("F0"));
    assert!(!is_typing_safe_single_key("F25"));
}

#[test]
fn is_single_key_shortcut_detection() {
    assert!(is_single_key_shortcut("F1"));
    assert!(is_single_key_shortcut("Home"));
    assert!(is_single_key_shortcut("Numpad0"));
    assert!(is_single_key_shortcut("A")); // letter IS single-key (just not typing-safe)

    assert!(!is_single_key_shortcut("F1+A"));
    assert!(!is_single_key_shortcut("CommandOrControl+F1"));
    assert!(!is_single_key_shortcut("Alt+Shift+F1"));
    assert!(!is_single_key_shortcut("Alt")); // bare modifier excluded
    assert!(!is_single_key_shortcut("Shift"));
    assert!(!is_single_key_shortcut("")); // empty excluded
}

#[test]
fn legacy_binding_json_defaults_to_combo() {
    // A binding persisted before the native-trigger fields existed must
    // deserialize with trigger_kind defaulting to Combo (zero behavior change).
    let json = r#"{
        "id": "abc",
        "action": "toggle_recording",
        "shortcut": "CommandOrControl+Shift+Space",
        "trigger": "pressed",
        "enabled": true,
        "allow_risky_combo": false
    }"#;
    let parsed: ShortcutBinding = serde_json::from_str(json).expect("deserialize legacy binding");
    assert_eq!(
        parsed.trigger_kind,
        crate::commands::shortcuts::TriggerKind::Combo
    );
    assert!(parsed.modifier.is_none());
}

/// A Right-Option HoldToRecord binding (the onboarding default) validates correctly
/// even when the primary hotkey field is explicitly empty (cleared by the user).
/// This exercises the native-engine `ModifierHold` path: the bare modifier is the
/// trigger, so there is no combo shortcut to parse or conflict-check against the
/// primary hotkey.
#[test]
fn modifier_hold_right_option_validates_with_empty_primary() {
    let binding = ShortcutBinding {
        id: "hold_to_record".to_string(),
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
    };
    let settings = ShortcutSettings {
        bindings: vec![binding],
    };
    // Primary hotkey explicitly cleared (empty string)
    let existing = ExistingShortcutStrings {
        primary_hotkey: Some(String::new()),
        ptt_hotkey: None,
    };
    assert!(
        validate_shortcut_settings(settings, &existing).is_ok(),
        "Right-Option ModifierHold binding must validate when primary hotkey is empty"
    );
}

/// When the primary hotkey is empty the engine-kind binding is its own recording
/// trigger. A second (combo) binding must still fail if it conflicts with itself,
/// but an engine-kind binding alongside an empty primary must not be rejected.
#[test]
fn engine_kind_binding_not_treated_as_combo_against_empty_primary() {
    // Engine-kind bindings get validate() from mapping.rs, not prepare_enabled_shortcut,
    // so an empty primary hotkey string must never cause a parse/conflict rejection.
    let binding = ShortcutBinding {
        id: "hold_to_record".to_string(),
        action: ShortcutAction::HoldToRecord,
        shortcut: String::new(),
        trigger: ShortcutTrigger::Hold,
        enabled: true,
        allow_risky_combo: false,
        trigger_kind: TriggerKind::ModifierHold,
        modifier: Some(ModifierSpec {
            modifier: ModifierKind::Meta,
            side: SideKind::Right,
        }),
    };
    let existing = ExistingShortcutStrings {
        primary_hotkey: Some(String::new()),
        ptt_hotkey: None,
    };
    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![binding]
        },
        &existing
    )
    .is_ok());
}
