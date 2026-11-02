//! Conservative no-op detector; uncertainty always runs Polish.
use super::prompts::EnhancementPreset;

pub fn should_skip(
    input: &str,
    preset: EnhancementPreset,
    transcript_language: Option<&str>,
) -> bool {
    let english = transcript_language.is_some_and(|language| {
        language
            .trim()
            .split(['-', '_'])
            .next()
            .is_some_and(|base| base.eq_ignore_ascii_case("en"))
    });
    if !english {
        return false;
    }
    if !matches!(
        preset,
        EnhancementPreset::CleanDictation | EnhancementPreset::Message | EnhancementPreset::Notes
    ) {
        return false;
    }
    // Whitespace repair, translation and style reshaping must still run.
    if input.trim() != input
        || input.contains('\n')
        || input.contains('\t')
        || input.contains("  ")
        || !input.chars().next().is_some_and(char::is_uppercase)
        || !input.ends_with(['.', '!', '?'])
        || input.split_whitespace().count() > 30
    {
        return false;
    }
    let lower = input.to_lowercase();
    let words: Vec<_> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .collect();
    if words.iter().any(|w| {
        [
            "no", "um", "uh", "er", "ah", "erm", "hmm", "like", "sorry", "rather", "actually",
        ]
        .contains(w)
    }) {
        return false;
    }
    for marker in [
        "you know",
        "i mean",
        "sort of",
        "kind of",
        "no wait",
        "scratch that",
    ] {
        let tokens: Vec<_> = marker.split_whitespace().collect();
        if words.windows(tokens.len()).any(|w| w == tokens) {
            return false;
        }
    }
    // All adjacent repeated n-grams, including single words and punctuation variants.
    for size in 1..=words.len() / 2 {
        if words.windows(size * 2).any(|w| w[..size] == w[size..]) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_english_transcripts_can_skip_polish() {
        for language in [
            None,
            Some(""),
            Some("de"),
            Some("de-AT"),
            Some("fr"),
            Some("es"),
            Some("pl"),
            Some("nl"),
        ] {
            assert!(!should_skip(
                "Ähm, wir kommen.",
                EnhancementPreset::CleanDictation,
                language
            ));
            assert!(!should_skip(
                "Ready.",
                EnhancementPreset::CleanDictation,
                language
            ));
        }
        for language in ["en", "en-US", "en_GB", " EN "] {
            assert!(should_skip(
                "Ready.",
                EnhancementPreset::CleanDictation,
                Some(language)
            ));
        }
    }

    #[test]
    fn uncertainty_and_reshaping_run() {
        for input in [
            "um hello.",
            "Hello",
            "hello.",
            "Hello hello.",
            "Send it send it.",
            "Like this.",
            "Tuesday, no wait, Wednesday.",
            "Actually, send it.",
            "I mean send it.",
            "Sort of done.",
            "Hello  there.",
        ] {
            assert!(!should_skip(
                input,
                EnhancementPreset::CleanDictation,
                Some("en")
            ));
        }
        for preset in [EnhancementPreset::Writing, EnhancementPreset::Code] {
            assert!(!should_skip("Ready.", preset, Some("en")));
        }
        for preset in [
            EnhancementPreset::CleanDictation,
            EnhancementPreset::Message,
            EnhancementPreset::Notes,
        ] {
            assert!(should_skip("Ready.", preset, Some("en")));
            let thirty = format!(
                "Ready {}.",
                (0..29)
                    .map(|i| format!("word{i}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            assert!(should_skip(&thirty, preset, Some("en")));
            assert!(!should_skip(&format!("Ready {thirty}"), preset, Some("en")));
        }
    }
}
