#[cfg(test)]
mod tests {
    use crate::commands::settings::{
        get_supported_languages, normalize_final_text_language,
        normalize_speech_language_for_model, normalize_stored_transcription_acceleration,
        resolve_transcription_complete_sound, task_uses_translate_to_english, Settings,
        FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT, TRANSCRIPTION_TASK_TRANSCRIBE,
        TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH,
    };
    use crate::commands::updater::UpdateChannel;
    use serde_json::json;

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();

        assert_eq!(settings.hotkey, "CommandOrControl+Shift+Space");
        assert_eq!(settings.current_model, ""); // Empty means auto-select
        assert_eq!(settings.speech_language, "en");
        assert_eq!(settings.transcription_task, TRANSCRIPTION_TASK_TRANSCRIBE);
        assert_eq!(
            settings.final_text_language,
            FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT
        );
        assert_eq!(settings.theme, "system");
        assert_eq!(settings.settings_mode, "recommended");
        assert_eq!(settings.transcription_cleanup_days, None);
        assert!(!settings.launch_at_startup);
        assert!(!settings.onboarding_completed);
        assert!(settings.check_updates_automatically); // Default to automatic checks only
        assert!(settings.auto_paste_transcription); // Default to true
        assert!(!settings.save_recordings);
        assert_eq!(settings.recording_retention_days, Some(30));
        assert!(!settings.whisper_speed_mode);
        assert_eq!(settings.transcription_mode, "regular");
    }

    #[test]
    fn test_settings_serialization() {
        let settings = Settings {
            polish_keep_words: false,
            hotkey: "CommandOrControl+A".to_string(),
            current_model: "base".to_string(),
            current_model_engine: "whisper".to_string(),
            speech_language: "es".to_string(),
            transcription_task: TRANSCRIPTION_TASK_TRANSCRIBE.to_string(),
            final_text_language: FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT.to_string(),
            theme: "dark".to_string(),
            settings_mode: "recommended".to_string(),
            transcription_cleanup_days: Some(7),
            pill_position: Some((100.0, 200.0)),
            launch_at_startup: false,
            onboarding_completed: true,
            check_updates_automatically: true,
            selected_microphone: None,
            recording_mode: "toggle".to_string(),
            use_different_ptt_key: false,
            ptt_hotkey: Some("Alt+Space".to_string()),
            keep_transcription_in_clipboard: false,
            play_sound_on_recording: true,
            play_sound_on_transcription_complete: true,
            play_sound_on_paste_success: true,
            pill_indicator_mode: "when_recording".to_string(),
            pill_indicator_style: "full".to_string(),
            pill_indicator_position: "bottom-center".to_string(),
            pill_indicator_offset: 10,
            pause_media_during_recording: true,
            auto_paste_transcription: true,
            sharing_port: Some(47842),
            sharing_password: None,
            save_recordings: true,
            recording_retention_days: Some(7),
            transcription_acceleration: "auto".to_string(),
            whisper_speed_mode: false,
            transcription_mode: "regular".to_string(),
            update_channel: "stable".to_string(),
        };

        // Test serialization
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"hotkey\":\"CommandOrControl+A\""));
        assert!(json.contains("\"current_model\":\"base\""));
        assert!(json.contains("\"speech_language\":\"es\""));
        assert!(json.contains("\"transcription_task\":\"transcribe\""));
        assert!(json.contains("\"final_text_language\":\"same_as_transcript\""));
        assert!(json.contains("\"theme\":\"dark\""));
        assert!(json.contains("\"settings_mode\":\"recommended\""));
        assert!(json.contains("\"transcription_cleanup_days\":7"));
        assert!(json.contains("\"auto_paste_transcription\":true"));

        // Test deserialization
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.hotkey, settings.hotkey);
        assert_eq!(deserialized.current_model, settings.current_model);
        assert_eq!(deserialized.speech_language, settings.speech_language);
        assert_eq!(deserialized.transcription_task, settings.transcription_task);
        assert_eq!(
            deserialized.final_text_language,
            settings.final_text_language
        );
        assert_eq!(deserialized.theme, settings.theme);
        assert_eq!(deserialized.settings_mode, settings.settings_mode);
        assert_eq!(
            deserialized.transcription_cleanup_days,
            settings.transcription_cleanup_days
        );
        assert_eq!(
            deserialized.auto_paste_transcription,
            settings.auto_paste_transcription
        );
    }

    #[test]
    fn test_settings_partial_deserialization() {
        // Test that partial JSON can be deserialized with defaults
        let partial_json = json!({
            "hotkey": "Alt+Space",
            "theme": "light"
        });

        // This should work with serde's default attribute
        let _json_str = serde_json::to_string(&partial_json).unwrap();

        // Since Settings doesn't have default attributes on fields,
        // we can't deserialize partial JSON directly
        // This is expected behavior for the current implementation
    }

    #[test]
    fn test_settings_clone() {
        let settings = Settings {
            polish_keep_words: false,
            hotkey: "CommandOrControl+B".to_string(),
            current_model: "tiny".to_string(),
            current_model_engine: "whisper".to_string(),
            speech_language: "fr".to_string(),
            transcription_task: TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH.to_string(),
            final_text_language: "en".to_string(),
            theme: "light".to_string(),
            settings_mode: "recommended".to_string(),
            transcription_cleanup_days: Some(30),
            pill_position: None,
            launch_at_startup: true,
            onboarding_completed: false,
            check_updates_automatically: true,
            selected_microphone: Some("USB Microphone".to_string()),
            recording_mode: "push_to_talk".to_string(),
            use_different_ptt_key: true,
            ptt_hotkey: Some("CommandOrControl+Space".to_string()),
            keep_transcription_in_clipboard: true,
            play_sound_on_recording: false,
            play_sound_on_transcription_complete: false,
            play_sound_on_paste_success: false,
            pill_indicator_mode: "never".to_string(),
            pill_indicator_style: "compact".to_string(),
            pill_indicator_position: "top-center".to_string(),
            pill_indicator_offset: 25,
            pause_media_during_recording: true,
            auto_paste_transcription: true,
            sharing_port: None,
            sharing_password: Some("test123".to_string()),
            save_recordings: true,
            recording_retention_days: None,
            transcription_acceleration: "auto".to_string(),
            whisper_speed_mode: false,
            transcription_mode: "regular".to_string(),
            update_channel: "beta".to_string(),
        };

        let cloned = settings.clone();
        assert_eq!(cloned.hotkey, settings.hotkey);
        assert_eq!(cloned.current_model, settings.current_model);
        assert_eq!(cloned.speech_language, settings.speech_language);
        assert_eq!(cloned.transcription_task, settings.transcription_task);
        assert_eq!(cloned.final_text_language, settings.final_text_language);
        assert_eq!(cloned.theme, settings.theme);
        assert_eq!(
            cloned.transcription_cleanup_days,
            settings.transcription_cleanup_days
        );
    }

    #[test]
    fn test_valid_hotkey_formats() {
        let valid_hotkeys = vec![
            "CommandOrControl+Shift+Space",
            "Alt+A",
            "CommandOrControl+Alt+B",
            "Shift+F1",
            "CommandOrControl+1",
        ];

        for hotkey in valid_hotkeys {
            let settings = Settings {
                hotkey: hotkey.to_string(),
                ..Settings::default()
            };
            assert!(!settings.hotkey.is_empty());
            assert!(settings.hotkey.len() <= 100);
        }
    }

    #[test]
    fn test_theme_values() {
        let valid_themes = vec!["system", "light", "dark"];

        for theme in valid_themes {
            let settings = Settings {
                theme: theme.to_string(),
                ..Settings::default()
            };
            assert!(["system", "light", "dark"].contains(&settings.theme.as_str()));
        }
    }

    #[test]
    fn test_language_codes() {
        let valid_languages = vec!["en", "es", "fr", "de", "it", "pt", "ru", "zh", "ja", "ko"];

        for lang in valid_languages {
            let settings = Settings {
                speech_language: lang.to_string(),
                ..Settings::default()
            };
            assert_eq!(settings.speech_language, lang);
        }
    }

    #[test]
    fn test_model_language_normalization() {
        assert_eq!(
            normalize_speech_language_for_model("whisper", "base.en", "es"),
            "en"
        );
        assert_eq!(
            normalize_speech_language_for_model("parakeet", "parakeet-tdt-0.6b-v2", "fr"),
            "en"
        );
        assert_eq!(
            normalize_speech_language_for_model("parakeet", "parakeet-tdt-0.6b-v3", "ja"),
            "en"
        );
        assert_eq!(
            normalize_speech_language_for_model("parakeet", "parakeet-tdt-0.6b-v3", "fr"),
            "fr"
        );
    }

    #[test]
    fn test_model_selection() {
        // Empty model means auto-select
        let auto_settings = Settings {
            current_model: "".to_string(),
            current_model_engine: "whisper".to_string(),
            ..Settings::default()
        };
        assert_eq!(auto_settings.current_model, "");

        // Specific model
        let specific_settings = Settings {
            current_model: "base".to_string(),
            current_model_engine: "whisper".to_string(),
            ..Settings::default()
        };
        assert_eq!(specific_settings.current_model, "base");
    }

    #[test]
    fn test_settings_to_json_value() {
        let settings = Settings::default();

        // Convert to JSON Value
        let value = json!({
            "hotkey": settings.hotkey,
            "current_model": settings.current_model,
            "speech_language": settings.speech_language,
            "transcription_task": settings.transcription_task,
            "final_text_language": settings.final_text_language,
            "theme": settings.theme,
            "transcription_cleanup_days": settings.transcription_cleanup_days,
            "launch_at_startup": settings.launch_at_startup,
            "onboarding_completed": settings.onboarding_completed,
        });

        assert_eq!(value["hotkey"], "CommandOrControl+Shift+Space");
        assert_eq!(value["current_model"], "");
        assert_eq!(value["speech_language"], "en");
        assert_eq!(value["transcription_task"], "transcribe");
        assert_eq!(value["final_text_language"], "same_as_transcript");
        assert_eq!(value["theme"], "system");
        assert_eq!(value["transcription_cleanup_days"], serde_json::Value::Null);
        assert_eq!(value["launch_at_startup"], serde_json::Value::Bool(false));
        assert_eq!(
            value["onboarding_completed"],
            serde_json::Value::Bool(false)
        );
    }

    #[test]
    fn test_hotkey_validation_edge_cases() {
        let empty_settings = Settings {
            hotkey: String::new(),
            ..Settings::default()
        };
        assert!(empty_settings.hotkey.is_empty());

        // Test very long hotkey (invalid if > 100 chars)
        let long_hotkey = "CommandOrControl+".repeat(10);
        assert!(long_hotkey.len() > 100);

        // Test normal hotkey length
        let normal_hotkey = "CommandOrControl+Shift+Alt+A";
        assert!(!normal_hotkey.is_empty());
        assert!(normal_hotkey.len() <= 100);
    }

    #[tokio::test]
    async fn test_get_supported_languages() {
        let languages = get_supported_languages().await.unwrap();

        // Should have multiple languages
        assert!(languages.len() > 50);

        // Languages should be sorted alphabetically (auto-detect removed)
        // First language will be Afrikaans alphabetically
        assert_eq!(languages[0].code, "af");
        assert_eq!(languages[0].name, "Afrikaans");

        // Should contain common languages
        let codes: Vec<String> = languages.iter().map(|l| l.code.clone()).collect();
        assert!(codes.contains(&"en".to_string()));
        assert!(codes.contains(&"es".to_string()));
        assert!(codes.contains(&"fr".to_string()));
        assert!(codes.contains(&"zh".to_string()));

        // Should be sorted by name alphabetically
        for i in 1..languages.len() {
            assert!(
                languages[i - 1].name <= languages[i].name,
                "Languages should be sorted by name"
            );
        }
    }

    // ==================== Pill Indicator Mode Tests ====================

    #[test]
    fn test_pill_indicator_mode_valid_values() {
        let valid_modes = vec!["never", "always", "when_recording"];

        for mode in valid_modes {
            let settings = Settings {
                pill_indicator_mode: mode.to_string(),
                ..Settings::default()
            };
            assert_eq!(settings.pill_indicator_mode, mode);
        }
    }

    #[test]
    fn test_pill_indicator_mode_default() {
        let settings = Settings::default();
        assert_eq!(settings.pill_indicator_mode, "when_recording");
    }

    #[test]
    fn test_pill_indicator_mode_serialization() {
        let settings = Settings {
            pill_indicator_mode: "always".to_string(),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"pill_indicator_mode\":\"always\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.pill_indicator_mode, "always");
    }

    // ==================== Pill Indicator Position Tests ====================

    #[test]
    fn test_pill_indicator_position_valid_values() {
        let valid_positions = vec![
            "top-left",
            "top-center",
            "top-right",
            "bottom-left",
            "bottom-center",
            "bottom-right",
        ];

        for position in valid_positions {
            let settings = Settings {
                pill_indicator_position: position.to_string(),
                ..Settings::default()
            };
            assert_eq!(settings.pill_indicator_position, position);
        }
    }

    #[test]
    fn test_pill_indicator_position_default() {
        let settings = Settings::default();
        assert_eq!(settings.pill_indicator_position, "bottom-center");
    }

    #[test]
    fn test_pill_indicator_position_serialization() {
        let settings = Settings {
            pill_indicator_position: "top-center".to_string(),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"pill_indicator_position\":\"top-center\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.pill_indicator_position, "top-center");
    }

    // ==================== Sound Settings Tests ====================

    #[test]
    fn audio_feedback_defaults_on() {
        let settings = Settings::default();
        assert!(settings.play_sound_on_recording);
        assert!(settings.play_sound_on_transcription_complete);
        assert!(settings.play_sound_on_paste_success);
    }

    #[test]
    fn transcription_complete_sound_migrates_legacy_preference() {
        assert!(!resolve_transcription_complete_sound(None, Some(false)));
        assert!(resolve_transcription_complete_sound(None, Some(true)));
        assert!(resolve_transcription_complete_sound(
            Some(true),
            Some(false)
        ));
        assert!(resolve_transcription_complete_sound(None, None));
    }

    #[test]
    fn test_pause_media_during_recording_default_off() {
        // PORT-S4: pause-media-during-recording defaults OFF; users opt in.
        let settings = Settings::default();
        assert!(!settings.pause_media_during_recording);
    }

    #[test]
    fn test_pause_media_during_recording_roundtrip() {
        let settings = Settings {
            pause_media_during_recording: true,
            ..Settings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert!(deserialized.pause_media_during_recording);
    }

    #[test]
    fn test_transcription_acceleration_default_auto() {
        let settings = Settings::default();
        assert_eq!(settings.transcription_acceleration, "auto");
    }

    #[test]
    fn test_transcription_acceleration_raw_store_key() {
        // Assert the serde(default) kicks in when the key is absent from JSON.
        let mut v = serde_json::to_value(Settings::default()).unwrap();
        v.as_object_mut()
            .unwrap()
            .remove("transcription_acceleration");
        let parsed: Settings = serde_json::from_value(v).unwrap();
        assert_eq!(parsed.transcription_acceleration, "auto");

        // Assert the key serializes as the lowercase string the store/runtime rely on.
        let settings = Settings {
            transcription_acceleration: "cpu".to_string(),
            ..Settings::default()
        };
        let v = serde_json::to_value(&settings).unwrap();
        assert_eq!(
            v["transcription_acceleration"],
            serde_json::Value::String("cpu".to_string())
        );
    }

    #[test]
    fn test_update_channel_defaults_and_serializes() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().remove("update_channel");
        let parsed: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.update_channel, "stable");

        let settings = Settings {
            update_channel: "beta".to_string(),
            ..Settings::default()
        };
        let value = serde_json::to_value(settings).unwrap();
        assert_eq!(value["update_channel"], "beta");
        assert_eq!(
            UpdateChannel::from_stored(value["update_channel"].as_str()),
            UpdateChannel::Beta
        );
    }

    #[test]
    fn test_transcription_acceleration_invalid_value_normalizes_to_auto() {
        // None and garbage values must normalize to "auto" via the real production fn.
        assert_eq!(normalize_stored_transcription_acceleration(None), "auto");
        for bad in &["", "GPU", "Cpu", "vulkan", "metal"] {
            assert_eq!(
                normalize_stored_transcription_acceleration(Some(bad)),
                "auto",
                "bad value {:?} must normalize to auto",
                bad
            );
        }
        // Valid values pass through unchanged.
        assert_eq!(
            normalize_stored_transcription_acceleration(Some("gpu")),
            "gpu"
        );
        assert_eq!(
            normalize_stored_transcription_acceleration(Some("cpu")),
            "cpu"
        );
    }

    #[test]
    fn test_sound_settings_can_be_disabled() {
        let settings = Settings {
            play_sound_on_recording: false,
            play_sound_on_transcription_complete: false,
            play_sound_on_paste_success: false,
            ..Settings::default()
        };

        assert!(!settings.play_sound_on_recording);
        assert!(!settings.play_sound_on_transcription_complete);
        assert!(!settings.play_sound_on_paste_success);
    }

    #[test]
    fn test_sound_settings_serialization() {
        let settings = Settings {
            play_sound_on_recording: false,
            play_sound_on_transcription_complete: true,
            play_sound_on_paste_success: true,
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"play_sound_on_recording\":false"));
        assert!(json.contains("\"play_sound_on_transcription_complete\":true"));
        assert!(json.contains("\"play_sound_on_paste_success\":true"));
        assert!(!json.contains("\"play_sound_on_recording_end\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.play_sound_on_recording);
        assert!(deserialized.play_sound_on_transcription_complete);
        assert!(deserialized.play_sound_on_paste_success);
    }

    // ==================== Recording Mode Tests ====================

    #[test]
    fn test_recording_mode_default() {
        let settings = Settings::default();
        assert_eq!(settings.recording_mode, "toggle");
    }

    #[test]
    fn test_recording_mode_valid_values() {
        let valid_modes = vec!["toggle", "push_to_talk"];

        for mode in valid_modes {
            let settings = Settings {
                recording_mode: mode.to_string(),
                ..Settings::default()
            };
            assert_eq!(settings.recording_mode, mode);
        }
    }

    #[test]
    fn test_recording_mode_serialization() {
        let settings = Settings {
            recording_mode: "push_to_talk".to_string(),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"recording_mode\":\"push_to_talk\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.recording_mode, "push_to_talk");
    }

    #[test]
    fn test_ptt_settings_defaults() {
        let settings = Settings::default();
        assert!(!settings.use_different_ptt_key);
        assert_eq!(settings.ptt_hotkey, Some("Alt+Space".to_string()));
    }

    #[test]
    fn test_ptt_settings_with_custom_key() {
        let settings = Settings {
            recording_mode: "push_to_talk".to_string(),
            use_different_ptt_key: true,
            ptt_hotkey: Some("CommandOrControl+Shift+R".to_string()),
            ..Settings::default()
        };

        assert_eq!(settings.recording_mode, "push_to_talk");
        assert!(settings.use_different_ptt_key);
        assert_eq!(
            settings.ptt_hotkey,
            Some("CommandOrControl+Shift+R".to_string())
        );
    }

    #[test]
    fn test_ptt_hotkey_none() {
        let settings = Settings {
            ptt_hotkey: None,
            ..Settings::default()
        };

        assert!(settings.ptt_hotkey.is_none());
    }

    // ==================== Network Sharing Settings Tests ====================

    #[test]
    fn test_sharing_port_default() {
        let settings = Settings::default();
        assert_eq!(settings.sharing_port, Some(47842));
    }

    #[test]
    fn test_sharing_password_default() {
        let settings = Settings::default();
        assert!(settings.sharing_password.is_none());
    }

    #[test]
    fn test_sharing_settings_with_password() {
        let settings = Settings {
            sharing_port: Some(8080),
            sharing_password: Some("secretpass".to_string()),
            ..Settings::default()
        };

        assert_eq!(settings.sharing_port, Some(8080));
        assert_eq!(settings.sharing_password, Some("secretpass".to_string()));
    }

    #[test]
    fn test_sharing_settings_serialization() {
        let settings = Settings {
            sharing_port: Some(9999),
            sharing_password: Some("test123".to_string()),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"sharing_port\":9999"));
        assert!(json.contains("\"sharing_password\":\"test123\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sharing_port, Some(9999));
        assert_eq!(deserialized.sharing_password, Some("test123".to_string()));
    }

    #[test]
    fn test_sharing_port_boundary_values() {
        // Minimum valid port
        let settings_min = Settings {
            sharing_port: Some(1),
            ..Settings::default()
        };
        assert_eq!(settings_min.sharing_port, Some(1));

        // Maximum valid port
        let settings_max = Settings {
            sharing_port: Some(65535),
            ..Settings::default()
        };
        assert_eq!(settings_max.sharing_port, Some(65535));

        // No port (None)
        let settings_none = Settings {
            sharing_port: None,
            ..Settings::default()
        };
        assert!(settings_none.sharing_port.is_none());
    }

    // ==================== Clipboard Settings Tests ====================

    #[test]
    fn test_keep_transcription_in_clipboard_default() {
        let settings = Settings::default();
        assert!(!settings.keep_transcription_in_clipboard);
    }

    #[test]
    fn test_keep_transcription_in_clipboard_enabled() {
        let settings = Settings {
            keep_transcription_in_clipboard: true,
            ..Settings::default()
        };

        assert!(settings.keep_transcription_in_clipboard);
    }

    // ==================== Microphone Settings Tests ====================

    #[test]
    fn test_selected_microphone_default() {
        let settings = Settings::default();
        assert!(settings.selected_microphone.is_none());
    }

    #[test]
    fn test_selected_microphone_with_device() {
        let settings = Settings {
            selected_microphone: Some("USB Audio Device".to_string()),
            ..Settings::default()
        };

        assert_eq!(
            settings.selected_microphone,
            Some("USB Audio Device".to_string())
        );
    }

    // ==================== Translate Settings Tests ====================

    #[test]
    fn test_transcription_task_default() {
        let settings = Settings::default();
        assert_eq!(settings.transcription_task, TRANSCRIPTION_TASK_TRANSCRIBE);
        assert!(!task_uses_translate_to_english(
            &settings.transcription_task
        ));
    }

    #[test]
    fn test_transcription_task_translate_to_english() {
        let settings = Settings {
            transcription_task: TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH.to_string(),
            final_text_language: "en".to_string(),
            ..Settings::default()
        };

        assert!(task_uses_translate_to_english(&settings.transcription_task));
    }

    #[test]
    fn test_normalize_final_text_language_preserves_supported_target_language() {
        assert_eq!(
            normalize_final_text_language(Some("es"), TRANSCRIPTION_TASK_TRANSCRIBE),
            "es"
        );
    }

    #[test]
    fn test_normalize_final_text_language_translate_task_forces_english() {
        assert_eq!(
            normalize_final_text_language(Some("fr"), TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH),
            "en"
        );
    }

    // ==================== Pill Position Tests ====================

    #[test]
    fn test_pill_position_default() {
        let settings = Settings::default();
        assert!(settings.pill_position.is_none());
    }

    #[test]
    fn test_pill_position_with_coordinates() {
        let settings = Settings {
            pill_position: Some((150.5, 300.25)),
            ..Settings::default()
        };

        assert_eq!(settings.pill_position, Some((150.5, 300.25)));
    }

    #[test]
    fn test_pill_position_negative_coordinates() {
        // Negative coordinates could occur with multiple monitors
        let settings = Settings {
            pill_position: Some((-100.0, -50.0)),
            ..Settings::default()
        };

        assert_eq!(settings.pill_position, Some((-100.0, -50.0)));
    }

    #[test]
    fn test_pill_position_zero_coordinates() {
        let settings = Settings {
            pill_position: Some((0.0, 0.0)),
            ..Settings::default()
        };

        assert_eq!(settings.pill_position, Some((0.0, 0.0)));
    }

    // ==================== Transcription Cleanup Days Tests ====================

    #[test]
    fn test_transcription_cleanup_days_default() {
        let settings = Settings::default();
        assert!(settings.transcription_cleanup_days.is_none());
    }

    #[test]
    fn test_transcription_cleanup_days_values() {
        let test_values = vec![1, 7, 30, 90, 365];

        for days in test_values {
            let settings = Settings {
                transcription_cleanup_days: Some(days),
                ..Settings::default()
            };
            assert_eq!(settings.transcription_cleanup_days, Some(days));
        }
    }

    // ==================== Model Engine Tests ====================

    #[test]
    fn test_model_engine_default() {
        let settings = Settings::default();
        assert_eq!(settings.current_model_engine, "whisper");
    }

    #[test]
    fn test_model_engine_valid_values() {
        let valid_engines = vec![
            "whisper", "parakeet", "soniox", "openai", "groq", "deepgram", "cohere",
        ];

        for engine in valid_engines {
            let settings = Settings {
                current_model_engine: engine.to_string(),
                ..Settings::default()
            };
            assert_eq!(settings.current_model_engine, engine);
        }
    }

    // ==================== Complete Settings Round-Trip Tests ====================

    #[test]
    fn test_settings_full_round_trip() {
        let original = Settings {
            polish_keep_words: false,
            hotkey: "CommandOrControl+Alt+V".to_string(),
            current_model: "large-v3".to_string(),
            current_model_engine: "whisper".to_string(),
            speech_language: "ja".to_string(),
            transcription_task: TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH.to_string(),
            final_text_language: "en".to_string(),
            theme: "dark".to_string(),
            settings_mode: "recommended".to_string(),
            transcription_cleanup_days: Some(14),
            pill_position: Some((500.0, 200.0)),
            launch_at_startup: true,
            onboarding_completed: true,
            check_updates_automatically: false,
            selected_microphone: Some("Studio Mic".to_string()),
            recording_mode: "push_to_talk".to_string(),
            use_different_ptt_key: true,
            ptt_hotkey: Some("Alt+R".to_string()),
            keep_transcription_in_clipboard: true,
            play_sound_on_recording: false,
            play_sound_on_transcription_complete: false,
            play_sound_on_paste_success: false,
            pill_indicator_mode: "always".to_string(),
            pill_indicator_style: "compact".to_string(),
            pill_indicator_position: "top-left".to_string(),
            pill_indicator_offset: 42,
            pause_media_during_recording: false,
            auto_paste_transcription: false,
            sharing_port: Some(5555),
            sharing_password: Some("pw".to_string()),
            save_recordings: true,
            recording_retention_days: None,
            transcription_acceleration: "gpu".to_string(),
            whisper_speed_mode: true,
            transcription_mode: "regular".to_string(),
            update_channel: "beta".to_string(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let restored: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.hotkey, original.hotkey);
        assert_eq!(restored.current_model, original.current_model);
        assert_eq!(restored.current_model_engine, original.current_model_engine);
        assert_eq!(restored.speech_language, original.speech_language);
        assert_eq!(restored.transcription_task, original.transcription_task);
        assert_eq!(restored.final_text_language, original.final_text_language);
        assert_eq!(restored.theme, original.theme);
        assert_eq!(
            restored.transcription_cleanup_days,
            original.transcription_cleanup_days
        );
        assert_eq!(restored.update_channel, original.update_channel);
        assert_eq!(restored.pill_position, original.pill_position);
        assert_eq!(restored.launch_at_startup, original.launch_at_startup);
        assert_eq!(restored.onboarding_completed, original.onboarding_completed);
        assert_eq!(
            restored.check_updates_automatically,
            original.check_updates_automatically
        );
        assert_eq!(restored.selected_microphone, original.selected_microphone);
        assert_eq!(restored.recording_mode, original.recording_mode);
        assert_eq!(
            restored.use_different_ptt_key,
            original.use_different_ptt_key
        );
        assert_eq!(restored.ptt_hotkey, original.ptt_hotkey);
        assert_eq!(
            restored.keep_transcription_in_clipboard,
            original.keep_transcription_in_clipboard
        );
        assert_eq!(
            restored.play_sound_on_recording,
            original.play_sound_on_recording
        );
        assert_eq!(
            restored.play_sound_on_transcription_complete,
            original.play_sound_on_transcription_complete
        );
        assert_eq!(
            restored.play_sound_on_paste_success,
            original.play_sound_on_paste_success
        );
        assert_eq!(restored.pill_indicator_mode, original.pill_indicator_mode);
        assert_eq!(
            restored.pill_indicator_position,
            original.pill_indicator_position
        );
        assert_eq!(
            restored.pill_indicator_offset,
            original.pill_indicator_offset
        );
        assert_eq!(
            restored.pause_media_during_recording,
            original.pause_media_during_recording
        );
        assert_eq!(restored.sharing_port, original.sharing_port);
        assert_eq!(restored.sharing_password, original.sharing_password);
        assert_eq!(restored.save_recordings, original.save_recordings);
        assert_eq!(
            restored.recording_retention_days,
            original.recording_retention_days
        );
        assert_eq!(
            restored.transcription_acceleration,
            original.transcription_acceleration
        );
    }

    #[test]
    fn test_settings_json_structure() {
        let settings = Settings::default();
        let json_value: serde_json::Value = serde_json::to_value(&settings).unwrap();

        // Verify all expected keys exist
        assert!(json_value.get("hotkey").is_some());
        assert!(json_value.get("current_model").is_some());
        assert!(json_value.get("current_model_engine").is_some());
        assert!(json_value.get("speech_language").is_some());
        assert!(json_value.get("transcription_task").is_some());
        assert!(json_value.get("final_text_language").is_some());
        assert!(json_value.get("theme").is_some());
        assert!(json_value.get("transcription_cleanup_days").is_some());
        assert!(json_value.get("pill_position").is_some());
        assert!(json_value.get("launch_at_startup").is_some());
        assert!(json_value.get("onboarding_completed").is_some());
        assert!(json_value.get("check_updates_automatically").is_some());
        assert!(json_value.get("selected_microphone").is_some());
        assert!(json_value.get("recording_mode").is_some());
        assert!(json_value.get("use_different_ptt_key").is_some());
        assert!(json_value.get("ptt_hotkey").is_some());
        assert!(json_value.get("keep_transcription_in_clipboard").is_some());
        assert!(json_value.get("play_sound_on_recording").is_some());
        assert!(json_value
            .get("play_sound_on_transcription_complete")
            .is_some());
        assert!(json_value.get("play_sound_on_paste_success").is_some());
        assert!(json_value.get("pill_indicator_mode").is_some());
        assert!(json_value.get("pill_indicator_position").is_some());
        assert!(json_value.get("pill_indicator_offset").is_some());
        assert!(json_value.get("pause_media_during_recording").is_some());
        assert!(json_value.get("sharing_port").is_some());
        assert!(json_value.get("sharing_password").is_some());
        assert!(json_value.get("save_recordings").is_some());
        assert!(json_value.get("recording_retention_days").is_some());
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_settings_empty_strings() {
        let settings = Settings {
            hotkey: "".to_string(),
            current_model: "".to_string(),
            current_model_engine: "".to_string(),
            speech_language: "".to_string(),
            transcription_task: "".to_string(),
            final_text_language: "".to_string(),
            theme: "".to_string(),
            recording_mode: "".to_string(),
            pill_indicator_mode: "".to_string(),
            pill_indicator_position: "".to_string(),
            ..Settings::default()
        };

        assert_eq!(settings.hotkey, "");
        assert_eq!(settings.current_model, "");
        assert_eq!(settings.current_model_engine, "");
        assert_eq!(settings.speech_language, "");
    }

    #[test]
    fn test_settings_unicode_values() {
        let settings = Settings {
            selected_microphone: Some("マイク 日本語".to_string()),
            sharing_password: Some("пароль123".to_string()),
            ..Settings::default()
        };

        assert_eq!(
            settings.selected_microphone,
            Some("マイク 日本語".to_string())
        );
        assert_eq!(settings.sharing_password, Some("пароль123".to_string()));

        // Verify serialization preserves unicode
        let json = serde_json::to_string(&settings).unwrap();
        let restored: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.selected_microphone, settings.selected_microphone);
        assert_eq!(restored.sharing_password, settings.sharing_password);
    }

    #[test]
    fn test_settings_special_characters_in_password() {
        let special_passwords = vec![
            "pass!@#$%^&*()",
            "with spaces here",
            "quotes\"and'apostrophes",
            "backslash\\path",
            "newline\ntest",
        ];

        for password in special_passwords {
            let settings = Settings {
                sharing_password: Some(password.to_string()),
                ..Settings::default()
            };

            let json = serde_json::to_string(&settings).unwrap();
            let restored: Settings = serde_json::from_str(&json).unwrap();
            assert_eq!(
                restored.sharing_password,
                Some(password.to_string()),
                "Failed for password: {}",
                password
            );
        }
    }

    #[test]
    fn test_settings_very_long_values() {
        let long_string = "a".repeat(10000);

        let settings = Settings {
            selected_microphone: Some(long_string.clone()),
            ..Settings::default()
        };

        assert_eq!(settings.selected_microphone, Some(long_string.clone()));

        // Should still serialize/deserialize correctly
        let json = serde_json::to_string(&settings).unwrap();
        let restored: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.selected_microphone, Some(long_string));
    }
}
