use super::{
    format::{format_for_transcription, protected_terms, FormatStageSettings},
    library_rules::{apply_library_rules, sanitize_transcript},
    CustomWord, TextReplacementRule, WritingSettings,
};
use crate::transcription::{
    TranscriptionJob, TranscriptionResult, TranscriptionSource, TranscriptionTask,
};

struct Fixture {
    id: &'static str,
    input: &'static str,
    language: Option<&'static str>,
    engine: &'static str,
    flags: &'static str,
    scope: Option<&'static str>,
    expected: &'static str,
}

#[test]
fn protected_lowercase_terms_never_leak_across_languages() {
    let fixtures = [
        (
            "foreign-terms",
            Some("en"),
            Some("de"),
            "I run pnpm. eBay works.",
        ),
        (
            "foreign-region",
            Some("en-GB"),
            Some("de-AT"),
            "I run pnpm. eBay works.",
        ),
        (
            "matching-base",
            Some("en-US"),
            Some("en"),
            "i um run pnpm. eBay works.",
        ),
        (
            "matching-region",
            Some("en"),
            Some("en-GB"),
            "i um run pnpm. eBay works.",
        ),
        ("global", Some("en"), None, "i um run pnpm. eBay works."),
        ("unknown", None, Some("en"), "i um run pnpm. eBay works."),
        (
            "empty-language",
            Some(""),
            Some("en"),
            "i um run pnpm. eBay works.",
        ),
        (
            "invalid-scope",
            Some("en"),
            Some("invalid"),
            "I run pnpm. eBay works.",
        ),
        (
            "german-conservative",
            Some("de"),
            Some("en"),
            "i um run pnpm. eBay works.",
        ),
    ];
    for (id, language, scope, expected) in fixtures {
        let case = Fixture {
            id,
            language,
            scope,
            expected,
            input: "i um run pnpm. eBay works.",
            engine: "whisper",
            flags: "final,polish-off,remove-fillers",
        };
        let settings = WritingSettings {
            // Replacement targets and vocabulary use exactly the same scope
            // semantics as their deterministic dictionary application.
            replacements: vec![TextReplacementRule {
                from: "a different phrase".into(),
                to: "i".into(),
                language: scope.map(str::to_string),
                enabled: true,
            }],
            custom_words: ["um", "pnpm"]
                .into_iter()
                .map(|phrase| CustomWord {
                    phrase: phrase.into(),
                    spoken_form: None,
                    language: scope.map(str::to_string),
                    enabled: true,
                })
                .collect(),
            ..WritingSettings::default()
        };
        let terms = protected_terms(&settings, case.language);
        let should_match = super::language_scope_matches(case.scope, case.language);
        assert_eq!(!terms.is_empty(), should_match, "{}", case.id);
        let job = TranscriptionJob {
            source: TranscriptionSource::AudioFile,
            engine: case.engine.into(),
            model: "fixture".into(),
            spoken_language: case.language.map(str::to_string),
            task: TranscriptionTask::Transcribe,
        };
        let transcription = TranscriptionResult::new(&job, case.input);
        let cleaned = sanitize_transcript(case.input, false);
        let library = apply_library_rules(&cleaned, &settings, case.language, &mut Vec::new());
        let stage = FormatStageSettings {
            paragraphs: false,
            remove_fillers: true,
        };
        let actual =
            format_for_transcription(&library.text, &transcription, case.language, stage, &terms)
                .unwrap_or(library.text);
        assert_eq!(
            actual.as_bytes(),
            case.expected.as_bytes(),
            "{} {} {}",
            case.id,
            case.engine,
            case.flags
        );
        assert_eq!(
            format_for_transcription(&actual, &transcription, case.language, stage, &terms),
            None,
            "{}",
            case.id
        );
    }
}

#[test]
fn identical_terms_use_only_matching_entries_and_global_user_vocabulary() {
    let settings = WritingSettings {
        custom_words: [
            ("pnpm", Some("en"), true),
            ("pnpm", Some("de"), true),
            ("eBay", None, true),
            ("disabled", None, false),
            ("invalid", Some("xx"), true),
        ]
        .into_iter()
        .map(|(phrase, language, enabled)| CustomWord {
            phrase: phrase.into(),
            spoken_form: None,
            language: language.map(str::to_string),
            enabled,
        })
        .collect(),
        ..WritingSettings::default()
    };
    assert_eq!(protected_terms(&settings, Some("en-US")), ["pnpm", "eBay"]);
    assert_eq!(protected_terms(&settings, Some("de-AT")), ["pnpm", "eBay"]);
    for language in [None, Some(""), Some(" \t "), Some("fr"), Some("xx")] {
        assert_eq!(protected_terms(&settings, language), ["eBay"]);
    }
}
