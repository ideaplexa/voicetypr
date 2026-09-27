use chrono::{Local, NaiveDate};
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::sync::OnceLock;
use tauri::Manager;

#[tauri::command]
pub async fn clear_old_logs(app: tauri::AppHandle, days_to_keep: u32) -> Result<u32, String> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    if !log_dir.exists() {
        return Ok(0);
    }

    let cutoff_date = Local::now().date_naive() - chrono::Duration::days(days_to_keep as i64);
    let mut deleted_count = 0;

    let entries =
        fs::read_dir(&log_dir).map_err(|e| format!("Failed to read log directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_file() {
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if let Some(file_date) = voicetypr_log_date(&file_name) {
                if file_date < cutoff_date {
                    fs::remove_file(&path)
                        .map_err(|e| format!("Failed to delete log file: {}", e))?;
                    deleted_count += 1;
                    log::info!("Deleted old log file: {}", file_name);
                }
            }
        }
    }

    Ok(deleted_count)
}

/// Strict Voicetypr log-name shape: `voicetypr-YYYY-MM-DD.log` or a rotation
/// `voicetypr-YYYY-MM-DD.log.N` with a NUMERIC suffix only. Anything else
/// sharing the prefix (e.g. `voicetypr-2026-01-01.log.1.exe`) is NOT a log
/// and must never be deleted or attached to a report. Returns the date
/// as a validated calendar date.
pub(crate) fn voicetypr_log_date(file_name: &str) -> Option<NaiveDate> {
    let stem = file_name.strip_prefix("voicetypr-")?;
    let date = if let Some(date) = stem.strip_suffix(".log") {
        date
    } else {
        let (date, rotation) = stem.split_once(".log.")?;
        if rotation.is_empty() || !rotation.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        date
    };
    // Chrono accepts variable-width fields; the on-disk contract does not.
    if date.len() != 10
        || !date.bytes().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return None;
    }
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

#[tauri::command]
pub async fn get_log_directory(app: tauri::AppHandle) -> Result<String, String> {
    app.path()
        .app_log_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| format!("Failed to get log directory: {}", e))
}

#[tauri::command]
pub async fn open_logs_folder(app: tauri::AppHandle) -> Result<(), String> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    // Create directory if it doesn't exist
    if !log_dir.exists() {
        std::fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Failed to create log directory: {}", e))?;
    }

    // Open the directory using the system's file manager
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&log_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        std::process::Command::new("explorer")
            .arg(&log_dir)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(())
}

/// Maximum bytes to read from the tail of the latest log for bug reports.
const MAX_TAIL_BYTES: u64 = 40_960; // ~40KB
/// Maximum bytes of the in-memory DEBUG ring to attach to bug reports.
const MAX_RING_BYTES: usize = 64 * 1024; // ~64KB

/// Response for the latest-log bug-report attachment command.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestLogAttachment {
    /// File name of the selected log (e.g. "voicetypr-2026-04-27.log").
    pub file_name: Option<String>,
    /// Redacted tail content ready for frontend display/copy.
    pub redacted_content: String,
    /// True when the file was larger than MAX_TAIL_BYTES and was truncated.
    pub truncated: bool,
    /// Human-readable status for the frontend (empty when log exists).
    pub status_note: String,
    /// Redacted dump of the in-memory DEBUG ring (plan 060): timings,
    /// budgets, and backend decisions that never reach the Info-filtered
    /// file log in release builds.
    pub debug_ring: String,
}

/// Find the newest `voicetypr-*.log` file in the given directory.
/// Returns the full path of the most recent matching file, or None.
pub fn find_newest_log(log_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut newest: Option<(std::path::PathBuf, std::time::SystemTime)> = None;

    let entries = match std::fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(_) => return None,
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        // Explicitly reject symlinks before regular files so the report flow cannot escape app_log_dir.
        if file_type.is_symlink() || !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if voicetypr_log_date(file_name).is_some() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    match &newest {
                        Some((_, best_modified)) if modified < *best_modified => {}
                        Some((best_path, best_modified)) if modified == *best_modified => {
                            let best_name =
                                best_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                            if file_name > best_name {
                                newest = Some((path.clone(), modified));
                            }
                        }
                        _ => newest = Some((path.clone(), modified)),
                    }
                }
            }
        }
    }

    newest.map(|(p, _)| p)
}

/// Read up to `max_bytes` from the tail of `path`.
/// Returns (content, original_byte_count, truncated).
pub fn read_log_tail(
    path: &std::path::Path,
    max_bytes: u64,
) -> std::io::Result<(String, u64, bool)> {
    let mut file = std::fs::File::open(path)?;
    let file_len = file.metadata()?.len();
    let mut bytes = Vec::new();

    if file_len <= max_bytes {
        file.read_to_end(&mut bytes)?;
        return Ok((
            String::from_utf8_lossy(&bytes).into_owned(),
            file_len,
            false,
        ));
    }

    use std::io::{Seek, SeekFrom};
    let seek_pos = file_len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(seek_pos))?;
    file.by_ref().take(max_bytes).read_to_end(&mut bytes)?;

    // Prefer complete log lines when possible, but keep the tail when it is one long line.
    let bytes = match bytes.iter().position(|byte| *byte == b'\n') {
        Some(index) if index + 1 < bytes.len() => &bytes[index + 1..],
        _ => bytes.as_slice(),
    };

    Ok((String::from_utf8_lossy(bytes).into_owned(), file_len, true))
}

/// Redact common sensitive patterns from log content.
/// Preserves context for debugging while removing secrets.
pub fn redact_log_content(content: &str) -> String {
    static WRAPPED_SECRET_RE: OnceLock<regex::Regex> = OnceLock::new();
    static UNQUOTED_SECRET_RE: OnceLock<regex::Regex> = OnceLock::new();
    static BEARER_RE: OnceLock<regex::Regex> = OnceLock::new();
    static SK_KEY_RE: OnceLock<regex::Regex> = OnceLock::new();
    static LICENSE_RE: OnceLock<regex::Regex> = OnceLock::new();
    static EMAIL_RE: OnceLock<regex::Regex> = OnceLock::new();
    static HOME_RE: OnceLock<regex::Regex> = OnceLock::new();
    static ABSOLUTE_PATH_RE: OnceLock<regex::Regex> = OnceLock::new();
    static UNC_PATH_RE: OnceLock<regex::Regex> = OnceLock::new();

    let mut result = content.to_string();

    // Quoted values close only at their matching, unescaped delimiter. Treat
    // backslash + the next character as a pair so escaped quotes/backslashes
    // in JSON or Rust Debug output cannot expose the remainder of a secret.
    let wrapped_secret_re = WRAPPED_SECRET_RE.get_or_init(|| {
        regex::Regex::new(
            r#"(?i)(["']?\b(?:api[_-]?key|apikey|secret[_-]?key|access[_-]?token|license[_-]?key|password|passwd)\b["']?\s*[:=]\s*(?:Some\(|String\()?)("(?:\\(?s:.)|[^"\\])*"|'(?:\\(?s:.)|[^'\\])*')(\)?)"#,
        )
        .unwrap()
    });
    result = wrapped_secret_re
        .replace_all(&result, |captures: &regex::Captures<'_>| {
            let delimiter = &captures[2][..1];
            format!(
                "{}{delimiter}[REDACTED]{delimiter}{}",
                &captures[1], &captures[3]
            )
        })
        .to_string();

    let unquoted_secret_re = UNQUOTED_SECRET_RE.get_or_init(|| {
        regex::Regex::new(
            r#"(?i)(\b(?:api[_-]?key|apikey|secret[_-]?key|access[_-]?token|license[_-]?key|password|passwd)\b\s*[:=]\s*)[^\s,;}]+"#,
        )
        .unwrap()
    });
    result = unquoted_secret_re
        .replace_all(&result, "$1[REDACTED]")
        .to_string();

    // Bearer token headers: Authorization: Bearer <token>.
    let bearer_re =
        BEARER_RE.get_or_init(|| regex::Regex::new(r"(?i)bearer\s+[^\s,;]+\b").unwrap());
    result = bearer_re
        .replace_all(&result, "bearer [REDACTED]")
        .to_string();

    // OpenAI / Anthropic key values: sk-<chars>, sk-ant-<chars>.
    let sk_key_re = SK_KEY_RE.get_or_init(|| {
        regex::Regex::new(r"\b(sk-(?:ant(?:hropic)?-)?)[a-zA-Z0-9_-]{20,}\b").unwrap()
    });
    result = sk_key_re.replace_all(&result, "$1[REDACTED]").to_string();

    // License-looking values only when surrounding text labels them as licenses.
    let license_re = LICENSE_RE.get_or_init(|| {
        regex::Regex::new(
            r#"(?i)\b(license(?:\s+(?:key|id))?\s+)['\"]?[A-Z0-9]{4,8}(?:-[A-Z0-9]{4,8}){3,}['\"]?"#,
        )
        .unwrap()
    });
    result = license_re
        .replace_all(&result, "$1[LICENSE_REDACTED]")
        .to_string();

    // Email addresses.
    let email_re = EMAIL_RE.get_or_init(|| {
        regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap()
    });
    result = email_re
        .replace_all(&result, "[EMAIL_REDACTED]")
        .to_string();

    // Home directory paths: /Users/<name>/ on macOS, /home/<name>/ on Linux, C:\Users\<name>\ on Windows.
    let home_re = HOME_RE.get_or_init(|| {
        regex::Regex::new(r"(?:/Users/[^/\r\n]+|/home/[^/\r\n]+|C:\\Users\\[^\\\r\n]+)").unwrap()
    });
    result = home_re.replace_all(&result, "[HOME_DIR]").to_string();

    // Other absolute local paths commonly emitted by diagnostics/logging.
    let absolute_path_re = ABSOLUTE_PATH_RE.get_or_init(|| {
        regex::Regex::new(
            r#"(^|[\s\"'(=])(/(?:Volumes|tmp|private|var|opt|Applications|Library|System|etc|usr|proc)/[^\r\n\"',)]+|[A-Za-z]:\\[^\r\n\"',)]+)"#,
        )
        .unwrap()
    });
    result = absolute_path_re
        .replace_all(&result, "$1[PATH_REDACTED]")
        .to_string();

    // UNC paths (\\server\share\...): the drive-letter and unix roots above
    // do not match these; user-selected save targets can be on network shares.
    let unc_path_re = UNC_PATH_RE.get_or_init(|| {
        regex::Regex::new(r#"\\\\[^\\/\"\s]+\\[^\\/\r\n\"',)]+(?:\\[^\r\n\"',)]+)*"#).unwrap()
    });
    result = unc_path_re
        .replace_all(&result, "[PATH_REDACTED]")
        .to_string();

    result
}

/// Tauri command: returns the latest log file content (redacted, tail-bounded) for bug reports.
/// Never accepts a frontend-supplied path; only reads from app_log_dir.
#[tauri::command]
pub async fn get_latest_log_for_bug_report(
    app: tauri::AppHandle,
) -> Result<LatestLogAttachment, String> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    let newest = find_newest_log(&log_dir);

    let debug_ring = redact_log_content(&crate::utils::ring_log::snapshot_joined(MAX_RING_BYTES));

    let Some(log_path) = newest else {
        return Ok(LatestLogAttachment {
            file_name: None,
            redacted_content: String::new(),
            truncated: false,
            status_note: "No log file found. Logs will be included automatically when available."
                .to_string(),
            debug_ring,
        });
    };

    let file_name = log_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string());

    let (raw, _original_byte_count, truncated) = match read_log_tail(&log_path, MAX_TAIL_BYTES) {
        Ok(result) => result,
        Err(e) => {
            log::warn!("Failed to read log tail {:?}: {}", log_path, e);
            return Ok(LatestLogAttachment {
                file_name,
                redacted_content: String::new(),
                truncated: false,
                status_note: "Found a log file, but it could not be read.".to_string(),
                debug_ring,
            });
        }
    };

    let redacted = redact_log_content(&raw);

    Ok(LatestLogAttachment {
        file_name,
        redacted_content: redacted,
        truncated,
        status_note: String::new(),
        debug_ring,
    })
}
