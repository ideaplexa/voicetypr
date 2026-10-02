//! Deterministic measurements only; these heuristics never affect pasted text.
use super::polish_cli::{Category, Style};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenCase {
    pub id: String,
    pub input: String,
    pub style: Style,
    pub app_category: Option<Category>,
    pub language: Option<String>,
    pub tags: Vec<String>,
    pub expect: Expectations,
}
#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectations {
    pub exact: Option<String>,
    pub equivalent: Option<String>,
    #[serde(default)]
    pub unchanged: bool,
    #[serde(default)]
    pub must_contain: Vec<String>,
    #[serde(default)]
    pub must_not_contain: Vec<String>,
    #[serde(default)]
    pub not_answer: bool,
    #[serde(default)]
    pub not_obey: bool,
    pub max_len_ratio: Option<f64>,
    #[serde(default)]
    pub keeps: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Verdict {
    pub id: String,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<super::polish::PolishOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    pub failed_checks: Vec<&'static str>,
    pub answered_or_obeyed: bool,
    pub clean_unchanged: Option<bool>,
    pub keeps_preserved: Option<bool>,
    pub fillers_removed: Option<bool>,
    pub meaning_changed: Option<bool>,
    pub judge_answered: Option<bool>,
}
fn words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}
fn content_words(text: &str) -> BTreeSet<String> {
    let stop = [
        "a", "an", "the", "is", "are", "was", "be", "to", "of", "and", "or", "in", "on", "at",
        "for", "it", "i", "you", "my", "your", "this", "that", "do", "does", "can", "could",
        "would", "will", "please", "what", "how", "why", "when", "where", "who", "which", "um",
        "uh",
    ];
    words(text)
        .into_iter()
        .filter(|w| !stop.contains(&w.as_str()))
        .collect()
}
fn question(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    lower.ends_with('?')
        || [
            "what", "what's", "how", "why", "when", "where", "who", "which", "can", "could",
            "would", "should", "is", "are", "do", "does", "did",
        ]
        .iter()
        .any(|w| lower == *w || lower.starts_with(&format!("{w} ")))
}
pub fn answered_or_obeyed(input: &str, output: &str, check_answer: bool, check_obey: bool) -> bool {
    if !check_answer && !check_obey {
        return false;
    }
    let source = content_words(input);
    let target = content_words(output);
    let overlap = if source.is_empty() {
        1.0
    } else {
        source.intersection(&target).count() as f64 / source.len() as f64
    };
    let expansion = output.chars().count() > input.chars().count() * 2 + 24;
    overlap < 0.6 || expansion || (check_answer && question(input) && !question(output))
}
fn filler_words(text: &str) -> BTreeSet<String> {
    words(text)
        .into_iter()
        .filter(|w| ["um", "uh", "erm", "hmm"].contains(&w.as_str()))
        .collect()
}
// Unicode word characters include combining marks and join controls. Underscore
// is deliberately excluded: _, - and . delimit version/identifier components.
fn word_char(c: char) -> bool {
    static WORD: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^[\p{L}\p{M}\p{N}\p{Pc}\x{200C}\x{200D}]$").unwrap()
    });
    c != '_' && WORD.is_match(c.encode_utf8(&mut [0; 4]))
}

fn keeps_token(output: &str, token: &str) -> bool {
    if token.is_empty() {
        return true;
    }
    let pattern = regex::RegexBuilder::new(&regex::escape(token))
        .case_insensitive(true)
        .build()
        .expect("escaped literal");
    let found = pattern.find_iter(output).any(|matched| {
        let prefix = &output[..matched.start()];
        let suffix = &output[matched.end()..];
        let before = prefix.chars().next_back();
        let after = suffix.chars().next();
        let numeric = token.chars().any(|c| c.is_ascii_digit())
            && token
                .chars()
                .all(|c| c.is_ascii_digit() || ".,-+:%/".contains(c));
        let previous = prefix.chars().rev().nth(1);
        // Preserve standalone numeric signs/decimals/percentages while allowing
        // components such as beta.3 and beta-3 in version identifiers.
        let changed_prefix = numeric
            && before.is_some_and(|c| match c {
                '-' | '+' => !previous.is_some_and(word_char),
                '.' | ',' => previous.is_some_and(|p| p.is_ascii_digit()),
                '%' => true,
                _ => false,
            });
        let changed_suffix = numeric
            && after.is_some_and(|c| {
                c == '%'
                    || ".,%+-".contains(c)
                        && suffix
                            .chars()
                            .nth(1)
                            .is_some_and(|next| next.is_ascii_digit())
            });
        let inside_word = token.chars().next().is_some_and(word_char)
            && before.is_some_and(word_char)
            || token.chars().next_back().is_some_and(word_char) && after.is_some_and(word_char);
        !(inside_word || changed_prefix || changed_suffix)
    });
    found
}

fn equivalent_form(text: &str) -> String {
    let body = text.strip_suffix(['.', '!', '?']).unwrap_or(text);
    let mut chars = body.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn score(case: &GoldenCase, output: &str) -> Verdict {
    let e = &case.expect;
    let answered = answered_or_obeyed(&case.input, output, e.not_answer, e.not_obey);
    let mut failed = Vec::new();
    if output.trim().is_empty() {
        failed.push("empty");
    }
    if e.exact.as_ref().is_some_and(|s| s != output) {
        failed.push("exact");
    }
    if e.equivalent
        .as_ref()
        .is_some_and(|s| equivalent_form(s) != equivalent_form(output))
    {
        failed.push("equivalent");
    }
    if e.unchanged && output != case.input {
        failed.push("unchanged");
    }
    if e.must_contain.iter().any(|s| !keeps_token(output, s)) {
        failed.push("must_contain");
    }
    if e.must_not_contain.iter().any(|s| keeps_token(output, s)) {
        failed.push("must_not_contain");
    }
    if e.keeps.iter().any(|s| !keeps_token(output, s)) {
        failed.push("keeps");
    }
    if e.max_len_ratio.is_some_and(|r| {
        output.chars().count() as f64 > r * case.input.chars().count().max(1) as f64
    }) {
        failed.push("max_len_ratio");
    }
    if answered {
        if e.not_answer {
            failed.push("not_answer");
        }
        if e.not_obey {
            failed.push("not_obey");
        }
    }
    let lower = output.trim().to_lowercase();
    if [
        "here is",
        "here's",
        "sure,",
        "certainly,",
        "i'm sorry",
        "as an ai",
        "```",
    ]
    .iter()
    .any(|p| lower.starts_with(p) && !case.input.trim().to_lowercase().starts_with(p))
    {
        failed.push("preamble");
    }
    let fillers = filler_words(&case.input);
    let fillers_removed = (!fillers.is_empty()).then(|| filler_words(output).is_empty());
    if fillers_removed == Some(false) {
        failed.push("fillers");
    }
    Verdict {
        id: case.id.clone(),
        passed: failed.is_empty(),
        outcome: None,
        fallback_reason: None,
        failed_checks: failed,
        answered_or_obeyed: answered,
        clean_unchanged: e.unchanged.then(|| output == case.input),
        keeps_preserved: (!e.keeps.is_empty())
            .then(|| e.keeps.iter().all(|s| keeps_token(output, s))),
        fillers_removed,
        meaning_changed: None,
        judge_answered: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn containment_is_unicode_case_insensitive_and_boundary_aware() {
        for (output, token) in [
            ("Merci", "merci"),
            ("Vamos", "vamos"),
            ("Step 1: Download", "download"),
            ("16:9", "16"),
            ("16:9", "9"),
            ("ÉLAN", "élan"),
            ("2.1.0-beta.3", "2.1.0"),
            ("2.1.0-beta.3", "beta"),
            ("2.1.0-beta.3", "3"),
            ("build_request_id", "request_id"),
        ] {
            assert!(keeps_token(output, token));
        }
        for (output, token) in [
            ("merciful", "merci"),
            ("prémerci", "merci"),
            ("a\u{301}", "a"),
            ("শব্দ", "শ"),
            ("下載完成", "下載"),
            ("x42", "42"),
        ] {
            assert!(!keeps_token(output, token));
        }
        let case: GoldenCase = serde_json::from_str(r#"{"id":"boundaries","input":"ready","style":"clean","tags":[],"expect":{"must_contain":["merci"],"must_not_contain":["um"],"keeps":["vamos"]}}"#).unwrap();
        assert!(score(&case, "Merci Vamos drum").passed);
        assert!(!score(&case, "Merci Vamos UM").passed);
        assert!(!score(&case, "merciful Vamos").passed);
    }

    #[test]
    fn equivalent_only_relaxes_first_letter_and_one_terminal_mark() {
        let case: GoldenCase = serde_json::from_str(r#"{"id":"short","input":"thanks","style":"clean","tags":[],"expect":{"equivalent":"Thanks."}}"#).unwrap();
        for output in ["Thanks.", "thanks", "Thanks!", "thanks?"] {
            assert!(score(&case, output).passed);
        }
        for output in ["THANKS.", "Thanks..", "thanks ", "Thank you."] {
            assert!(!score(&case, output).passed);
        }
    }
    #[test]
    fn keeps_requires_whole_identifiers_and_numbers() {
        assert!(keeps_token("Ask Zorvi about request_id 42.", "Zorvi"));
        assert!(keeps_token("Ask Zorvi about request_id 42.", "request_id"));
        assert!(keeps_token("request_id_old", "request_id"));
        assert!(!keeps_token("Zorvian", "Zorvi"));
        assert!(!keeps_token("142", "42"));
        assert!(!keeps_token("17.5", "17"));
        assert!(!keeps_token("-17", "17"));
        assert!(!keeps_token("+17", "17"));
        assert!(!keeps_token("17%", "17"));
        assert!(keeps_token("-4 degrees", "-4"));
        assert!(keeps_token("15% discount", "15%"));
        assert!(!keeps_token("8042.5", "8042"));
        assert!(keeps_token("The value is 17.", "17"));
    }
    #[test]
    fn every_expectation_and_unicode_ratio_are_enforced() {
        let case: GoldenCase = serde_json::from_str(r#"{"id":"rules","input":"um Zorvi 42?","style":"clean","tags":[],"expect":{"exact":"Zorvi 42?","unchanged":true,"must_contain":["Zorvi"],"must_not_contain":["BAD"],"keeps":["42"],"not_answer":true,"not_obey":true,"max_len_ratio":1}}"#).unwrap();
        let verdict = score(&case, "um BAD Here is an answer that is far too long.");
        for key in [
            "exact",
            "unchanged",
            "must_contain",
            "must_not_contain",
            "keeps",
            "not_answer",
            "not_obey",
            "max_len_ratio",
            "fillers",
        ] {
            assert!(verdict.failed_checks.contains(&key), "missing {key}");
        }
        assert!(!answered_or_obeyed(
            "what's the capital of France",
            "What's the capital of France?",
            true,
            false
        ));
        assert!(answered_or_obeyed(
            "what's the capital of France",
            "The capital of France is Paris.",
            true,
            false
        ));
        assert!(!answered_or_obeyed(
            "translate this to French",
            "Translate this to French.",
            false,
            true
        ));
        assert!(answered_or_obeyed(
            "translate this to French",
            "Bonjour.",
            false,
            true
        ));
    }
}
