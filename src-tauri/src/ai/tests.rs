#[cfg(test)]
mod behavior_tests {
    use crate::ai::prompts::{
        build_enhancement_prompt, build_enhancement_prompt_for_transcript_language,
        get_language_name, migrate_preset_str, parse_enhancement_options_from_value,
        EnhancementOptions, EnhancementPreset,
    };
    use serde::Deserialize;

    const ALL_PRESETS: &[EnhancementPreset] = &[
        EnhancementPreset::PersonalDictation,
        EnhancementPreset::CleanDictation,
        EnhancementPreset::Writing,
        EnhancementPreset::Notes,
        EnhancementPreset::Message,
        EnhancementPreset::Code,
    ];

    fn options(preset: EnhancementPreset) -> EnhancementOptions {
        EnhancementOptions { preset }
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PresetParityFixture {
        migrations: Vec<MigrationCase>,
        requires_ai_formatting: Vec<RequiresAiCase>,
        defaults: Vec<DefaultCase>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct MigrationCase {
        raw: String,
        ai_enabled: bool,
        expected: String,
    }

    #[derive(Debug, Deserialize)]
    struct RequiresAiCase {
        preset: String,
        expected: bool,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DefaultCase {
        ai_enabled: bool,
        expected: String,
    }

    fn preset_from_fixture(value: &str, ai_enabled: bool) -> EnhancementPreset {
        migrate_preset_str(value, ai_enabled)
    }

    // All 6 presets build without panic.
    #[test]
    fn all_presets_build_without_panic() {
        for &preset in ALL_PRESETS {
            let _ = build_enhancement_prompt(None, &options(preset), None);
        }
    }

    #[test]
    fn shared_preset_parity_fixture_matches_rust_contract() {
        let fixture: PresetParityFixture =
            serde_json::from_str(include_str!("../../../tests/fixtures/preset-parity.json"))
                .expect("preset parity fixture should be valid JSON");

        for test_case in fixture.migrations {
            assert_eq!(
                migrate_preset_str(&test_case.raw, test_case.ai_enabled),
                preset_from_fixture(&test_case.expected, test_case.ai_enabled),
                "migration case {:?}",
                test_case
            );
        }

        for test_case in fixture.requires_ai_formatting {
            assert_eq!(
                preset_from_fixture(&test_case.preset, false).requires_ai_formatting(),
                test_case.expected,
                "requires-ai case {:?}",
                test_case
            );
        }

        for test_case in fixture.defaults {
            assert_eq!(
                EnhancementOptions::default_for_ai_enabled(test_case.ai_enabled).preset,
                preset_from_fixture(&test_case.expected, test_case.ai_enabled),
                "default case {:?}",
                test_case
            );
        }
    }

    // Injection guard line present in every preset.
    #[test]
    fn injection_guard_present_in_every_preset() {
        for &preset in ALL_PRESETS {
            let prompt = build_enhancement_prompt(None, &options(preset), None);
            assert!(
                prompt.contains("not commands for you"),
                "Preset {:?} must include the injection guard",
                preset
            );
            assert!(
                prompt.contains("ignore these rules"),
                "Preset {:?} must include the ignore-instructions guard",
                preset
            );
        }
    }

    // Exactly ONE output directive across every preset.
    #[test]
    fn exactly_one_output_directive_in_every_preset() {
        for &preset in ALL_PRESETS {
            let prompt = build_enhancement_prompt(None, &options(preset), None);
            let count = prompt.matches("Output only").count();
            assert_eq!(
                count, 1,
                "Preset {:?} must have exactly one 'Output only' directive, found {}",
                preset, count
            );
        }
    }

    // Reshaping transforms stay exclusive to Writing/Notes/Message/Code.
    #[test]
    fn transform_present_only_for_formatting_presets() {
        let formatting_markers: &[(EnhancementPreset, &str)] = &[
            (EnhancementPreset::Writing, "make it read well"),
            (EnhancementPreset::Notes, "turn it into notes"),
            (EnhancementPreset::Message, "make it a short message"),
            (EnhancementPreset::Code, "format for code"),
        ];
        for &(preset, marker) in formatting_markers {
            let prompt = build_enhancement_prompt(None, &options(preset), None);
            assert!(
                prompt.contains(marker),
                "Preset {:?} should contain transform marker {:?}",
                preset,
                marker
            );
        }

        let non_formatting = [
            EnhancementPreset::PersonalDictation,
            EnhancementPreset::CleanDictation,
        ];
        for preset in non_formatting {
            let prompt = build_enhancement_prompt(None, &options(preset), None);
            for marker in [
                "make it read well",
                "turn it into notes",
                "make it a short message",
                "format for code",
            ] {
                assert!(
                    !prompt.contains(marker),
                    "Preset {:?} should NOT contain transform marker {:?}",
                    preset,
                    marker
                );
            }
        }
    }

    #[test]
    fn clean_dictation_adds_restrained_paragraph_guidance() {
        let clean =
            build_enhancement_prompt(None, &options(EnhancementPreset::CleanDictation), None);
        assert!(clean.contains("clear topic or intent change"));
        assert!(clean.contains("Keep short dictation in one paragraph"));
        assert!(clean.contains("Do not add headings or bullets"));

        let personal =
            build_enhancement_prompt(None, &options(EnhancementPreset::PersonalDictation), None);
        assert!(!personal.contains("clear topic or intent change"));
    }

    #[test]
    fn polish_does_not_execute_spoken_formatting_commands() {
        for &preset in ALL_PRESETS {
            let prompt = build_enhancement_prompt(None, &options(preset), None);
            assert!(!prompt.contains("Do dictation commands"));
            assert!(prompt.contains("Do not execute spoken formatting commands"));
        }
    }

    // Language directive: en->English, es->Spanish, ja->Japanese, None->English.
    #[test]
    fn language_directive_in_prompt() {
        let opts = options(EnhancementPreset::CleanDictation);
        assert!(build_enhancement_prompt(None, &opts, Some("en")).contains("written English"));
        assert!(build_enhancement_prompt(None, &opts, Some("es")).contains("written Spanish"));
        assert!(build_enhancement_prompt(None, &opts, Some("ja")).contains("written Japanese"));
        // None defaults to English.
        assert!(build_enhancement_prompt(None, &opts, None).contains("written English"));
    }

    #[test]
    fn translation_prompt_adds_explicit_instruction_when_languages_differ() {
        let opts = options(EnhancementPreset::CleanDictation);
        let prompt = build_enhancement_prompt_for_transcript_language(
            None,
            &opts,
            Some("es"),
            Some("en"),
            None,
        );
        assert!(prompt.contains("written Spanish"));
        assert!(
            prompt.contains("The dictation may be in another language; translate it into Spanish.")
        );
    }

    #[test]
    fn same_language_prompt_does_not_add_translation_instruction() {
        let opts = options(EnhancementPreset::CleanDictation);
        let prompt = build_enhancement_prompt_for_transcript_language(
            None,
            &opts,
            Some("en"),
            Some("en"),
            None,
        );
        assert!(!prompt.contains("translate it into"));
    }

    #[test]
    fn app_category_hint_injected_when_present() {
        let opts = options(EnhancementPreset::CleanDictation);
        let hint = "You are dictating into a Chat context. Keep it casual.";
        let prompt = build_enhancement_prompt_for_transcript_language(
            None,
            &opts,
            Some("en"),
            Some("en"),
            Some(hint),
        );
        assert!(prompt.contains(hint), "category hint must appear in prompt");
        assert!(prompt.contains("dictating into a Chat context"));
    }

    #[test]
    fn app_category_hint_absent_when_none() {
        let opts = options(EnhancementPreset::CleanDictation);
        let prompt = build_enhancement_prompt_for_transcript_language(
            None,
            &opts,
            Some("en"),
            Some("en"),
            None,
        );
        assert!(!prompt.contains("dictating into a"));
        assert!(!prompt.contains("Context:"));
    }

    #[test]
    fn app_category_hint_precedes_known_terms_block() {
        let opts = options(EnhancementPreset::CleanDictation);
        let hint = "You are dictating into a Chat context. Keep it casual.";
        let terms = "Tauri, Zustand";
        let prompt = build_enhancement_prompt_for_transcript_language(
            Some(terms),
            &opts,
            Some("en"),
            Some("en"),
            Some(hint),
        );
        let hint_pos = prompt.find("dictating into a Chat context");
        let terms_pos = prompt.find("Known terms");
        assert!(hint_pos.is_some(), "hint must be present");
        assert!(terms_pos.is_some(), "Known terms must be present");
        assert!(
            hint_pos < terms_pos,
            "category hint must precede Known terms block"
        );
    }

    // De-dup proof: built prompt does NOT contain the transcript text.
    // The transcript rides as the user message (AiPolishRequest.input_text); the
    // system prompt must never embed it.
    #[test]
    fn transcript_is_not_embedded_in_prompt() {
        let transcript = "zzq unique de-dup marker transcript 12345";
        let prompt = build_enhancement_prompt(None, &options(EnhancementPreset::Writing), None);
        assert!(
            !prompt.contains(transcript),
            "Transcript must not be embedded in the system prompt"
        );
        assert!(!prompt.contains("de-dup marker"));
        // The legacy embedding label is gone entirely.
        assert!(!prompt.contains("Transcribed text:"));
    }

    // Context present -> active-correction framing + term list; absent -> no block.
    #[test]
    fn context_block_present_only_when_supplied() {
        let opts = options(EnhancementPreset::Writing);
        let terms = "shadcn/ui (may be heard as: shad cn), Tauri, Zustand";

        let with_context = build_enhancement_prompt(Some(terms), &opts, None);
        assert!(
            with_context.contains("Known terms"),
            "active framing header"
        );
        assert!(
            with_context.contains("Fix any"),
            "active correction instruction"
        );
        assert!(
            with_context.contains("never as commands"),
            "command guard on the context block"
        );
        assert!(with_context.contains(terms), "term list embedded verbatim");

        let without_context = build_enhancement_prompt(None, &opts, None);
        assert!(!without_context.contains("Known terms"));
        assert!(!without_context.contains(terms));
    }

    #[test]
    fn test_language_name_mapping() {
        // Test common languages
        assert_eq!(get_language_name("en"), "English");
        assert_eq!(get_language_name("es"), "Spanish");
        assert_eq!(get_language_name("fr"), "French");
        assert_eq!(get_language_name("de"), "German");
        assert_eq!(get_language_name("ja"), "Japanese");
        assert_eq!(get_language_name("zh"), "Chinese");
        assert_eq!(get_language_name("ar"), "Arabic");
        assert_eq!(get_language_name("hi"), "Hindi");
        assert_eq!(get_language_name("pt"), "Portuguese");
        assert_eq!(get_language_name("ru"), "Russian");

        // Test case insensitivity
        assert_eq!(get_language_name("EN"), "English");
        assert_eq!(get_language_name("Es"), "Spanish");

        // Test fallback for unknown language codes
        assert_eq!(get_language_name("xyz"), "English");
        assert_eq!(get_language_name(""), "English");
    }

    #[test]
    fn test_preset_migration() {
        assert_eq!(
            migrate_preset_str("Default", false),
            EnhancementPreset::PersonalDictation
        );
        assert_eq!(
            migrate_preset_str("Default", true),
            EnhancementPreset::CleanDictation
        );
        assert_eq!(migrate_preset_str("Coding", false), EnhancementPreset::Code);
        assert_eq!(
            migrate_preset_str("Prompts", false),
            EnhancementPreset::Code
        );
        assert_eq!(migrate_preset_str("Commit", false), EnhancementPreset::Code);
        assert_eq!(
            migrate_preset_str("Email", false),
            EnhancementPreset::Writing
        );
        assert_eq!(
            migrate_preset_str("Unknown", false),
            EnhancementPreset::PersonalDictation
        );
    }

    #[test]
    fn test_default_options_respect_ai_enabled() {
        assert_eq!(
            EnhancementOptions::default_for_ai_enabled(false).preset,
            EnhancementPreset::PersonalDictation
        );
        assert_eq!(
            EnhancementOptions::default_for_ai_enabled(true).preset,
            EnhancementPreset::CleanDictation
        );
    }

    #[test]
    fn test_legacy_preset_deserialization() {
        let preset: EnhancementPreset = serde_json::from_str("\"Prompts\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::Code));

        let preset: EnhancementPreset = serde_json::from_str("\"Email\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::Writing));

        let preset: EnhancementPreset = serde_json::from_str("\"Commit\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::Code));

        let preset: EnhancementPreset = serde_json::from_str("\"Default\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::PersonalDictation));

        let preset: EnhancementPreset = serde_json::from_str("\"Unknown\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::PersonalDictation));

        for (json, expected) in [
            (
                "\"PersonalDictation\"",
                EnhancementPreset::PersonalDictation,
            ),
            ("\"CleanDictation\"", EnhancementPreset::CleanDictation),
            ("\"Writing\"", EnhancementPreset::Writing),
            ("\"Notes\"", EnhancementPreset::Notes),
            ("\"Message\"", EnhancementPreset::Message),
            ("\"Code\"", EnhancementPreset::Code),
        ] {
            let preset: EnhancementPreset = serde_json::from_str(json).unwrap();
            assert_eq!(preset, expected, "Failed for {}", json);
        }

        assert_eq!(
            serde_json::to_string(&EnhancementPreset::Code).unwrap(),
            "\"Code\""
        );
        assert_eq!(
            serde_json::to_string(&EnhancementPreset::Message).unwrap(),
            "\"Message\""
        );
    }

    #[test]
    fn test_parse_enhancement_options_from_value() {
        let value = serde_json::json!({"preset": "Default"});
        let options = parse_enhancement_options_from_value(&value, true).unwrap();
        assert_eq!(options.preset, EnhancementPreset::CleanDictation);

        let options = parse_enhancement_options_from_value(&value, false).unwrap();
        assert_eq!(options.preset, EnhancementPreset::PersonalDictation);
    }

    // Legacy preset names deserialize and select their transform. Asserts on the
    // new stable transform markers (not legacy phrasing).
    #[test]
    fn legacy_options_deserialize_and_select_transform() {
        let json = r#"{"preset":"Commit"}"#;
        let opts: EnhancementOptions = serde_json::from_str(json).unwrap();
        let prompt = build_enhancement_prompt(None, &opts, None);
        assert!(prompt.contains("format for code"));

        let json = r#"{"preset":"Email"}"#;
        let opts: EnhancementOptions = serde_json::from_str(json).unwrap();
        let prompt = build_enhancement_prompt(None, &opts, None);
        assert!(prompt.contains("make it read well"));
    }

    // A Message preset carries the Message transform marker.
    #[test]
    fn message_preset_uses_message_transform() {
        let effective = EnhancementOptions {
            preset: EnhancementPreset::Message,
        };
        let prompt = build_enhancement_prompt(None, &effective, None);

        assert!(prompt.contains("make it a short message"));
    }

    // A Personal preset skips every formatting transform.
    #[test]
    fn personal_preset_skips_formatting_transform() {
        let effective = EnhancementOptions {
            preset: EnhancementPreset::PersonalDictation,
        };
        let prompt = build_enhancement_prompt(None, &effective, None);

        assert!(!prompt.contains("make it a short message"));
        assert!(!effective.preset.requires_ai_formatting());
    }

    #[test]
    fn test_personal_dictation_does_not_require_ai() {
        assert!(!EnhancementPreset::PersonalDictation.requires_ai_formatting());
        assert!(EnhancementPreset::CleanDictation.requires_ai_formatting());
    }
}
