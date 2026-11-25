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

/// Where Polish stands for the next dictation: the user's own provider, or
/// Voicetypr's built-in Polish, by the same settings dictation reads.
#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct PolishStatus {
    /// The next dictation will be polished.
    will_run: bool,
    own_provider_ready: bool,
    own_provider_enabled: bool,
    builtin_enabled: bool,
    builtin: crate::ai::builtin::Availability,
}

fn status(
    own_provider_ready: bool,
    own_provider_enabled: bool,
    builtin_enabled: bool,
    builtin: crate::ai::builtin::Availability,
) -> PolishStatus {
    let will_run = (own_provider_ready && own_provider_enabled)
        || (builtin_enabled && builtin == crate::ai::builtin::Availability::Ready);
    PolishStatus {
        will_run,
        own_provider_ready,
        own_provider_enabled,
        builtin_enabled,
        builtin,
    }
}

#[tauri::command]
pub async fn polish_status(app: tauri::AppHandle) -> Result<PolishStatus, String> {
    use tauri_plugin_store::StoreExt;
    let own_provider_ready = crate::commands::ai::has_ai_model_and_key(&app)?;
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let own_provider_enabled = store
        .get("ai_enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let builtin_enabled = crate::ai::builtin::setting_on(&store);
    let builtin = crate::ai::builtin::availability(&app, own_provider_ready).await;
    Ok(status(
        own_provider_ready,
        own_provider_enabled,
        builtin_enabled,
        builtin,
    ))
}

/// Whether the next dictation will be polished, by the user's own provider or
/// by built-in Polish once Voicetypr's server has confirmed it for this device.
#[tauri::command]
pub async fn polish_can_run(app: tauri::AppHandle) -> Result<bool, String> {
    Ok(polish_status(app).await?.will_run)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polish_runs_only_when_switched_on_and_confirmed() {
        use crate::ai::builtin::Availability::*;
        assert!(status(true, true, false, OwnProvider).will_run);
        assert!(!status(true, false, true, OwnProvider).will_run);
        assert!(status(false, false, true, Ready).will_run);
        assert!(!status(false, false, false, Ready).will_run);
        for refused in [NoAccess, NotEntitled, Unreachable] {
            assert!(!status(false, false, true, refused).will_run);
            assert!(!status(false, false, true, refused).will_run);
        }
    }

    #[test]
    fn dictation_is_never_polished_on_the_first_try_allowance() {
        use crate::ai::builtin::Availability::FirstTry;
        assert!(!status(false, false, true, FirstTry).will_run);
    }

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
