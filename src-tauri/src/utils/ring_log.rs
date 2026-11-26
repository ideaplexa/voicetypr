//! Always-on in-memory DEBUG ring (plan 060).
//!
//! tauri-plugin-log owns the global logger and, in release builds, the file
//! sink is filtered at Info — so `log::debug!` reaches no sink in production
//! and bug reports can never explain a failure's inner timings, budgets, or
//! backend decisions. The ring keeps the most recent DEBUG lines in RAM
//! (entry- and byte-capped) and every bug report attaches a redacted dump.
//! There is no user-facing toggle: users cannot get it wrong because there
//! is nothing to get right.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::LazyLock;

use tauri_plugin_log::fern;

/// Scrub a complete record before fern dispatches it to any output target.
pub fn scrub_log_message(message: &str) -> String {
    static HOME: LazyLock<Option<String>> = LazyLock::new(|| {
        dirs::home_dir()
            .map(|path| path.to_string_lossy().into_owned())
            .filter(|path| !path.is_empty())
    });
    static HOME_PREFIX: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r#"(?i)(?:/Users/[^/\r\n"',):]+|/home/[^/\r\n"',):]+|[A-Z]:\\Users\\[^\\\r\n"',):]+)"#,
        )
        .unwrap()
    });
    static URL: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"(?i)\b[a-z][a-z0-9+.-]*://[^\s"'<>]+"#).unwrap());
    let result = URL.replace_all(message, |caps: &regex::Captures<'_>| {
        let url = &caps[0];
        let Some((scheme, rest)) = url.split_once("://") else {
            return url.to_string();
        };
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let authority = &rest[..end];
        let host = authority.rsplit('@').next().unwrap_or(authority);
        let suffix = &rest[end..];
        let suffix = suffix.split(['?', '#']).next().unwrap_or("");
        format!("{scheme}://{host}{suffix}")
    });
    let result = if let Some(home) = HOME.as_ref() {
        result.replace(home.as_str(), "~")
    } else {
        result.into_owned()
    };
    redact_local_paths(&HOME_PREFIX.replace_all(&result, "~"))
}

/// Absolute paths outside the home folder (other volumes, the app bundle,
/// temp folders, drive letters, network shares, file URLs) become
/// `[PATH_REDACTED]`. A path may contain spaces, apostrophes and
/// parentheses; it ends at a quote, a line end, or punctuation that closes it
/// ("path: reason", "path, next", "(path)").
pub fn redact_local_paths(message: &str) -> String {
    static START: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"(?m)(?:^|[\s"'(=\[]|file://)(/[^/\s"'<>]+/|[A-Za-z]:[\\/]|\\\\)"#)
            .unwrap()
    });
    let bytes = message.as_bytes();
    let closes = |i: usize| bytes.get(i + 1).is_none_or(|n| n.is_ascii_whitespace());
    let mut out = String::with_capacity(message.len());
    let mut pos = 0;
    while let Some(found) = START.captures_at(message, pos).and_then(|c| c.get(1)) {
        let (start, mut end, mut depth) = (found.start(), found.end(), 0usize);
        while end < bytes.len() {
            match bytes[end] {
                b'\r' | b'\n' | b'"' | b'<' | b'>' | b'`' => break,
                b'(' => depth += 1,
                b')' if depth == 0 => break,
                b')' => depth -= 1,
                b'\'' | b',' | b';' | b':' | b']' if closes(end) => break,
                b' ' if bytes.get(end + 1) == Some(&b'(') => {
                    // "C:\Program Files (x86)\…" continues; "path (os error 2)" ends.
                    let inner = message[end + 2..].find(')').map(|k| end + 3 + k);
                    if !inner.is_some_and(|k| matches!(bytes.get(k), Some(b'/' | b'\\'))) {
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
        out.push_str(&message[pos..start]);
        out.push_str("[PATH_REDACTED]");
        pos = end;
    }
    out.push_str(&message[pos..]);
    out
}

/// For text that comes from native code or third-party errors, where a path
/// can hold spaces, parentheses or a file name and no pattern can find its
/// end: everything from the first path-like token to the end of its line
/// becomes `[PATH_REDACTED]`. A path-like token starts with `/`, `~/`, `./`,
/// `../`, `\\\\`, a drive letter or a URL scheme, or contains `/` or `\\` next to
/// a letter. Counts such as `1/3` and sizes such as `~500MB` are kept.
pub fn redact_all_paths(message: &str) -> String {
    message
        .split('\n')
        .map(|line| {
            let mut offset = 0;
            for token in line.split(' ') {
                let core = token.trim_start_matches(|c: char| "\"'([{<=`".contains(c));
                let start = offset + (token.len() - core.len());
                offset += token.len() + 1;
                let bytes = core.as_bytes();
                let drive = bytes.len() > 2
                    && bytes[0].is_ascii_alphabetic()
                    && bytes[1] == b':'
                    && matches!(bytes[2], b'/' | b'\\');
                let separated = (core.contains('/') || core.contains('\\'))
                    && core.chars().any(|c| c.is_alphabetic());
                if core.starts_with('/')
                    || core.starts_with("~/")
                    || core.starts_with("./")
                    || core.starts_with("../")
                    || core.starts_with("\\\\")
                    || core.contains("://")
                    || drive
                    || separated
                {
                    return format!("{}[PATH_REDACTED]", &line[..start]);
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const MAX_ENTRIES: usize = 1000;
const MAX_TOTAL_BYTES: usize = 256 * 1024;

#[derive(Default)]
struct State {
    queue: VecDeque<String>,
    bytes: usize,
}

#[derive(Default)]
struct DebugRing {
    state: parking_lot::Mutex<State>,
}

impl DebugRing {
    fn push_line(&self, line: &str) {
        let mut state = self.state.lock();
        state.bytes += line.len();
        state.queue.push_back(line.to_string());
        while state.bytes > MAX_TOTAL_BYTES || state.queue.len() > MAX_ENTRIES {
            let Some(dropped) = state.queue.pop_front() else {
                break;
            };
            state.bytes = state.bytes.saturating_sub(dropped.len());
        }
    }

    /// Most recent lines, oldest first, capped at `max_bytes`.
    fn snapshot_lines(&self, max_bytes: usize) -> Vec<String> {
        let state = self.state.lock();
        let mut out = Vec::new();
        let mut used = 0usize;
        for line in state.queue.iter().rev() {
            used = used.saturating_add(line.len());
            if used > max_bytes {
                break;
            }
            out.push(line.clone());
        }
        out.reverse();
        out
    }
}

static RING: LazyLock<DebugRing> = LazyLock::new(DebugRing::default);

/// Fern sink: receives every formatted record that passes the global level
/// filter. fern composes a record via `write!` and may deliver it in
/// MULTIPLE `write()` fragments (message, then line separator), so partial
/// writes accumulate in a line buffer and only complete lines enter the
/// ring — otherwise a single record would be split into fabricated lines
/// and redaction patterns could miss secrets split across fragments.
struct RingWriter {
    pending: Vec<u8>,
}

/// A pathological record with no newline must not grow `pending` unboundedly.
const MAX_PENDING_BYTES: usize = 64 * 1024;

impl Write for RingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.pending.extend_from_slice(buf);
        let mut consumed = 0usize;
        while let Some(idx) = self.pending[consumed..].iter().position(|&b| b == b'\n') {
            let end = consumed + idx;
            let line = String::from_utf8_lossy(&self.pending[consumed..end]);
            RING.push_line(line.trim_end_matches('\r'));
            consumed = end + 1;
        }
        self.pending.drain(..consumed);
        if self.pending.len() > MAX_PENDING_BYTES {
            // Flush the oversized fragment as its own line to bound memory.
            let line = String::from_utf8_lossy(&self.pending).into_owned();
            RING.push_line(line.trim_end_matches(['\r', '\n']));
            self.pending.clear();
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The dispatch handed to `tauri_plugin_log::TargetKind::Dispatch` — the
/// only release-build sink that sees DEBUG records.
pub fn ring_dispatch() -> fern::Dispatch {
    fern::Dispatch::new().chain(Box::new(RingWriter {
        pending: Vec::new(),
    }) as Box<dyn Write + Send>)
}

/// Capped snapshot joined with newlines, ready for `redact_log_content`.
pub fn snapshot_joined(max_bytes: usize) -> String {
    RING.snapshot_lines(max_bytes).join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_error_text_keeps_no_path_of_any_kind() {
        for (line, expected) in [
            (
                "Audio file not found: /Users/maya/Library/Caches/rec 1.wav",
                "Audio file not found: [PATH_REDACTED]",
            ),
            (
                "Vocabulary missing at ~/Library/Application Support/FluidAudio/Private meeting.wav. Please re-download",
                "Vocabulary missing at [PATH_REDACTED]",
            ),
            ("open ./private meeting.wav failed", "open [PATH_REDACTED]"),
            (
                "C:\\Users\\maya\\Voice Notes\\Session (Private) Final.wav missing",
                "[PATH_REDACTED]",
            ),
            (
                "Failed to download parakeet-tdt/encoder.mlmodelc (404)",
                "Failed to download [PATH_REDACTED]",
            ),
            ("could not read 'clips\\take.wav': denied", "could not read '[PATH_REDACTED]"),
            ("無効 録音/昨日 です", "無効 [PATH_REDACTED]"),
            (
                "fetch file://server/share/Voice Notes.wav\nretrying",
                "fetch [PATH_REDACTED]\nretrying",
            ),
            ("downloading 1/3", "downloading 1/3"),
            ("This will download ~500MB", "This will download ~500MB"),
            ("load failed: invalidModel", "load failed: invalidModel"),
        ] {
            assert_eq!(redact_all_paths(line), expected);
        }
    }

    #[test]
    fn paths_outside_home_never_reach_a_sink() {
        for (line, expected) in [
            (
                "Creating sidecar /Applications/Voicetypr.app/Contents/MacOS/voicetypr-parakeet",
                "Creating sidecar [PATH_REDACTED]",
            ),
            (
                "Creating sidecar /Volumes/Client's Drive/voicetypr/target/debug/voicetypr-parakeet",
                "Creating sidecar [PATH_REDACTED]",
            ),
            (
                "Creating sidecar C:\\Program Files (x86)\\Voicetypr\\voicetypr-parakeet.exe",
                "Creating sidecar [PATH_REDACTED]",
            ),
            (
                "Creating sidecar D:/Apps/Voicetypr/voicetypr-parakeet.exe",
                "Creating sidecar [PATH_REDACTED]",
            ),
            (
                "Opening \\\\nas\\share\\clips\\take.wav",
                "Opening [PATH_REDACTED]",
            ),
            (
                "temp file \"/private/var/folders/xy/T/recording.wav\"",
                "temp file \"[PATH_REDACTED]\"",
            ),
            (
                "Failed to load /Applications/Voicetypr.app/model.bin: Permission denied (os error 13)",
                "Failed to load [PATH_REDACTED]: Permission denied (os error 13)",
            ),
            (
                "open failed for /data/Client/private/take.wav (os error 2)",
                "open failed for [PATH_REDACTED] (os error 2)",
            ),
            (
                "loading file:///Volumes/ClientDrive/private/take.wav, retrying",
                "loading file://[PATH_REDACTED], retrying",
            ),
        ] {
            assert_eq!(scrub_log_message(line), expected);
        }
    }

    #[test]
    fn scrubbing_keeps_home_relative_paths_urls_and_ordinary_text() {
        assert_eq!(
            scrub_log_message("models in /Users/alice/Library/Application Support/x"),
            "models in ~/Library/Application Support/x"
        );
        assert_eq!(
            scrub_log_message("POST https://user:pass@api.example.com/trial/check took 12ms"),
            "POST https://api.example.com/trial/check took 12ms"
        );
        assert_eq!(
            scrub_log_message("ratio 3/4 and/or 12:30, done"),
            "ratio 3/4 and/or 12:30, done"
        );
    }

    #[test]
    fn ring_keeps_most_recent_lines_under_entry_cap() {
        let lines: Vec<String> = (0..=MAX_ENTRIES).map(|i| format!("line-{i}")).collect();
        let ring = DebugRing::default();
        for line in &lines {
            ring.push_line(line);
        }

        let snapshot = ring.snapshot_lines(usize::MAX);
        assert_eq!(snapshot.len(), MAX_ENTRIES, "one over the cap must drop");
        assert_eq!(snapshot.first().map(String::as_str), Some("line-1"));
        assert_eq!(snapshot.last().map(String::as_str), Some("line-1000"));
    }

    #[test]
    fn ring_enforces_total_byte_cap() {
        let big = "x".repeat(600);
        let ring = DebugRing::default();
        for _ in 0..1000 {
            ring.push_line(&big);
        }

        let state = ring.state.lock();
        assert!(state.bytes <= MAX_TOTAL_BYTES);
        assert!(state.queue.len() < 1000, "byte cap must evict entries");
    }

    #[test]
    fn snapshot_byte_cap_takes_most_recent_tail() {
        let ring = DebugRing::default();
        for line in ["a", "b", "c", "d"] {
            ring.push_line(line);
        }
        let snapshot = ring.snapshot_lines(2);
        assert_eq!(snapshot, vec!["c", "d"], "oldest lines drop first");
    }

    #[test]
    fn ring_writer_buffers_fragments_until_complete_lines() {
        let mut writer = RingWriter {
            pending: Vec::new(),
        };
        // fern-style fragmentation: message fragment without a newline...
        writer.write_all(b"WHISPER_BACKEND cpu").unwrap();
        // ...then the line separator arrives as its own write.
        writer.write_all(b"\n").unwrap();

        let snapshot = RING.snapshot_lines(usize::MAX);
        assert_eq!(
            snapshot.last().map(String::as_str),
            Some("WHISPER_BACKEND cpu"),
            "fragments of one record must join into a single ring line"
        );
    }
}
