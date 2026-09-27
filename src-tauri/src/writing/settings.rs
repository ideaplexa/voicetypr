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

pub fn load_writing_settings(app: &AppHandle) -> Result<WritingSettings, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    if let Some(value) = store.get(WRITING_SETTINGS_KEY) {
        let settings: WritingSettings = serde_json::from_value(value.clone())
            .map_err(|e| format!("Failed to parse writing settings: {}", e))?;
        Ok(sanitize_writing_settings(settings))
    } else {
        Ok(WritingSettings::default())
    }
}

pub fn save_writing_settings(app: &AppHandle, settings: &WritingSettings) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
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
