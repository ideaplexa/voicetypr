use super::language_intent::TranslationIntent;
use crate::commands::settings::FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT as SAME;

pub(crate) struct Fixture {
    pub id: &'static str,
    pub input: &'static str,
    pub language: Option<&'static str>,
    pub target: Option<&'static str>,
    pub engine: &'static str,
    pub flags: &'static str,
    pub expected: TranslationIntent<'static>,
}

pub(crate) fn fixtures() -> Vec<Fixture> {
    let mut cases = Vec::new();
    for engine in ["whisper", "deepgram", "soniox", "fixture"] {
        for language in [
            None,
            Some(""),
            Some(" \t "),
            Some("auto"),
            Some("und"),
            Some("zz"),
        ] {
            for (id, target, expected) in [
                ("unknown-same", Some(SAME), TranslationIntent::Preserve),
                ("unknown-missing-target", None, TranslationIntent::Preserve),
                (
                    "unknown-empty-target",
                    Some(" "),
                    TranslationIntent::Preserve,
                ),
                (
                    "unknown-explicit-fr",
                    Some("fr"),
                    TranslationIntent::TranslateTo("fr"),
                ),
                (
                    "unknown-explicit-en",
                    Some("en"),
                    TranslationIntent::TranslateTo("en"),
                ),
            ] {
                cases.push(Fixture {
                    id,
                    input: "Bonjour, salut.",
                    language,
                    target,
                    engine,
                    flags: "polish-on,final",
                    expected,
                });
            }
        }
        for (id, language, target, expected) in [
            ("english", "en", "en", TranslationIntent::Preserve),
            ("regional-same", "fr-CA", "fr", TranslationIntent::Preserve),
            (
                "regional-case",
                " EN_us ",
                "en-GB",
                TranslationIntent::Preserve,
            ),
            ("known-same", "de-AT", SAME, TranslationIntent::Preserve),
            (
                "supported-unmapped",
                "la",
                SAME,
                TranslationIntent::Preserve,
            ),
            (
                "known-translate",
                "es",
                "fr-FR",
                TranslationIntent::TranslateTo("fr-FR"),
            ),
        ] {
            cases.push(Fixture {
                id,
                input: "Une phrase.",
                language: Some(language),
                target: Some(target),
                engine,
                flags: "polish-on,final",
                expected,
            });
        }
    }
    cases
}

#[test]
fn prompt_matches_explicit_translation_intent() {
    use super::{
        polish::assemble_prompt,
        prompts::{EnhancementOptions, EnhancementPreset},
    };
    for case in fixtures() {
        assert_eq!(
            TranslationIntent::resolve(case.language, case.target),
            case.expected,
            "{}",
            case.id
        );
        for keep_words in [false, true] {
            let prompt = assemble_prompt(
                &EnhancementOptions {
                    preset: EnhancementPreset::CleanDictation,
                },
                case.target,
                case.language,
                None,
                None,
                keep_words,
            );
            assert_eq!(
                prompt.contains("translate it into"),
                case.expected.target().is_some(),
                "{} {} {}",
                case.id,
                case.engine,
                case.flags
            );
            assert!(
                !prompt.contains(case.input),
                "transcript must not enter system prompt"
            );
            if case.expected == TranslationIntent::Preserve
                && super::language_intent::present_language(case.language).is_none()
            {
                assert!(!prompt.contains("English"), "{}", case.id);
                assert!(prompt.contains("Do not translate."), "{}", case.id);
            }
            if case.id == "supported-unmapped" {
                assert!(prompt.contains("written Latin"));
                assert!(!prompt.contains("English"));
            }
        }
    }
}
