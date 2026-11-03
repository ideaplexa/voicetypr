//! Soniox tokens own their spacing; bare fragments are not separate words.

use crate::transcription::TranscriptionWord;

pub(super) fn text(tokens: &[serde_json::Value]) -> String {
    tokens
        .iter()
        .filter_map(|token| token.get("text").and_then(serde_json::Value::as_str))
        .collect()
}

/// Coalesce subword pieces before the provider-neutral speaker renderer sees
/// them. Deepgram supplies whole bare words; Soniox supplies pre-spaced pieces.
/// Preserve speaker boundaries and extend each word's timing through its tail.
pub(super) fn words(tokens: impl Iterator<Item = TranscriptionWord>) -> Vec<TranscriptionWord> {
    let mut words: Vec<TranscriptionWord> = Vec::new();
    for token in tokens {
        if token.text.is_empty() {
            continue;
        }
        if let Some(word) = words.last_mut() {
            let same_speaker = word.speaker_id == token.speaker_id
                || word.speaker_id.is_none()
                || token.speaker_id.is_none();
            if same_speaker && !token.text.starts_with(char::is_whitespace) {
                word.text.push_str(&token.text);
                word.start_ms = word.start_ms.or(token.start_ms);
                word.end_ms = token.end_ms.or(word.end_ms);
                word.speaker_id = word.speaker_id.take().or(token.speaker_id);
                continue;
            }
        }
        words.push(token);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(text: &str, start: u64, end: u64, speaker: Option<&str>) -> TranscriptionWord {
        TranscriptionWord {
            text: text.into(),
            start_ms: Some(start),
            end_ms: Some(end),
            speaker_id: speaker.map(str::to_owned),
            confidence: None,
        }
    }

    #[test]
    fn real_shaped_subwords_keep_spacing_and_word_timing() {
        let tokens = [
            ("my", 0, 10),
            (" f", 10, 20),
            ("ellow", 20, 30),
            (" Amer", 30, 40),
            ("ic", 40, 50),
            ("ans", 50, 60),
            (".", 60, 70),
        ];
        let json: Vec<_> = tokens
            .iter()
            .map(|(text, _, _)| serde_json::json!({"text":text}))
            .collect();
        assert_eq!(text(&json), "my fellow Americans.");
        let words = words(
            tokens
                .into_iter()
                .map(|(text, start, end)| token(text, start, end, Some("Speaker 1"))),
        );
        assert_eq!(words.len(), 3);
        assert_eq!(words[1].start_ms, Some(10));
        assert_eq!(words[1].end_ms, Some(30));
        assert_eq!(words[2].end_ms, Some(70));
        assert_eq!(
            crate::commands::audio::group_words_into_speaker_text(&words),
            "Speaker 1: my fellow Americans."
        );
    }

    #[test]
    fn speaker_changes_split_but_untagged_fragments_continue() {
        let words = words(
            [
                token("Hel", 0, 10, Some("Speaker 1")),
                token("lo", 10, 20, None),
                token("Bye", 20, 30, Some("Speaker 2")),
                token(".", 30, 40, Some("Speaker 2")),
            ]
            .into_iter(),
        );
        assert_eq!(words.len(), 2);
        assert_eq!(
            crate::commands::audio::group_words_into_speaker_text(&words),
            "Speaker 1: Hello\n\nSpeaker 2: Bye."
        );
    }
}
