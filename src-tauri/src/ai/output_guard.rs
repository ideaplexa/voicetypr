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

pub fn check(input: &str, output: &str) -> Result<(), OutputGuardReason> {
    let source = normalize(input);
    let target = normalize(output);
    if META_SIGNALS
        .iter()
        .any(|signal| target.contains(signal) && !source.contains(signal))
    {
        return Err(OutputGuardReason::MetaReply);
    }
    let expanded =
        output.chars().count() > input.chars().count().saturating_mul(2).saturating_add(24);
    let low_overlap = if question_or_imperative(input) {
        let source = content_words(input);
        let target = content_words(output);
        !source.is_empty()
            && (source.intersection(&target).count() as f64 / source.len() as f64)
                < MIN_CONTENT_OVERLAP
    } else {
        false
    };
    if expanded || low_overlap {
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
