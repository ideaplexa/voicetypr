use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::ai::error::AiProviderError;
use crate::ai::prompts::EnhancementPreset;

use super::normalize_language_scope;

const WRITING_SETTINGS_KEY: &str = "writing_settings";

fn default_enabled() -> bool {
    true
}

fn default_preserve_literal() -> bool {
    true
}

#[derive(Debug, Clone)]
pub enum WritingError {
    TranslationFailed {
        target_language: String,
        detail: String,
    },
    OutputLanguageRequiresAi,
    Config(String),
}

impl WritingError {
    pub fn user_message(&self) -> String {
        match self {
            Self::TranslationFailed {
                target_language, ..
            } => format!("Translation to {} failed", target_language),
            Self::OutputLanguageRequiresAi => {
                "Final output language requires AI enhancement or native translation".to_string()
            }
            Self::Config(message) => message.clone(),
        }
    }
}

impl std::fmt::Display for WritingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TranslationFailed { detail, .. } if !detail.is_empty() => {
                write!(f, "{}: {}", self.user_message(), detail)
            }
            _ => f.write_str(&self.user_message()),
        }
    }
}

impl std::error::Error for WritingError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TextReplacementRule {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CustomWord {
    pub phrase: String,
    #[serde(default)]
    pub spoken_form: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Snippet {
    pub trigger: String,
    pub body: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_preserve_literal")]
    pub preserve_literal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppFormattingRule {
    pub app_name: String,
    pub preset: EnhancementPreset,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WritingSettings {
    #[serde(default)]
    pub replacements: Vec<TextReplacementRule>,
    #[serde(default)]
    pub custom_words: Vec<CustomWord>,
    #[serde(default)]
    pub snippets: Vec<Snippet>,
    #[serde(default)]
    pub app_formatting_rules: Vec<AppFormattingRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WritingProfile {
    pub mode: EnhancementPreset,
    pub final_text_language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingOperationKind {
    TranscriptCleanup,
    Replacement,
    Snippet,
    Translation,
    AiCleanup,
    // Retained so history written before spoken commands were removed still deserializes.
    VoiceCommand,
    FinalGuard,
    /// Shared local formatting: capitals, spacing, optional fillers, paragraph breaks.
    FormatTranscript,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedWritingOperation {
    pub kind: WritingOperationKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WritingWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContextHint {
    #[serde(default)]
    pub app_name: Option<String>,
    // window_title is potentially sensitive (document names, DM subjects).
    // Captured for classification but NEVER serialized — #[serde(skip)] ensures
    // it never reaches history metadata or any JSON output.
    #[serde(skip)]
    pub window_title: Option<String>,
    #[serde(default)]
    pub process_path: Option<String>,
    #[serde(default)]
    pub category: Option<crate::writing::AppCategory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WritingStageTimings {
    pub deterministic_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_polish_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insertion_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiExecutionMetadata {
    pub provider_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WritingResult {
    pub raw_text: String,
    pub final_text: String,
    pub output_language: String,
    pub mode: EnhancementPreset,
    pub ai_applied: bool,
    #[serde(default)]
    pub applied_operations: Vec<AppliedWritingOperation>,
    #[serde(default)]
    pub warnings: Vec<WritingWarning>,
    #[serde(default)]
    pub context_hint: Option<ContextHint>,
    #[serde(default)]
    pub stage_timings: WritingStageTimings,
    #[serde(skip)]
    pub polish_enabled: bool,
    #[serde(skip)]
    pub ai_execution: Option<AiExecutionMetadata>,
    #[serde(skip)]
    pub ai_error: Option<AiProviderError>,
}

pub fn sanitize_writing_settings(settings: WritingSettings) -> WritingSettings {
    WritingSettings {
        replacements: settings
            .replacements
            .into_iter()
            .filter_map(|rule| {
                let from = rule.from.trim();
                let to_trimmed = rule.to.trim();
                // Trim surrounding whitespace from the replacement target, but when
                // it is intentionally whitespace-only (e.g. de-hyphenation "-" ->
                // " ") preserve it verbatim instead of dropping the rule.
                let to = if to_trimmed.is_empty() {
                    rule.to.as_str()
                } else {
                    to_trimmed
                };
                if from.is_empty() || to.is_empty() {
                    return None;
                }
                Some(TextReplacementRule {
                    from: from.to_string(),
                    to: to.to_string(),
                    language: normalize_language_scope(rule.language.as_deref()),
                    enabled: rule.enabled,
                })
            })
            .collect(),
        custom_words: settings
            .custom_words
            .into_iter()
            .filter_map(|word| {
                let phrase = word.phrase.trim();
                if phrase.is_empty() {
                    return None;
                }
                let spoken_form = word.spoken_form.and_then(|value| {
                    let trimmed = value.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                });
                Some(CustomWord {
                    phrase: phrase.to_string(),
                    spoken_form,
                    language: normalize_language_scope(word.language.as_deref()),
                    enabled: word.enabled,
                })
            })
            .collect(),
        snippets: settings
            .snippets
            .into_iter()
            .filter_map(|snippet| {
                let trigger = snippet.trigger.trim();
                let body = snippet.body.trim_end();
                if trigger.is_empty() || body.is_empty() {
                    return None;
                }
                Some(Snippet {
                    trigger: trigger.to_string(),
                    body: body.to_string(),
                    language: normalize_language_scope(snippet.language.as_deref()),
                    enabled: snippet.enabled,
                    preserve_literal: snippet.preserve_literal,
                })
            })
            .collect(),
        app_formatting_rules: settings
            .app_formatting_rules
            .into_iter()
            .filter_map(|rule| {
                let app_name = rule.app_name.trim();
                if app_name.is_empty() {
                    return None;
                }
                Some(AppFormattingRule {
                    app_name: app_name.to_string(),
                    preset: rule.preset,
                    enabled: rule.enabled,
                })
            })
            .collect(),
    }
}

fn validate_entry_languages<T>(
    entries: &[T],
    previous: &[T],
    identity: fn(&T) -> (&str, Option<&str>),
) -> Result<(), String> {
    // Consume each stored identity/scope once: copying a legacy invalid row is
    // still a new invalid entry. Other fields may change without altering scope.
    let mut legacy = previous.iter().map(identity).collect::<Vec<_>>();
    for entry in entries {
        let (key, scope) = identity(entry);
        let Some(scope) = scope.filter(|scope| !scope.trim().is_empty()) else {
            continue;
        };
        if super::pipeline::supported_language_base(scope).is_some() {
            continue;
        }
        let Some(index) = legacy.iter().position(|(old_key, old_scope)| {
            old_key.trim() == key.trim() && *old_scope == Some(scope)
        }) else {
            return Err("Unsupported dictionary language code. Use a supported code such as en, de or de-AT, or leave it empty for all languages.".to_string());
        };
        legacy.remove(index);
    }
    Ok(())
}

fn validate_dictionary_languages(
    settings: &WritingSettings,
    previous: &WritingSettings,
) -> Result<(), String> {
    validate_entry_languages(&settings.replacements, &previous.replacements, |entry| {
        (&entry.from, entry.language.as_deref())
    })?;
    validate_entry_languages(&settings.custom_words, &previous.custom_words, |entry| {
        (&entry.phrase, entry.language.as_deref())
    })?;
    validate_entry_languages(&settings.snippets, &previous.snippets, |entry| {
        (&entry.trigger, entry.language.as_deref())
    })
}

pub fn load_writing_settings<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Result<WritingSettings, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    if let Some(value) = store.get(WRITING_SETTINGS_KEY) {
        let settings: WritingSettings = serde_json::from_value(value.clone())
            .map_err(|e| format!("Failed to parse writing settings: {}", e))?;
        if validate_dictionary_languages(&settings, &WritingSettings::default()).is_err() {
            log::warn!("Stored dictionary contains an unsupported language scope; preserving it");
        }
        Ok(sanitize_writing_settings(settings))
    } else {
        Ok(WritingSettings::default())
    }
}

pub fn save_writing_settings<R: tauri::Runtime>(
    app: &AppHandle<R>,
    settings: &WritingSettings,
) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let previous = store
        .get(WRITING_SETTINGS_KEY)
        .map(serde_json::from_value::<WritingSettings>)
        .transpose()
        .map_err(|e| format!("Failed to parse stored writing settings: {}", e))?
        .unwrap_or_default();
    validate_dictionary_languages(settings, &previous)?;
    let sanitized = sanitize_writing_settings(settings.clone());
    store.set(
        WRITING_SETTINGS_KEY,
        serde_json::to_value(&sanitized)
            .map_err(|e| format!("Failed to serialize writing settings: {}", e))?,
    );
    store
        .save()
        .map_err(|e| format!("Failed to save writing settings: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_languages_accept_empty_and_case_insensitive_supported_codes() {
        for language in [
            None,
            Some(""),
            Some("  "),
            Some(" de "),
            Some("DE"),
            Some("yue"),
            Some("de-AT"),
            Some("de-CH"),
            Some("de_DE"),
            Some("en-GB"),
        ] {
            let settings = WritingSettings {
                custom_words: vec![CustomWord {
                    phrase: "Test".into(),
                    language: language.map(str::to_string),
                    ..CustomWord::default()
                }],
                ..WritingSettings::default()
            };
            assert!(
                validate_dictionary_languages(&settings, &WritingSettings::default()).is_ok(),
                "{language:?}"
            );
        }
        assert_eq!(
            normalize_language_scope(Some(" DE ")).as_deref(),
            Some("de")
        );
        assert_eq!(normalize_language_scope(Some(" ")), None);
        assert_eq!(normalize_language_scope(None), None);
    }

    #[test]
    fn dictionary_languages_reject_invalid_scopes_in_every_entry_kind() {
        for language in ["d", "invalid", "auto", "de-", "de_CH_extra", "english"] {
            let scope = Some(language.to_string());
            let cases = [
                WritingSettings {
                    custom_words: vec![CustomWord {
                        phrase: "Test".into(),
                        language: scope.clone(),
                        ..CustomWord::default()
                    }],
                    ..WritingSettings::default()
                },
                WritingSettings {
                    replacements: vec![TextReplacementRule {
                        from: "A".into(),
                        to: "B".into(),
                        language: scope.clone(),
                        ..TextReplacementRule::default()
                    }],
                    ..WritingSettings::default()
                },
                WritingSettings {
                    snippets: vec![Snippet {
                        trigger: "insert test".into(),
                        body: "Test".into(),
                        language: scope.clone(),
                        ..Snippet::default()
                    }],
                    ..WritingSettings::default()
                },
            ];
            for settings in cases {
                assert!(
                    validate_dictionary_languages(&settings, &WritingSettings::default())
                        .unwrap_err()
                        .contains("Unsupported dictionary language code")
                );
            }
        }
    }

    #[test]
    fn legacy_invalid_dictionary_scopes_are_preserved_not_reinterpreted() {
        let settings = sanitize_writing_settings(WritingSettings {
            custom_words: vec![CustomWord {
                phrase: "Test".into(),
                language: Some(" d ".into()),
                ..CustomWord::default()
            }],
            replacements: vec![TextReplacementRule {
                from: "A".into(),
                to: "B".into(),
                language: Some("invalid".into()),
                ..TextReplacementRule::default()
            }],
            snippets: vec![Snippet {
                trigger: "insert test".into(),
                body: "Test".into(),
                language: Some("zz-AT".into()),
                ..Snippet::default()
            }],
            ..WritingSettings::default()
        });
        assert_eq!(settings.custom_words[0].language.as_deref(), Some(" d "));
        assert_eq!(
            settings.replacements[0].language.as_deref(),
            Some("invalid")
        );
        assert_eq!(settings.snippets[0].language.as_deref(), Some("zz-AT"));
        assert!(!super::super::language_scope_matches(
            settings.custom_words[0].language.as_deref(),
            Some("en")
        ));
        assert!(!super::super::language_scope_matches(
            settings.replacements[0].language.as_deref(),
            Some("en")
        ));
        assert!(!super::super::language_scope_matches(
            settings.snippets[0].language.as_deref(),
            Some("en")
        ));
        assert!(validate_dictionary_languages(&settings, &WritingSettings::default()).is_err());
    }

    #[test]
    fn legacy_invalid_scopes_do_not_block_unrelated_saves() {
        let dir = tempfile::tempdir().unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = dir.path().to_str().unwrap().to_string();
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .build(context)
            .unwrap();
        let previous = WritingSettings {
            custom_words: vec![CustomWord {
                phrase: "Alpha".into(),
                language: Some(" d ".into()),
                enabled: true,
                ..CustomWord::default()
            }],
            replacements: vec![TextReplacementRule {
                from: "A".into(),
                to: "B".into(),
                language: Some("invalid".into()),
                enabled: true,
            }],
            snippets: vec![Snippet {
                trigger: "insert test".into(),
                body: "Test".into(),
                language: Some("zz-AT".into()),
                enabled: true,
                preserve_literal: true,
            }],
            ..WritingSettings::default()
        };
        let store = app.store("settings").unwrap();
        store.set(
            WRITING_SETTINGS_KEY,
            serde_json::to_value(&previous).unwrap(),
        );
        store.save().unwrap();
        let mut next = load_writing_settings(app.handle()).unwrap();
        next.app_formatting_rules.push(AppFormattingRule {
            app_name: "Editor".into(),
            preset: EnhancementPreset::Notes,
            enabled: true,
        });
        next.custom_words.push(CustomWord {
            phrase: "Beta".into(),
            language: Some("de-AT".into()),
            enabled: true,
            ..CustomWord::default()
        });
        assert!(save_writing_settings(app.handle(), &next).is_ok());
        let saved = load_writing_settings(app.handle()).unwrap();
        assert_eq!(saved.custom_words[0], previous.custom_words[0]);
        assert_eq!(saved.replacements, previous.replacements);
        assert_eq!(saved.snippets, previous.snippets);
        assert_eq!(saved.app_formatting_rules.len(), 1);
        assert_eq!(saved.custom_words[1].language.as_deref(), Some("de"));
        next.custom_words[0].language = Some("new-invalid".into());
        assert!(save_writing_settings(app.handle(), &next).is_err());
        assert_eq!(load_writing_settings(app.handle()).unwrap(), saved);
    }

    #[test]
    fn preserving_legacy_scopes_allows_other_fields_but_not_new_or_changed_invalid_values() {
        let previous = WritingSettings {
            custom_words: vec![CustomWord {
                phrase: "Alpha".into(),
                language: Some("d".into()),
                enabled: true,
                ..CustomWord::default()
            }],
            ..WritingSettings::default()
        };
        let mut next = previous.clone();
        next.custom_words[0].enabled = false;
        assert!(validate_dictionary_languages(&next, &previous).is_ok());
        next.custom_words[0].spoken_form = Some("alternate".into());
        assert!(validate_dictionary_languages(&next, &previous).is_ok());
        next.custom_words.push(next.custom_words[0].clone());
        assert!(validate_dictionary_languages(&next, &previous).is_err());
        next.custom_words.pop();
        next.custom_words[0].language = Some("D".into());
        assert!(validate_dictionary_languages(&next, &previous).is_err());
        next.custom_words[0].language = Some("d".into());
        next.custom_words[0].phrase = "New identity".into();
        assert!(validate_dictionary_languages(&next, &previous).is_err());
    }

    #[test]
    fn test_sanitize_writing_settings_trims_and_drops_empty_entries() {
        let settings = sanitize_writing_settings(WritingSettings {
            replacements: vec![
                TextReplacementRule {
                    from: " voice typer ".to_string(),
                    to: " Voicetypr ".to_string(),
                    language: Some(" en ".to_string()),
                    enabled: true,
                },
                TextReplacementRule::default(),
            ],
            custom_words: vec![
                CustomWord {
                    phrase: " OpenAI ".to_string(),
                    spoken_form: Some(" open ai ".to_string()),
                    language: Some("en".to_string()),
                    enabled: true,
                },
                CustomWord::default(),
            ],
            snippets: vec![
                Snippet {
                    trigger: " insert note ".to_string(),
                    body: "Hello\n".to_string(),
                    language: Some("en".to_string()),
                    enabled: true,
                    preserve_literal: true,
                },
                Snippet::default(),
            ],
            ..WritingSettings::default()
        });

        assert_eq!(settings.replacements.len(), 1);
        assert_eq!(settings.replacements[0].from, "voice typer");
        assert_eq!(settings.replacements[0].to, "Voicetypr");
        assert_eq!(settings.custom_words.len(), 1);
        assert_eq!(settings.custom_words[0].phrase, "OpenAI");
        assert_eq!(
            settings.custom_words[0].spoken_form.as_deref(),
            Some("open ai")
        );
        assert_eq!(settings.snippets.len(), 1);
        assert_eq!(settings.snippets[0].trigger, "insert note");
        assert_eq!(settings.snippets[0].body, "Hello");
    }

    #[test]
    fn test_sanitize_writing_settings_trims_app_formatting_rules() {
        let settings = sanitize_writing_settings(WritingSettings {
            app_formatting_rules: vec![
                AppFormattingRule {
                    app_name: "  Slack  ".to_string(),
                    preset: EnhancementPreset::Message,
                    enabled: true,
                },
                AppFormattingRule {
                    app_name: "   ".to_string(),
                    preset: EnhancementPreset::Writing,
                    enabled: true,
                },
            ],
            ..WritingSettings::default()
        });

        assert_eq!(settings.app_formatting_rules.len(), 1);
        assert_eq!(settings.app_formatting_rules[0].app_name, "Slack");
        assert_eq!(
            settings.app_formatting_rules[0].preset,
            EnhancementPreset::Message
        );
    }
}
