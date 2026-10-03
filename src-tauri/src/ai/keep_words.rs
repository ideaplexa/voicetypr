//! Wording constraint shared by desktop and CLI, after per-app format selection.
use super::prompts::EnhancementPreset;
use tauri_plugin_store::StoreExt;

pub(crate) fn enabled(value: Option<&serde_json::Value>, cli_flag: bool) -> bool {
    cli_flag || value.and_then(serde_json::Value::as_bool).unwrap_or(false)
}

pub(crate) fn read(app: &tauri::AppHandle) -> bool {
    app.store("settings")
        .ok()
        .and_then(|store| store.get("polish_keep_words"))
        .is_some_and(|value| enabled(Some(&value), false))
}

pub(crate) const INSTRUCTION: &str =
    "\n\nKeep my words takes priority over all grammar, style and context instructions. \
Fix only punctuation, capitals, spacing, filler words, false starts and stutters. \
Never reword, reorder, summarise or restyle. Keep the speaker's wording, \
word order, scripts and digits. The selected style chooses only the format. Explicitly requested output-language translation still applies.";

pub(crate) fn format_instruction(preset: EnhancementPreset) -> &'static str {
    match preset {
        EnhancementPreset::Notes => {
            r#"Format as notes: plain "- " bullets only for items the speaker listed, in spoken order."#
        }
        EnhancementPreset::Message => {
            "Format as a message, keeping the speaker's wording and order."
        }
        EnhancementPreset::Code => "Format for code, keeping the speaker's wording and order.",
        EnhancementPreset::Writing => {
            "Format as paragraphs, keeping the speaker's wording and order."
        }
        _ => "Keep short dictation in one paragraph; add no headings or bullets.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_defaults_off_and_cli_enables_without_changing_saved_setting() {
        assert!(!enabled(None, false));
        assert!(enabled(None, true));
        assert!(enabled(Some(&serde_json::json!(true)), false));
        assert!(enabled(Some(&serde_json::json!(false)), true));
        assert!(!enabled(Some(&serde_json::json!("true")), false));
    }
    #[test]
    fn settings_default_and_serialization_keep_the_contract() {
        let settings = crate::commands::settings::Settings::default();
        assert!(!settings.polish_keep_words);
        let mut value = serde_json::to_value(settings).unwrap();
        value.as_object_mut().unwrap().remove("polish_keep_words");
        let old: crate::commands::settings::Settings =
            serde_json::from_value(value.clone()).unwrap();
        assert!(!old.polish_keep_words);
        value["polish_keep_words"] = serde_json::json!(true);
        let new: crate::commands::settings::Settings = serde_json::from_value(value).unwrap();
        assert!(new.polish_keep_words);
        assert_eq!(
            serde_json::to_value(new).unwrap()["polish_keep_words"],
            true
        );
    }
    #[test]
    fn explicit_translation_is_still_requested_separately() {
        let prompt = super::super::prompts::build_prompt_with_wording(
            None,
            &super::super::EnhancementOptions {
                preset: EnhancementPreset::CleanDictation,
            },
            Some("fr"),
            Some("en"),
            None,
            true,
        );
        assert!(prompt.contains("translate it into French"));
        assert!(prompt.contains(INSTRUCTION));
    }
    #[test]
    fn every_style_preserves_words_without_rewriting_transform() {
        for preset in [
            EnhancementPreset::CleanDictation,
            EnhancementPreset::Notes,
            EnhancementPreset::Message,
            EnhancementPreset::Writing,
            EnhancementPreset::Code,
        ] {
            let prompt = super::super::prompts::build_prompt_with_wording(
                Some("private term"),
                &super::super::EnhancementOptions { preset },
                Some("en"),
                Some("en"),
                Some("app context"),
                true,
            );
            assert!(prompt.contains(INSTRUCTION));
            assert!(prompt.contains(format_instruction(preset)));
            assert!(!prompt.contains("Stronger words"));
            assert!(!prompt.contains("Lead with the main point"));
            assert!(!prompt.contains("translate it into"));
            assert!(!prompt.contains("private term"));
        }
    }
}
