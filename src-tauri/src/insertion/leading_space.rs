//! Best-effort insertion-boundary spacing. Context never leaves this module.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(any(target_os = "macos", target_os = "windows", test))]
mod worker;

pub(crate) fn prepare(text: String) -> String {
    // Don't query accessibility at all for output that cannot need spacing.
    if !starts_word(&text) {
        return text;
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let previous = {
        static WORKER: std::sync::LazyLock<worker::QueryWorker> =
            std::sync::LazyLock::new(worker::QueryWorker::default);
        #[cfg(target_os = "macos")]
        let query = macos::previous_character;
        #[cfg(target_os = "windows")]
        let query = windows::previous_character;
        WORKER.read(query, std::time::Duration::from_millis(150))
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let previous = None;

    if needs_leading_space(previous, &text) {
        format!(" {text}")
    } else {
        text
    }
}

fn starts_word(text: &str) -> bool {
    text.chars().next().is_some_and(|first| {
        !is_cjk(first)
            && (first.is_alphanumeric()
                || matches!(first, '(' | '[' | '{' | '"' | '\'' | '“' | '‘' | '«'))
    })
}

fn needs_leading_space(previous: Option<char>, text: &str) -> bool {
    previous.is_some_and(|previous| {
        !previous.is_whitespace()
            && !is_cjk(previous)
            && !matches!(
                previous,
                '(' | '[' | '{' | '"' | '\'' | '“' | '‘' | '«' | '/' | '-'
            )
    }) && starts_word(text)
}

fn is_cjk(c: char) -> bool {
    // No inserted spaces adjacent to Han, kana, hangul or CJK/fullwidth
    // punctuation. This deliberately simple script-range rule is not language
    // detection; mixed Latin/CJK text keeps its existing boundaries.
    matches!(c as u32,
        0x1100..=0x11ff | 0x2e80..=0x30ff | 0x3100..=0x318f |
        0x31a0..=0x31ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff |
        0xa960..=0xa97f | 0xac00..=0xd7ff | 0xf900..=0xfaff |
        0xfe10..=0xfe1f | 0xfe30..=0xfe4f | 0xff00..=0xffef |
        0x20000..=0x323af)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_and_line_starts_or_unknown_context_are_unchanged() {
        for previous in [
            None,
            Some(' '),
            Some('\n'),
            Some('\r'),
            Some('\t'),
            Some('\u{a0}'),
            Some('\u{3000}'),
        ] {
            assert!(!needs_leading_space(previous, "The silver fox."));
        }
    }

    #[test]
    fn words_digits_and_sentence_closers_need_a_boundary() {
        for previous in ['o', '.', '!', '?', ')', ']', '}', '”', '’', '9', 'é'] {
            for text in [
                "The silver fox.",
                "123",
                "éclair",
                "(hello)",
                "[hello]",
                "{hello}",
                "\"Hello\"",
                "'Hello'",
                "“Hello”",
                "‘Hello’",
                "«Hello»",
            ] {
                assert!(
                    needs_leading_space(Some(previous), text),
                    "{previous:?} {text:?}"
                );
            }
        }
    }

    #[test]
    fn opening_delimiters_and_joiners_never_add_a_space() {
        for previous in ['(', '[', '{', '"', '\'', '“', '‘', '«', '/', '-'] {
            assert!(!needs_leading_space(Some(previous), "Hello"));
        }
    }

    #[test]
    fn punctuation_empty_and_already_spaced_output_are_unchanged() {
        for text in [
            "",
            " hello",
            "\thello",
            "\nHello",
            "\u{a0}Hello",
            ", hello",
            ". Hello",
            "? Hello",
            "! Hello",
            ") hello",
            ": hello",
            "; hello",
            "] hello",
            "} hello",
            "/path",
            "-word",
            "🙂",
        ] {
            assert!(!needs_leading_space(Some('o'), text), "{text:?}");
        }
    }

    #[test]
    fn cjk_boundaries_are_not_westernized() {
        for c in ['中', 'あ', 'ア', '한', '。', '，', 'Ａ', 'ｶ', '𠀀'] {
            assert!(!needs_leading_space(Some(c), "Hello"));
            assert!(!needs_leading_space(Some('o'), &format!("{c}world")));
        }
    }
}
