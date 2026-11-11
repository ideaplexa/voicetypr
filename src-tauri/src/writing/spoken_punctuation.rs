//! Opt-in German spoken punctuation commands.
//!
//! Recognizers write a dictated command such as "Strichpunkt" as a word. When
//! the user enables this stage and the transcript is German, whole-word
//! commands become their symbols before Corrections and Polish run, so it works
//! offline with Polish off. Only command names that are not ordinary German
//! prose are matched; ambiguous words such as "Komma", "Punkt" or a bare
//! "Anführungszeichen" stay literal (plan 085 owns their boundary rules).

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};

/// How a symbol joins the surrounding words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// `, ; : ? !` attach to the previous word; one space follows.
    AfterWord,
    /// Line or paragraph break; the next word starts the new line.
    Break,
    /// `( [ { „` attach to the next word.
    Opening,
    /// `) ] } “` attach to the previous word.
    Closing,
    /// `– § % & # * + - =` stand alone with one space on each side.
    Spaced,
    /// `- / \` join the neighbouring words without spaces.
    Joining,
}

struct Command {
    phrases: &'static [&'static str],
    symbol: &'static str,
    placement: Placement,
}

const GERMAN_COMMANDS: &[Command] = &[
    Command {
        phrases: &["Beistrich"],
        symbol: ",",
        placement: Placement::AfterWord,
    },
    Command {
        phrases: &["Strichpunkt", "Semikolon"],
        symbol: ";",
        placement: Placement::AfterWord,
    },
    Command {
        phrases: &["Doppelpunkt"],
        symbol: ":",
        placement: Placement::AfterWord,
    },
    Command {
        phrases: &["Fragezeichen"],
        symbol: "?",
        placement: Placement::AfterWord,
    },
    Command {
        phrases: &["Ausrufezeichen", "Ausrufzeichen", "Rufzeichen"],
        symbol: "!",
        placement: Placement::AfterWord,
    },
    Command {
        phrases: &["neue Zeile"],
        symbol: "\n",
        placement: Placement::Break,
    },
    Command {
        phrases: &["neuer Absatz"],
        symbol: "\n\n",
        placement: Placement::Break,
    },
    Command {
        phrases: &[
            "Anführungszeichen auf",
            "Anführungszeichen unten",
            "Ausführungszeichen auf",
            "Ausführungszeichen unten",
        ],
        symbol: "„",
        placement: Placement::Opening,
    },
    Command {
        phrases: &[
            "Anführungszeichen zu",
            "Anführungszeichen oben",
            "Ausführungszeichen zu",
            "Ausführungszeichen oben",
        ],
        symbol: "“",
        placement: Placement::Closing,
    },
    Command {
        phrases: &["Klammer auf"],
        symbol: "(",
        placement: Placement::Opening,
    },
    Command {
        phrases: &["Klammer zu"],
        symbol: ")",
        placement: Placement::Closing,
    },
    Command {
        phrases: &["eckige Klammer auf"],
        symbol: "[",
        placement: Placement::Opening,
    },
    Command {
        phrases: &["eckige Klammer zu"],
        symbol: "]",
        placement: Placement::Closing,
    },
    Command {
        phrases: &["geschweifte Klammer auf"],
        symbol: "{",
        placement: Placement::Opening,
    },
    Command {
        phrases: &["geschweifte Klammer zu"],
        symbol: "}",
        placement: Placement::Closing,
    },
    Command {
        phrases: &["Gedankenstrich"],
        symbol: "–",
        placement: Placement::Spaced,
    },
    Command {
        phrases: &["Paragraphzeichen", "Paragrafzeichen", "Paragraphenzeichen"],
        symbol: "§",
        placement: Placement::Spaced,
    },
    Command {
        phrases: &["Prozentzeichen"],
        symbol: "%",
        placement: Placement::Spaced,
    },
    Command {
        phrases: &["Und-Zeichen", "Undzeichen", "Ampersand"],
        symbol: "&",
        placement: Placement::Spaced,
    },
    Command {
        phrases: &["Rautezeichen", "Raute"],
        symbol: "#",
        placement: Placement::Spaced,
    },
    Command {
        phrases: &["Sternchen"],
        symbol: "*",
        placement: Placement::Spaced,
    },
    Command {
        phrases: &["Pluszeichen"],
        symbol: "+",
        placement: Placement::Spaced,
    },
    Command {
        phrases: &["Minuszeichen"],
        symbol: "-",
        placement: Placement::Spaced,
    },
    Command {
        phrases: &["Gleichheitszeichen", "Istgleichzeichen"],
        symbol: "=",
        placement: Placement::Spaced,
    },
    Command {
        phrases: &["Bindestrich"],
        symbol: "-",
        placement: Placement::Joining,
    },
    Command {
        phrases: &["Schrägstrich"],
        symbol: "/",
        placement: Placement::Joining,
    },
    Command {
        phrases: &["umgekehrter Schrägstrich", "Backslash"],
        symbol: "\\",
        placement: Placement::Joining,
    },
];

/// Marks a recognizer adds on its own around a spoken command word.
const ENGINE_MARKS: &[char] = &[',', '.', ';', ':', '!', '?'];
/// Marks that cannot sensibly precede a mark or break the user dictated.
const CLAUSE_MARKS: &[char] = &[',', ';', ':'];
/// Characters that attach to the preceding word, so no space goes before them.
const ATTACHES_LEFT: &[char] = &[
    ',', '.', ';', ':', '!', '?', ')', ']', '}', '“', '”', '"', '\'', '’', '…',
];
const OPENING_SYMBOLS: &[char] = &['(', '[', '{', '„'];

struct Matcher {
    regex: Regex,
    commands: HashMap<String, &'static Command>,
}

fn normalize_phrase(phrase: &str) -> String {
    phrase
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

static MATCHER: LazyLock<Matcher> = LazyLock::new(build_matcher);

fn build_matcher() -> Matcher {
    let mut phrases: Vec<(&'static str, &'static Command)> = GERMAN_COMMANDS
        .iter()
        .flat_map(|command| command.phrases.iter().map(move |phrase| (*phrase, command)))
        .collect();
    // Longest first, so "eckige Klammer auf" wins over "Klammer auf".
    phrases.sort_by_key(|(phrase, _)| std::cmp::Reverse(phrase.chars().count()));
    let pattern = phrases
        .iter()
        .map(|(phrase, _)| {
            phrase
                .split_whitespace()
                .map(regex::escape)
                .collect::<Vec<_>>()
                .join(r"\s+")
        })
        .collect::<Vec<_>>()
        .join("|");
    let regex = RegexBuilder::new(&format!("(?:{pattern})"))
        .case_insensitive(true)
        .build()
        .expect("spoken punctuation pattern is valid");
    let commands = phrases
        .into_iter()
        .map(|(phrase, command)| (normalize_phrase(phrase), command))
        .collect();
    Matcher { regex, commands }
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-'
}

/// A command must be a whole word: "Strichpunkte" or "Doppelpunkt-Regel" stay literal.
fn is_whole_word(text: &str, start: usize, end: usize) -> bool {
    let left_ok = text[..start]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_word_char(ch));
    let right_ok = text[end..]
        .chars()
        .next()
        .is_none_or(|ch| !is_word_char(ch));
    left_ok && right_ok
}

fn is_german(transcript_language: Option<&str>) -> bool {
    transcript_language
        .and_then(|language| language.split(['-', '_']).next())
        .is_some_and(|base| base.trim().eq_ignore_ascii_case("de"))
}

fn trim_trailing_spaces(out: &mut String) {
    let kept = out.trim_end_matches([' ', '\t']).len();
    out.truncate(kept);
}

/// Remove spaces and one recognizer-added mark from `marks` before a symbol.
fn drop_preceding_mark(out: &mut String, marks: &[char]) {
    trim_trailing_spaces(out);
    if out
        .chars()
        .next_back()
        .is_some_and(|ch| marks.contains(&ch))
    {
        out.pop();
        trim_trailing_spaces(out);
    }
}

fn skip_spaces(text: &str, mut pos: usize) -> usize {
    while let Some(ch) = text[pos..].chars().next() {
        if ch != ' ' && ch != '\t' {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

/// Skip spaces, then one recognizer-added mark from `marks`, then spaces.
fn skip_following_mark(text: &str, pos: usize, marks: &[char]) -> usize {
    let pos = skip_spaces(text, pos);
    match text[pos..].chars().next() {
        Some(ch) if marks.contains(&ch) => skip_spaces(text, pos + ch.len_utf8()),
        _ => pos,
    }
}

fn push_space_before_symbol(out: &mut String) {
    if out
        .chars()
        .next_back()
        .is_some_and(|ch| ch != '\n' && !OPENING_SYMBOLS.contains(&ch))
    {
        out.push(' ');
    }
}

fn push_space_before_next_word(out: &mut String, text: &str, pos: usize) {
    if text[pos..]
        .chars()
        .next()
        .is_some_and(|ch| ch != '\n' && !ATTACHES_LEFT.contains(&ch))
    {
        out.push(' ');
    }
}

/// Write `command` to `out` and return where copying the transcript resumes.
fn render(out: &mut String, text: &str, after: usize, command: &Command) -> usize {
    match command.placement {
        Placement::AfterWord => {
            let sentence_end = matches!(command.symbol, "?" | "!");
            drop_preceding_mark(
                out,
                if sentence_end {
                    &[',', ';', ':', '.']
                } else {
                    CLAUSE_MARKS
                },
            );
            out.push_str(command.symbol);
            let pos = skip_following_mark(text, after, ENGINE_MARKS);
            push_space_before_next_word(out, text, pos);
            pos
        }
        Placement::Break => {
            drop_preceding_mark(out, CLAUSE_MARKS);
            out.push_str(command.symbol);
            skip_following_mark(text, after, &[',', '.', ';', ':'])
        }
        Placement::Opening => {
            trim_trailing_spaces(out);
            push_space_before_symbol(out);
            out.push_str(command.symbol);
            skip_following_mark(text, after, &[',', '.', ';', ':'])
        }
        Placement::Closing => {
            drop_preceding_mark(out, CLAUSE_MARKS);
            out.push_str(command.symbol);
            let pos = skip_spaces(text, after);
            push_space_before_next_word(out, text, pos);
            pos
        }
        Placement::Spaced => {
            drop_preceding_mark(out, &[',']);
            push_space_before_symbol(out);
            out.push_str(command.symbol);
            let pos = skip_following_mark(text, after, &[',']);
            push_space_before_next_word(out, text, pos);
            pos
        }
        Placement::Joining => {
            trim_trailing_spaces(out);
            out.push_str(command.symbol);
            skip_spaces(text, after)
        }
    }
}

/// Replace German spoken punctuation commands with their symbols.
///
/// Returns `None` when the transcript is not German or contains no command.
pub(crate) fn apply_spoken_punctuation(
    text: &str,
    transcript_language: Option<&str>,
) -> Option<String> {
    if !is_german(transcript_language) {
        return None;
    }
    let matcher = &*MATCHER;
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut applied = false;
    for found in matcher.regex.find_iter(text) {
        if found.start() < cursor || !is_whole_word(text, found.start(), found.end()) {
            continue;
        }
        let Some(command) = matcher.commands.get(&normalize_phrase(found.as_str())) else {
            continue;
        };
        out.push_str(&text[cursor..found.start()]);
        cursor = render(&mut out, text, found.end(), command);
        applied = true;
    }
    if !applied {
        return None;
    }
    out.push_str(&text[cursor..]);
    Some(out)
}
