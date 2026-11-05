use super::needs_leading_space;

struct Fixture {
    id: &'static str,
    input: &'static str,
    language: &'static str,
    engine: &'static str,
    flags: &'static str,
    previous: Option<char>,
    expected: &'static str,
}

#[test]
fn korean_word_boundaries_preserve_words_and_existing_spaces() {
    let cases = [
        ("korean", Some('은'), "좋은 날입니다.", " 좋은 날입니다."),
        ("korean-latin", Some('은'), "Rust 코드", " Rust 코드"),
        ("latin-korean", Some('t'), "한국어 문장", " 한국어 문장"),
        ("decomposed", Some('ᆫ'), "가나", " 가나"),
        ("compatibility", Some('ㄴ'), "ㄱㄴ", " ㄱㄴ"),
        ("extended-a", Some('\u{a960}'), "한글", " 한글"),
        ("extended-b", Some('\u{d7cb}'), "한글", " 한글"),
        ("halfwidth", Some('\u{ffa1}'), "\u{ffa2}", " \u{ffa2}"),
        ("tone-mark", Some('\u{302e}'), "한글", " 한글"),
        ("already-spaced", Some('은'), " 좋은 날", " 좋은 날"),
        ("previous-space", Some(' '), "좋은 날", "좋은 날"),
        ("unknown-context", None, "좋은 날", "좋은 날"),
        ("punctuation", Some('은'), "! 좋은 날", "! 좋은 날"),
        ("opening", Some('('), "한글", "한글"),
        ("han-to-hangul", Some('中'), "한국어", "한국어"),
        ("hangul-to-han", Some('은'), "中文", "中文"),
        ("kana-to-hangul", Some('あ'), "한국어", "한국어"),
        ("hangul-to-kana", Some('은'), "かな", "かな"),
        ("english", Some('d'), "Hello world.", " Hello world."),
    ];
    for engine in ["whisper", "deepgram", "soniox", "fixture"] {
        for (id, previous, input, expected) in cases {
            let case = Fixture {
                id,
                previous,
                input,
                expected,
                language: "ko",
                engine,
                flags: "final,insertion",
            };
            let actual = if needs_leading_space(case.previous, case.input) {
                format!(" {}", case.input)
            } else {
                case.input.to_string()
            };
            assert_eq!(
                actual.as_bytes(),
                case.expected.as_bytes(),
                "{} {} {} {}",
                case.id,
                case.language,
                case.engine,
                case.flags
            );
            if actual.starts_with(' ') {
                assert!(!needs_leading_space(case.previous, &actual), "{}", case.id);
            }
        }
    }
}
