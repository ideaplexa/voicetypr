use serde::{Deserialize, Serialize};

use crate::writing::ContextHint;

/// Curated Chat substrings.
const CHAT: &[&str] = &[
    "slack", "discord", "telegram", "whatsapp", "messages", "signal", "teams",
];

/// Curated Email substrings.
const EMAIL: &[&str] = &[
    "mail",
    "gmail",
    "hotmail",
    "protonmail",
    "fastmail",
    "outlook",
    "spark",
    "airmail",
    "thunderbird",
    "superhuman",
];

/// Curated Code substrings.
const CODE: &[&str] = &[
    "code",
    "cursor",
    "xcode",
    "intellij",
    "pycharm",
    "webstorm",
    "zed",
    "sublime",
    "android studio",
    "nova",
];

/// Curated Terminal substrings.
const TERMINAL: &[&str] = &[
    "terminal",
    "iterm",
    "warp",
    "alacritty",
    "kitty",
    "wezterm",
    "ghostty",
    "powershell",
    "cmd",
    "windowsterminal",
];

/// Curated Docs substrings.
const DOCS: &[&str] = &["word", "pages", "google docs", "confluence", "libreoffice"];

/// Curated Social substrings.
const SOCIAL: &[&str] = &["x", "twitter", "mastodon", "reddit", "linkedin", "bluesky"];

/// Curated Notes substrings.
const NOTES: &[&str] = &["notes", "obsidian", "bear", "craft", "logseq"];

/// Curated Browser substrings (neutral category — checked last).
const BROWSER: &[&str] = &["safari", "chrome", "firefox", "arc", "edge", "brave"];

/// Coarse-grained category of the app the user is dictating into.
///
/// Drives a behavioral nudge injected into the Polish prompt (see
/// [`category_prompt_hint`]). Classification happens locally from the active app
/// identity; only the coarse category guidance, never the raw identity, reaches AI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCategory {
    Chat,
    Email,
    Docs,
    Code,
    Terminal,
    Social,
    Notes,
    Browser,
    Other,
}

/// True when `name` contains any curated token as a whole-word occurrence.
///
/// A token matches only when an occurrence is bounded on BOTH sides by a
/// non-ASCII-alphanumeric character (or the start/end of the string). This
/// avoids the false positives plain substring matching would cause — e.g.
/// "1Password" contains "word" but must not classify as Docs, "Barcode
/// Scanner" contains "code" but must not classify as Code, and "firefox"
/// contains "x" but must not classify as Social. Whole-word matching also
/// handles the single-character "x" token, so no special case is needed.
/// Multi-word phrase tokens ("google docs", "android studio") and fused
/// process-name tokens ("windowsterminal") still match.
fn matches_any(name: &str, tokens: &[&str]) -> bool {
    tokens
        .iter()
        .any(|token| token_occurs_whole_word(name, token))
}

/// True when `token` occurs in `name` with ASCII-alphanumeric boundaries on
/// both sides (or the string edge). See [`matches_any`].
fn token_occurs_whole_word(name: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let mut search_from = 0;
    while let Some(rel) = name[search_from..].find(token) {
        let start = search_from + rel;
        let end = start + token.len();
        let left_is_boundary = start == 0
            || !name[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric());
        let right_is_boundary = end == name.len()
            || !name[end..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric());
        if left_is_boundary && right_is_boundary {
            return true;
        }
        search_from = start + 1;
    }
    false
}

/// Classify a lowercased app name or process stem against the curated taxonomy.
///
/// Categories are checked in priority order: specific behavioral categories
/// first, neutral `Browser` last. Returns `None` when nothing matches.
fn classify_name(name: &str) -> Option<AppCategory> {
    if matches_any(name, CHAT) {
        return Some(AppCategory::Chat);
    }
    if matches_any(name, EMAIL) {
        return Some(AppCategory::Email);
    }
    if matches_any(name, CODE) {
        return Some(AppCategory::Code);
    }
    if matches_any(name, TERMINAL) {
        return Some(AppCategory::Terminal);
    }
    if matches_any(name, DOCS) {
        return Some(AppCategory::Docs);
    }
    if matches_any(name, SOCIAL) {
        return Some(AppCategory::Social);
    }
    if matches_any(name, NOTES) {
        return Some(AppCategory::Notes);
    }
    if matches_any(name, BROWSER) {
        return Some(AppCategory::Browser);
    }
    None
}

/// Lowercased file stem of a process path, handling both POSIX `/` and
/// Windows `\` separators. Done manually (not via [`std::path::Path`]) so the
/// stem resolves correctly regardless of the host platform — e.g. a Windows
/// path captured on a machine classified on macOS.
fn process_stem_lower(process_path: &str) -> Option<String> {
    let file_name = process_path.rsplit(['/', '\\']).next()?;
    let stem = file_name
        .rsplit_once('.')
        .map(|(base, _ext)| base)
        .unwrap_or(file_name);
    let lower = stem.to_lowercase();
    (!lower.is_empty()).then_some(lower)
}

/// Classify the active app into an [`AppCategory`].
///
/// Tries the lowercased `app_name` first, then falls back to the lowercased
/// file stem of `process_path`. Returns [`AppCategory::Other`] when neither is
/// present or nothing matches.
pub fn classify(hint: &ContextHint) -> AppCategory {
    if let Some(name) = hint.app_name.as_deref() {
        let lower = name.to_lowercase();
        if let Some(category) = classify_name(&lower) {
            return category;
        }
    }
    if let Some(path) = hint.process_path.as_deref() {
        if let Some(stem) = process_stem_lower(path) {
            if let Some(category) = classify_name(&stem) {
                return category;
            }
        }
    }
    AppCategory::Other
}

/// Behavioral nudge text injected into the polish prompt for a category.
///
/// Returns `None` for neutral categories ([`AppCategory::Browser`] and
/// [`AppCategory::Other`]) so no nudge is added.
pub fn category_prompt_hint(category: AppCategory) -> Option<&'static str> {
    match category {
        AppCategory::Chat => Some("Keep it casual and conversational. For a short single sentence, omit the trailing period. Preserve emoji and @mentions."),
        AppCategory::Email => Some("Structure as clear prose with proper punctuation. Keep it professional but natural."),
        AppCategory::Docs => Some("Write well-structured prose with full punctuation. Add paragraph breaks where the speaker paused."),
        AppCategory::Code => Some("Keep identifiers, camelCase, snake_case, and symbols exactly as spoken. Do not wrap them in prose."),
        AppCategory::Terminal => Some("Treat this as a literal command line. Do not change capitalization or add a trailing period. Preserve flags and paths exactly."),
        AppCategory::Social => Some("Keep it short, punchy, and casual. Preserve hashtags, @handles, and emoji. For a one-liner, omit the trailing period."),
        AppCategory::Notes => Some("Use a terse note style. Keep sentence fragments as fragments."),
        AppCategory::Browser => None,
        AppCategory::Other => None,
    }
}

/// Human-readable label for a category (e.g. for the history badge).
pub fn category_label(category: AppCategory) -> &'static str {
    match category {
        AppCategory::Chat => "Chat",
        AppCategory::Email => "Email",
        AppCategory::Docs => "Docs",
        AppCategory::Code => "Code",
        AppCategory::Terminal => "Terminal",
        AppCategory::Social => "Social",
        AppCategory::Notes => "Notes",
        AppCategory::Browser => "Browser",
        AppCategory::Other => "Other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writing::ContextHint;

    fn hint(app_name: Option<&str>, process_path: Option<&str>) -> ContextHint {
        ContextHint {
            app_name: app_name.map(str::to_string),
            process_path: process_path.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn classify_maps_known_apps_by_name() {
        assert_eq!(classify(&hint(Some("Slack"), None)), AppCategory::Chat);
        assert_eq!(classify(&hint(Some("Discord"), None)), AppCategory::Chat);
        assert_eq!(classify(&hint(Some("Mail"), None)), AppCategory::Email);
        assert_eq!(classify(&hint(Some("Outlook"), None)), AppCategory::Email);
        assert_eq!(classify(&hint(Some("Code"), None)), AppCategory::Code);
        assert_eq!(classify(&hint(Some("Cursor"), None)), AppCategory::Code);
        assert_eq!(
            classify(&hint(Some("Terminal"), None)),
            AppCategory::Terminal
        );
        assert_eq!(classify(&hint(Some("iTerm"), None)), AppCategory::Terminal);
        assert_eq!(classify(&hint(Some("Obsidian"), None)), AppCategory::Notes);
    }

    #[test]
    fn classify_maps_windows_process_path_stem() {
        let code_path = "C:\\Users\\dev\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe";
        assert_eq!(classify(&hint(None, Some(code_path))), AppCategory::Code);
        assert_eq!(classify(&hint(None, Some("Slack.exe"))), AppCategory::Chat);
        assert_eq!(
            classify(&hint(None, Some("WhatsApp.exe"))),
            AppCategory::Chat
        );
    }

    #[test]
    fn classify_falls_back_to_process_path_when_app_name_unknown() {
        assert_eq!(
            classify(&hint(
                Some("SomeUnknownApp"),
                Some("/applications/xcode.app")
            )),
            AppCategory::Code
        );
    }

    #[test]
    fn classify_returns_other_for_unknown() {
        assert_eq!(classify(&hint(Some("AcmeEdit"), None)), AppCategory::Other);
        assert_eq!(classify(&hint(Some(""), None)), AppCategory::Other);
    }

    #[test]
    fn classify_returns_browser_for_browsers() {
        assert_eq!(classify(&hint(Some("Safari"), None)), AppCategory::Browser);
        assert_eq!(classify(&hint(Some("Chrome"), None)), AppCategory::Browser);
        assert_eq!(classify(&hint(Some("Firefox"), None)), AppCategory::Browser);
    }

    #[test]
    fn classify_returns_terminal_for_ghostty() {
        assert_eq!(
            classify(&hint(Some("Ghostty"), Some("/Applications/Ghostty.app"))),
            AppCategory::Terminal
        );
    }

    #[test]
    fn category_prompt_hint_returns_some_for_behavioral_categories() {
        for category in [
            AppCategory::Chat,
            AppCategory::Email,
            AppCategory::Docs,
            AppCategory::Code,
            AppCategory::Terminal,
            AppCategory::Social,
            AppCategory::Notes,
        ] {
            let hint_text = category_prompt_hint(category)
                .expect("behavioral category should have a prompt hint");
            assert!(!hint_text.is_empty(), "hint for {category:?} is empty");
        }
    }

    #[test]
    fn category_prompt_hint_returns_none_for_browser_and_other() {
        assert!(category_prompt_hint(AppCategory::Browser).is_none());
        assert!(category_prompt_hint(AppCategory::Other).is_none());
    }

    #[test]
    fn category_label_returns_human_readable() {
        assert_eq!(category_label(AppCategory::Chat), "Chat");
        assert_eq!(category_label(AppCategory::Terminal), "Terminal");
        assert_eq!(category_label(AppCategory::Other), "Other");
    }
    #[test]
    fn classify_avoids_false_positive_substring_matches() {
        // Whole-word matching: substrings fused inside larger names must not fire.
        assert_eq!(classify(&hint(Some("1Password"), None)), AppCategory::Other);
        assert_eq!(
            classify(&hint(Some("Barcode Scanner"), None)),
            AppCategory::Other
        );
        assert_eq!(classify(&hint(Some("WordPress"), None)), AppCategory::Other);
    }

    #[test]
    fn classify_preserves_compound_and_true_positive_matches() {
        // Compound email clients still classify (carried as explicit tokens).
        assert_eq!(classify(&hint(Some("Gmail"), None)), AppCategory::Email);
        // True positives that rely on whole-word boundaries.
        assert_eq!(
            classify(&hint(Some("Microsoft Word"), None)),
            AppCategory::Docs
        );
        assert_eq!(classify(&hint(Some("VS Code"), None)), AppCategory::Code);
        assert_eq!(classify(&hint(Some("Xcode"), None)), AppCategory::Code);
        assert_eq!(classify(&hint(Some("Firefox"), None)), AppCategory::Browser);
        assert_eq!(classify(&hint(Some("X"), None)), AppCategory::Social);
        assert_eq!(classify(&hint(Some("Slack"), None)), AppCategory::Chat);
    }
}
