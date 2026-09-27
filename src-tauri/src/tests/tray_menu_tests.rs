//! Tests for tray menu remote server integration
//!
//! Tests the tray menu changes for remote server selection and status display.

use crate::menu::{
    format_tray_model_label, format_tray_polish_label, should_include_remote_connection_in_tray,
    should_mark_model_selected,
};
use crate::remote::settings::ConnectionStatus;

// ============================================================================
// should_mark_model_selected Tests
// ============================================================================

#[test]
fn test_mark_model_selected_when_onboarding_done_and_match() {
    let result = should_mark_model_selected(true, "base.en", "base.en");
    assert!(
        result,
        "Should be selected when onboarding done and model matches"
    );
}

#[test]
fn test_mark_model_not_selected_when_onboarding_not_done() {
    let result = should_mark_model_selected(false, "base.en", "base.en");
    assert!(!result, "Should not be selected when onboarding not done");
}

#[test]
fn test_mark_model_not_selected_when_model_mismatch() {
    let result = should_mark_model_selected(true, "base.en", "large-v3");
    assert!(!result, "Should not be selected when model doesn't match");
}

#[test]
fn test_mark_model_not_selected_when_both_false() {
    let result = should_mark_model_selected(false, "base.en", "large-v3");
    assert!(
        !result,
        "Should not be selected when onboarding false and mismatch"
    );
}

#[test]
fn test_mark_model_selected_empty_model_name() {
    let result = should_mark_model_selected(true, "", "");
    assert!(result, "Empty strings should match when onboarding done");
}

#[test]
fn test_mark_model_selected_case_sensitive() {
    let result = should_mark_model_selected(true, "Base.En", "base.en");
    assert!(!result, "Model matching should be case-sensitive");
}

#[test]
fn test_mark_model_selected_with_special_chars() {
    let result = should_mark_model_selected(true, "large-v3-turbo", "large-v3-turbo");
    assert!(result, "Should handle hyphenated model names");
}

#[test]
fn test_mark_model_selected_whitespace_mismatch() {
    let result = should_mark_model_selected(true, "base.en ", "base.en");
    assert!(!result, "Trailing whitespace should cause mismatch");
}

// ============================================================================
// format_tray_model_label Tests
// ============================================================================

#[test]
fn test_format_label_when_onboarding_not_done() {
    let result = format_tray_model_label(false, "base.en", None);
    assert_eq!(result, "Model: None");
}

#[test]
fn test_format_label_when_model_empty() {
    let result = format_tray_model_label(true, "", None);
    assert_eq!(result, "Model: None");
}

#[test]
fn test_format_label_with_model_no_display_name() {
    let result = format_tray_model_label(true, "base.en", None);
    assert_eq!(result, "Model: Base English");
}

#[test]
fn test_format_label_with_display_name() {
    let result =
        format_tray_model_label(true, "base.en", Some("Whisper Base (English)".to_string()));
    assert_eq!(result, "Model: Whisper Base (English)");
}

#[test]
fn test_format_label_display_name_overrides_model() {
    let result = format_tray_model_label(true, "some-model", Some("Custom Display".to_string()));
    assert_eq!(result, "Model: Custom Display");
}

#[test]
fn test_format_label_empty_display_name_falls_back() {
    // Empty string in Some still counts as provided
    let result = format_tray_model_label(true, "base.en", Some("".to_string()));
    assert_eq!(result, "Model: ");
}

#[test]
fn test_format_label_onboarding_false_ignores_display_name() {
    let result = format_tray_model_label(false, "base.en", Some("Custom".to_string()));
    assert_eq!(
        result, "Model: None",
        "Should show None when onboarding not done"
    );
}

#[test]
fn test_format_label_both_empty() {
    let result = format_tray_model_label(false, "", None);
    assert_eq!(result, "Model: None");
}

#[test]
fn test_format_label_unicode_display_name() {
    let result = format_tray_model_label(true, "model", Some("模型 🎤".to_string()));
    assert_eq!(result, "Model: 模型 🎤");
}

#[test]
fn test_format_label_long_display_name() {
    let long_name = "A".repeat(100);
    let result = format_tray_model_label(true, "model", Some(long_name.clone()));
    assert_eq!(result, format!("Model: {}", long_name));
}

#[test]
fn test_format_polish_label_shows_state() {
    assert_eq!(format_tray_polish_label(true), "Polish: On");
    assert_eq!(format_tray_polish_label(false), "Polish: Off");
}

// ============================================================================
// Remote Server Display Format Tests
// ============================================================================

mod remote_display_tests {
    /// Helper function to format remote server display as done in tray.rs
    fn format_remote_server_display(server_name: &str, model: Option<&str>) -> String {
        if let Some(m) = model {
            format!("{} - {}", server_name, m)
        } else {
            server_name.to_string()
        }
    }

    #[test]
    fn test_remote_display_with_model() {
        let result = format_remote_server_display("My Server", Some("whisper-large"));
        assert_eq!(result, "My Server - whisper-large");
    }

    #[test]
    fn test_remote_display_without_model() {
        let result = format_remote_server_display("My Server", None);
        assert_eq!(result, "My Server");
    }

    #[test]
    fn test_remote_display_empty_server_name() {
        let result = format_remote_server_display("", Some("model"));
        assert_eq!(result, " - model");
    }

    #[test]
    fn test_remote_display_empty_model() {
        let result = format_remote_server_display("Server", Some(""));
        assert_eq!(result, "Server - ");
    }

    #[test]
    fn test_remote_display_with_ip_address() {
        let result = format_remote_server_display("192.168.1.100:47842", Some("base.en"));
        assert_eq!(result, "192.168.1.100:47842 - base.en");
    }

    #[test]
    fn test_remote_display_unicode_server_name() {
        let result = format_remote_server_display("服务器", Some("model"));
        assert_eq!(result, "服务器 - model");
    }
}

// ============================================================================
// Model Selection Logic Tests (simulating tray menu behavior)
// ============================================================================

mod selection_logic_tests {
    use super::*;

    /// Simulate the logic for determining if a local model is selected
    fn is_local_model_selected(
        active_remote_id: Option<&str>,
        onboarding_done: bool,
        model_name: &str,
        current_model: &str,
    ) -> bool {
        active_remote_id.is_none()
            && should_mark_model_selected(onboarding_done, model_name, current_model)
    }

    /// Simulate the logic for determining if a remote server is selected
    fn is_remote_server_selected(active_remote_id: Option<&str>, connection_id: &str) -> bool {
        active_remote_id == Some(connection_id)
    }

    #[test]
    fn test_local_model_selected_no_remote() {
        let result = is_local_model_selected(None, true, "base.en", "base.en");
        assert!(result);
    }

    #[test]
    fn test_local_model_not_selected_when_remote_active() {
        let result = is_local_model_selected(Some("remote_1"), true, "base.en", "base.en");
        assert!(
            !result,
            "Local model should not be selected when remote is active"
        );
    }

    #[test]
    fn test_local_model_not_selected_different_model() {
        let result = is_local_model_selected(None, true, "base.en", "large-v3");
        assert!(!result);
    }

    #[test]
    fn test_remote_server_selected() {
        let result = is_remote_server_selected(Some("conn_123"), "conn_123");
        assert!(result);
    }

    #[test]
    fn test_remote_server_not_selected_different_id() {
        let result = is_remote_server_selected(Some("conn_123"), "conn_456");
        assert!(!result);
    }

    #[test]
    fn test_remote_server_not_selected_no_active() {
        let result = is_remote_server_selected(None, "conn_123");
        assert!(!result);
    }
}

mod remote_connection_visibility_tests {
    use super::*;

    #[test]
    fn test_self_connection_is_hidden_from_tray() {
        assert!(!should_include_remote_connection_in_tray(
            &ConnectionStatus::SelfConnection
        ));
    }

    #[test]
    fn test_online_connection_is_shown_in_tray() {
        assert!(should_include_remote_connection_in_tray(
            &ConnectionStatus::Online
        ));
    }
}

// ============================================================================
// Menu Item ID Format Tests
// ============================================================================

mod menu_id_tests {
    #[test]
    fn test_model_menu_id_format() {
        let model_name = "base.en";
        let id = format!("model_{}", model_name);
        assert_eq!(id, "model_base.en");
    }

    #[test]
    fn test_remote_model_menu_id_format() {
        let conn_id = "conn_12345_0";
        let model_id = format!("remote_{}", conn_id);
        let id = format!("model_{}", model_id);
        assert_eq!(id, "model_remote_conn_12345_0");
    }

    #[test]
    fn test_microphone_menu_id_format() {
        let device_name = "Built-in Microphone";
        let id = format!("microphone_{}", device_name);
        assert_eq!(id, "microphone_Built-in Microphone");
    }

    #[test]
    fn test_recent_copy_menu_id_format() {
        let timestamp = "1705123456789";
        let id = format!("recent_copy_{}", timestamp);
        assert_eq!(id, "recent_copy_1705123456789");
    }
}

// ============================================================================
// Microphone Display Tests
// ============================================================================

mod microphone_display_tests {
    fn format_microphone_label(selected: Option<&str>) -> String {
        if let Some(mic_name) = selected {
            format!("Microphone: {}", mic_name)
        } else {
            "Microphone: Default".to_string()
        }
    }

    #[test]
    fn test_microphone_default() {
        let result = format_microphone_label(None);
        assert_eq!(result, "Microphone: Default");
    }

    #[test]
    fn test_microphone_selected() {
        let result = format_microphone_label(Some("USB Microphone"));
        assert_eq!(result, "Microphone: USB Microphone");
    }

    #[test]
    fn test_microphone_empty_name() {
        let result = format_microphone_label(Some(""));
        assert_eq!(result, "Microphone: ");
    }

    #[test]
    fn test_microphone_long_name() {
        let long_name = "Very Long Microphone Name That Goes On And On";
        let result = format_microphone_label(Some(long_name));
        assert_eq!(result, format!("Microphone: {}", long_name));
    }
}

// ============================================================================
// Recent Transcription Preview Tests
// ============================================================================

mod transcription_preview_tests {
    fn format_transcription_preview(text: &str) -> String {
        let first_line = text.lines().next().unwrap_or("").trim();
        let char_count = first_line.chars().count();
        let mut preview: String = first_line.chars().take(40).collect();
        if char_count > 40 {
            preview.push('\u{2026}'); // Ellipsis
        }
        if preview.is_empty() {
            "(empty)".to_string()
        } else {
            preview
        }
    }

    #[test]
    fn test_preview_short_text() {
        let result = format_transcription_preview("Hello world");
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_preview_exactly_40_chars() {
        let text = "A".repeat(40);
        let result = format_transcription_preview(&text);
        assert_eq!(result, text);
        assert!(!result.contains('\u{2026}'));
    }

    #[test]
    fn test_preview_over_40_chars_truncated() {
        let text = "A".repeat(50);
        let result = format_transcription_preview(&text);
        assert_eq!(result.chars().count(), 41); // 40 + ellipsis
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn test_preview_multiline_takes_first() {
        let text = "First line\nSecond line\nThird line";
        let result = format_transcription_preview(text);
        assert_eq!(result, "First line");
    }

    #[test]
    fn test_preview_empty_text() {
        let result = format_transcription_preview("");
        assert_eq!(result, "(empty)");
    }

    #[test]
    fn test_preview_whitespace_only() {
        let result = format_transcription_preview("   \n   \t   ");
        assert_eq!(result, "(empty)");
    }

    #[test]
    fn test_preview_trims_whitespace() {
        let result = format_transcription_preview("  Hello world  ");
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_preview_unicode_chars() {
        let text = "你好世界！这是一个很长的中文句子，需要被截断显示。";
        let result = format_transcription_preview(text);
        assert_eq!(result.chars().count(), text.chars().count());
        assert_eq!(result, text);
    }

    #[test]
    fn test_preview_long_unicode_truncated() {
        let text = "你".repeat(50);
        let result = format_transcription_preview(&text);
        assert_eq!(result.chars().count(), 41); // 40 + ellipsis
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn test_preview_emoji() {
        let result = format_transcription_preview("🎤 Voice recording started");
        assert_eq!(result, "🎤 Voice recording started");
    }
}

// ============================================================================
// Recording Mode Tests
// ============================================================================

mod recording_mode_tests {
    fn is_toggle_mode(mode: &str) -> bool {
        mode == "toggle"
    }

    fn is_push_to_talk_mode(mode: &str) -> bool {
        mode == "push_to_talk"
    }

    #[test]
    fn test_toggle_mode_detection() {
        assert!(is_toggle_mode("toggle"));
        assert!(!is_toggle_mode("push_to_talk"));
        assert!(!is_toggle_mode(""));
        assert!(!is_toggle_mode("Toggle")); // Case sensitive
    }

    #[test]
    fn test_push_to_talk_mode_detection() {
        assert!(is_push_to_talk_mode("push_to_talk"));
        assert!(!is_push_to_talk_mode("toggle"));
        assert!(!is_push_to_talk_mode(""));
        assert!(!is_push_to_talk_mode("Push_To_Talk")); // Case sensitive
    }

    #[test]
    fn test_default_mode_is_toggle() {
        let default_mode = "toggle";
        assert!(is_toggle_mode(default_mode));
    }
}

// ============================================================================
// URL Format Tests (for fetch_server_status)
// ============================================================================

mod url_format_tests {
    fn format_status_url(host: &str, port: u16) -> String {
        format!("http://{}:{}/api/v1/status", host, port)
    }

    #[test]
    fn test_status_url_format() {
        let url = format_status_url("192.168.1.100", 47842);
        assert_eq!(url, "http://192.168.1.100:47842/api/v1/status");
    }

    #[test]
    fn test_status_url_localhost() {
        let url = format_status_url("localhost", 8080);
        assert_eq!(url, "http://localhost:8080/api/v1/status");
    }

    #[test]
    fn test_status_url_hostname() {
        let url = format_status_url("my-server.local", 47842);
        assert_eq!(url, "http://my-server.local:47842/api/v1/status");
    }

    #[test]
    fn test_status_url_ipv6() {
        // Note: IPv6 in URLs should be bracketed, but the format string doesn't do that
        // This test documents the current behavior
        let url = format_status_url("::1", 47842);
        assert_eq!(url, "http://::1:47842/api/v1/status");
    }
}
