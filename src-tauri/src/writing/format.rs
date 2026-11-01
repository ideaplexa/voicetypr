//! Shared local formatting stage.
//!
//! Runs for every engine when Polish is off (and the take is not a literal
//! snippet): per-line spacing, sentence-start capitals, standalone English "i",
//! optional filler/stutter removal and pause-based paragraph breaks.
//!
//! The stage is pure (no I/O, no settings, no logging of text) and idempotent:
//! formatting its own output changes nothing. Anything that looks like code, a
//! URL, an email or a mixed-case brand name is left alone; when in doubt a rule
//! does nothing.

use std::borrow::Cow;

use super::WritingSettings;
use crate::transcription::{TranscriptionResult, TranscriptionSegment, TranscriptionWord};

/// A paragraph needs a silence at least this long (milliseconds).
pub const PARAGRAPH_MIN_GAP_MS: u64 = 1_500;
/// A take is "long" at this many seconds of audio ...
pub const PARAGRAPH_MIN_AUDIO_MS: u64 = 30_000;
/// ... or this many words.
pub const PARAGRAPH_MIN_WORDS: usize = 60;
/// At most one break per this many words.
pub const PARAGRAPH_MIN_WORDS_BETWEEN_BREAKS: usize = 40;
/// A break is skipped when fewer words than this would follow it.
pub const PARAGRAPH_MIN_TAIL_WORDS: usize = 6;

const FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "erm", "er", "hmm", "mm"];
// "is is" can be a double copula; "in in" can follow a phrasal verb ("come in in June").
// Also keep "you you"/"it it" (object then subject), "to to" (stranded/infinitive
// then purpose), "of of" ("unheard of of course"), and "and and" when naming
// conjunctions ("between and and or"). Prefer speech to deletion.
const STUTTER_REPEATS: &[&str] = &["the", "a", "an", "i", "we"];
/// Words whose trailing period does not end a sentence.
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "mt", "vs", "etc", "inc", "ltd", "co",
    "corp", "dept", "est", "fig", "no", "nr", "vol", "approx", "gen", "sen", "rep", "gov", "lt",
    "col", "capt", "sgt", "hon", "rev", "pres", "al", "ca", "bzw", "usw", "min", "max", "tel",
];
/// Characters that may open a token before its first letter ("(hello", "“hello").
const OPENING_PUNCTUATION: &[char] = &['"', '\'', '“', '‘', '(', '[', '«', '¿', '¡'];
/// Characters that may close a sentence after its terminal punctuation.
const CLOSING_PUNCTUATION: &[char] = &['"', '\'', '”', '’', ')', ']', '}', '»'];

/// A pause in the audio, located by the number of words spoken before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PauseHint {
    /// Alphanumeric whitespace-separated words that precede the pause (see [`count_words`]).
    pub words_before: usize,
    pub gap_ms: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FormatOptions<'a> {
    /// Spoken/transcript language code ("en", "en-US", "es", ...); empty when unknown.
    pub language: &'a str,
    pub paragraphs: bool,
    pub remove_fillers: bool,
    /// The engine already produces its own casing (Parakeet, Soniox, Deepgram):
    /// sentence-start capitals and "i" are left to it.
    pub engine_casing_native: bool,
    /// Audio length of the take, when known.
    pub audio_duration_ms: Option<u64>,
    /// Custom words and replacement targets whose spelling must not be changed.
    pub protected_terms: &'a [String],
}

/// A span of text with optional timing (a segment or a word).
#[derive(Debug, Clone, Copy)]
pub struct TimedSpan<'a> {
    pub text: &'a str,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}

/// Number of whitespace-separated tokens that contain a letter or digit.
/// Punctuation-only tokens ("-", "—") are not words, so engines that space
/// punctuation differently still agree on the count.
pub fn count_words(text: &str) -> usize {
    text.split_whitespace().filter(|t| has_alnum(t)).count()
}

fn has_alnum(token: &str) -> bool {
    token.chars().any(char::is_alphanumeric)
}

/// Turn timed spans into pause hints. Returns nothing unless the spans carry
/// exactly the words of the text being formatted (`expected_words`), so a
/// text that was rewritten after transcription never gets misplaced breaks.
pub fn pause_hints_from_spans(spans: &[TimedSpan<'_>], expected_words: usize) -> Vec<PauseHint> {
    if spans.len() < 2 {
        return Vec::new();
    }
    let total: usize = spans.iter().map(|span| count_words(span.text)).sum();
    if total != expected_words {
        return Vec::new();
    }

    let mut hints = Vec::new();
    let mut words = 0usize;
    for (index, span) in spans.iter().enumerate() {
        words += count_words(span.text);
        let Some(next) = spans.get(index + 1) else {
            break;
        };
        if let (Some(end), Some(next_start)) = (span.end_ms, next.start_ms) {
            if next_start > end {
                hints.push(PauseHint {
                    words_before: words,
                    gap_ms: next_start - end,
                });
            }
        }
    }
    hints
}

/// Pause hints for `text` from a transcription's word or segment timings.
/// Words are preferred when every one carries a time; otherwise segments.
pub fn pause_hints_for_transcription(
    text: &str,
    transcription: &TranscriptionResult,
) -> Vec<PauseHint> {
    if text.contains('\n') {
        return Vec::new();
    }
    let expected = count_words(text);

    if let Some(words) = transcription.words.as_deref() {
        if words.len() > 1 {
            let spans: Vec<TimedSpan<'_>> = words
                .iter()
                .map(|word: &TranscriptionWord| TimedSpan {
                    text: word.text.as_str(),
                    start_ms: word.start_ms,
                    end_ms: word.end_ms,
                })
                .collect();
            let hints = pause_hints_from_spans(&spans, expected);
            if !hints.is_empty() {
                return hints;
            }
        }
    }

    if let Some(segments) = transcription.segments.as_deref() {
        let spans: Vec<TimedSpan<'_>> = segments
            .iter()
            .map(|segment: &TranscriptionSegment| TimedSpan {
                text: segment.text.as_str(),
                start_ms: segment.start_ms,
                end_ms: segment.end_ms,
            })
            .collect();
        return pause_hints_from_spans(&spans, expected);
    }
    Vec::new()
}

/// Audio length of a take: the recorded duration, else where the last timed span ends.
pub fn audio_duration_ms(transcription: &TranscriptionResult) -> Option<u64> {
    transcription.timings.audio_duration_ms.or_else(|| {
        let segment_end = transcription
            .segments
            .as_deref()
            .and_then(|segments| segments.iter().filter_map(|s| s.end_ms).max());
        let word_end = transcription
            .words
            .as_deref()
            .and_then(|words| words.iter().filter_map(|w| w.end_ms).max());
        segment_end.max(word_end)
    })
}

/// Engines whose own output is already cased and punctuated.
pub fn engine_has_native_casing(engine: &str) -> bool {
    matches!(
        engine.trim().to_ascii_lowercase().as_str(),
        "parakeet" | "soniox" | "deepgram"
    )
}

/// What the pipeline decides before running the stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FormatStageSettings {
    pub paragraphs: bool,
    pub remove_fillers: bool,
}

/// The stage runs only for deterministic output: never for a literal snippet
/// and never when Polish rewrites the text itself.
pub(crate) fn should_run_format_stage(literal_locked: bool, polish_runs: bool) -> bool {
    !literal_locked && !polish_runs
}

/// Spellings the user chose (custom words, replacement targets) that casing rules must keep.
pub(crate) fn protected_terms(settings: &WritingSettings) -> Vec<String> {
    let replacements = settings
        .replacements
        .iter()
        .filter(|rule| rule.enabled)
        .map(|rule| rule.to.clone());
    let words = settings
        .custom_words
        .iter()
        .filter(|word| word.enabled)
        .map(|word| word.phrase.clone());
    replacements
        .chain(words)
        .filter(|term| !term.trim().is_empty())
        .collect()
}

/// Runs the stage for one transcription. Returns the formatted text only when it
/// differs from `text`; `None` means "keep the unformatted text", which is also
/// the answer if anything inside the stage goes wrong.
pub(crate) fn format_for_transcription(
    text: &str,
    transcription: &TranscriptionResult,
    language: Option<&str>,
    stage: FormatStageSettings,
    protected: &[String],
) -> Option<String> {
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let pauses = if stage.paragraphs {
            pause_hints_for_transcription(text, transcription)
        } else {
            Vec::new()
        };
        let opts = FormatOptions {
            language: language.unwrap_or(""),
            paragraphs: stage.paragraphs,
            remove_fillers: stage.remove_fillers,
            engine_casing_native: engine_has_native_casing(&transcription.engine),
            audio_duration_ms: audio_duration_ms(transcription),
            protected_terms: protected,
        };
        match format_transcript(text, &pauses, &opts) {
            Cow::Borrowed(_) => None,
            Cow::Owned(formatted) => Some(formatted),
        }
    }));
    attempt.unwrap_or(None)
}

/// Formats `text`. Returns the input unchanged ([`Cow::Borrowed`]) when no rule applies.
pub fn format_transcript<'a>(
    text: &'a str,
    pauses: &[PauseHint],
    opts: &FormatOptions<'_>,
) -> Cow<'a, str> {
    if text.trim().is_empty() {
        return Cow::Borrowed(text);
    }

    // Paragraph breaks from pauses only apply to engine text without its own breaks.
    let single_line = !text.contains('\n');
    let mut out = String::with_capacity(text.len() + 8);
    let mut at_sentence_start = true;
    let mut code_fence = None;
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        // CommonMark allows at most three leading spaces, not indented code.
        // Do not trim tabs: each counts as four spaces and cannot precede a fence.
        let fence = line.trim_start_matches(' ');
        let fence_indent = line.len() - fence.len();
        let delimiter = fence.as_bytes().first().copied().unwrap_or_default();
        let length = if fence_indent <= 3 && matches!(delimiter, b'`' | b'~') {
            fence.bytes().take_while(|&byte| byte == delimiter).count()
        } else {
            0
        };
        // CommonMark forbids backticks in a backtick fence's info string:
        // a leading inline span such as ```echo``` is prose, not a fence.
        let opens_fence = length >= 3 && (delimiter != b'`' || !fence[length..].contains('`'));
        if code_fence.is_some() || opens_fence {
            // Do this before spacing normalization: even prose-looking code is literal.
            out.push_str(line);
            match code_fence {
                Some((opening_delimiter, opening_length))
                    if delimiter == opening_delimiter
                        && length >= opening_length
                        && fence[length..].trim().is_empty() =>
                {
                    code_fence = None;
                }
                None => code_fence = Some((delimiter, length)),
                _ => {}
            }
            at_sentence_start = false;
            continue;
        }
        let normalized = transcript_text::normalize_transcript_spacing(line);
        let hints = if single_line { pauses } else { &[] };
        let (formatted, next_at_start) = format_line(&normalized, at_sentence_start, hints, opts);
        out.push_str(&formatted);
        at_sentence_start = next_at_start;
    }

    // Never turn a take into nothing (a lone "um" stays as spoken).
    if out.trim().is_empty() || out == text {
        return Cow::Borrowed(text);
    }
    Cow::Owned(out)
}

#[derive(Debug, Clone)]
struct Token {
    text: String,
    /// Index of the word this token belongs to in the line, counting alphanumeric tokens.
    idx: usize,
}

fn tokenize(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut words = 0usize;
    for part in line.split(' ').filter(|part| !part.is_empty()) {
        if has_alnum(part) {
            tokens.push(Token {
                text: part.to_string(),
                idx: words,
            });
            words += 1;
        } else {
            tokens.push(Token {
                text: part.to_string(),
                idx: words.saturating_sub(1),
            });
        }
    }
    tokens
}

/// Formats one line. Returns the text and whether the next line starts a sentence.
fn format_line(
    line: &str,
    starts_sentence: bool,
    pauses: &[PauseHint],
    opts: &FormatOptions<'_>,
) -> (String, bool) {
    if line.trim().is_empty() {
        return (line.to_string(), starts_sentence);
    }
    // Code, tables and other structured lines stay exactly as the engine wrote them.
    if transcript_text::line_has_structural_guard(line) {
        return (line.to_string(), false);
    }

    let trailing_space = line.ends_with(' ');
    let mut tokens = tokenize(line);
    let english = is_english(opts.language);

    if opts.remove_fillers && english {
        tokens = strip_fillers(tokens, starts_sentence, opts.protected_terms);
    }
    if !opts.engine_casing_native && !has_no_dotted_case(opts.language) {
        capitalize_tokens(&mut tokens, starts_sentence, english, opts.protected_terms);
    }

    let breaks = if opts.paragraphs {
        paragraph_breaks(&tokens, pauses, opts)
    } else {
        Vec::new()
    };

    let mut out = String::with_capacity(line.len() + 4);
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 {
            out.push_str(if breaks.contains(&(index - 1)) {
                "\n\n"
            } else {
                " "
            });
        }
        out.push_str(&token.text);
    }
    if out.is_empty() {
        return (line.to_string(), starts_sentence);
    }
    if trailing_space {
        out.push(' ');
    }
    let next_starts = tokens
        .last()
        .map(|token| ends_sentence(&token.text))
        .unwrap_or(starts_sentence);
    (out, next_starts)
}

// --- language gates ---------------------------------------------------------

fn language_base(language: &str) -> String {
    language
        .trim()
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub(super) fn is_english(language: &str) -> bool {
    language_base(language) == "en"
}

/// Turkic languages upper-case "i" to a dotted capital; leave their casing alone.
fn has_no_dotted_case(language: &str) -> bool {
    matches!(language_base(language).as_str(), "tr" | "az")
}

// --- token anatomy ----------------------------------------------------------

/// Splits a token into (leading punctuation, core, trailing punctuation). The
/// core runs from the first to the last letter/digit, so interior punctuation
/// ("e.g", "uh-huh", "don't") stays inside it.
fn split_token(token: &str) -> (&str, &str, &str) {
    let Some(first) = token.char_indices().find(|(_, c)| c.is_alphanumeric()) else {
        return (token, "", "");
    };
    let last = token
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_alphanumeric())
        .map(|(index, c)| index + c.len_utf8())
        .unwrap_or(token.len());
    (&token[..first.0], &token[first.0..last], &token[last..])
}

/// Sentence punctuation/quotes may surround speech; flag/path/identifier
/// attachments must not make their inner core look like a standalone word.
fn has_prose_boundaries(lead: &str, trail: &str) -> bool {
    lead.chars().all(|c| OPENING_PUNCTUATION.contains(&c))
        && trail.chars().all(|c| {
            CLOSING_PUNCTUATION.contains(&c)
                || matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '…' | '—' | '–')
        })
}

fn is_wordlike(core: &str) -> bool {
    !core.is_empty()
        && core
            .chars()
            .all(|c| c.is_alphabetic() || matches!(c, '\'' | '’' | '-'))
}

fn is_protected_word(core: &str, protected_terms: &[String]) -> bool {
    protected_terms
        .iter()
        .any(|term| term.split_whitespace().any(|word| word == core))
}

/// True when the token ends a sentence. Abbreviations, initials, numbers,
/// ellipses, URLs and emails do not.
fn ends_sentence(token: &str) -> bool {
    let trimmed = token.trim_end_matches(CLOSING_PUNCTUATION);
    if trimmed.ends_with("..") || trimmed.ends_with('…') {
        return false;
    }
    if trimmed.contains("://") || trimmed.contains('@') {
        return false;
    }
    match trimmed.chars().last() {
        Some('!' | '?') => true,
        Some('.') => {
            let body = trimmed.trim_end_matches('.');
            if body.is_empty() || body.contains('.') {
                return false;
            }
            let (_, core, _) = split_token(body);
            if core.is_empty() || core.chars().all(|c| c.is_ascii_digit()) {
                return false;
            }
            if core.chars().count() == 1 && core.chars().all(char::is_alphabetic) {
                return false;
            }
            !ABBREVIATIONS.contains(&core.to_lowercase().as_str())
        }
        _ => false,
    }
}

fn at_sentence_start(out: &[Token], line_starts_sentence: bool) -> bool {
    match out.last() {
        Some(last) => ends_sentence(&last.text),
        None => line_starts_sentence,
    }
}

// --- capitals ---------------------------------------------------------------

/// Upper-cases the first letter of `token` when that is safe. Returns whether it changed.
fn capitalize_token(token: &mut String, protected_terms: &[String]) -> bool {
    let (lead, core, trail) = split_token(token);
    if !has_prose_boundaries(lead, trail) || !is_wordlike(core) {
        return false;
    }
    let mut chars = core.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let rest = chars.as_str();
    if !first.is_lowercase() || rest.chars().any(char::is_uppercase) {
        return false;
    }
    if is_protected_word(core, protected_terms) {
        return false;
    }
    let upper: String = first.to_uppercase().collect();
    if upper.chars().count() != 1 {
        return false;
    }
    *token = format!("{lead}{upper}{rest}{trail}");
    true
}

/// "i", "i'm", "i've", "i'll", "i'd" → "I...". Returns whether it changed.
fn capitalize_standalone_i(token: &mut String, protected_terms: &[String]) -> bool {
    let (lead, core, trail) = split_token(token);
    if !has_prose_boundaries(lead, trail)
        || lead.contains(['(', '['])
        || is_protected_word(core, protected_terms)
    {
        return false;
    }
    let lowered = core.replace('’', "'");
    if !matches!(lowered.as_str(), "i" | "i'm" | "i've" | "i'll" | "i'd") {
        return false;
    }
    let rest = &core[1..];
    *token = format!("{lead}I{rest}{trail}");
    true
}

fn capitalize_tokens(
    tokens: &mut [Token],
    starts_sentence: bool,
    english: bool,
    protected_terms: &[String],
) {
    let mut at_start = starts_sentence;
    for token in tokens.iter_mut() {
        if at_start {
            capitalize_token(&mut token.text, protected_terms);
        }
        if english {
            capitalize_standalone_i(&mut token.text, protected_terms);
        }
        at_start = ends_sentence(&token.text);
    }
}

// --- fillers and stutters ---------------------------------------------------

fn is_filler(core: &str, trail: &str, sentence_start: bool) -> bool {
    if FILLERS.contains(&core) {
        return true;
    }
    // ASR often sentence-cases a punctuated hesitation ("Um,", "Uh.").
    // Never fold all caps (ER/UM), and keep even "Er," because it may be a name.
    sentence_start
        && trail.starts_with([',', '.', '!', '?'])
        && matches!(core, "Um" | "Umm" | "Uh" | "Uhh" | "Erm" | "Hmm" | "Mm")
}

fn contains_quantity(text: &str) -> bool {
    text.chars().any(char::is_numeric)
}

/// "th-" before "the", "I-" before "I": a word cut off and restarted.
fn is_cut_off_fragment(token: &str, next: Option<&Token>, protected_terms: &[String]) -> bool {
    let (lead, core, trail) = split_token(token);
    if !lead.is_empty()
        || core.is_empty()
        || trail.is_empty()
        || !trail.chars().all(|c| c == '-')
        || !core.chars().all(char::is_alphabetic)
        || is_protected_word(token, protected_terms)
        || is_protected_word(core, protected_terms)
    {
        return false;
    }
    let Some(next) = next else {
        return false;
    };
    let (next_lead, next_core, next_trail) = split_token(&next.text);
    // Grades, blood types and short acronyms are not interrupted speech.
    // Only the English "I- I"/"I- I'm" restart gets an uppercase exception.
    if core.chars().count() <= 2
        && core.chars().all(char::is_uppercase)
        && !(core == "I" && next_core.starts_with('I'))
    {
        return false;
    }
    has_prose_boundaries(next_lead, next_trail)
        && is_wordlike(next_core)
        && next_core.to_lowercase().starts_with(&core.to_lowercase())
}

struct FillerState {
    recapitalize_next: bool,
    pending_lead: String,
}

fn has_paired_wrapper(lead: &str, trail: &str) -> bool {
    [
        ('"', '"'),
        ('\'', '\''),
        ('“', '”'),
        ('‘', '’'),
        ('(', ')'),
        ('[', ']'),
        ('«', '»'),
    ]
    .iter()
    .any(|(open, close)| lead.contains(*open) && trail.contains(*close))
}

fn strip_fillers(
    tokens: Vec<Token>,
    starts_sentence: bool,
    protected_terms: &[String],
) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut state = FillerState {
        recapitalize_next: false,
        pending_lead: String::new(),
    };

    for (index, token) in tokens.iter().enumerate() {
        let next = tokens.get(index + 1);
        let (lead, core, trail) = split_token(&token.text);

        let removable_filler = has_prose_boundaries(lead, trail)
            && !has_paired_wrapper(lead, trail)
            && is_filler(core, trail, at_sentence_start(&out, starts_sentence))
            && !is_protected_word(core, protected_terms)
            && !((core.eq_ignore_ascii_case("mm") || core.eq_ignore_ascii_case("um"))
                && (out.last().is_some_and(|prev| contains_quantity(&prev.text))
                    || next.is_some_and(|n| n.text.starts_with(char::is_numeric))));

        if removable_filler {
            let was_start = at_sentence_start(&out, starts_sentence);
            // A sentence end the filler carried ("went uh.") moves to the word before it.
            let carried_end = trail
                .chars()
                .find(|c| matches!(c, '.' | '!' | '?'))
                .filter(|_| !trail.contains(".."));
            let mut sentence_ended = false;
            if let Some(end) = carried_end {
                if let Some(prev) = out.last_mut() {
                    if !prev.text.ends_with(['.', '!', '?', '…']) {
                        let kept = prev.text.trim_end_matches([',', ';', ':']).len();
                        prev.text.truncate(kept);
                        prev.text.push(end);
                    }
                    sentence_ended = true;
                }
            }
            if was_start || sentence_ended {
                state.recapitalize_next = true;
            }
            if !lead.is_empty() {
                state.pending_lead = lead.to_string();
            }
            continue;
        }

        if is_cut_off_fragment(&token.text, next, protected_terms) {
            if at_sentence_start(&out, starts_sentence) {
                state.recapitalize_next = true;
            }
            continue;
        }

        push_with_repeat_check(
            &mut out,
            token.clone(),
            starts_sentence,
            &mut state,
            protected_terms,
        );
    }
    out
}

fn push_with_repeat_check(
    out: &mut Vec<Token>,
    mut token: Token,
    starts_sentence: bool,
    state: &mut FillerState,
    protected_terms: &[String],
) {
    if let Some(last) = out.last() {
        let (last_lead, last_core, last_trail) = split_token(&last.text);
        let (lead, core, _) = split_token(&token.text);
        if last_lead.is_empty()
            && last_trail.is_empty()
            && lead.is_empty()
            && !last_core.is_empty()
            && last_core.chars().all(char::is_alphabetic)
            && last_core.eq_ignore_ascii_case(core)
            && STUTTER_REPEATS.contains(&last_core.to_lowercase().as_str())
            // Keep meaningful case changes ("an A a week ago"). Only a
            // sentence-initial titlecase -> lowercase pair is a casing stutter.
            && (last_core == core
                || (at_sentence_start(&out[..out.len() - 1], starts_sentence)
                    && last_core.starts_with(char::is_uppercase)
                    && last_core.chars().skip(1).all(char::is_lowercase)
                    && core.chars().all(char::is_lowercase)))
        {
            out.pop();
            if at_sentence_start(out, starts_sentence) {
                state.recapitalize_next = true;
            }
        }
    }

    if state.recapitalize_next {
        capitalize_token(&mut token.text, protected_terms);
        state.recapitalize_next = false;
    }
    if !state.pending_lead.is_empty() {
        token.text = format!("{}{}", state.pending_lead, token.text);
        state.pending_lead.clear();
    }
    out.push(token);
}

// --- paragraphs -------------------------------------------------------------

/// Positions `j` where a paragraph break goes between `tokens[j]` and `tokens[j + 1]`.
fn paragraph_breaks(
    tokens: &[Token],
    pauses: &[PauseHint],
    opts: &FormatOptions<'_>,
) -> Vec<usize> {
    if pauses.is_empty() {
        return Vec::new();
    }
    let long_take = tokens.len() >= PARAGRAPH_MIN_WORDS
        || opts
            .audio_duration_ms
            .is_some_and(|ms| ms >= PARAGRAPH_MIN_AUDIO_MS);
    if !long_take {
        return Vec::new();
    }

    let mut hints: Vec<PauseHint> = pauses
        .iter()
        .copied()
        .filter(|hint| hint.gap_ms >= PARAGRAPH_MIN_GAP_MS && hint.words_before > 0)
        .collect();
    hints.sort_by_key(|hint| hint.words_before);

    let mut breaks: Vec<usize> = Vec::new();
    let mut last_break: isize = -1;
    for hint in hints {
        let after = tokens.partition_point(|token| token.idx < hint.words_before);
        if after == 0 || after >= tokens.len() {
            continue;
        }
        let j = after - 1;
        if breaks.last() == Some(&j) || !ends_sentence(&tokens[j].text) {
            continue;
        }
        if (j as isize) - last_break < PARAGRAPH_MIN_WORDS_BETWEEN_BREAKS as isize {
            continue;
        }
        if tokens.len() - after < PARAGRAPH_MIN_TAIL_WORDS {
            continue;
        }
        breaks.push(j);
        last_break = j as isize;
    }
    breaks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcription::{TranscriptionJob, TranscriptionSource, TranscriptionTask};

    fn en() -> FormatOptions<'static> {
        FormatOptions {
            language: "en",
            ..FormatOptions::default()
        }
    }

    fn fillers() -> FormatOptions<'static> {
        FormatOptions {
            language: "en",
            remove_fillers: true,
            engine_casing_native: true,
            ..FormatOptions::default()
        }
    }

    fn fmt(text: &str, opts: &FormatOptions<'_>) -> String {
        format_transcript(text, &[], opts).into_owned()
    }

    fn words(n: usize, ending: &str) -> String {
        let mut parts: Vec<String> = (0..n)
            .map(|i| format!("word{}", (b'a' + (i % 26) as u8) as char))
            .collect();
        if let Some(last) = parts.last_mut() {
            last.push_str(ending);
        }
        parts.join(" ")
    }

    // --- capitals ---

    #[test]
    fn capitalizes_first_letter_of_text() {
        assert_eq!(fmt("hello world.", &en()), "Hello world.");
    }

    #[test]
    fn capitalizes_after_sentence_ends() {
        assert_eq!(
            fmt("it works. is it? yes! ok then.", &en()),
            "It works. Is it? Yes! Ok then."
        );
    }

    #[test]
    fn capitalizes_after_closing_quote() {
        assert_eq!(
            fmt("he said \"go.\" then left.", &en()),
            "He said \"go.\" Then left."
        );
    }

    #[test]
    fn capitalizes_after_opening_quote() {
        assert_eq!(fmt("done. \"ok\" he said.", &en()), "Done. \"Ok\" he said.");
    }

    #[test]
    fn leaves_already_capitalized_text_alone() {
        let s = "Hello there. How are you?";
        assert!(matches!(format_transcript(s, &[], &en()), Cow::Borrowed(_)));
    }

    #[test]
    fn does_not_capitalize_urls_emails_or_domains() {
        assert_eq!(
            fmt("see. https://example.com for more.", &en()),
            "See. https://example.com for more."
        );
        assert_eq!(
            fmt("write. user@example.com today.", &en()),
            "Write. user@example.com today."
        );
        assert_eq!(
            fmt("ok. example.com is down.", &en()),
            "Ok. example.com is down."
        );
    }

    #[test]
    fn does_not_end_sentence_inside_url_or_email() {
        assert_eq!(
            fmt("go to example.com. then relax", &en()),
            // A domain-like token never ends a sentence: the next word is left alone.
            "Go to example.com. then relax"
        );
        assert_eq!(
            fmt("visit https://example.com/a. then wait", &en()),
            "Visit https://example.com/a. then wait"
        );
    }

    #[test]
    fn keeps_mixed_case_brand_names() {
        assert_eq!(fmt("iPhone is great.", &en()), "iPhone is great.");
        assert_eq!(fmt("ok. eBay sells it.", &en()), "Ok. eBay sells it.");
    }

    #[test]
    fn keeps_protected_terms_lowercase() {
        let terms = vec!["kubectl".to_string()];
        let opts = FormatOptions {
            language: "en",
            protected_terms: &terms,
            ..FormatOptions::default()
        };
        assert_eq!(fmt("kubectl apply works.", &opts), "kubectl apply works.");
        assert_eq!(
            fmt("run it. kubectl works.", &opts),
            "Run it. kubectl works."
        );
    }

    #[test]
    fn abbreviations_and_initials_do_not_end_sentences() {
        assert_eq!(
            fmt("ask dr. smith about it.", &en()),
            "Ask dr. smith about it."
        );
        assert_eq!(fmt("use e.g. the tool.", &en()), "Use e.g. the tool.");
        assert_eq!(fmt("meet j. smith today.", &en()), "Meet j. smith today.");
        assert_eq!(
            fmt("it costs 3. then more.", &en()),
            "It costs 3. then more."
        );
    }

    #[test]
    fn ellipsis_does_not_end_sentence() {
        assert_eq!(fmt("well... maybe not.", &en()), "Well... maybe not.");
    }

    #[test]
    fn code_like_lines_are_left_verbatim() {
        for sample in [
            "let x = 1",
            "use crate::foo",
            "a -> b",
            "fn main() {}",
            "run `ls`",
        ] {
            assert_eq!(fmt(sample, &en()), sample, "sample: {sample}");
        }
    }

    #[test]
    fn capitalizes_each_prose_line_of_multi_line_text() {
        assert_eq!(
            fmt("first part.\n\nsecond part.", &en()),
            "First part.\n\nSecond part."
        );
    }

    #[test]
    fn line_after_unfinished_sentence_is_not_capitalized() {
        assert_eq!(
            fmt("first part\nsecond part", &en()),
            "First part\nsecond part"
        );
    }

    #[test]
    fn code_line_is_verbatim_and_neighbours_are_formatted() {
        assert_eq!(
            fmt("see this.\nlet x  =  1\nand  that.", &en()),
            "See this.\nlet x  =  1\nand that."
        );
    }

    #[test]
    fn normalizes_spacing_per_line() {
        assert_eq!(
            fmt("hello  world .\n\nsecond  line ,  ok", &en()),
            "Hello world.\n\nSecond line, ok"
        );
    }

    #[test]
    fn native_casing_engines_keep_their_own_casing() {
        let opts = FormatOptions {
            language: "en",
            engine_casing_native: true,
            ..FormatOptions::default()
        };
        assert_eq!(fmt("hello. i think so.", &opts), "hello. i think so.");
        // Spacing is still normalized.
        assert_eq!(fmt("hello  world .", &opts), "hello world.");
    }

    #[test]
    fn turkish_casing_is_left_alone() {
        let opts = FormatOptions {
            language: "tr",
            ..FormatOptions::default()
        };
        assert_eq!(fmt("iyi günler. ilk söz.", &opts), "iyi günler. ilk söz.");
    }

    #[test]
    fn capitalizes_other_latin_languages() {
        let opts = FormatOptions {
            language: "es",
            ..FormatOptions::default()
        };
        assert_eq!(fmt("hola. ¿qué tal?", &opts), "Hola. ¿Qué tal?");
    }

    // --- standalone i ---

    #[test]
    fn beta5_pronoun_casing_preserves_flags_and_token_attachments() {
        for sample in [
            "Use grep -i and /i and \\i.",
            "Keep _i and/i and\\i and.i and_i.",
            ".i is a filename.",
            "Keep i/ i\\ i_ i- and i.txt.",
            "-i is a flag.",
            "Done. /i is a path.",
            "i/ is a path.",
        ] {
            assert_eq!(fmt(sample, &en()), sample, "sample: {sample}");
        }
        assert_eq!(
            fmt("so i, \"i\" and i'm sure i’ll go.", &en()),
            "So I, \"I\" and I'm sure I’ll go."
        );
    }

    #[test]
    fn standalone_i_is_capitalized_in_english() {
        assert_eq!(
            fmt("so i think i'm right and i've seen i'll go i'd say.", &en()),
            "So I think I'm right and I've seen I'll go I'd say."
        );
        // "I." reads as an initial, so no sentence starts after it.
        assert_eq!(fmt("then i. yes", &en()), "Then I. yes");
    }

    #[test]
    fn standalone_i_handles_typographic_apostrophe_and_punctuation() {
        assert_eq!(fmt("well, i’m here, i.", &en()), "Well, I’m here, I.");
    }

    #[test]
    fn i_is_not_touched_outside_english_or_inside_words() {
        let es = FormatOptions {
            language: "es",
            ..FormatOptions::default()
        };
        assert_eq!(fmt("dijo i luego.", &es), "Dijo i luego.");
        assert_eq!(fmt("this is it.", &en()), "This is it.");
        assert_eq!(fmt("see (i) below", &en()), "See (i) below");
    }

    #[test]
    fn english_region_codes_count_as_english() {
        let opts = FormatOptions {
            language: "en-US",
            ..FormatOptions::default()
        };
        assert_eq!(fmt("so i know.", &opts), "So I know.");
        let opts = FormatOptions {
            language: "en_GB",
            ..FormatOptions::default()
        };
        assert_eq!(fmt("so i know.", &opts), "So I know.");
    }

    // --- fillers ---

    #[test]
    fn review_preserves_short_uppercase_cutoffs_and_dictionary_terms() {
        for sample in [
            "The patient has B- blood.",
            "The patient has AB- blood.",
            "The patient has O- only.",
            "She earned an A- average.",
            "Keep I- intact.",
        ] {
            assert_eq!(fmt(sample, &fillers()), sample);
        }
        assert_eq!(fmt("th- the plan", &fillers()), "The plan");
        assert_eq!(fmt("I- I think", &fillers()), "I think");
        assert_eq!(fmt("I- I'm sure", &fillers()), "I'm sure");
        for term in ["th", "th-"] {
            let terms = [term.to_string()];
            let opts = FormatOptions {
                protected_terms: &terms,
                ..fillers()
            };
            assert_eq!(fmt("th- the plan", &opts), "th- the plan");
        }
    }

    #[test]
    fn review_preserves_paired_quoted_and_bracketed_fillers() {
        for sample in [
            "Write \"um\" on the board.",
            "Write 'uh', then go.",
            "Write “um” on the board.",
            "Write ‘um’ on the board.",
            "Write (um) on the board.",
            "Write [um] on the board.",
            "Write «um» on the board.",
        ] {
            assert_eq!(fmt(sample, &fillers()), sample);
        }
        assert_eq!(fmt("\"um, go now.\"", &fillers()), "\"Go now.\"");
    }

    #[test]
    fn review_standalone_i_respects_dictionary_spelling() {
        let terms = ["i".to_string()];
        let opts = FormatOptions {
            protected_terms: &terms,
            ..en()
        };
        assert_eq!(fmt("increment i by one", &opts), "Increment i by one");
        assert_eq!(fmt("i is a variable", &opts), "i is a variable");
    }

    #[test]
    fn beta5_fillers_preserve_acronyms_names_and_attached_tokens() {
        for sample in [
            "They took him to the ER.",
            "ER staff called Er today.",
            "Er, the engineer, called.",
            "Keep UM and UH as written.",
            "Use /er -er _er er/ er_ er- and er.txt.",
        ] {
            assert_eq!(fmt(sample, &fillers()), sample, "sample: {sample}");
        }
        assert_eq!(fmt("we er, went home.", &fillers()), "we went home.");
        assert_eq!(fmt("er, we went home.", &fillers()), "We went home.");
    }

    #[test]
    fn beta5_pipeline_formatter_preserves_real_words() {
        let stage = FormatStageSettings {
            paragraphs: false,
            remove_fillers: true,
        };
        for (input, expected) in [
            ("They took him to the ER.", "They took him to the ER."),
            ("What it is is a problem.", "What it is is a problem."),
            ("Use grep -i and /i.", "Use grep -i and /i."),
            ("um, i saw the the ER.", "I saw the ER."),
        ] {
            let result = transcription(input);
            let output = format_for_transcription(input, &result, Some("en"), stage, &[])
                .unwrap_or_else(|| input.to_string());
            assert_eq!(output, expected, "sample: {input}");
        }
    }

    #[test]
    fn removes_each_filler_word() {
        for filler in ["um", "umm", "uh", "uhh", "erm", "er", "hmm", "mm"] {
            let input = format!("we {filler} went home.");
            assert_eq!(fmt(&input, &fillers()), "we went home.", "filler: {filler}");
        }
    }

    #[test]
    fn filler_removal_preserves_all_caps_and_matches_whole_words() {
        assert_eq!(fmt("we UM went. Uh, ok.", &fillers()), "we UM went. Ok.");
        assert_eq!(
            fmt("the umbrella and the error.", &fillers()),
            "the umbrella and the error."
        );
    }

    #[test]
    fn filler_removal_cleans_adjacent_punctuation() {
        assert_eq!(fmt("Um, I think so.", &fillers()), "I think so.");
        assert_eq!(fmt("so, uh, we left.", &fillers()), "so, we left.");
        assert_eq!(fmt("so uh, we left.", &fillers()), "so we left.");
        assert_eq!(fmt("we went, uh.", &fillers()), "we went.");
    }

    #[test]
    fn filler_removal_recapitalizes_a_sentence_start() {
        assert_eq!(fmt("Um, the plan works.", &fillers()), "The plan works.");
        assert_eq!(
            fmt("It works. Uh, then we ship.", &fillers()),
            "It works. Then we ship."
        );
        assert_eq!(fmt("Um, uh, so it goes.", &fillers()), "So it goes.");
    }

    #[test]
    fn filler_that_is_a_whole_sentence_disappears() {
        assert_eq!(
            fmt("It works. Um. Then we ship.", &fillers()),
            "It works. Then we ship."
        );
    }

    #[test]
    fn filler_removal_never_touches_real_words() {
        for sample in [
            "I like it.",
            "That that is odd.",
            "She had had enough.",
            "It was very, very good.",
            "uh-huh, okay.",
            "mm-hmm.",
        ] {
            assert_eq!(fmt(sample, &fillers()), sample, "sample: {sample}");
        }
    }

    #[test]
    fn mm_after_a_number_is_a_unit() {
        assert_eq!(fmt("a 10 mm bolt.", &fillers()), "a 10 mm bolt.");
        assert_eq!(
            fmt("a 10mm bolt, mm 5 more.", &fillers()),
            "a 10mm bolt, mm 5 more."
        );
    }

    #[test]
    fn filler_removal_is_english_only() {
        let opts = FormatOptions {
            language: "de",
            remove_fillers: true,
            engine_casing_native: true,
            ..FormatOptions::default()
        };
        assert_eq!(fmt("ich uh gehe er heim.", &opts), "ich uh gehe er heim.");
    }

    #[test]
    fn filler_removal_is_off_by_default() {
        assert_eq!(fmt("we um went.", &en()), "We um went.");
    }

    #[test]
    fn a_take_of_only_fillers_is_kept() {
        assert_eq!(fmt("Um.", &fillers()), "Um.");
        assert_eq!(fmt("uh, um", &fillers()), "uh, um");
    }

    // --- stutters ---

    #[test]
    fn beta5_4d_repeats_preserve_meaningful_words_and_case_changes() {
        for sample in [
            "She received an A a week ago.",
            "Explain the difference between and and or.",
            "Keep the The label.",
            "Keep a A label.",
            "THE the meeting",
            "the The meeting",
        ] {
            assert_eq!(fmt(sample, &fillers()), sample, "sample: {sample}");
        }
        for (sample, expected) in [
            ("the the", "The"),
            ("The the meeting", "The meeting"),
            ("a a", "A"),
            ("I I think", "I think"),
            ("I i think", "I think"),
            ("ok. The the meeting", "ok. The meeting"),
        ] {
            assert_eq!(fmt(sample, &fillers()), expected, "sample: {sample}");
        }
    }

    #[test]
    fn beta5_4d_indented_fence_closers_stay_code() {
        for delimiter in ["~~~", "```"] {
            for indent in ["    ", "     ", "\t", " \t"] {
                let input = format!("{delimiter}text\n{indent}{delimiter}\num  is a variable .\n{delimiter}\nafter  code .");
                let expected = input.replace("after  code .", "after code.");
                assert_eq!(fmt(&input, &fillers()), expected, "indent: {indent:?}");
            }
        }
    }

    #[test]
    fn beta5_4d_indented_fence_openers_do_not_start_code() {
        for delimiter in ["~~~", "```"] {
            for indent in ["    ", "     ", "\t", " \t"] {
                let input = format!("{indent}{delimiter}text\nhello  world .");
                assert!(
                    fmt(&input, &fillers()).ends_with("\nhello world."),
                    "delimiter: {delimiter}, indent: {indent:?}"
                );
            }
        }
    }

    #[test]
    fn beta5_4d_fences_allow_up_to_three_spaces() {
        for delimiter in ["~~~", "```"] {
            for opening_indent in 0..=3 {
                for closing_indent in 0..=3 {
                    let input = format!(
                        "{}{delimiter}text\num  is a variable .\n{}{delimiter}\nafter  code .",
                        " ".repeat(opening_indent),
                        " ".repeat(closing_indent)
                    );
                    let expected = input.replace("after  code .", "after code.");
                    assert_eq!(fmt(&input, &fillers()), expected);
                }
            }
        }
    }

    #[test]
    fn beta5_4c_grammatical_doubles_are_not_stutters() {
        for sample in [
            "I told you you were right.",
            "Whatever you call it it works.",
            "I do what I have to to finish on time.",
            "That's unheard of of course.",
            "Did you get it? It works.",
            "Did we? We did.",
            "Should I? I should.",
            "She earned an A. A new record.",
            "Was that an? An article is missing.",
            "Use and. And then continue.",
            "Use the. The definite article fits.",
        ] {
            assert_eq!(fmt(sample, &fillers()), sample, "sample: {sample}");
        }
    }

    #[test]
    fn beta5_4c_fences_match_delimiter_and_minimum_length() {
        for delimiter in ['`', '~'] {
            let opening = delimiter.to_string().repeat(4);
            let wrong = if delimiter == '`' { "~~~~" } else { "````" };
            let short = delimiter.to_string().repeat(3);
            let closing = delimiter.to_string().repeat(5);
            let input = format!(
                "before  code .\n{opening}sh\n  echo um  .\n{wrong}\n{short}\necho uh\n{closing}\nafter  code ."
            );
            let expected = format!(
                "before code.\n{opening}sh\n  echo um  .\n{wrong}\n{short}\necho uh\n{closing}\nafter code."
            );
            assert_eq!(fmt(&input, &fillers()), expected, "delimiter: {delimiter}");
        }
        let tilde = "~~~sh\necho um\n~~~";
        assert_eq!(fmt(tilde, &fillers()), tilde);
    }

    #[test]
    fn beta5_4c_leading_inline_code_is_not_a_fence_opener() {
        for input in ["```echo```\nhello  world .", "```sh `echo`\nhello  world ."] {
            let expected = input.replace("hello  world .", "hello world.");
            assert_eq!(fmt(input, &fillers()), expected);
        }
        let tilde = "~~~sh `echo`\n  echo um  .\n~~~\nhello  world .";
        assert_eq!(
            fmt(tilde, &fillers()),
            "~~~sh `echo`\n  echo um  .\n~~~\nhello world."
        );
    }

    #[test]
    fn beta5_stutter_cleanup_preserves_double_copulas() {
        for sample in [
            "What it is is a problem.",
            "The thing is is that we need help.",
            "What the issue is is unclear.",
            "My point is is that it works.",
            "What it is is the the problem.",
        ] {
            let expected = sample.replace("the the", "the");
            assert_eq!(fmt(sample, &fillers()), expected, "sample: {sample}");
        }
        assert_eq!(
            fmt("so the the plan works.", &fillers()),
            "so the plan works."
        );
    }

    #[test]
    fn removes_cut_off_fragments() {
        assert_eq!(
            fmt("so th- the plan works.", &fillers()),
            "so the plan works."
        );
        assert_eq!(fmt("so I- I think so.", &fillers()), "so I think so.");
        assert_eq!(fmt("I- I think so.", &fillers()), "I think so.");
        assert_eq!(fmt("we should wh- what", &fillers()), "we should what");
        assert_eq!(fmt("I- I'm here.", &fillers()), "I'm here.");
    }

    #[test]
    fn cut_off_fragment_at_sentence_start_recapitalizes() {
        assert_eq!(fmt("Th- the plan works.", &fillers()), "The plan works.");
        assert_eq!(fmt("ok. th- the end.", &fillers()), "ok. The end.");
    }

    #[test]
    fn hyphen_that_is_not_a_cut_off_stays() {
        assert_eq!(
            fmt("a well- known fact.", &fillers()),
            "a well- known fact."
        );
        assert_eq!(
            fmt("pre- and post-release.", &fillers()),
            "pre- and post-release."
        );
        assert_eq!(fmt("a well-known fact.", &fillers()), "a well-known fact.");
    }

    #[test]
    fn removes_immediate_repeats_of_function_words() {
        assert_eq!(
            fmt("so the the plan works.", &fillers()),
            "so the plan works."
        );
        assert_eq!(fmt("the the plan works.", &fillers()), "The plan works.");
        assert_eq!(
            fmt("go to to the store and and buy.", &fillers()),
            "go to to the store and and buy."
        );
        assert_eq!(
            fmt("so we we are, you you know.", &fillers()),
            "so we are, you you know."
        );
        assert_eq!(
            fmt("so a a plan and and an an idea.", &fillers()),
            "so a plan and and an idea."
        );
        assert_eq!(fmt("so the the the end.", &fillers()), "so the end.");
        assert_eq!(fmt("I I think so.", &fillers()), "I think so.");
    }

    #[test]
    fn repeat_at_sentence_start_keeps_a_capital() {
        assert_eq!(fmt("The the plan works.", &fillers()), "The plan works.");
        assert_eq!(fmt("ok. The the end.", &fillers()), "ok. The end.");
    }

    #[test]
    fn repeats_outside_the_list_or_with_punctuation_stay() {
        for sample in [
            "that that is odd.",
            "she had had enough.",
            "very very good.",
            "no no no.",
            "you, you know.",
            "the. The end.",
        ] {
            assert_eq!(fmt(sample, &fillers()), sample, "sample: {sample}");
        }
    }

    #[test]
    fn filler_between_repeats_collapses_them() {
        assert_eq!(fmt("so the uh the plan.", &fillers()), "so the plan.");
        assert_eq!(fmt("the uh the plan.", &fillers()), "The plan.");
    }

    // --- paragraphs ---

    fn para_opts() -> FormatOptions<'static> {
        FormatOptions {
            language: "en",
            paragraphs: true,
            engine_casing_native: true,
            ..FormatOptions::default()
        }
    }

    #[test]
    fn long_take_breaks_at_a_long_pause_after_a_sentence_end() {
        let text = format!("{} {}", words(45, "."), words(20, "."));
        let hints = [PauseHint {
            words_before: 45,
            gap_ms: 2_000,
        }];
        let out = format_transcript(&text, &hints, &para_opts()).into_owned();
        assert_eq!(out, format!("{}\n\n{}", words(45, "."), words(20, ".")));
    }

    #[test]
    fn pause_in_the_middle_of_a_sentence_does_not_break() {
        let text = format!("{} {}", words(45, ""), words(20, "."));
        let hints = [PauseHint {
            words_before: 45,
            gap_ms: 2_000,
        }];
        let out = format_transcript(&text, &hints, &para_opts()).into_owned();
        assert!(!out.contains('\n'));
    }

    #[test]
    fn short_pause_does_not_break() {
        let text = format!("{} {}", words(45, "."), words(20, "."));
        let hints = [PauseHint {
            words_before: 45,
            gap_ms: 1_499,
        }];
        assert!(!format_transcript(&text, &hints, &para_opts()).contains('\n'));
    }

    #[test]
    fn short_take_never_breaks() {
        let text = format!("{} {}", words(10, "."), words(10, "."));
        let hints = [PauseHint {
            words_before: 10,
            gap_ms: 5_000,
        }];
        assert!(!format_transcript(&text, &hints, &para_opts()).contains('\n'));
    }

    #[test]
    fn thirty_seconds_of_audio_makes_a_take_long() {
        let text = format!("{} {}", words(40, "."), words(10, "."));
        let hints = [PauseHint {
            words_before: 40,
            gap_ms: 2_000,
        }];
        let mut opts = para_opts();
        assert!(!format_transcript(&text, &hints, &opts).contains('\n'));
        opts.audio_duration_ms = Some(30_000);
        assert!(format_transcript(&text, &hints, &opts).contains("\n\n"));
    }

    #[test]
    fn paragraphs_off_never_breaks() {
        let text = format!("{} {}", words(45, "."), words(20, "."));
        let hints = [PauseHint {
            words_before: 45,
            gap_ms: 2_000,
        }];
        let mut opts = para_opts();
        opts.paragraphs = false;
        assert!(!format_transcript(&text, &hints, &opts).contains('\n'));
    }

    #[test]
    fn no_break_without_pause_hints() {
        let text = format!("{} {}", words(45, "."), words(20, "."));
        assert!(!format_transcript(&text, &[], &para_opts()).contains('\n'));
    }

    #[test]
    fn at_most_one_break_per_forty_words() {
        let text = format!("{} {} {}", words(45, "."), words(20, "."), words(30, "."));
        let hints = [
            PauseHint {
                words_before: 45,
                gap_ms: 2_000,
            },
            PauseHint {
                words_before: 65,
                gap_ms: 3_000,
            },
        ];
        let out = format_transcript(&text, &hints, &para_opts()).into_owned();
        assert_eq!(out.matches("\n\n").count(), 1);
        assert!(out.contains(&format!("{}\n\n", words(45, "."))));
    }

    #[test]
    fn later_break_is_allowed_after_forty_more_words() {
        let text = format!("{} {} {}", words(45, "."), words(45, "."), words(20, "."));
        let hints = [
            PauseHint {
                words_before: 45,
                gap_ms: 2_000,
            },
            PauseHint {
                words_before: 90,
                gap_ms: 2_000,
            },
        ];
        let out = format_transcript(&text, &hints, &para_opts()).into_owned();
        assert_eq!(out.matches("\n\n").count(), 2);
    }

    #[test]
    fn no_break_before_a_very_short_last_sentence() {
        let text = format!("{} {}", words(65, "."), words(4, "."));
        let hints = [PauseHint {
            words_before: 65,
            gap_ms: 3_000,
        }];
        assert!(!format_transcript(&text, &hints, &para_opts()).contains('\n'));
    }

    #[test]
    fn no_break_inside_a_url() {
        let mut text = words(44, "");
        text.push_str(" https://example.com/page.");
        text.push(' ');
        text.push_str(&words(20, "."));
        let hints = [PauseHint {
            words_before: 45,
            gap_ms: 3_000,
        }];
        let out = format_transcript(&text, &hints, &para_opts()).into_owned();
        assert!(!out.contains('\n'));
    }

    #[test]
    fn engine_paragraphs_are_left_untouched() {
        let text = format!("{}\n\n{}", words(45, "."), words(45, "."));
        let hints = [PauseHint {
            words_before: 45,
            gap_ms: 3_000,
        }];
        let out = format_transcript(&text, &hints, &para_opts()).into_owned();
        assert_eq!(out, text);
    }

    #[test]
    fn break_survives_filler_removal_before_it() {
        let mut first = words(44, "");
        first.push_str(" um uh.");
        let text = format!("{} {}", first, words(20, "."));
        // The pause comes after the 45 words including the two fillers.
        let hints = [PauseHint {
            words_before: 46,
            gap_ms: 2_500,
        }];
        let mut opts = para_opts();
        opts.remove_fillers = true;
        let out = format_transcript(&text, &hints, &opts).into_owned();
        assert!(out.contains(".\n\n"), "{out}");
        assert!(!out.contains("um"));
    }

    #[test]
    fn capital_follows_a_paragraph_break() {
        let text = format!("{} {}", words(45, "."), words(20, "."));
        let hints = [PauseHint {
            words_before: 45,
            gap_ms: 2_000,
        }];
        let opts = FormatOptions {
            language: "en",
            paragraphs: true,
            ..FormatOptions::default()
        };
        let out = format_transcript(&text, &hints, &opts).into_owned();
        assert!(out.contains("\n\nWorda"), "{out}");
    }

    // --- pause hints ---

    #[test]
    fn hints_come_from_gaps_between_spans() {
        let spans = [
            TimedSpan {
                text: " one two.",
                start_ms: Some(0),
                end_ms: Some(1_000),
            },
            TimedSpan {
                text: " three.",
                start_ms: Some(3_200),
                end_ms: Some(4_000),
            },
            TimedSpan {
                text: " four five.",
                start_ms: Some(4_100),
                end_ms: Some(5_000),
            },
        ];
        let hints = pause_hints_from_spans(&spans, 5);
        assert_eq!(
            hints,
            vec![
                PauseHint {
                    words_before: 2,
                    gap_ms: 2_200
                },
                PauseHint {
                    words_before: 3,
                    gap_ms: 100
                },
            ]
        );
    }

    #[test]
    fn hints_require_matching_word_counts() {
        let spans = [
            TimedSpan {
                text: "one two.",
                start_ms: Some(0),
                end_ms: Some(1_000),
            },
            TimedSpan {
                text: "three.",
                start_ms: Some(3_000),
                end_ms: Some(4_000),
            },
        ];
        assert!(pause_hints_from_spans(&spans, 4).is_empty());
        assert_eq!(pause_hints_from_spans(&spans, 3).len(), 1);
    }

    #[test]
    fn hints_skip_untimed_gaps_and_ignore_punctuation_only_tokens() {
        let spans = [
            TimedSpan {
                text: "one ,",
                start_ms: None,
                end_ms: None,
            },
            TimedSpan {
                text: "two",
                start_ms: Some(2_000),
                end_ms: Some(3_000),
            },
            TimedSpan {
                text: "three",
                start_ms: Some(6_000),
                end_ms: Some(7_000),
            },
        ];
        let hints = pause_hints_from_spans(&spans, 3);
        assert_eq!(
            hints,
            vec![PauseHint {
                words_before: 2,
                gap_ms: 3_000
            }]
        );
    }

    fn transcription(text: &str) -> TranscriptionResult {
        let job = TranscriptionJob {
            source: TranscriptionSource::DesktopRecording,
            engine: "whisper".to_string(),
            model: "base".to_string(),
            spoken_language: Some("en".to_string()),
            task: TranscriptionTask::Transcribe,
        };
        TranscriptionResult::new(&job, text.to_string())
    }

    fn segment(text: &str, start_ms: u64, end_ms: u64) -> TranscriptionSegment {
        TranscriptionSegment {
            text: text.to_string(),
            start_ms: Some(start_ms),
            end_ms: Some(end_ms),
            speaker_id: None,
        }
    }

    #[test]
    fn transcription_hints_use_segments() {
        let result = transcription("one two. three four.").with_segments(vec![
            segment(" one two.", 0, 1_000),
            segment(" three four.", 4_000, 5_000),
        ]);
        let hints = pause_hints_for_transcription("one two. three four.", &result);
        assert_eq!(
            hints,
            vec![PauseHint {
                words_before: 2,
                gap_ms: 3_000
            }]
        );
        assert_eq!(audio_duration_ms(&result), Some(5_000));
    }

    #[test]
    fn transcription_hints_are_empty_when_text_was_rewritten() {
        let result = transcription("one two. three four.").with_segments(vec![
            segment(" one two.", 0, 1_000),
            segment(" three four.", 4_000, 5_000),
        ]);
        assert!(pause_hints_for_transcription("one two. three.", &result).is_empty());
        assert!(pause_hints_for_transcription("one two.\nthree four.", &result).is_empty());
    }

    #[test]
    fn transcription_hints_prefer_timed_words() {
        let mut result = transcription("a b c");
        result.words = Some(
            ["a", "b", "c"]
                .iter()
                .enumerate()
                .map(|(i, text)| TranscriptionWord {
                    text: (*text).to_string(),
                    start_ms: Some(i as u64 * 3_000),
                    end_ms: Some(i as u64 * 3_000 + 500),
                    speaker_id: None,
                    confidence: None,
                })
                .collect(),
        );
        let hints = pause_hints_for_transcription("a b c", &result);
        assert_eq!(hints.len(), 2);
        assert_eq!(
            hints[0],
            PauseHint {
                words_before: 1,
                gap_ms: 2_500
            }
        );
    }

    #[test]
    fn native_casing_engines_are_named() {
        assert!(engine_has_native_casing("parakeet"));
        assert!(engine_has_native_casing("Soniox"));
        assert!(engine_has_native_casing("deepgram"));
        assert!(!engine_has_native_casing("whisper"));
        assert!(!engine_has_native_casing("groq"));
    }

    // --- scripts without English rules ---

    #[test]
    fn bangla_hindi_and_cjk_are_untouched() {
        let opts = FormatOptions {
            language: "en",
            remove_fillers: true,
            paragraphs: true,
            ..FormatOptions::default()
        };
        for sample in [
            "আমি ভালো আছি। তুমি কেমন আছো?",
            "मैं ठीक हूँ। आप कैसे हैं?",
            "今日は天気がいいです。散歩に行きましょう。",
            "你好，我今天很好。",
        ] {
            assert_eq!(fmt(sample, &opts), sample, "sample: {sample}");
        }
    }

    #[test]
    fn non_latin_text_with_stray_spaces_is_only_space_normalized() {
        assert_eq!(fmt("আমি  ভালো .", &en()), "আমি ভালো.");
    }

    // --- pipeline glue ---

    #[test]
    fn stage_runs_only_for_deterministic_non_literal_output() {
        assert!(should_run_format_stage(false, false));
        assert!(!should_run_format_stage(false, true));
        assert!(!should_run_format_stage(true, false));
        assert!(!should_run_format_stage(true, true));
    }

    #[test]
    fn glue_formats_with_engine_casing_and_language() {
        let stage = FormatStageSettings {
            paragraphs: true,
            remove_fillers: false,
        };
        let whisper = transcription("hello world. i agree");
        assert_eq!(
            format_for_transcription("hello world. i agree", &whisper, Some("en"), stage, &[]),
            Some("Hello world. I agree".to_string())
        );
        let mut parakeet = transcription("hello world. i agree");
        parakeet.engine = "parakeet".to_string();
        assert_eq!(
            format_for_transcription("hello world. i agree", &parakeet, Some("en"), stage, &[]),
            None
        );
    }

    #[test]
    fn glue_inserts_paragraphs_from_segment_gaps() {
        let first = words(45, ".");
        let second = words(20, ".");
        let text = format!("{first} {second}");
        let result = transcription(&text).with_segments(vec![
            segment(&format!(" {first}"), 0, 20_000),
            segment(&format!(" {second}"), 23_000, 30_000),
        ]);
        let stage = FormatStageSettings {
            paragraphs: true,
            remove_fillers: false,
        };
        let out = format_for_transcription(&text, &result, Some("en"), stage, &[]).unwrap();
        assert!(out.contains(".\n\n"));
        let off = FormatStageSettings {
            paragraphs: false,
            remove_fillers: false,
        };
        let out = format_for_transcription(&text, &result, Some("en"), off, &[]).unwrap();
        assert!(!out.contains('\n'));
    }

    #[test]
    fn protected_terms_come_from_enabled_library_entries() {
        use crate::writing::{CustomWord, TextReplacementRule};
        let settings = WritingSettings {
            replacements: vec![
                TextReplacementRule {
                    from: "a".into(),
                    to: "kubectl".into(),
                    language: None,
                    enabled: true,
                },
                TextReplacementRule {
                    from: "b".into(),
                    to: "off".into(),
                    language: None,
                    enabled: false,
                },
            ],
            custom_words: vec![CustomWord {
                phrase: "pnpm".into(),
                spoken_form: None,
                language: None,
                enabled: true,
            }],
            ..WritingSettings::default()
        };
        assert_eq!(protected_terms(&settings), vec!["kubectl", "pnpm"]);
    }

    // --- idempotence and edge cases ---

    #[test]
    fn formatting_twice_equals_formatting_once() {
        let samples = [
            "um, i think th- the plan works. so, uh, we we go. is it? yes.",
            "hello  world .\n\nsecond  line ,  ok. then i go",
            "see https://example.com/a. then user@example.com. ok",
            "Th- the the end. Um. It works.",
            "he said \"go.\" then left... maybe? ok",
            "let x = 1\nafter that. i guess",
        ];
        let opts = FormatOptions {
            language: "en",
            remove_fillers: true,
            paragraphs: true,
            ..FormatOptions::default()
        };
        for sample in samples {
            let once = format_transcript(sample, &[], &opts).into_owned();
            let twice = format_transcript(&once, &[], &opts).into_owned();
            assert_eq!(once, twice, "sample: {sample}");
        }
    }

    #[test]
    fn paragraph_output_is_idempotent() {
        let text = format!("{} {}", words(45, "."), words(20, "."));
        let hints = [PauseHint {
            words_before: 45,
            gap_ms: 2_000,
        }];
        let once = format_transcript(&text, &hints, &para_opts()).into_owned();
        let twice = format_transcript(&once, &hints, &para_opts()).into_owned();
        assert_eq!(once, twice);
    }

    #[test]
    fn empty_and_blank_text_is_borrowed() {
        assert!(matches!(
            format_transcript("", &[], &en()),
            Cow::Borrowed("")
        ));
        assert!(matches!(
            format_transcript("   ", &[], &en()),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn trailing_space_is_kept() {
        assert_eq!(fmt("hello there. ", &en()), "Hello there. ");
    }

    #[test]
    fn lone_punctuation_tokens_survive() {
        assert_eq!(fmt("one - two. three", &en()), "One - two. Three");
    }
}
