//! Opt-in voice commands: new line, new paragraph, undo that, delete that,
//! select all, send.
//!
//! The rule: a dictation is a command only when the command is everything the
//! user said in that dictation. Press the hotkey, say "new line", stop. The
//! same words inside a longer dictation ("add a new line to the report", "send
//! it to Anna") are always typed. Recognizer punctuation and capitals around
//! the phrase are ignored ("New line.", "Send!").
//!
//! Why this rule: the master plan says commands get their own trigger and that
//! we never guess command versus dictation from the words. A separate
//! dictation is that trigger: the user marks the command's start and end with
//! the hotkey they already hold, so nothing is inferred from a sentence, and
//! commands stay usable in the middle of dictating without a second hotkey or
//! a mode. A pause inside one dictation was rejected as a boundary because the
//! final transcript carries no pause information on most engines.
//!
//! The spoken punctuation escape word keeps a command as words: "literal send"
//! types "send". Line and paragraph phrases come from the spoken punctuation
//! table of the transcript language; the other four are English.

use super::spoken_punctuation::{apply_spoken_punctuation, break_for_phrase, escape_word};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VoiceCommand {
    NewLine,
    NewParagraph,
    UndoThat,
    DeleteThat,
    SelectAll,
    Send,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Utterance {
    /// The whole dictation is this command; nothing is typed.
    Command(VoiceCommand),
    /// An escaped command ("literal send"): type this text instead.
    Literal(String),
    Dictation,
}

const ENGLISH_ESCAPE: &str = "literal";

fn command_for(phrase: &str, transcript_language: Option<&str>) -> Option<VoiceCommand> {
    match phrase {
        "undo that" => return Some(VoiceCommand::UndoThat),
        "delete that" | "scratch that" => return Some(VoiceCommand::DeleteThat),
        "select all" => return Some(VoiceCommand::SelectAll),
        "send" => return Some(VoiceCommand::Send),
        _ => {}
    }
    // An unknown transcript language uses the English phrases.
    let symbol = break_for_phrase(phrase, transcript_language)
        .or_else(|| break_for_phrase(phrase, Some("en")))?;
    Some(if symbol == "\n" {
        VoiceCommand::NewLine
    } else {
        VoiceCommand::NewParagraph
    })
}

/// Decide what one finished dictation is. `spoken_punctuation` says whether
/// that stage will run on the text afterwards, so an escaped phrase it owns is
/// left for it to unescape.
pub(crate) fn classify_utterance(
    text: &str,
    transcript_language: Option<&str>,
    spoken_punctuation: bool,
) -> Utterance {
    let spoken = text.trim_matches(|ch: char| !ch.is_alphanumeric());
    let separator = |ch: char| ch.is_whitespace() || ch == ',';
    // Each word with its byte offset in `spoken`.
    let offsets: Vec<(usize, &str)> = spoken
        .split(separator)
        .filter(|word| !word.is_empty())
        .map(|word| (word.as_ptr() as usize - spoken.as_ptr() as usize, word))
        .collect();
    let words: Vec<String> = offsets
        .iter()
        .map(|(_, word)| word.to_lowercase())
        .collect();
    // No command is longer than a handful of words; skip real dictations early.
    if words.is_empty() || words.len() > 6 {
        return Utterance::Dictation;
    }
    if let Some(command) = command_for(&words.join(" "), transcript_language) {
        return Utterance::Command(command);
    }

    let escaped = words[0] == ENGLISH_ESCAPE
        || escape_word(transcript_language).is_some_and(|escape| words[0] == escape.to_lowercase());
    if !escaped || command_for(&words[1..].join(" "), transcript_language).is_none() {
        return Utterance::Dictation;
    }
    if spoken_punctuation && apply_spoken_punctuation(text, transcript_language).is_some() {
        return Utterance::Dictation;
    }
    let rest = &spoken[offsets[1].0..];
    let mut literal = String::with_capacity(rest.len());
    let mut chars = rest.chars();
    if let Some(first) = chars.next() {
        if spoken.starts_with(char::is_uppercase) {
            literal.extend(first.to_uppercase());
        } else {
            literal.push(first);
        }
    }
    literal.push_str(chars.as_str());
    Utterance::Literal(literal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(text: &str) -> Utterance {
        classify_utterance(text, Some("en"), false)
    }

    #[test]
    fn a_command_said_alone_runs_whatever_the_recognizer_wrapped_it_in() {
        for (said, command) in [
            ("new line", VoiceCommand::NewLine),
            ("New line.", VoiceCommand::NewLine),
            (" New paragraph. ", VoiceCommand::NewParagraph),
            ("Undo that.", VoiceCommand::UndoThat),
            ("Delete that!", VoiceCommand::DeleteThat),
            ("Scratch that.", VoiceCommand::DeleteThat),
            ("Select all.", VoiceCommand::SelectAll),
            ("Select, all", VoiceCommand::SelectAll),
            ("Send.", VoiceCommand::Send),
            ("SEND", VoiceCommand::Send),
        ] {
            assert_eq!(classify(said), Utterance::Command(command), "{said}");
        }
    }

    #[test]
    fn the_same_words_inside_a_sentence_are_dictation() {
        for said in [
            "Please send the report to Anna.",
            "Send it.",
            "Send.  Thanks.",
            "Add a new line to the report.",
            "New line of products.",
            "I had to undo that change.",
            "Undo that, please.",
            "Do not delete that file.",
            "Select all the rows and copy them.",
            "We need a new paragraph here.",
            "Sending.",
            "",
            "...",
        ] {
            assert_eq!(classify(said), Utterance::Dictation, "{said}");
        }
    }

    #[test]
    fn the_escape_word_types_the_command_as_words() {
        assert_eq!(classify("Literal send."), Utterance::Literal("Send".into()));
        assert_eq!(
            classify("literal select all"),
            Utterance::Literal("select all".into())
        );
        assert_eq!(classify("Literal meaning."), Utterance::Dictation);
        assert_eq!(classify("literal,send"), Utterance::Literal("send".into()));
        assert_eq!(
            classify("Literal, new line"),
            Utterance::Literal("New line".into())
        );
        // Spoken punctuation owns "new line" and unescapes it itself.
        assert_eq!(
            classify_utterance("Literal new line.", Some("en"), true),
            Utterance::Dictation
        );
        assert_eq!(
            classify_utterance("Literal new line.", Some("en"), false),
            Utterance::Literal("New line".into())
        );
        assert_eq!(
            classify_utterance("Literal send.", Some("en"), true),
            Utterance::Literal("Send".into())
        );
    }

    #[test]
    fn line_and_paragraph_follow_the_transcript_language() {
        assert_eq!(
            classify_utterance("Neue Zeile.", Some("de"), false),
            Utterance::Command(VoiceCommand::NewLine)
        );
        assert_eq!(
            classify_utterance("Nouveau paragraphe", Some("fr-CA"), false),
            Utterance::Command(VoiceCommand::NewParagraph)
        );
        // German words are not commands in an English transcript.
        assert_eq!(
            classify_utterance("Neue Zeile.", Some("en"), false),
            Utterance::Dictation
        );
        // Engines that report no language still get the English set.
        assert_eq!(
            classify_utterance("New line.", None, false),
            Utterance::Command(VoiceCommand::NewLine)
        );
        assert_eq!(
            classify_utterance("wörtlich neue Zeile", Some("de"), false),
            Utterance::Literal("neue Zeile".into())
        );
    }
}
