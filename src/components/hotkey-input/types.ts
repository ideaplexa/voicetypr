import type { KeyValidationRules } from "@/lib/keyboard-normalizer";

export interface BareModifierSpec {
  /** One of: "alt" | "control" | "meta" | "shift" */
  modifier: string;
  /** One of: "left" | "right" | "either" */
  side: string;
}

export interface HotkeyInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  validationRules?: KeyValidationRules;
  label?: string; // e.g., "Recording Hotkey", "Custom Hotkey"
  onEditingChange?: (isEditing: boolean) => void; // Notify parent when edit mode changes
  /** When true, a lone side-specific modifier (e.g. Right-Option) is accepted as a
   *  valid selection and reported via onBareModifier instead of onChange. */
  allowBareModifier?: boolean;
  /** Called when the user confirms a bare modifier selection. */
  onBareModifier?: (spec: BareModifierSpec) => void;
  /** When true, render as a bare always-capturing field — no internal display/edit
   *  toggle and no Save/Cancel/Edit buttons. Selections report live via onChange /
   *  onBareModifier so the parent owns the controls. */
  inline?: boolean;
}
