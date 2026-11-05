use super::*;
use crate::transcription::{
    TranscriptionJob, TranscriptionResult, TranscriptionSource, TranscriptionTask,
};
use crate::writing::{
    format::{format_for_transcription, should_run_format_stage, FormatStageSettings},
    library_rules::{apply_library_rules, sanitize_transcript},
    WritingSettings,
};

struct Fixture {
    id: &'static str,
    input: &'static str,
    language: Option<&'static str>,
    engine: &'static str,
    flags: &'static str,
    trigger: &'static str,
    expected: bool,
}

#[test]
fn multilingual_snippets_match_only_the_intended_whole_utterance() {
    let fixtures = [
        ("de-case", "ÖFFNE.", Some("de"), "Öffne", true),
        ("de-region", "öffne!", Some("de-AT"), "Öffne", true),
        ("de-nfd", "O\u{308}ffne?", Some("de-CH"), "öffne", true),
        ("de-sharp-s-case", "STRAẞE", Some("de"), "Straße", true),
        ("de-no-full-fold", "STRASSE", Some("de"), "Straße", false),
        (
            "de-no-substring",
            "Bitte öffne.",
            Some("de"),
            "Öffne",
            false,
        ),
        ("de-no-accent-loss", "Offne", Some("de"), "Öffne", false),
        ("ru-case", "ПРИВЕТ!", Some("ru"), "Привет", true),
        ("ru-nonmatch", "Приветы", Some("ru"), "Привет", false),
        ("uk-case", "ВІТАЮ?", Some("uk"), "Вітаю", true),
        ("ja-period", "挨拶。", Some("ja"), "挨拶", true),
        ("ja-fullwidth-question", "挨拶？", Some("ja"), "挨拶", true),
        (
            "ja-fullwidth-exclamation",
            "挨拶！",
            Some("ja"),
            "挨拶",
            true,
        ),
        ("ja-exact-punctuation", "挨拶。", Some("ja"), "挨拶。", true),
        (
            "ja-no-repeated-ending",
            "挨拶。。",
            Some("ja"),
            "挨拶",
            false,
        ),
        ("ja-no-internal-strip", "挨。拶", Some("ja"), "挨拶", false),
        ("ja-no-substring", "挨拶します。", Some("ja"), "挨拶", false),
        ("ja-unknown-conservative", "挨拶。", None, "挨拶", false),
        ("zh-fullwidth", "问候！", Some("zh-CN"), "问候", true),
        ("es-inverted", "¿SALUDO?", Some("es"), "¿saludo", true),
        (
            "es-exact-inverted",
            "¿SALUDO?",
            Some("es"),
            "¿saludo?",
            true,
        ),
        (
            "es-leading-meaningful",
            "¿saludo?",
            Some("es"),
            "saludo",
            false,
        ),
        (
            "es-abbreviation-internal",
            "Sr. saludo",
            Some("es"),
            "Sr saludo",
            false,
        ),
        ("pt-case", "OLÁ!", Some("pt"), "Olá", true),
        ("it-case", "CIAO?", Some("it"), "ciao", true),
        ("fr-nfc", "ÉCRIS.", Some("fr"), "E\u{301}cris", true),
        ("el-simple-fold", "ΟΣ", Some("el"), "ος", true),
        ("tr-dotted", "İSİM.", Some("tr"), "isim", true),
        ("tr-dotless", "IŞIK!", Some("tr-TR"), "ışık", true),
        ("tr-nfd", "I\u{307}SI\u{307}M", Some("tr"), "isim", true),
        ("tr-nonmatch-dotless", "ISIM", Some("tr"), "isim", false),
        ("tr-nonmatch-dotted", "İ", Some("tr"), "ı", false),
        ("tr-ascii-distinct", "I", Some("tr"), "i", false),
        ("unknown-unicode", "ПРИВЕТ", None, "Привет", true),
        ("unknown-no-turkic-guess", "İ", None, "i", false),
        ("en-legacy", "INSERT NOTE.", Some("en"), "insert note", true),
        (
            "en-no-new-fullwidth-strip",
            "Insert note！",
            Some("en"),
            "insert note",
            false,
        ),
        (
            "en-no-internal-strip",
            "Insert. Note",
            Some("en"),
            "insert note",
            false,
        ),
        (
            "en-no-double-strip",
            "Insert note..",
            Some("en"),
            "insert note",
            false,
        ),
        (
            "en-no-compatibility-fold",
            "ＦＯＯ",
            Some("en"),
            "foo",
            false,
        ),
        ("en-no-trim-trigger", "Foo", Some("en"), " foo", false),
    ];
    let body = "  Literal Öİ\nkeep。\u{a0}";
    for engine in ["whisper", "deepgram", "soniox", "fixture"] {
        for (id, input, language, trigger, expected) in fixtures {
            let case = Fixture {
                id,
                input,
                language,
                trigger,
                expected,
                engine,
                flags: "final,polish-off",
            };
            let settings = WritingSettings {
                snippets: vec![Snippet {
                    trigger: case.trigger.into(),
                    body: body.into(),
                    language: None,
                    enabled: true,
                    preserve_literal: true,
                }],
                ..WritingSettings::default()
            };
            let matched = match_snippet(case.input, &settings.snippets, case.language);
            assert_eq!(
                matched.is_some(),
                case.expected,
                "{} {} {}",
                case.id,
                case.engine,
                case.flags
            );
            let key = comparison_text(case.input, matches!(case.language, Some("tr" | "tr-TR")));
            assert_eq!(
                comparison_text(&key, matches!(case.language, Some("tr" | "tr-TR"))),
                key,
                "{}",
                case.id
            );

            let cleaned = sanitize_transcript(case.input, false);
            let library = apply_library_rules(&cleaned, &settings, case.language, &mut Vec::new());
            let mut actual = library.text;
            if should_run_format_stage(library.literal_locked, false) {
                let job = TranscriptionJob {
                    source: TranscriptionSource::AudioFile,
                    engine: engine.into(),
                    model: "fixture".into(),
                    spoken_language: case.language.map(str::to_string),
                    task: TranscriptionTask::Transcribe,
                };
                let transcription = TranscriptionResult::new(&job, case.input);
                actual = format_for_transcription(
                    &actual,
                    &transcription,
                    case.language,
                    FormatStageSettings {
                        paragraphs: false,
                        remove_fillers: false,
                    },
                    &[],
                )
                .unwrap_or(actual);
            }
            assert_eq!(
                actual.as_bytes(),
                if case.expected {
                    body.as_bytes()
                } else {
                    case.input.as_bytes()
                },
                "{} {}",
                case.id,
                engine
            );
        }
    }
}

#[test]
fn unicode_matching_keeps_scope_enabled_state_and_exact_punctuation_priority() {
    let mut snippets = vec![
        Snippet {
            trigger: "Öffne".into(),
            body: "fallback".into(),
            language: Some("de".into()),
            enabled: true,
            preserve_literal: true,
        },
        Snippet {
            trigger: "öFFNE.".into(),
            body: "exact".into(),
            language: Some("de".into()),
            enabled: true,
            preserve_literal: true,
        },
    ];
    assert_eq!(
        match_snippet("ÖFFNE.", &snippets, Some("de-AT"))
            .unwrap()
            .body,
        "exact"
    );
    assert_eq!(
        match_snippet("ÖFFNE!", &snippets, Some("de-AT"))
            .unwrap()
            .body,
        "fallback"
    );
    assert!(match_snippet("ÖFFNE.", &snippets, Some("en")).is_none());
    assert!(match_snippet("ÖFFNE.", &snippets, None).is_none());
    snippets
        .iter_mut()
        .for_each(|snippet| snippet.enabled = false);
    assert!(match_snippet("ÖFFNE.", &snippets, Some("de")).is_none());
}
