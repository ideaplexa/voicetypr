//! Shared exact-byte fixtures for the engine normalizer and writing stage.
pub struct Fixture {
    pub id: String,
    pub input: String,
    pub language: &'static str,
    pub engine: &'static str,
    pub flags: &'static str,
    pub expected: String,
}

pub fn fixtures() -> Vec<Fixture> {
    let mut cases = Vec::new();
    for language in ["fr", "fr-FR", "fr-CA", " FR_fr "] {
        for separator in [
            " ",
            "\u{a0}",
            "\u{202f}",
            "  ",
            "  \u{202f}",
            "\u{202f}  ",
            "\u{a0} \u{202f}",
        ] {
            for punctuation in ['?', '!', ':', ';'] {
                let text = format!("Mme et M. Dupont{separator}{punctuation}");
                cases.push(Fixture {
                    id: format!("{language}-{punctuation}-{:x?}", separator.as_bytes()),
                    input: text.clone(),
                    language,
                    engine: "whisper",
                    flags: "final,polish-off",
                    expected: text,
                });
            }
            let text = format!("Il dit{separator}«{separator}bonjour{separator}»{separator}!");
            cases.push(Fixture {
                id: format!("{language}-guillemets-{:x?}", separator.as_bytes()),
                input: text.clone(),
                language,
                engine: "whisper",
                flags: "final,polish-off",
                expected: text,
            });
        }
    }
    for (id, input, language, expected) in [
        ("missing-spaces", "Bonjour!«oui»?", "fr", "Bonjour!«oui»?"),
        (
            "other-mechanics",
            "bonjour  monde , oui .",
            "fr",
            "bonjour monde, oui.",
        ),
        ("english", "Hello  !", "en", "Hello!"),
        ("unknown", "Hello  !", "", "Hello!"),
        ("not-french", "Hello  !", "frank", "Hello!"),
    ] {
        cases.push(Fixture {
            id: id.into(),
            input: input.into(),
            language,
            engine: "whisper",
            flags: "final,polish-off,native-casing",
            expected: expected.into(),
        });
    }
    cases
}
