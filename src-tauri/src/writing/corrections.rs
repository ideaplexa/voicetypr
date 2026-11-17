//! Finds the words a user fixed when they edited a dictation, so they can be
//! offered for the Dictionary. Pure text comparison: nothing here logs or stores.
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CorrectedWord {
    /// What the dictation contained.
    pub heard: String,
    /// What the user changed it to.
    pub word: String,
}

const MAX_CHANGED_WORDS: usize = 400;
const MAX_SPAN_WORDS: usize = 3;
const MAX_PHRASE_CHARS: usize = 60;
const MAX_PER_EDIT: usize = 5;
/// Below this the edit reads as a rewording ("good" → "great"), not a misheard word.
const MIN_SIMILARITY: f32 = 0.4;

struct Token<'a> {
    core: &'a str,
    starts_sentence: bool,
}

/// Same spelling, same bytes: composed accents and straight quotes.
fn normalized(text: &str) -> String {
    text.nfc()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{02bc}' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            other => other,
        })
        .collect()
}

fn tokens(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    for line in text.lines() {
        let mut starts_sentence = true;
        for raw in line.split_whitespace() {
            let core = raw.trim_matches(|c: char| !(c.is_alphanumeric() || is_combining_mark(c)));
            if core.is_empty() {
                continue;
            }
            tokens.push(Token {
                core,
                starts_sentence,
            });
            starts_sentence = raw
                .trim_end_matches(['"', '\'', '”', '’', ')', ']'])
                .ends_with(['.', '!', '?']);
        }
    }
    tokens
}

fn joined(tokens: &[Token<'_>]) -> String {
    tokens
        .iter()
        .map(|token| token.core)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A single ordinary lowercase word replaced by another ("remote" → "remove",
/// "increase" → "decrease") is a typo or a change of mind, and as a global rule
/// it would rewrite that real word everywhere. Names, jargon and split words are
/// what the Dictionary is for.
fn looks_like_a_term(heard: &[Token<'_>], word: &[Token<'_>]) -> bool {
    heard.len() > 1
        || word.len() > 1
        || word.iter().any(|token| {
            token
                .core
                .chars()
                .any(|c| c.is_uppercase() || c.is_numeric())
        })
}

fn similarity(left: &str, right: &str) -> f32 {
    let letters = |text: &str| -> Vec<char> {
        text.chars()
            .filter(|c| c.is_alphanumeric() || is_combining_mark(*c))
            .flat_map(char::to_lowercase)
            .collect()
    };
    let (left, right) = (letters(left), letters(right));
    let longest = left.len().max(right.len());
    if longest == 0 {
        return 0.0;
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    for (i, a) in left.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, b) in right.iter().enumerate() {
            current.push(
                (previous[j] + usize::from(a != b))
                    .min(previous[j + 1] + 1)
                    .min(current[j] + 1),
            );
        }
        previous = current;
    }
    1.0 - previous[right.len()] as f32 / longest as f32
}

/// The engine can merge a neighbouring word into the misheard term ("in
/// Voicetypr" heard as "invoice typer"). The neighbour is not part of the term,
/// so it leaves the fix and its sound leaves what was heard.
fn without_merged_neighbour<'t, 'a>(
    heard: &[Token<'_>],
    word: &'t [Token<'a>],
) -> (String, &'t [Token<'a>]) {
    let ordinary = |token: &Token<'_>| token.core.chars().all(char::is_lowercase);
    let names_a_term = |tokens: &[Token<'_>]| {
        tokens
            .iter()
            .any(|t| t.core.chars().any(|c| c.is_uppercase() || c.is_numeric()))
    };
    let cores = |tokens: &[Token<'_>]| {
        tokens
            .iter()
            .map(|t| t.core.to_string())
            .collect::<Vec<_>>()
    };
    if word.len() >= 2 && ordinary(&word[0]) && names_a_term(&word[1..]) {
        let first = heard[0].core.to_lowercase();
        if let Some(rest) = first.strip_prefix(&word[0].core.to_lowercase()) {
            if rest.chars().count() >= 2 {
                let mut parts = vec![rest.to_string()];
                parts.extend(cores(&heard[1..]));
                return (parts.join(" "), &word[1..]);
            }
        }
    }
    let end = word.len().saturating_sub(1);
    if word.len() >= 2 && ordinary(&word[end]) && names_a_term(&word[..end]) {
        let last = heard[heard.len() - 1].core.to_lowercase();
        if let Some(rest) = last.strip_suffix(&word[end].core.to_lowercase()) {
            if rest.chars().count() >= 2 {
                let mut parts = cores(&heard[..heard.len() - 1]);
                parts.push(rest.to_string());
                return (parts.join(" "), &word[..end]);
            }
        }
    }
    (joined(heard), word)
}

fn candidate(heard: &[Token<'_>], word: &[Token<'_>]) -> Option<CorrectedWord> {
    if heard.is_empty()
        || word.is_empty()
        || heard.len() > MAX_SPAN_WORDS
        || word.len() > MAX_SPAN_WORDS
    {
        return None;
    }
    let (heard_text, word) = without_merged_neighbour(heard, word);
    let word_text = joined(word);
    let word_chars = word_text.chars().count();
    if !(2..=MAX_PHRASE_CHARS).contains(&word_chars)
        || !(2..=MAX_PHRASE_CHARS).contains(&heard_text.chars().count())
        || !word_text.chars().any(char::is_alphabetic)
    {
        return None;
    }
    let keep = if heard_text.to_lowercase() == word_text.to_lowercase() {
        // Only the capitals changed. "the" → "The" at a sentence start is ordinary
        // editing; "github" → "GitHub" or a mid-sentence "moin" → "Moin" is a name.
        heard_text.split(' ').zip(word).any(|(old, new)| {
            old != new.core
                && (new.core.chars().skip(1).any(char::is_uppercase)
                    || (!new.starts_sentence
                        && new.core.chars().next().is_some_and(char::is_uppercase)))
        })
    } else {
        looks_like_a_term(heard, word) && similarity(&heard_text, &word_text) >= MIN_SIMILARITY
    };
    keep.then_some(CorrectedWord {
        heard: heard_text,
        word: word_text,
    })
}

/// Words the user replaced between `before` and `after`, in reading order.
/// Added or removed text, long rewrites, punctuation and ordinary-word changes
/// are ignored.
pub(crate) fn corrected_words(before: &str, after: &str) -> Vec<CorrectedWord> {
    let (before, after) = (normalized(before), normalized(after));
    let (before, after) = (tokens(&before), tokens(&after));
    let prefix = before
        .iter()
        .zip(&after)
        .take_while(|(a, b)| a.core == b.core)
        .count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(a, b)| a.core == b.core)
        .count();
    let old = &before[prefix..before.len() - suffix];
    let new = &after[prefix..after.len() - suffix];
    if old.len() > MAX_CHANGED_WORDS || new.len() > MAX_CHANGED_WORDS {
        return Vec::new();
    }

    // common[i * width + j]: longest common run of words in old[i..] and new[j..].
    let width = new.len() + 1;
    let mut common = vec![0u16; (old.len() + 1) * width];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            common[i * width + j] = if old[i].core == new[j].core {
                common[(i + 1) * width + j + 1] + 1
            } else {
                common[(i + 1) * width + j].max(common[i * width + j + 1])
            };
        }
    }

    let mut found: Vec<CorrectedWord> = Vec::new();
    let mut push = |heard: &[Token<'_>], word: &[Token<'_>]| {
        if let Some(candidate) = candidate(heard, word) {
            if found.len() < MAX_PER_EDIT && !found.contains(&candidate) {
                found.push(candidate);
            }
        }
    };
    let (mut i, mut j, mut start_i, mut start_j) = (0, 0, 0, 0);
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i].core == new[j].core {
            push(&old[start_i..i], &new[start_j..j]);
            i += 1;
            j += 1;
            (start_i, start_j) = (i, j);
        } else if i < old.len()
            && (j == new.len() || common[(i + 1) * width + j] >= common[i * width + j + 1])
        {
            i += 1;
        } else {
            j += 1;
        }
    }
    push(&old[start_i..], &new[start_j..]);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(before: &str, after: &str) -> Vec<(String, String)> {
        corrected_words(before, after)
            .into_iter()
            .map(|c| (c.heard, c.word))
            .collect()
    }

    fn pair(heard: &str, word: &str) -> (String, String) {
        (heard.to_string(), word.to_string())
    }

    #[test]
    fn a_fixed_name_is_offered_with_what_was_heard() {
        assert_eq!(
            pairs(
                "I pushed the fix to voice typer and asked cloud code to review it.",
                "I pushed the fix to Voicetypr and asked Claude Code to review it."
            ),
            vec![
                pair("voice typer", "Voicetypr"),
                pair("cloud code", "Claude Code")
            ]
        );
    }

    #[test]
    fn a_neighbouring_word_the_engine_merged_is_not_learned() {
        assert_eq!(
            pairs(
                "It works like invoice typer for me.",
                "It works like in Voicetypr for me."
            ),
            vec![pair("voice typer", "Voicetypr")]
        );
        assert_eq!(
            pairs("push it to git hubat noon", "push it to GitHub at noon"),
            vec![pair("git hub", "GitHub")]
        );
        assert_eq!(
            pairs("ask ingithub today", "ask in GitHub today"),
            vec![pair("github", "GitHub")]
        );
    }

    #[test]
    fn punctuation_and_sentence_capitals_are_not_words_to_learn() {
        assert!(pairs("hello there. thanks", "Hello there! Thanks").is_empty());
        assert!(pairs("so, the plan", "So the plan.").is_empty());
    }

    #[test]
    fn a_name_capitalised_mid_sentence_or_inside_the_word_is_offered() {
        assert_eq!(
            pairs("push it to github for moin", "push it to GitHub for Moin"),
            vec![pair("github", "GitHub"), pair("moin", "Moin")]
        );
    }

    #[test]
    fn rewording_added_and_removed_text_are_ignored() {
        assert!(pairs("that was a good idea", "that was a great idea").is_empty());
        assert!(pairs("we should increase it", "we should decrease it").is_empty());
        assert!(pairs("remote the file", "remove the file").is_empty());
        assert!(pairs("I\u{2019}m sure", "I'm sure").is_empty());
        assert!(pairs("send it today", "send it to the team today").is_empty());
        assert!(pairs("um send it today please", "send it today").is_empty());
        assert!(pairs(
            "we should probably think about it later",
            "let us revisit this next week instead"
        )
        .is_empty());
    }

    #[test]
    fn accents_and_combining_marks_stay_part_of_the_word() {
        assert_eq!(
            pairs("ask jose today", "ask Jose\u{301} today"),
            vec![pair("jose", "Jos\u{e9}")]
        );
        assert!(pairs("Jose\u{301} said", "Jos\u{e9} said").is_empty());
        let tamil = "\u{b95}\u{bae}\u{bb2}\u{bcd}";
        assert_eq!(tokens(&format!("{tamil}.")).first().unwrap().core, tamil);
    }

    #[test]
    fn unchanged_text_offers_nothing_and_one_edit_offers_at_most_five() {
        assert!(pairs("same text", "same text").is_empty());
        let before = "aaab x bbbc x cccd x ddde x eeef x fffg x ggga";
        let after = "aaaB x bbbC x cccD x dddE x eeeF x fffG x gggA";
        assert_eq!(pairs(before, after).len(), 5);
    }
}
