use keyboard_types::{Code, Key};
use std::str::FromStr;

/// Normalize keyboard shortcut keys for cross-platform compatibility
/// Converts frontend key names to Tauri-compatible format
pub fn normalize_shortcut_keys(shortcut: &str) -> String {
    shortcut
        .split('+')
        .map(normalize_single_key)
        .collect::<Vec<_>>()
        .join("+")
}

/// Normalize a single key string
fn normalize_single_key(key: &str) -> String {
    // First check case-insensitive matches for common modifiers
    match key.to_lowercase().as_str() {
        "cmd" => return "CommandOrControl".to_string(),
        "ctrl" => return "CommandOrControl".to_string(),
        "control" => return "Control".to_string(), // Keep Control separate for macOS Cmd+Ctrl support
        "command" => return "CommandOrControl".to_string(),
        "meta" => return "CommandOrControl".to_string(),
        "super" => return "CommandOrControl".to_string(), // Super maps to CommandOrControl for Tauri
        "option" => return "Alt".to_string(),
        "alt" => return "Alt".to_string(),
        "shift" => return "Shift".to_string(),
        "space" => return "Space".to_string(),
        _ => {}
    }

    // Normalize common punctuation to their Code enum names
    // This handles cases where the frontend sends the actual character instead of the Code name
    match key {
        "," => return "Comma".to_string(),
        "." => return "Period".to_string(),
        "/" => return "Slash".to_string(),
        ";" => return "Semicolon".to_string(),
        "'" => return "Quote".to_string(),
        "[" => return "BracketLeft".to_string(),
        "]" => return "BracketRight".to_string(),
        "\\" => return "Backslash".to_string(),
        "=" => return "Equal".to_string(),
        "-" => return "Minus".to_string(),
        "`" => return "Backquote".to_string(),
        _ => {}
    }

    // First, try to parse as keyboard_types::Key for semantic normalization
    if let Ok(parsed_key) = Key::from_str(key) {
        match parsed_key {
            Key::Enter => "Enter".to_string(),
            Key::Tab => "Tab".to_string(),
            Key::Backspace => "Backspace".to_string(),
            Key::Escape => "Escape".to_string(),
            Key::Character(s) if s == " " => "Space".to_string(),
            Key::ArrowDown => "Down".to_string(),
            Key::ArrowLeft => "Left".to_string(),
            Key::ArrowRight => "Right".to_string(),
            Key::ArrowUp => "Up".to_string(),
            Key::End => "End".to_string(),
            Key::Home => "Home".to_string(),
            Key::PageDown => "PageDown".to_string(),
            Key::PageUp => "PageUp".to_string(),
            Key::Delete => "Delete".to_string(),
            Key::F1 => "F1".to_string(),
            Key::F2 => "F2".to_string(),
            Key::F3 => "F3".to_string(),
            Key::F4 => "F4".to_string(),
            Key::F5 => "F5".to_string(),
            Key::F6 => "F6".to_string(),
            Key::F7 => "F7".to_string(),
            Key::F8 => "F8".to_string(),
            Key::F9 => "F9".to_string(),
            Key::F10 => "F10".to_string(),
            Key::F11 => "F11".to_string(),
            Key::F12 => "F12".to_string(),
            _ => key.to_string(), // Return original if no normalization needed
        }
    } else {
        // Handle special cases that might not parse
        match key {
            // Navigation keys
            "Return" => "Enter".to_string(),
            "ArrowUp" => "Up".to_string(),
            "ArrowDown" => "Down".to_string(),
            "ArrowLeft" => "Left".to_string(),
            "ArrowRight" => "Right".to_string(),
            "Insert" => "Insert".to_string(),

            // Modifiers
            "CommandOrControl" => "CommandOrControl".to_string(), // Keep as-is for Tauri
            "Cmd" => "CommandOrControl".to_string(),
            "Ctrl" => "CommandOrControl".to_string(),
            "Control" => "Control".to_string(), // Keep Control separate
            "Command" => "CommandOrControl".to_string(),
            "Super" => "CommandOrControl".to_string(), // Super maps to CommandOrControl for Tauri
            "Option" => "Alt".to_string(),
            "Meta" => "CommandOrControl".to_string(),

            // Function keys (F1-F24)
            k if k.starts_with('F') && k.len() <= 3 => {
                // Validate it's a function key F1-F24
                if let Ok(num) = k[1..].parse::<u8>() {
                    if (1..=24).contains(&num) {
                        return key.to_string(); // Valid function key
                    }
                }
                key.to_string() // Return as-is if not valid
            }

            // Numpad keys
            k if k.starts_with("Numpad") => key.to_string(), // NumpadX, NumpadAdd, etc.
            "NumLock" => "NumLock".to_string(),
            "ScrollLock" => "ScrollLock".to_string(),
            "Pause" => "Pause".to_string(),
            "PrintScreen" => "PrintScreen".to_string(),
            "Clear" => "Clear".to_string(),

            // Media keys (common ones)
            "AudioVolumeUp" => "AudioVolumeUp".to_string(),
            "AudioVolumeDown" => "AudioVolumeDown".to_string(),
            "AudioVolumeMute" => "AudioVolumeMute".to_string(),
            "MediaPlayPause" => "MediaPlayPause".to_string(),
            "MediaStop" => "MediaStop".to_string(),
            "MediaTrackNext" => "MediaTrackNext".to_string(),
            "MediaTrackPrevious" => "MediaTrackPrevious".to_string(),

            // Letter keys - ensure uppercase
            k if k.len() == 1 && k.chars().all(|c| c.is_alphabetic()) => k.to_uppercase(),

            // Number keys (already handled above for shifted versions)
            "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => key.to_string(),

            _ => key.to_string(),
        }
    }
}

/// Validation rules for key combinations
#[derive(Debug, Clone)]
pub struct KeyValidationRules {
    pub min_keys: usize,
    pub max_keys: usize,
    pub require_modifier: bool,
    pub require_modifier_for_multi_key: bool,
}

impl KeyValidationRules {
    /// Standard hotkey rules (2-5 keys, must include at least one modifier)
    pub fn standard() -> Self {
        Self {
            min_keys: 2,
            max_keys: 5,
            require_modifier: true,
            require_modifier_for_multi_key: true,
        }
    }
}

/// Validate that a key combination is allowed with default rules
pub fn validate_key_combination(shortcut: &str) -> Result<(), String> {
    validate_key_combination_with_rules(shortcut, &KeyValidationRules::standard())
}

/// Validate a shortcut for contexts that explicitly allow safe single-key hotkeys.
///
/// Single keys are restricted to non-typing keys so a global shortcut cannot
/// capture normal text input everywhere. Multi-key shortcuts keep the standard
/// global-hotkey rules.
pub fn validate_key_combination_allowing_safe_single_key(shortcut: &str) -> Result<(), String> {
    if is_single_key_shortcut(shortcut) {
        validate_key_combination_with_rules(
            shortcut,
            &KeyValidationRules {
                min_keys: 1,
                max_keys: 5,
                require_modifier: false,
                require_modifier_for_multi_key: true,
            },
        )?;

        if !is_typing_safe_single_key(shortcut) {
            return Err(single_key_safety_error());
        }

        return Ok(());
    }

    reject_single_modifier_only(shortcut)?;
    validate_key_combination(shortcut)
}

fn reject_single_modifier_only(shortcut: &str) -> Result<(), String> {
    let mut parts = shortcut.split('+');
    let Some(first) = parts.next() else {
        return Err("Shortcut is required".to_string());
    };
    if parts.next().is_none() && is_modifier(first) {
        return Err("Single modifier-only shortcuts are not allowed".to_string());
    }
    Ok(())
}

pub fn is_single_key_shortcut(shortcut: &str) -> bool {
    let mut parts = shortcut.split('+');
    let Some(first) = parts.next() else {
        return false;
    };
    !first.is_empty() && parts.next().is_none() && !is_modifier(first)
}

/// Allowlist of keys safe to bind as single-key global shortcuts.
/// Only keys that don't appear in normal typing are permitted.
/// Default-deny: any key not listed here returns false.
pub fn is_typing_safe_single_key(shortcut: &str) -> bool {
    // Function keys F1-F24
    if let Some(rest) = shortcut.strip_prefix('F') {
        if rest.len() <= 2 {
            if let Ok(num) = rest.parse::<u8>() {
                if (1..=24).contains(&num) {
                    return true;
                }
            }
        }
    }
    // Numpad keys (Numpad0-Numpad9, NumpadAdd, etc.)
    if shortcut.starts_with("Numpad") {
        return true;
    }
    // Navigation cluster (non-typing)
    matches!(
        shortcut,
        "Home" | "End" | "PageUp" | "PageDown" | "Insert" | "Up" | "Down" | "Left" | "Right"
    )
}

fn single_key_safety_error() -> String {
    "Single-key shortcuts must be a function key (F1-F24), a numpad key, \
     or a navigation key (arrows/Home/End/Page Up/Page Down/Insert) \
     -- typing keys like letters would be captured everywhere."
        .to_string()
}

fn is_modifier(key: &str) -> bool {
    matches!(
        key,
        "CommandOrControl"
            | "Shift"
            | "Alt"
            | "Control"
            | "Command"
            | "Cmd"
            | "Ctrl"
            | "Option"
            | "Meta"
            | "Super" // Super will be normalized to CommandOrControl
    )
}

/// Validate that a key combination is allowed with custom rules
pub fn validate_key_combination_with_rules(
    shortcut: &str,
    rules: &KeyValidationRules,
) -> Result<(), String> {
    let parts: Vec<&str> = shortcut.split('+').collect();

    // Check minimum keys
    if parts.len() < rules.min_keys {
        return Err(format!("Minimum {} key(s) required", rules.min_keys));
    }

    // Check maximum keys
    if parts.len() > rules.max_keys {
        return Err(format!(
            "Maximum {} keys allowed in combination",
            rules.max_keys
        ));
    }

    let has_modifier = parts.iter().any(|&key| is_modifier(key));
    if rules.require_modifier && !has_modifier {
        return Err("At least one modifier key is required".to_string());
    }

    // Check that the shortcut starts with a modifier
    if rules.require_modifier && !parts.is_empty() && !is_modifier(parts[0]) {
        return Err(
            "Keyboard shortcuts must start with a modifier key (Cmd/Ctrl, Alt, or Shift)"
                .to_string(),
        );
    }

    if rules.require_modifier_for_multi_key && !has_modifier && parts.len() > 1 {
        return Err("Multi-key shortcuts must include at least one modifier key".to_string());
    }

    // Validate each key
    for key in parts {
        if !is_valid_key(key) {
            return Err(format!("Invalid key: {}", key));
        }
    }

    Ok(())
}

/// Check if a key string is valid
fn is_valid_key(key: &str) -> bool {
    // Empty keys are invalid
    if key.is_empty() {
        return false;
    }

    // Try to parse as keyboard_types Key or Code first
    if Key::from_str(key).is_ok() || Code::from_str(key).is_ok() {
        return true;
    }

    // Allow any non-empty string as a potential key
    // The actual validation will happen when we try to register the shortcut
    // This allows for maximum flexibility with different keyboard layouts and OS-specific keys
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_shortcut_keys() {
        // Test default shortcut remains unchanged
        assert_eq!(
            normalize_shortcut_keys("CommandOrControl+Shift+Space"),
            "CommandOrControl+Shift+Space"
        );

        assert_eq!(normalize_shortcut_keys("Return"), "Enter");
        assert_eq!(normalize_shortcut_keys("ArrowUp"), "Up");
        assert_eq!(
            normalize_shortcut_keys("CommandOrControl+ArrowDown"),
            "CommandOrControl+Down"
        );
        assert_eq!(normalize_shortcut_keys("Shift+Return"), "Shift+Enter");
        assert_eq!(
            normalize_shortcut_keys("Cmd+Shift+Space"),
            "CommandOrControl+Shift+Space"
        );
        assert_eq!(normalize_shortcut_keys("Space"), "Space");
        assert_eq!(normalize_shortcut_keys("F1"), "F1");

        // Test case-insensitive normalization
        assert_eq!(
            normalize_shortcut_keys("cmd+shift+space"),
            "CommandOrControl+Shift+Space"
        );
        assert_eq!(
            normalize_shortcut_keys("ctrl+shift+space"),
            "CommandOrControl+Shift+Space"
        );
        assert_eq!(
            normalize_shortcut_keys("CMD+SHIFT+SPACE"),
            "CommandOrControl+Shift+Space"
        );
        assert_eq!(normalize_shortcut_keys("alt+a"), "Alt+a");
        assert_eq!(normalize_shortcut_keys("SHIFT+F1"), "Shift+F1");

        // Test modifier keys stay as-is (no special single key handling anymore)
        assert_eq!(normalize_shortcut_keys("Alt"), "Alt");
        assert_eq!(normalize_shortcut_keys("Shift"), "Shift");
        assert_eq!(normalize_shortcut_keys("Control"), "Control");
        assert_eq!(normalize_shortcut_keys("Ctrl"), "CommandOrControl");
        assert_eq!(normalize_shortcut_keys("Command"), "CommandOrControl");
        assert_eq!(normalize_shortcut_keys("Cmd"), "CommandOrControl");

        // In combinations, they get normalized too
        assert_eq!(normalize_shortcut_keys("Alt+A"), "Alt+A");
        assert_eq!(normalize_shortcut_keys("Shift+Space"), "Shift+Space");

        // Test Super modifier (maps to CommandOrControl for Tauri)
        assert_eq!(normalize_shortcut_keys("Super+A"), "CommandOrControl+A");
        assert_eq!(
            normalize_shortcut_keys("Super+Control+A"),
            "CommandOrControl+Control+A"
        );
        assert_eq!(
            normalize_shortcut_keys("Super+Control+Alt+A"),
            "CommandOrControl+Control+Alt+A"
        );
        assert_eq!(
            normalize_shortcut_keys("Super+Control+Alt+Shift+A"),
            "CommandOrControl+Control+Alt+Shift+A"
        );
    }

    #[test]
    fn test_validate_key_combination() {
        assert!(validate_key_combination("CommandOrControl+A").is_ok());
        assert!(validate_key_combination("Shift+Alt+F1").is_ok());
        assert!(validate_key_combination("A").is_err()); // Single key not allowed by default
        assert!(validate_key_combination_allowing_safe_single_key("F1").is_ok());
        assert!(validate_key_combination_allowing_safe_single_key("Home").is_ok());
        assert!(validate_key_combination_allowing_safe_single_key("Up").is_ok());
        assert!(validate_key_combination_allowing_safe_single_key("NumpadAdd").is_ok());
        assert!(validate_key_combination_allowing_safe_single_key("A").is_err());
        assert!(validate_key_combination_allowing_safe_single_key("CommandOrControl").is_err());
        assert!(validate_key_combination("A+B").is_err()); // Multi-key without modifier not allowed
        assert!(validate_key_combination("A+CommandOrControl").is_err()); // Must start with modifier
        assert!(validate_key_combination("Space+Shift").is_err()); // Must start with modifier
        assert!(validate_key_combination("Cmd+Shift+Alt+Ctrl+A+B").is_err()); // Too many keys
        assert!(validate_key_combination("CommandOrControl+InvalidKey").is_ok()); // Any non-empty key is valid

        // Test punctuation keys
        assert!(validate_key_combination("CommandOrControl+/").is_ok());
        assert!(validate_key_combination("Cmd+\\").is_ok());
        assert!(validate_key_combination("Ctrl+,").is_ok());
        assert!(validate_key_combination("Shift+.").is_ok());
        assert!(validate_key_combination("Alt+;").is_ok());
        assert!(validate_key_combination("CommandOrControl+[").is_ok());
        assert!(validate_key_combination("CommandOrControl+]").is_ok());
        assert!(validate_key_combination("Cmd+-").is_ok());
        assert!(validate_key_combination("Cmd+=").is_ok());
        assert!(validate_key_combination("/").is_err()); // Single punctuation key not allowed

        // Test special named keys
        assert!(validate_key_combination("CommandOrControl+Slash").is_ok());
        assert!(validate_key_combination("Cmd+BracketLeft").is_ok());
        assert!(validate_key_combination("Ctrl+Minus").is_ok());

        // Test numpad keys
        assert!(validate_key_combination("CommandOrControl+Numpad0").is_ok());
        assert!(validate_key_combination("Alt+NumpadAdd").is_ok());
        assert!(validate_key_combination("Shift+NumpadEnter").is_ok());

        // Test media keys
        assert!(validate_key_combination("MediaPlayPause").is_err()); // Single media key not allowed
        assert!(validate_key_combination("AudioVolumeUp").is_err()); // Single media key not allowed

        // Test system keys
        assert!(validate_key_combination("PrintScreen").is_err()); // Single system key not allowed
        assert!(validate_key_combination("Cmd+Insert").is_ok());
        assert!(validate_key_combination("Alt+CapsLock").is_ok());

        // Test number row symbols
        assert!(validate_key_combination("Shift+1").is_ok()); // !
        assert!(validate_key_combination("Shift+2").is_ok()); // @
        assert!(validate_key_combination("Cmd+Shift+3").is_ok()); // #

        // Test international keys
        assert!(validate_key_combination("CommandOrControl+ü").is_ok());
        assert!(validate_key_combination("Alt+ñ").is_ok());

        // Test Super modifier combinations (maps to CommandOrControl for Tauri)
        assert!(validate_key_combination("Super+A").is_ok()); // Will be normalized to CommandOrControl
        assert!(validate_key_combination("Super+Control+A").is_ok());
        assert!(validate_key_combination("Super+Control+Alt+A").is_ok());
        assert!(validate_key_combination("Super+Control+Alt+Shift+A").is_ok());
        assert!(validate_key_combination("Control+Alt+A").is_ok());
        assert!(validate_key_combination("Control+Shift+A").is_ok());
    }

    #[test]
    fn test_single_modifier_validation() {
        // Bare modifiers are not standalone shortcuts; modifier-only triggers are
        // represented explicitly as ModifierHold/IsolatedTap bindings.
        assert!(validate_key_combination("Alt").is_err());
        assert!(validate_key_combination("Shift").is_err());
        assert!(validate_key_combination("Control").is_err());

        // Combinations with generic modifiers remain valid.
        assert!(validate_key_combination("Alt+A").is_ok());
        assert!(validate_key_combination("Shift+Space").is_ok());
    }
}
