//! Whole-utterance matching only. Canonical equivalence is for comparison, never
//! a rewrite of transcript or literal body bytes.
use regex::RegexBuilder;
use unicode_normalization::UnicodeNormalization;

use super::{language_scope_matches, pipeline::supported_language_base, Snippet};

fn comparison_text(text: &str, turkic: bool) -> String {
    text.nfc()
        .map(|ch| match (turkic, ch) {
            (true, 'I') => 'ı',
            (true, 'İ') => 'i',
            _ => ch,
        })
        .collect()
}

fn is_terminator(ch: char, language: Option<&str>) -> bool {
    matches!(ch, '.' | '!' | '?')
        || (matches!(language, Some("ja" | "zh")) && matches!(ch, '。' | '！' | '？'))
}

pub(super) fn match_snippet<'a>(
    text: &str,
    snippets: &'a [Snippet],
    transcript_language: Option<&str>,
) -> Option<&'a Snippet> {
    if snippets.is_empty() {
        return None;
    }
    let language = transcript_language.and_then(supported_language_base);
    let turkic = matches!(language.as_deref(), Some("tr" | "az"));
    let trimmed = comparison_text(text.trim(), turkic);
    let text_chars = trimmed.chars().count();
    let eligible: Vec<_> = snippets
        .iter()
        .filter(|snippet| {
            snippet.enabled
                && language_scope_matches(snippet.language.as_deref(), transcript_language)
        })
        .map(|snippet| (snippet, comparison_text(&snippet.trigger, turkic)))
        // Simple folding preserves scalar count. Avoid compiling a pattern for
        // long dictation that cannot possibly be a whole-utterance trigger.
        .filter(|(_, trigger)| {
            let chars = trigger.chars().count();
            chars == text_chars || chars + 1 == text_chars
        })
        .collect();
    if eligible.is_empty() {
        return None;
    }

    // regex's Unicode simple case folding keeps Straße distinct from Strasse.
    // Do not use compatibility normalization, accent stripping or full folding:
    // those would make unrelated trigger spellings equivalent.
    let exact = RegexBuilder::new(&format!(r"\A{}\z", regex::escape(&trimmed)))
        .case_insensitive(true)
        .build()
        .ok()?;
    if let Some((snippet, _)) = eligible
        .iter()
        .filter(|(_, trigger)| exact.is_match(trigger))
        .max_by_key(|(snippet, _)| snippet.trigger.len())
    {
        return Some(snippet);
    }

    // Tolerate exactly one provider-added terminator. Never strip a leading ¿,
    // internal punctuation, repeated endings or punctuation in a stored trigger.
    let last = trimmed.chars().last()?;
    if !is_terminator(last, language.as_deref()) {
        return None;
    }
    let without_terminator = &trimmed[..trimmed.len() - last.len_utf8()];
    let fallback = RegexBuilder::new(&format!(r"\A{}\z", regex::escape(without_terminator)))
        .case_insensitive(true)
        .build()
        .ok()?;
    eligible
        .into_iter()
        .filter(|(_, trigger)| {
            !trigger
                .chars()
                .last()
                .is_some_and(|ch| is_terminator(ch, language.as_deref()))
                && fallback.is_match(trigger)
        })
        .max_by_key(|(snippet, _)| snippet.trigger.len())
        .map(|(snippet, _)| snippet)
}

#[cfg(test)]
#[path = "snippet_tests.rs"]
mod tests;
