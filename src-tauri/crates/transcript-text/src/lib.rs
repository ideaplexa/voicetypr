use std::borrow::Cow;

/// Normalize mechanical dictation spacing: collapse ASCII space runs, drop space-before-punctuation.
/// Leaves newlines/tabs/non-breaking-spaces/non-ASCII intact. Returns [`Cow::Borrowed`] when no change needed.
/// Expects already-trimmed input: a leading space is dropped and a trailing space run collapses to one (not zero).
pub fn normalize_transcript_spacing(input: &str) -> Cow<'_, str> {
    if input.is_empty() || has_structural_guard(input) {
        return Cow::Borrowed(input);
    }

    let mut out = String::with_capacity(input.len());
    let mut pending_ascii_space = false;

    for ch in input.chars() {
        if ch == ' ' {
            pending_ascii_space = true;
            continue;
        }

        if pending_ascii_space {
            if !should_drop_space_before(ch) && !out.is_empty() {
                out.push(' ');
            }
            pending_ascii_space = false;
        }

        out.push(ch);
    }

    if pending_ascii_space && !out.is_empty() {
        out.push(' ');
    }

    if out == input {
        Cow::Borrowed(input)
    } else {
        Cow::Owned(out)
    }
}

/// Apply [`normalize_transcript_spacing`] to each `\n`-separated line and rejoin.
///
/// Every line keeps the single-line guards, so a code-like line is left alone
/// while its prose neighbours are normalised. Returns [`Cow::Borrowed`] when no
/// line changed. Single-line input behaves exactly like the single-line function.
pub fn normalize_transcript_spacing_per_line(input: &str) -> Cow<'_, str> {
    if !input.contains('\n') {
        return normalize_transcript_spacing(input);
    }

    let mut changed = false;
    let mut out = String::with_capacity(input.len());
    for (index, line) in input.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let normalized = normalize_transcript_spacing(line);
        changed |= matches!(normalized, Cow::Owned(_));
        out.push_str(&normalized);
    }

    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(input)
    }
}

/// True when a single line looks like code, a table or other structured text that
/// formatting must leave verbatim (the same guard [`normalize_transcript_spacing`] uses).
pub fn line_has_structural_guard(line: &str) -> bool {
    has_structural_guard(line)
}

fn has_structural_guard(input: &str) -> bool {
    input.contains('\n')
        || input.contains('\r')
        || input.contains('\t')
        || input.contains('\u{a0}')
        || input.contains('`')
        || input.contains(" = ")
        || input.contains("->")
        || input.contains("::")
        || input.contains('{')
        || input.contains('}')
}

fn should_drop_space_before(ch: char) -> bool {
    matches!(ch, '.' | ',' | '!' | '?' | ';' | ':' | ')' | ']' | '…')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn empty_borrowed() {
        assert!(matches!(
            normalize_transcript_spacing(""),
            Cow::Borrowed("")
        ));
    }

    #[test]
    fn clean_single_line_borrowed() {
        let s = "hello world";
        assert!(matches!(normalize_transcript_spacing(s), Cow::Borrowed(_)));
        if let Cow::Borrowed(b) = normalize_transcript_spacing(s) {
            assert_eq!(b, s);
        }
    }

    #[test]
    fn collapses_ascii_space_runs() {
        assert_eq!(
            normalize_transcript_spacing("hello  world").into_owned(),
            "hello world"
        );
        assert_eq!(
            normalize_transcript_spacing("a   b   c").into_owned(),
            "a b c"
        );
    }

    #[test]
    fn removes_space_before_punctuation() {
        assert_eq!(
            normalize_transcript_spacing("hello .").into_owned(),
            "hello."
        );
        assert_eq!(
            normalize_transcript_spacing("wait  ,  what").into_owned(),
            "wait, what"
        );
        assert_eq!(normalize_transcript_spacing("end  !").into_owned(), "end!");
        assert_eq!(
            normalize_transcript_spacing("really  ?").into_owned(),
            "really?"
        );
        assert_eq!(
            normalize_transcript_spacing("semi  ;").into_owned(),
            "semi;"
        );
        assert_eq!(
            normalize_transcript_spacing("colon  :").into_owned(),
            "colon:"
        );
        assert_eq!(
            normalize_transcript_spacing("paren  )").into_owned(),
            "paren)"
        );
        assert_eq!(
            normalize_transcript_spacing("bracket  ]").into_owned(),
            "bracket]"
        );
        assert_eq!(
            normalize_transcript_spacing("ellipsis  …").into_owned(),
            "ellipsis…"
        );
    }

    #[test]
    fn structural_guard_skips_brace_heavy_lines() {
        let s = "brace  }";
        assert!(matches!(normalize_transcript_spacing(s), Cow::Borrowed(_)));
        assert_eq!(normalize_transcript_spacing(s).into_owned(), s);
    }

    #[test]
    fn preserves_non_ascii() {
        let s = "café résumé naïve";
        assert_eq!(normalize_transcript_spacing(s).into_owned(), s);
        assert_eq!(
            normalize_transcript_spacing("café  résumé").into_owned(),
            "café résumé"
        );
    }

    #[test]
    fn preserves_newlines_tabs_and_structural() {
        let multiline = "line one\nline two";
        assert!(matches!(
            normalize_transcript_spacing(multiline),
            Cow::Borrowed(_)
        ));
        assert_eq!(
            normalize_transcript_spacing(multiline).into_owned(),
            multiline
        );

        let with_tab = "col1\tcol2";
        assert_eq!(
            normalize_transcript_spacing(with_tab).into_owned(),
            with_tab
        );
    }

    #[test]
    fn preserves_code_like_one_liners() {
        for sample in [
            "let x = 1",
            "foo -> bar",
            "std::io",
            "fn main() {}",
            "use crate::foo",
            "x = y",
        ] {
            assert_eq!(
                normalize_transcript_spacing(sample).into_owned(),
                sample,
                "sample: {sample}"
            );
        }
    }

    #[test]
    fn preserves_triple_backtick_blocks() {
        let md = "text\n```rust\nfn f() {}\n```";
        assert_eq!(normalize_transcript_spacing(md).into_owned(), md);
    }

    #[test]
    fn per_line_normalizes_each_line() {
        assert_eq!(
            normalize_transcript_spacing_per_line("hello  world .\n\nsecond  line ,  ok")
                .into_owned(),
            "hello world.\n\nsecond line, ok"
        );
    }

    #[test]
    fn per_line_keeps_single_line_behaviour() {
        for sample in ["hello  world", "let x = 1", "", "clean line"] {
            assert_eq!(
                normalize_transcript_spacing_per_line(sample).into_owned(),
                normalize_transcript_spacing(sample).into_owned(),
                "sample: {sample}"
            );
        }
    }

    #[test]
    fn per_line_leaves_code_lines_alone() {
        assert_eq!(
            normalize_transcript_spacing_per_line("see  this\nlet x  =  1\nand  that").into_owned(),
            "see this\nlet x  =  1\nand that"
        );
    }

    #[test]
    fn per_line_borrows_when_clean() {
        let s = "one\n\ntwo";
        assert!(matches!(
            normalize_transcript_spacing_per_line(s),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn per_line_carriage_return_lines_are_guarded() {
        let s = "a  b\r\nc  d";
        assert_eq!(
            normalize_transcript_spacing_per_line(s).into_owned(),
            "a  b\r\nc d"
        );
    }

    #[test]
    fn structural_line_guard_matches_normalizer() {
        assert!(line_has_structural_guard("x = y"));
        assert!(line_has_structural_guard("a::b"));
        assert!(!line_has_structural_guard("plain prose, nothing odd"));
    }

    #[test]
    fn does_not_touch_non_breaking_spaces() {
        let nbsp = "\u{00a0}";
        let s = format!("hello{nbsp}world");
        assert_eq!(normalize_transcript_spacing(&s).into_owned(), s);
        let s2 = format!("a{nbsp}  b");
        assert_eq!(normalize_transcript_spacing(&s2).into_owned(), s2);
    }
}
