//! The Polish style a new user picked in onboarding ("What will you mostly
//! dictate?"). It waits in settings until Polish first turns on, becomes the
//! Default style then, and is cleared so later changes follow Settings.
use crate::ai::prompts::{migrate_preset_str, EnhancementOptions, EnhancementPreset};
use tauri_plugin_store::Store;

pub(crate) const KEY: &str = "starting_polish_style";

/// A stored starting style, if it names a Polish style; anything else is dropped.
pub(crate) fn normalize(stored: Option<&str>) -> Option<String> {
    let preset = migrate_preset_str(stored?, true);
    preset
        .requires_ai_formatting()
        .then(|| {
            serde_json::to_value(preset)
                .ok()?
                .as_str()
                .map(str::to_owned)
        })
        .flatten()
}

fn enabled_preset(stored: Option<&str>) -> EnhancementPreset {
    normalize(stored)
        .map(|name| migrate_preset_str(&name, true))
        .unwrap_or(EnhancementPreset::CleanDictation)
}

/// Global Polish options for turning Polish on: the starting style once, else Clean.
pub(crate) fn take_enabled_options<R: tauri::Runtime>(store: &Store<R>) -> EnhancementOptions {
    let stored = store.get(KEY);
    store.delete(KEY);
    EnhancementOptions {
        preset: enabled_preset(stored.as_ref().and_then(|value| value.as_str())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polish_starts_in_the_style_picked_during_onboarding() {
        assert_eq!(enabled_preset(Some("Message")), EnhancementPreset::Message);
        assert_eq!(enabled_preset(Some("Code")), EnhancementPreset::Code);
        assert_eq!(enabled_preset(Some("Writing")), EnhancementPreset::Writing);
    }

    #[test]
    fn without_a_usable_pick_polish_starts_clean() {
        assert_eq!(enabled_preset(None), EnhancementPreset::CleanDictation);
        assert_eq!(
            enabled_preset(Some("PersonalDictation")),
            EnhancementPreset::CleanDictation
        );
        assert_eq!(
            enabled_preset(Some("nonsense")),
            EnhancementPreset::CleanDictation
        );
    }

    #[test]
    fn only_polish_style_names_are_saved() {
        assert_eq!(normalize(Some("Message")).as_deref(), Some("Message"));
        assert_eq!(normalize(Some("PersonalDictation")), None);
        assert_eq!(normalize(Some("")), None);
        assert_eq!(normalize(None), None);
    }
}
