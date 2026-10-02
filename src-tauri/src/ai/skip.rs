//! Conservative no-op detector; uncertainty always runs Polish.
use super::prompts::EnhancementPreset;

pub fn should_skip(input: &str, preset: EnhancementPreset) -> bool {
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
            "um", "uh", "er", "ah", "erm", "hmm", "like", "sorry", "rather", "actually",
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
            assert!(!should_skip(input, EnhancementPreset::CleanDictation));
        }
        for preset in [EnhancementPreset::Writing, EnhancementPreset::Code] {
            assert!(!should_skip("Ready.", preset));
        }
        for preset in [
            EnhancementPreset::CleanDictation,
            EnhancementPreset::Message,
            EnhancementPreset::Notes,
        ] {
            assert!(should_skip("Ready.", preset));
            let thirty = format!(
                "Ready {}.",
                (0..29)
                    .map(|i| format!("word{i}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            assert!(should_skip(&thirty, preset));
            assert!(!should_skip(&format!("Ready {thirty}"), preset));
        }
    }
}
