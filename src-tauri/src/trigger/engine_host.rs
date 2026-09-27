//! Engine lifecycle + binding application for the host app.

use keytrigger::KeyPhase;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

use crate::commands::shortcuts::{ShortcutAction, ShortcutBinding, ShortcutTrigger, TriggerKind};
use crate::state::app_state::AppState;
use crate::{RecordingMode, RecordingState};

use super::{mapping, EngineBinding};

/// Recompute the engine's binding set from the full binding list and apply it.
///
/// Ordering matters for the core recording path. Before swapping the lookup
/// map we synthesize a `Released` for every OLD binding that is gone, OR that
/// persists with a changed action/trigger (e.g. `primary` flipping between
/// HoldToRecord/PushToTalk and ToggleRecording when recording mode changes) —
/// otherwise a mid-hold binding is never released and recording stays stuck.
/// `dispatch_action(Released)` is idempotent, so releasing unconditionally is safe.
pub fn apply_engine_bindings(app: &AppHandle, bindings: &[ShortcutBinding]) {
    let app_state = app.state::<AppState>();

    let mut new_bindings: Vec<EngineBinding> = Vec::new();
    let mut triggers = Vec::new();
    for binding in bindings
        .iter()
        .filter(|b| b.enabled && mapping::is_engine_kind(b))
    {
        match mapping::to_trigger(binding) {
            Some(trigger) => {
                new_bindings.push(EngineBinding {
                    id: binding.id.clone(),
                    action: binding.action,
                    trigger: binding.trigger,
                });
                triggers.push((binding.id.clone(), trigger));
            }
            None => {
                // A binding that survived validation/rebuild but still resolves
                // to no trigger is a silent failure waiting to happen — its id
                // would vanish from the engine with no signal. Surface it.
                log::warn!(
                    "keytrigger: dropping binding '{}' ({:?}, shortcut={:?}) — no resolvable trigger; not installed",
                    binding.id,
                    binding.trigger_kind,
                    binding.shortcut,
                );
            }
        }
    }

    // Old bindings needing a synthetic Released BEFORE the swap: ids that are
    // gone, OR ids that persist with a changed action/trigger (e.g. `primary`
    // flipping recording mode). Released with the OLD binding so it routes
    // through the correct old hold/PTT handler; `dispatch_action(Released)`
    // is idempotent.
    let needs_release: Vec<EngineBinding> = match app_state.engine_bindings.lock() {
        Ok(guard) => bindings_needing_release(&guard, &new_bindings),
        Err(error) => {
            log::error!("keytrigger: engine_bindings lock poisoned: {}", error);
            Vec::new()
        }
    };
    for binding in &needs_release {
        crate::recording::hotkeys::dispatch_action(
            app,
            app_state.inner(),
            &binding.id,
            binding.action,
            binding.trigger,
            KeyPhase::Released,
        );
    }

    match app_state.engine_bindings.lock() {
        Ok(mut guard) => *guard = new_bindings,
        Err(error) => log::error!("keytrigger: engine_bindings lock poisoned: {}", error),
    }

    app_state.trigger_engine.set_bindings(triggers);
}

/// Old bindings that need a synthetic `Released` dispatched before the app-map
/// swap: ids that disappear entirely, OR ids that persist with a changed
/// action/trigger (e.g. `primary` flipping between HoldToRecord/Hold and
/// ToggleRecording/Pressed when recording mode changes). Returns the OLD
/// `EngineBinding`s so the release routes through the correct old hold/PTT
/// handler. Pure (no app state) so it can be unit-tested directly.
fn bindings_needing_release(old: &[EngineBinding], new: &[EngineBinding]) -> Vec<EngineBinding> {
    old.iter()
        .filter(|old_b| match new.iter().find(|n| n.id == old_b.id) {
            None => true,
            Some(new_b) => new_b.action != old_b.action || new_b.trigger != old_b.trigger,
        })
        .cloned()
        .collect()
}

/// Rebuild the complete native-engine binding list from persisted settings and runtime state.
///
/// Decision logic lives in [`plan_engine_bindings`] (pure, unit-tested). This
/// wrapper loads persisted state, runs the plan, performs the one-time durable
/// bare-modifier-primary migration it requests, and applies the result.
pub fn rebuild_engine_bindings(app: &AppHandle) {
    let settings = match crate::commands::shortcuts::load_shortcut_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            log::error!("keytrigger: failed to load shortcut settings: {}", error);
            return;
        }
    };

    let store = app.store("settings").ok();
    let hotkey = store
        .as_ref()
        .and_then(|store| store.get("hotkey"))
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "CommandOrControl+Shift+Space".to_string());
    let recording_mode_str = store
        .as_ref()
        .and_then(|store| store.get("recording_mode"))
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "toggle".to_string());
    let recording_mode = if recording_mode_str == "push_to_talk" {
        RecordingMode::PushToTalk
    } else {
        RecordingMode::Toggle
    };
    let use_different_ptt_key = store
        .as_ref()
        .and_then(|store| store.get("use_different_ptt_key"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let ptt_hotkey = store
        .as_ref()
        .and_then(|store| store.get("ptt_hotkey"))
        .and_then(|value| value.as_str().map(str::to_string));
    let is_recording = matches!(crate::get_recording_state(app), RecordingState::Recording);

    let (bindings, stale_id) = plan_engine_bindings(
        &settings.bindings,
        &hotkey,
        recording_mode,
        use_different_ptt_key,
        ptt_hotkey.as_deref(),
        is_recording,
    );

    // One-time durable migration (Issue A): when the combo hotkey is
    // authoritative, persist-disable the single stale bare-modifier recording
    // primary it superseded, so the next rebuild (and the Settings UI) no
    // longer see an enabled binding that would suppress combo synthesis.
    if let Some(id) = stale_id {
        let mut repaired = settings;
        if let Some(binding) = repaired.bindings.iter_mut().find(|b| b.id == id) {
            binding.enabled = false;
            match crate::commands::shortcuts::save_shortcut_settings(app, &repaired) {
                Ok(()) => log::info!(
                    "keytrigger: disabled stale bare-modifier primary '{}' — combo hotkey is authoritative",
                    id
                ),
                Err(error) => log::warn!(
                    "keytrigger: failed to persist bare-modifier primary migration for '{}': {}",
                    id,
                    error
                ),
            }
        }
    }

    apply_engine_bindings(app, &bindings);
}

/// Pure decision core for [`rebuild_engine_bindings`]. Given the persisted
/// bindings and runtime state, returns:
///   * the final engine binding list to install, and
///   * the id of a SINGLE stale bare-modifier recording primary to
///     persist-disable as a one-time migration, or `None`.
///
/// When the combo `hotkey` is non-empty it is authoritative: the combo
/// `primary` always wins, and a stale enabled bare-modifier recording binding
/// (left by an older migration) is repaired by removing that single primary
/// candidate from this rebuild and flagging it for persistence. When `hotkey`
/// is empty, a remaining bare-modifier primary owns recording and no combo is
/// synthesized (default-combo fallback otherwise). Other bare-modifier bindings
/// the user added as additional shortcuts are never touched.
///
/// Pure (no app handle) so the migration decision can be unit-tested directly.
fn plan_engine_bindings(
    persisted: &[ShortcutBinding],
    hotkey: &str,
    recording_mode: RecordingMode,
    use_different_ptt_key: bool,
    ptt_hotkey: Option<&str>,
    is_recording: bool,
) -> (Vec<ShortcutBinding>, Option<String>) {
    // Persisted bindings may predate current validation rules; the update/save
    // path rejects them, but at startup we read straight from the store. Skip
    // (and warn) any enabled binding that fails per-binding validation so it is
    // not installed into the engine. At minimum this excludes a custom combo
    // that resolves to bare `SingleKey(Escape)` — Escape is reserved for the
    // synthesized `escape-cancel`. The synthesized escape-cancel/primary/ptt
    // ids are appended below and bypass this gate.
    let mut bindings: Vec<ShortcutBinding> = persisted
        .iter()
        .filter(|binding| {
            if !binding.enabled {
                return false;
            }
            if let Err(error) = mapping::validate(binding) {
                log::warn!(
                    "keytrigger: skipping invalid persisted binding '{}' at rebuild: {}",
                    binding.id,
                    error
                );
                return false;
            }
            if binding.trigger_kind == TriggerKind::Combo
                && crate::commands::key_normalizer::normalize_shortcut_keys(&binding.shortcut)
                    == "Escape"
            {
                log::warn!(
                    "keytrigger: skipping persisted binding '{}' at rebuild: Escape is reserved",
                    binding.id
                );
                return false;
            }
            true
        })
        .cloned()
        .collect();

    // Issue A: a non-empty combo hotkey makes the combo `primary` authoritative.
    // A stale enabled bare-modifier recording binding left by an older migration
    // would otherwise suppress combo synthesis below; repair it by removing the
    // SINGLE active-primary candidate from this rebuild and flagging it for a
    // durable disable. Additional bare-modifier shortcuts are preserved.
    let stale_id = stale_primary_candidate(&bindings, hotkey);
    if let Some(id) = &stale_id {
        bindings.retain(|b| b.id != *id);
    }

    // Synthesize the combo `primary` when the combo owns recording. With a
    // non-empty hotkey the combo always wins (the repair above removed any
    // stale bare-modifier primary). With an empty hotkey, a remaining
    // bare-modifier primary owns recording and we synthesize no combo; otherwise
    // fall back to the default combo so the user always has a way to start
    // recording.
    let combo_owns_primary = !hotkey.trim().is_empty();
    let has_bare_modifier_primary = bindings.iter().any(|binding| {
        binding.enabled
            && matches!(
                binding.action,
                ShortcutAction::HoldToRecord | ShortcutAction::ToggleRecording
            )
            && matches!(
                binding.trigger_kind,
                TriggerKind::ModifierHold | TriggerKind::IsolatedTap
            )
    });
    if combo_owns_primary || !has_bare_modifier_primary {
        // Default-combo fallback: if the stored hotkey is empty/blank and there
        // is no bare-modifier primary either, the user would otherwise be left
        // with NO way to start recording by hotkey. The retired
        // global-shortcut startup fell back to this same default for that
        // inconsistent state.
        let combo_hotkey = if hotkey.trim().is_empty() {
            "CommandOrControl+Shift+Space".to_string()
        } else {
            hotkey.to_string()
        };
        let hold_to_record = recording_mode == RecordingMode::PushToTalk;
        bindings.push(ShortcutBinding {
            id: "primary".to_string(),
            action: if hold_to_record {
                ShortcutAction::HoldToRecord
            } else {
                ShortcutAction::ToggleRecording
            },
            shortcut: combo_hotkey,
            trigger: if hold_to_record {
                ShortcutTrigger::Hold
            } else {
                ShortcutTrigger::Pressed
            },
            enabled: true,
            allow_risky_combo: false,
            trigger_kind: TriggerKind::Combo,
            modifier: None,
        });
    }

    if recording_mode == RecordingMode::PushToTalk && use_different_ptt_key {
        if let Some(ptt_hotkey) = ptt_hotkey.filter(|value| !value.trim().is_empty()) {
            bindings.push(ShortcutBinding {
                id: "ptt".to_string(),
                action: ShortcutAction::HoldToRecord,
                shortcut: ptt_hotkey.to_string(),
                trigger: ShortcutTrigger::Hold,
                enabled: true,
                allow_risky_combo: false,
                trigger_kind: TriggerKind::Combo,
                modifier: None,
            });
        } else {
            log::warn!(
                "keytrigger: push-to-talk is set to use a different PTT key, but ptt_hotkey is empty — PTT will not arm"
            );
        }
    }

    if is_recording {
        bindings.push(ShortcutBinding {
            id: "escape-cancel".to_string(),
            action: ShortcutAction::CancelRecording,
            shortcut: "Escape".to_string(),
            trigger: ShortcutTrigger::Pressed,
            enabled: true,
            allow_risky_combo: true,
            trigger_kind: TriggerKind::Combo,
            modifier: None,
        });
    }

    (bindings, stale_id)
}

/// Identify the single stale bare-modifier recording primary to disable so a
/// non-empty combo `hotkey` can own recording. Returns its id, or `None` when
/// the combo is not authoritative (empty hotkey) or no enabled recording
/// bare-modifier binding exists.
///
/// Precedence mirrors the frontend's primary selection (GeneralSettings.tsx /
/// shortcut-display.ts): prefer the binding whose id is `onboarding-primary-hold`,
/// else the FIRST enabled recording bare-modifier binding. Only this single
/// candidate is ever returned; any additional bare-modifier recording bindings
/// the user added as separate shortcuts are preserved. Pure (no app state).
fn stale_primary_candidate(bindings: &[ShortcutBinding], hotkey: &str) -> Option<String> {
    if hotkey.trim().is_empty() {
        return None;
    }
    let is_primary_candidate = |b: &ShortcutBinding| {
        b.enabled
            && matches!(
                b.action,
                ShortcutAction::HoldToRecord | ShortcutAction::ToggleRecording
            )
            && matches!(
                b.trigger_kind,
                TriggerKind::ModifierHold | TriggerKind::IsolatedTap
            )
    };
    bindings
        .iter()
        .find(|b| b.id == "onboarding-primary-hold" && is_primary_candidate(b))
        .or_else(|| bindings.iter().find(|b| is_primary_candidate(b)))
        .map(|b| b.id.clone())
}

/// Attempt to start the native trigger engine. On macOS without Accessibility
/// the tap fails to create and `start` returns an error; we log and rely on a
/// retry when `accessibility-granted` fires. Idempotent.
pub fn start_engine(app: &AppHandle) {
    let app_state = app.state::<AppState>();
    if app_state.trigger_engine.is_running() {
        return;
    }
    let handle = app.clone();
    match app_state
        .trigger_engine
        .start(move |ev| super::dispatch::on_engine_event(&handle, ev))
    {
        Ok(()) => log::info!("keytrigger: native trigger engine started"),
        // Two authorized checks can race before either observes `is_running`.
        // TriggerEngine serializes startup internally, so this is the expected
        // idempotent outcome rather than a failed permission retry.
        Err(keytrigger::EngineError::AlreadyRunning) => {}
        Err(error) => log::warn!(
            "keytrigger: trigger engine not started ({}); will retry on permission grant",
            error
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{bindings_needing_release, plan_engine_bindings, stale_primary_candidate};
    use crate::commands::shortcuts::{
        ModifierKind, ModifierSpec, ShortcutAction, ShortcutBinding, ShortcutTrigger, SideKind,
        TriggerKind,
    };
    use crate::trigger::EngineBinding;
    use crate::RecordingMode;

    fn binding(id: &str, action: ShortcutAction, trigger: ShortcutTrigger) -> EngineBinding {
        EngineBinding {
            id: id.to_string(),
            action,
            trigger,
        }
    }

    /// Fix E: a stable id (`primary`) that persists but flips its action/trigger
    /// when recording mode changes must be treated as needing a synthetic
    /// `Released`, carried out with the OLD binding so it routes through the old
    /// hold/PTT handler and recording is not left stuck.
    #[test]
    fn fix_e_changed_action_or_trigger_marks_for_release() {
        let old = vec![binding(
            "primary",
            ShortcutAction::HoldToRecord,
            ShortcutTrigger::Hold,
        )];
        let new = vec![binding(
            "primary",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Pressed,
        )];
        let needs = bindings_needing_release(&old, &new);
        assert_eq!(needs.len(), 1, "a changed persistent id must be released");
        assert_eq!(needs[0].id, "primary");
        // The release must carry the OLD action/trigger to hit the old handler.
        assert_eq!(needs[0].action, ShortcutAction::HoldToRecord);
        assert_eq!(needs[0].trigger, ShortcutTrigger::Hold);
    }

    #[test]
    fn fix_e_unchanged_persistent_id_is_not_released() {
        let old = vec![binding(
            "primary",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Pressed,
        )];
        let new = vec![binding(
            "primary",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Pressed,
        )];
        assert!(bindings_needing_release(&old, &new).is_empty());
    }

    /// A genuinely removed id (e.g. escape-cancel when recording stops) must
    /// still be released — Fix E broadens the release path, it does not replace it.
    #[test]
    fn fix_e_removed_id_is_still_released() {
        let old = vec![binding(
            "escape-cancel",
            ShortcutAction::CancelRecording,
            ShortcutTrigger::Pressed,
        )];
        let new: Vec<EngineBinding> = vec![];
        let needs = bindings_needing_release(&old, &new);
        assert_eq!(needs.len(), 1);
        assert_eq!(needs[0].id, "escape-cancel");
    }

    #[test]
    fn fix_e_action_only_change_marks_for_release() {
        // Same trigger, different action — still a changed binding.
        let old = vec![binding(
            "x",
            ShortcutAction::HoldToRecord,
            ShortcutTrigger::Hold,
        )];
        let new = vec![binding(
            "x",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Hold,
        )];
        assert_eq!(bindings_needing_release(&old, &new).len(), 1);
    }

    #[test]
    fn fix_e_trigger_only_change_marks_for_release() {
        // Same action, different trigger — still a changed binding.
        let old = vec![binding(
            "x",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Pressed,
        )];
        let new = vec![binding(
            "x",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Hold,
        )];
        assert_eq!(bindings_needing_release(&old, &new).len(), 1);
    }

    fn modifier_hold_binding(id: &str) -> ShortcutBinding {
        ShortcutBinding {
            id: id.to_string(),
            action: ShortcutAction::HoldToRecord,
            shortcut: String::new(),
            trigger: ShortcutTrigger::Hold,
            enabled: true,
            allow_risky_combo: false,
            trigger_kind: TriggerKind::ModifierHold,
            modifier: Some(ModifierSpec {
                modifier: ModifierKind::Alt,
                side: SideKind::Right,
            }),
        }
    }

    /// Issue A: on upgrade a saved combo hotkey must arm even when a stale
    /// enabled bare-modifier recording primary is present. The stale primary is
    /// flagged for a durable disable and removed from the runtime rebuild; the
    /// combo `primary` is synthesized from settings.hotkey.
    #[test]
    fn nonempty_hotkey_disables_stale_bare_primary_and_synthesizes_combo() {
        let stale = modifier_hold_binding("onboarding-primary-hold");
        let (bindings, stale_id) = plan_engine_bindings(
            &[stale],
            "CommandOrControl+Space",
            RecordingMode::Toggle,
            false,
            None,
            false,
        );

        assert_eq!(stale_id.as_deref(), Some("onboarding-primary-hold"));
        assert!(
            !bindings.iter().any(|b| b.id == "onboarding-primary-hold"),
            "stale bare primary must not be installed"
        );

        let primary = bindings
            .iter()
            .find(|b| b.id == "primary")
            .expect("combo primary must be synthesized when hotkey is non-empty");
        assert_eq!(primary.trigger_kind, TriggerKind::Combo);
        assert_eq!(primary.shortcut, "CommandOrControl+Space");
        assert!(primary.enabled);
    }

    fn isolated_tap_binding(id: &str) -> ShortcutBinding {
        ShortcutBinding {
            id: id.to_string(),
            action: ShortcutAction::ToggleRecording,
            shortcut: String::new(),
            trigger: ShortcutTrigger::Pressed,
            enabled: true,
            allow_risky_combo: false,
            trigger_kind: TriggerKind::IsolatedTap,
            modifier: Some(ModifierSpec {
                modifier: ModifierKind::Alt,
                side: SideKind::Right,
            }),
        }
    }

    /// Issue A migration also covers an `IsolatedTap` stale primary
    /// (`stale_primary_candidate` matches both ModifierHold and IsolatedTap):
    /// a non-empty combo hotkey disables the tap primary and synthesizes the combo.
    #[test]
    fn nonempty_hotkey_disables_stale_isolated_tap_primary() {
        let stale = isolated_tap_binding("onboarding-primary-hold");
        let (bindings, stale_id) = plan_engine_bindings(
            &[stale],
            "CommandOrControl+Space",
            RecordingMode::Toggle,
            false,
            None,
            false,
        );

        assert_eq!(stale_id.as_deref(), Some("onboarding-primary-hold"));
        assert!(
            !bindings.iter().any(|b| b.id == "onboarding-primary-hold"),
            "stale isolated-tap primary must not be installed"
        );
        assert!(
            bindings
                .iter()
                .any(|b| b.id == "primary" && b.trigger_kind == TriggerKind::Combo),
            "combo primary must be synthesized over a stale isolated-tap primary"
        );
    }

    /// Empty hotkey + a valid bare-modifier primary: the bare primary owns
    /// recording, no combo is synthesized, and nothing is migrated (unchanged
    /// behavior).
    #[test]
    fn empty_hotkey_with_bare_primary_synthesizes_no_combo() {
        let bare = modifier_hold_binding("onboarding-primary-hold");
        let (bindings, stale_id) =
            plan_engine_bindings(&[bare], "", RecordingMode::Toggle, false, None, false);

        assert!(
            stale_id.is_none(),
            "empty hotkey must not trigger migration"
        );
        assert!(
            bindings
                .iter()
                .any(|b| b.id == "onboarding-primary-hold" && b.enabled),
            "bare primary must stay enabled"
        );
        assert!(
            !bindings.iter().any(|b| b.id == "primary"),
            "no combo primary when a bare primary owns recording"
        );
    }

    /// A non-empty combo hotkey with both the stale primary and an extra
    /// user-added bare-modifier shortcut: only the single primary candidate is
    /// disabled; the additional shortcut is preserved and stays enabled.
    #[test]
    fn additional_bare_modifier_shortcut_is_not_disabled() {
        let stale = modifier_hold_binding("onboarding-primary-hold");
        let mut extra = modifier_hold_binding("my-extra-mod");
        extra.modifier = Some(ModifierSpec {
            modifier: ModifierKind::Control,
            side: SideKind::Left,
        });
        let (bindings, stale_id) = plan_engine_bindings(
            &[stale, extra],
            "CommandOrControl+Space",
            RecordingMode::Toggle,
            false,
            None,
            false,
        );

        assert_eq!(stale_id.as_deref(), Some("onboarding-primary-hold"));
        let extra_installed = bindings
            .iter()
            .find(|b| b.id == "my-extra-mod")
            .expect("additional bare-modifier shortcut must stay installed");
        assert!(
            extra_installed.enabled,
            "additional shortcut must stay enabled"
        );
        assert!(bindings
            .iter()
            .any(|b| b.id == "primary" && b.trigger_kind == TriggerKind::Combo));
        assert!(!bindings.iter().any(|b| b.id == "onboarding-primary-hold"));
    }

    /// Precedence: without an `onboarding-primary-hold` id, the FIRST enabled
    /// recording bare-modifier binding is the primary candidate; a second one is
    /// an additional shortcut and is preserved.
    #[test]
    fn stale_primary_falls_back_to_first_bare_modifier_binding() {
        let first = modifier_hold_binding("legacy-hold");
        let mut second = modifier_hold_binding("legacy-hold-2");
        second.modifier = Some(ModifierSpec {
            modifier: ModifierKind::Control,
            side: SideKind::Left,
        });
        let (bindings, stale_id) = plan_engine_bindings(
            &[first, second],
            "CommandOrControl+Space",
            RecordingMode::Toggle,
            false,
            None,
            false,
        );

        assert_eq!(stale_id.as_deref(), Some("legacy-hold"));
        assert!(!bindings.iter().any(|b| b.id == "legacy-hold"));
        assert!(bindings
            .iter()
            .any(|b| b.id == "legacy-hold-2" && b.enabled));
        assert!(bindings
            .iter()
            .any(|b| b.id == "primary" && b.trigger_kind == TriggerKind::Combo));
    }

    /// `stale_primary_candidate` is a pure no-op for an empty hotkey even when a
    /// bare-modifier recording binding exists.
    #[test]
    fn stale_primary_candidate_returns_none_for_empty_hotkey() {
        assert_eq!(
            stale_primary_candidate(&[modifier_hold_binding("onboarding-primary-hold")], ""),
            None
        );
        assert_eq!(
            stale_primary_candidate(&[modifier_hold_binding("onboarding-primary-hold")], "   "),
            None,
            "whitespace-only hotkey is treated as empty"
        );
    }
}
