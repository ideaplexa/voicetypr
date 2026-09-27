use std::collections::HashSet;

use keytrigger::KeyPhase;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

pub use crate::commands::key_normalizer::is_single_key_shortcut;
use crate::commands::key_normalizer::{
    normalize_shortcut_keys, validate_key_combination,
    validate_key_combination_allowing_safe_single_key,
};
use crate::AppState;

const SHORTCUT_BINDINGS_KEY: &str = "shortcut_bindings";
const RETIRED_FORMATTING_SHORTCUTS_NOTICE_KEY: &str = "retired_formatting_shortcuts_notice_shown";

pub const MAX_SINGLE_KEY_BINDINGS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutAction {
    ToggleRecording,
    HoldToRecord,
    CancelRecording,
    CopyLastTranscription,
    PasteLastTranscription,
    ToggleAiFormatting,
    OpenDashboard,
    Unknown,
}

impl ShortcutAction {
    fn from_persisted_action(action: &str) -> Self {
        match action {
            "toggle_recording" => Self::ToggleRecording,
            "hold_to_record" => Self::HoldToRecord,
            "cancel_recording" => Self::CancelRecording,
            "copy_last_transcription" => Self::CopyLastTranscription,
            "paste_last_transcription" => Self::PasteLastTranscription,
            "toggle_ai_formatting" => Self::ToggleAiFormatting,
            "open_dashboard" => Self::OpenDashboard,
            _ => Self::Unknown,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::ToggleRecording => "toggle_recording",
            Self::HoldToRecord => "hold_to_record",
            Self::CancelRecording => "cancel_recording",
            Self::CopyLastTranscription => "copy_last_transcription",
            Self::PasteLastTranscription => "paste_last_transcription",
            Self::ToggleAiFormatting => "toggle_ai_formatting",
            Self::OpenDashboard => "open_dashboard",
            Self::Unknown => "unknown",
        }
    }
}

impl Serialize for ShortcutAction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ShortcutAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let action = String::deserialize(deserializer)?;
        Ok(Self::from_persisted_action(&action))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutTrigger {
    Pressed,
    Hold,
}

/// How a binding is triggered. All shortcut kinds are routed through the native engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    #[default]
    Combo,
    ModifierHold,
    IsolatedTap,
}

/// A side-specific modifier for native modifier bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierSpec {
    pub modifier: ModifierKind,
    pub side: SideKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierKind {
    Alt,
    Control,
    Meta,
    Shift,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideKind {
    Left,
    Right,
    Either,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutBinding {
    pub id: String,
    pub action: ShortcutAction,
    pub shortcut: String,
    pub trigger: ShortcutTrigger,
    pub enabled: bool,
    pub allow_risky_combo: bool,
    #[serde(default)]
    pub trigger_kind: TriggerKind,
    #[serde(default)]
    pub modifier: Option<ModifierSpec>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutSettings {
    pub bindings: Vec<ShortcutBinding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShortcutActionDefinition {
    pub action: ShortcutAction,
    pub label: &'static str,
    pub description: &'static str,
    pub section: &'static str,
    pub recommended_trigger: ShortcutTrigger,
    pub allows_single_key: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CustomHoldTransition {
    Start,
    Stop,
    Noop,
}

pub(crate) fn pressed_shortcut_should_run(
    active_bindings: &mut HashSet<String>,
    binding_id: &str,
    event_state: KeyPhase,
) -> bool {
    match event_state {
        KeyPhase::Pressed => active_bindings.insert(binding_id.to_string()),
        KeyPhase::Released => {
            active_bindings.remove(binding_id);
            false
        }
    }
}

pub(crate) fn hold_shortcut_transition(
    active_bindings: &mut HashSet<String>,
    binding_id: &str,
    event_state: KeyPhase,
) -> CustomHoldTransition {
    match event_state {
        KeyPhase::Pressed => {
            if active_bindings.insert(binding_id.to_string()) && active_bindings.len() == 1 {
                CustomHoldTransition::Start
            } else {
                CustomHoldTransition::Noop
            }
        }
        KeyPhase::Released => {
            if active_bindings.remove(binding_id) && active_bindings.is_empty() {
                CustomHoldTransition::Stop
            } else {
                CustomHoldTransition::Noop
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExistingShortcutStrings {
    pub primary_hotkey: Option<String>,
    pub ptt_hotkey: Option<String>,
}

#[tauri::command]
pub fn get_shortcut_settings(app: AppHandle) -> Result<ShortcutSettings, String> {
    load_shortcut_settings(&app)
}

#[tauri::command]
pub fn list_shortcut_actions() -> Vec<ShortcutActionDefinition> {
    shortcut_action_definitions()
}

#[tauri::command]
pub fn update_shortcut_settings(
    app: AppHandle,
    settings: ShortcutSettings,
) -> Result<ShortcutSettings, String> {
    let existing = current_existing_shortcuts(&app);
    let prepared = prepare_shortcut_settings(settings, &existing)?;
    let sanitized = ShortcutSettings {
        bindings: prepared.clone(),
    };

    save_shortcut_settings(&app, &sanitized)?;

    // Apply after the settings commit so runtime state reflects the durable source of truth.
    crate::trigger::engine_host::rebuild_engine_bindings(&app);

    let app_state = app.state::<AppState>();
    clear_active_custom_shortcut_state(&app_state);

    // Notify the frontend so an in-session binding change refreshes the in-app
    // bare-modifier fallback (and any other shortcut-derived state) without an
    // app restart.
    let _ = app.emit("shortcut-settings-changed", ());
    Ok(sanitized)
}

#[cfg(test)]
pub fn normalized_custom_shortcut_conflict(
    normalized_shortcut: &str,
    settings: &ShortcutSettings,
) -> Option<String> {
    settings
        .bindings
        .iter()
        .filter(|binding| binding.enabled)
        .find(|binding| normalize_shortcut_keys(&binding.shortcut) == normalized_shortcut)
        .map(|binding| binding.id.clone())
}

pub async fn latest_copyable_transcription_text(app: &AppHandle) -> Result<Option<String>, String> {
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to open transcriptions store: {}", e))?;
    let mut entries: Vec<(String, serde_json::Value)> = Vec::new();
    for key in store.keys() {
        if let Some(value) = store.get(&key) {
            entries.push((key.to_string(), value));
        }
    }

    Ok(
        crate::menu::latest_copyable_transcription_id(&entries).and_then(|timestamp| {
            store.get(&timestamp).and_then(|value| {
                value
                    .get("text")
                    .and_then(|text| text.as_str().map(str::to_string))
            })
        }),
    )
}

/// Pure decision function: what should ai_enabled become?
/// Returns None if enabling is refused (no usable AI setup).
pub fn next_ai_enabled(current: bool, can_enable: bool) -> Option<bool> {
    if current {
        Some(false) // always allow disabling
    } else if can_enable {
        Some(true)
    } else {
        None // refuse to enable without a usable AI setup
    }
}

fn clean_dictation_options() -> serde_json::Value {
    serde_json::json!({ "preset": "CleanDictation" })
}

fn personal_dictation_options() -> serde_json::Value {
    serde_json::json!({ "preset": "PersonalDictation" })
}

/// Pure decision: when enabling Polish, should the stored global preset be
/// normalized to Clean? Clean is the only enabled global preset; reshaping is
/// selected through per-app rules. Any stored preset that does not already
/// resolve to Clean is normalized so a tray or shortcut enable cannot resurrect
/// PersonalDictation or a legacy global reshaping preset.
pub fn should_normalize_preset_to_clean_on_enable(stored_preset: Option<&str>) -> bool {
    match stored_preset {
        Some(preset) => {
            crate::ai::prompts::migrate_preset_str(preset, true)
                != crate::ai::prompts::EnhancementPreset::CleanDictation
        }
        None => false,
    }
}

fn stored_global_preset_needs_clean_normalization(app: &AppHandle) -> Result<bool, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let raw = store.get("enhancement_options").and_then(|value| {
        value
            .get("preset")
            .and_then(|p| p.as_str().map(String::from))
    });
    Ok(should_normalize_preset_to_clean_on_enable(raw.as_deref()))
}

pub async fn toggle_ai_formatting(app: AppHandle) -> Result<(), String> {
    // Shared CLI-aware readiness — the same gate the Polish screen uses.
    // Agent-CLI providers (Claude Code) are subscription-authenticated local
    // CLIs with no API key and no catalog model, so the previous
    // `has_api_key && !model.is_empty()` check permanently refused to enable
    // Polish for them from the tray or the shortcut. Cloud providers keep
    // their model + credential requirements.
    let can_enable = crate::commands::ai::has_ai_model_and_key(&app)?;
    let current = {
        let store = app.store("settings").map_err(|e| e.to_string())?;
        store
            .get("ai_enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    };

    match next_ai_enabled(current, can_enable) {
        Some(true) => {
            // Enabling ai_enabled alone is not enough to polish: the resolver
            // only runs AI when the effective preset requires it. A preset left
            // at PersonalDictation means "on but never polishing," and a stale
            // global reshaping preset (Notes/Message/…) would silently reshape.
            // Move the global preset in lockstep with the switch (mirrors the
            // Polish screen toggle) — Clean is the only "on" global preset now,
            // reshaping is per-app — so the tray/shortcut can't persist an
            // inert or legacy-reshaping state without the screen's migration.
            let normalize_to_clean = stored_global_preset_needs_clean_normalization(&app)?;
            crate::commands::settings::persist_settings_and_invalidate(
                &app,
                move |store| {
                    store.set("ai_enabled", serde_json::Value::Bool(true));
                    if normalize_to_clean {
                        store.set("enhancement_options", clean_dictation_options());
                    }
                    Ok(())
                },
                std::convert::identity,
            )
            .await?;
            crate::commands::audio::pill_toast(&app, "Polish on", 2500);
            let _ = crate::emit_to_window(&app, "main", "ai-enabled-changed", true);
        }
        Some(false) => {
            crate::commands::settings::persist_settings_and_invalidate(
                &app,
                |store| {
                    store.set("ai_enabled", serde_json::Value::Bool(false));
                    store.set("enhancement_options", personal_dictation_options());
                    Ok(())
                },
                std::convert::identity,
            )
            .await?;
            crate::commands::audio::pill_toast(&app, "Polish off", 2500);
            let _ = crate::emit_to_window(&app, "main", "ai-enabled-changed", false);
        }
        None => {
            crate::commands::audio::pill_toast(
                &app,
                "Set up an AI model in Settings to use Polish",
                3500,
            );
        }
    }
    Ok(())
}

pub fn validate_shortcut_settings(
    settings: ShortcutSettings,
    existing: &ExistingShortcutStrings,
) -> Result<ShortcutSettings, String> {
    prepare_shortcut_settings(settings, existing)
        .map(|prepared| ShortcutSettings { bindings: prepared })
}

fn prepare_shortcut_settings(
    settings: ShortcutSettings,
    existing: &ExistingShortcutStrings,
) -> Result<Vec<ShortcutBinding>, String> {
    let mut seen_enabled = HashSet::new();
    let primary = existing
        .primary_hotkey
        .as_deref()
        .map(normalize_shortcut_keys)
        .filter(|value| !value.is_empty())
        .map(|value| trigger_dedup_key(&value));
    let ptt = existing
        .ptt_hotkey
        .as_deref()
        .map(normalize_shortcut_keys)
        .filter(|value| !value.is_empty())
        .map(|value| trigger_dedup_key(&value));

    // The primary recording hotkey and the push-to-talk hotkey must not resolve
    // to the SAME physical trigger. The retired OS shortcut registrar rejected
    // this; without the check engine_host would synthesize both a `primary` and a
    // `ptt` binding for one keypress. Compared on the resolved trigger, not the
    // raw string.
    if let (Some(primary_key), Some(ptt_key)) = (&primary, &ptt) {
        if primary_key == ptt_key {
            return Err("The recording hotkey and the push-to-talk hotkey are the \
                        same. Choose a different push-to-talk key."
                .to_string());
        }
    }

    let mut prepared = Vec::with_capacity(settings.bindings.len());
    for binding in settings.bindings {
        if binding.action == ShortcutAction::Unknown {
            continue;
        }

        let sanitized = ShortcutBinding {
            id: binding.id.trim().to_string(),
            shortcut: binding.shortcut.trim().to_string(),
            ..binding
        };

        if sanitized.enabled {
            prepare_enabled_shortcut(
                &sanitized,
                primary.as_deref(),
                ptt.as_deref(),
                &mut seen_enabled,
            )?;
        }
        prepared.push(sanitized);
    }

    let single_key_count = prepared
        .iter()
        .filter(|pb| {
            pb.enabled
                && !pb.shortcut.is_empty()
                && is_single_key_shortcut(&normalize_shortcut_keys(&pb.shortcut))
        })
        .count();
    if single_key_count > MAX_SINGLE_KEY_BINDINGS {
        return Err(format!(
            "You can set at most {} single-key shortcuts -- you have {}. \
             Disable or remove some single-key bindings.",
            MAX_SINGLE_KEY_BINDINGS, single_key_count
        ));
    }

    Ok(prepared)
}

/// Canonical dedup/collision key for a NORMALIZED shortcut string, resolved
/// through the engine's `parse_combo` so platform-equivalent combos compare
/// equal (e.g. on Windows `CommandOrControl+A` and `Control+A` both resolve to
/// the same Control+A trigger). Falls back to the normalized string for inputs
/// the engine cannot parse. `ModSet` is a `u8` bitset so the trigger's `Debug`
/// is a stable canonical key.
fn trigger_dedup_key(normalized: &str) -> String {
    match crate::trigger::mapping::parse_combo(normalized) {
        Ok(trigger) => format!("{:?}", trigger),
        Err(_) => normalized.to_string(),
    }
}

fn prepare_enabled_shortcut(
    binding: &ShortcutBinding,
    primary: Option<&str>,
    ptt: Option<&str>,
    seen_enabled: &mut HashSet<String>,
) -> Result<(), String> {
    if binding.trigger_kind != TriggerKind::Combo {
        crate::trigger::mapping::validate(binding)?;
        return Ok(());
    }

    validate_enabled_binding(binding)?;
    crate::trigger::mapping::validate(binding)?;

    let normalized = normalize_shortcut_keys(&binding.shortcut);
    // Dedup/collision on the RESOLVED trigger, not the raw string: on Windows
    // `CommandOrControl+A` and `Control+A` are distinct strings that parse_combo
    // collapses to one trigger, so a string compare would let both fire at once.
    let dedup_key = trigger_dedup_key(&normalized);

    if primary == Some(dedup_key.as_str()) {
        return Err(format!(
            "Shortcut '{}' duplicates the primary recording hotkey",
            binding.shortcut
        ));
    }
    if ptt == Some(dedup_key.as_str()) {
        return Err(format!(
            "Shortcut '{}' duplicates the push-to-talk hotkey",
            binding.shortcut
        ));
    }
    if !seen_enabled.insert(dedup_key) {
        return Err(format!(
            "Duplicate enabled shortcut binding: {}",
            binding.shortcut
        ));
    }

    if normalized == "Escape" {
        return Err("Escape is reserved for recording cancellation".to_string());
    }

    Ok(())
}

pub(crate) fn load_shortcut_settings(app: &AppHandle) -> Result<ShortcutSettings, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    match store.get(SHORTCUT_BINDINGS_KEY) {
        Some(value) => {
            let loaded = decode_shortcut_settings_value(value.clone())?;
            let should_notice = loaded.dropped_unknown_actions
                && !store
                    .get(RETIRED_FORMATTING_SHORTCUTS_NOTICE_KEY)
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);

            if loaded.migrated_tap_bindings || loaded.dropped_unknown_actions {
                store.set(
                    SHORTCUT_BINDINGS_KEY,
                    serde_json::to_value(&loaded.settings)
                        .map_err(|e| format!("Failed to serialize shortcut settings: {}", e))?,
                );
            }
            if should_notice {
                store.set(
                    RETIRED_FORMATTING_SHORTCUTS_NOTICE_KEY,
                    serde_json::Value::Bool(true),
                );
            }
            if loaded.migrated_tap_bindings || loaded.dropped_unknown_actions || should_notice {
                store.save().map_err(|e| e.to_string())?;
            }
            drop(store);

            if should_notice {
                let message = "Formatting-mode shortcuts were retired.";
                crate::commands::audio::pill_toast(app, message, 3500);
                let _ = app.emit("shortcut-bindings-retired", message);
            }

            Ok(loaded.settings)
        }
        None => Ok(ShortcutSettings::default()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedShortcutSettings {
    pub settings: ShortcutSettings,
    pub migrated_tap_bindings: bool,
    pub dropped_unknown_actions: bool,
}

pub(crate) fn decode_shortcut_settings_value(
    mut raw: serde_json::Value,
) -> Result<LoadedShortcutSettings, String> {
    let migrated_tap_bindings = migrate_legacy_tap_bindings(&mut raw);
    let settings_value = if raw.is_array() {
        serde_json::json!({ "bindings": raw })
    } else {
        raw
    };
    let mut settings: ShortcutSettings = serde_json::from_value(settings_value)
        .map_err(|e| format!("Failed to parse shortcut settings: {}", e))?;
    let original_len = settings.bindings.len();
    settings
        .bindings
        .retain(|binding| binding.action != ShortcutAction::Unknown);
    let dropped_unknown_actions = settings.bindings.len() != original_len;

    Ok(LoadedShortcutSettings {
        settings,
        migrated_tap_bindings,
        dropped_unknown_actions,
    })
}

fn migrate_legacy_tap_bindings(value: &mut serde_json::Value) -> bool {
    let Some(bindings) = (if value.is_array() {
        value.as_array_mut()
    } else {
        value
            .get_mut("bindings")
            .and_then(serde_json::Value::as_array_mut)
    }) else {
        return false;
    };

    let mut changed = false;
    for binding in bindings {
        let Some(object) = binding.as_object_mut() else {
            continue;
        };

        if object
            .get("trigger_kind")
            .and_then(serde_json::Value::as_str)
            == Some(concat!("double", "_tap"))
        {
            object.insert(
                "trigger_kind".to_string(),
                serde_json::Value::String("isolated_tap".to_string()),
            );
            changed = true;
        }
        if object.remove(concat!("double", "_tap_ms")).is_some() {
            changed = true;
        }
    }

    changed
}

pub(crate) fn save_shortcut_settings(
    app: &AppHandle,
    settings: &ShortcutSettings,
) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    store.set(
        SHORTCUT_BINDINGS_KEY,
        serde_json::to_value(settings)
            .map_err(|e| format!("Failed to serialize shortcut settings: {}", e))?,
    );
    store.save().map_err(|e| e.to_string())
}

fn current_existing_shortcuts(app: &AppHandle) -> ExistingShortcutStrings {
    let store = match app.store("settings") {
        Ok(store) => store,
        Err(_) => {
            return ExistingShortcutStrings {
                primary_hotkey: Some("CommandOrControl+Shift+Space".to_string()),
                ptt_hotkey: None,
            }
        }
    };

    let primary_hotkey = store
        .get("hotkey")
        .and_then(|value| value.as_str().map(str::to_string))
        .or_else(|| Some("CommandOrControl+Shift+Space".to_string()));

    let recording_mode = store
        .get("recording_mode")
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "toggle".to_string());
    let use_different_ptt_key = store
        .get("use_different_ptt_key")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let ptt_hotkey = if recording_mode == "push_to_talk" && use_different_ptt_key {
        store
            .get("ptt_hotkey")
            .and_then(|value| value.as_str().map(str::to_string))
    } else {
        None
    };

    ExistingShortcutStrings {
        primary_hotkey,
        ptt_hotkey,
    }
}

fn validate_enabled_binding(binding: &ShortcutBinding) -> Result<(), String> {
    if binding.id.is_empty() {
        return Err("Shortcut binding id is required".to_string());
    }
    if binding.shortcut.is_empty() || binding.shortcut.len() > 100 {
        return Err(format!("Invalid shortcut for binding '{}'", binding.id));
    }

    validate_trigger_matches_action(binding)?;

    let normalized = normalize_shortcut_keys(&binding.shortcut);
    if allows_single_key(binding) {
        validate_key_combination_allowing_safe_single_key(&normalized)?;
    } else {
        validate_key_combination(&normalized)?;
    }

    Ok(())
}

fn validate_trigger_matches_action(binding: &ShortcutBinding) -> Result<(), String> {
    match (binding.action, binding.trigger) {
        (ShortcutAction::HoldToRecord, ShortcutTrigger::Hold) => Ok(()),
        (ShortcutAction::HoldToRecord, ShortcutTrigger::Pressed) => {
            Err("Hold-to-record shortcuts must use the hold trigger".to_string())
        }
        (_, ShortcutTrigger::Pressed) => Ok(()),
        (_, ShortcutTrigger::Hold) => {
            Err("Only hold-to-record shortcuts can use the hold trigger".to_string())
        }
    }
}

fn allows_single_key(binding: &ShortcutBinding) -> bool {
    binding.allow_risky_combo
}

fn clear_active_custom_shortcut_state(app_state: &AppState) {
    if let Ok(mut active_holds) = app_state.active_custom_hold_bindings.lock() {
        active_holds.clear();
    }
    if let Ok(mut active_pressed) = app_state.active_custom_pressed_bindings.lock() {
        active_pressed.clear();
    }
}

fn shortcut_action_definitions() -> Vec<ShortcutActionDefinition> {
    vec![
        ShortcutActionDefinition {
            action: ShortcutAction::ToggleRecording,
            label: "Toggle recording",
            description: "Start or stop recording with one press.",
            section: "Recording",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::HoldToRecord,
            label: "Hold to record",
            description: "Record while the shortcut is held.",
            section: "Recording",
            recommended_trigger: ShortcutTrigger::Hold,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::CancelRecording,
            label: "Cancel recording",
            description: "Cancel the current recording.",
            section: "Recording",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::CopyLastTranscription,
            label: "Copy last transcription",
            description: "Copy the latest finished transcription to the clipboard.",
            section: "History",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::PasteLastTranscription,
            label: "Paste last transcription",
            description: "Paste the latest finished transcription into the active app.",
            section: "History",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::ToggleAiFormatting,
            label: "Toggle Polish",
            description: "Turn Polish on or off.",
            section: "Polish",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::OpenDashboard,
            label: "Open dashboard",
            description: "Focus the Voicetypr dashboard.",
            section: "Dashboard",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
    ]
}
