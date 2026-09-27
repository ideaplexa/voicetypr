export type ShortcutAction =
  | "toggle_recording"
  | "hold_to_record"
  | "cancel_recording"
  | "copy_last_transcription"
  | "paste_last_transcription"
  | "toggle_ai_formatting"
  | "open_dashboard";

export type ShortcutTrigger = "pressed" | "hold";

export type TriggerKind = "combo" | "modifier_hold" | "isolated_tap";
export type ModifierKind = "alt" | "control" | "meta" | "shift";
export type ModifierSide = "left" | "right" | "either";

export interface ModifierSpec {
  modifier: ModifierKind;
  side: ModifierSide;
}

export interface ShortcutBinding {
  id: string;
  action: ShortcutAction;
  shortcut: string;
  trigger: ShortcutTrigger;
  enabled: boolean;
  allow_risky_combo: boolean;
  /** Defaults to "combo" (legacy global_shortcut). Native kinds use the engine. */
  trigger_kind?: TriggerKind;
  /** Modifier target for "modifier_hold" / "isolated_tap" kinds. */
  modifier?: ModifierSpec | null;
}

export interface ShortcutSettings {
  bindings: ShortcutBinding[];
}

export interface ShortcutActionDefinition {
  action: ShortcutAction;
  label: string;
  description: string;
  section: string;
  recommended_trigger: ShortcutTrigger;
  allows_single_key: boolean;
}
