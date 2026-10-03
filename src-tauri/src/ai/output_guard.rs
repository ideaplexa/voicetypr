//! Content-only safety check. Reasons never contain input or provider output.
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OutputGuardReason {
    #[error("meta_reply")]
    MetaReply,
    #[error("answered")]
    Answered,
}
impl OutputGuardReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::MetaReply => "meta_reply",
            Self::Answered => "answered",
        }
    }
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// Strong task/assistant signals, exempt when dictated by the speaker. Avoid
// generic words like "text" or "sorry" that commonly belong to real messages.
const META_SIGNALS: &[&str] = &[
    "voice dictation",
    "clean up",
    "dictation to",
    "please provide",
    "i'm here to",
    "i'm ready to",
    "there is no dictation",
    "as an ai",
    "i can't",
    "i cannot",
    "the text you'd like",
];
// Golden self-corrections legitimately drop half the source content words.
const MIN_CONTENT_OVERLAP: f64 = 0.50;

fn content_words(text: &str) -> BTreeSet<String> {
    const STOP: &[&str] = &[
        "a", "an", "the", "is", "are", "was", "be", "to", "of", "and", "or", "in", "on", "at",
        "for", "it", "i", "you", "my", "your", "this", "that", "do", "does", "can", "could",
        "would", "will", "please", "what", "how", "why", "when", "where", "who", "which", "um",
        "uh",
    ];
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .filter(|w| !STOP.contains(&w.as_str()))
        .collect()
}
fn question_or_imperative(input: &str) -> bool {
    let lower = normalize(input);
    lower.ends_with('?')
        || [
            "what",
            "what's",
            "how",
            "why",
            "when",
            "where",
            "who",
            "which",
            "can",
            "could",
            "would",
            "should",
            "is",
            "are",
            "do",
            "does",
            "did",
            "will",
            "please",
            "let's",
            "send",
            "keep",
            "make",
            "write",
            "translate",
            "ignore",
            "answer",
            "pretend",
            "summarize",
            "explain",
            "tell",
            "show",
            "give",
            "delete",
            "run",
            "change",
            "stop",
            "don't",
            "review",
        ]
        .iter()
        .any(|w| lower == *w || lower.starts_with(&format!("{w} ")))
}

fn is_question(text: &str) -> bool {
    let lower = normalize(text);
    lower.ends_with('?')
        || [
            "what", "what's", "how", "why", "when", "where", "who", "which", "can", "could",
            "would", "should", "is", "are", "do", "does", "did", "will",
        ]
        .iter()
        .any(|w| lower == *w || lower.starts_with(&format!("{w} ")))
}

/// Only remove a leading wrapper introduced by the model. The executor validates
/// the remaining payload normally, including answer and expansion checks.
pub(crate) fn strip_inline_wrapper<'a>(output: &'a str, input: &str) -> &'a str {
    let Some((clause, rest)) = output.split_once(':') else {
        return output;
    };
    let candidate = normalize(clause);
    let source = normalize(input);
    let wrapper = candidate.starts_with("here's ")
        || candidate.starts_with("here is ")
        || candidate.starts_with("sure, ")
        || candidate == "cleaned text";
    if wrapper && !source.starts_with(&candidate) {
        rest.trim()
    } else {
        output
    }
}

pub(crate) fn starts_with_refusal_or_commentary(output: &str, input: &str) -> bool {
    fn collapse_stutters(text: &str) -> String {
        let normalized = normalize(text);
        let mut words = Vec::new();
        for word in normalized.split_whitespace() {
            if words.last().copied() != Some(word) {
                words.push(word);
            }
        }
        words.join(" ")
    }
    let output = collapse_stutters(output);
    let input = collapse_stutters(input);
    ["i can't", "i cannot", "i'm sorry", "i am sorry"]
        .iter()
        .any(|phrase| output.starts_with(phrase) && !input.starts_with(phrase))
}

pub fn check(input: &str, output: &str) -> Result<(), OutputGuardReason> {
    check_with_intent(input, output, false)
}

pub fn check_with_intent(
    input: &str,
    output: &str,
    translation: bool,
) -> Result<(), OutputGuardReason> {
    let source = normalize(input);
    let target = normalize(output);
    if META_SIGNALS
        .iter()
        .any(|signal| target.contains(signal) && !source.contains(signal))
    {
        return Err(OutputGuardReason::MetaReply);
    }
    let cap = input.chars().count().saturating_mul(2).saturating_add(24);
    // Language changes can expand text independently of cleanup.
    let cap = if translation {
        cap.saturating_mul(8) / 5
    } else {
        cap
    };
    let expanded = output.chars().count() > cap;
    let low_overlap = if !translation && question_or_imperative(input) {
        let source = content_words(input);
        let target = content_words(output);
        !source.is_empty()
            && (source.intersection(&target).count() as f64 / source.len() as f64)
                < MIN_CONTENT_OVERLAP
    } else {
        false
    };
    let answered_question = !translation
        && is_question(input)
        && !is_question(output)
        && !content_words(output).is_subset(&content_words(input));
    if expanded || low_overlap || answered_question {
        Err(OutputGuardReason::Answered)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signals_are_new_case_insensitive_and_whitespace_normalized() {
        for signal in META_SIGNALS {
            assert_eq!(check("hello", signal), Err(OutputGuardReason::MetaReply));
            assert!(check(signal, signal).is_ok());
        }
        assert!(check("I CANNOT\n join the call.", "I cannot join the call.").is_ok());
        assert_eq!(
            check("hello", "I'm ready to\nclean up voice dictation."),
            Err(OutputGuardReason::MetaReply)
        );
    }
    #[test]
    fn overlap_and_unicode_expansion_boundaries() {
        assert!(check("can you send the draft", "Can you send the draft?").is_ok());
        assert_eq!(
            check("translate this to French", "Bonjour."),
            Err(OutputGuardReason::Answered)
        );
        assert!(check("hello", &"é".repeat(34)).is_ok());
        assert_eq!(
            check("hello", &"é".repeat(35)),
            Err(OutputGuardReason::Answered)
        );
        assert!(check("please send draft tomorrow", "Send draft.").is_ok());
        assert_eq!(
            check("please send draft tomorrow", "Draft."),
            Err(OutputGuardReason::Answered)
        );
    }
}
