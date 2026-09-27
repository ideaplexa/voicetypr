import type { BareModifierSpec } from "@/components/HotkeyInput";
import { normalizeShortcutKeys, ValidationPresets } from "@/lib/keyboard-normalizer";
import type {
  ModifierKind,
  ModifierSide,
  ShortcutAction,
  ShortcutActionDefinition,
  ShortcutBinding,
  ShortcutSettings,
} from "@/types/shortcuts";

export const emptySettings: ShortcutSettings = { bindings: [] };

export const singleKeyValidation = ValidationPresets.custom({
  minKeys: 1,
  requireModifier: false,
  requireModifierForMultiKey: true,
});

export const MAX_SINGLE_KEY_BINDINGS = 5;

export function isSingleKeyShortcut(shortcut: string): boolean {
  if (!shortcut) return false;
  const normalized = normalizeShortcutKeys(shortcut);
  const parts = normalized.split("+").filter(Boolean);
  if (parts.length !== 1) return false;
  const modifiers = [
    "CommandOrControl",
    "Super",
    "Shift",
    "Alt",
    "Control",
    "Command",
    "Cmd",
    "Ctrl",
    "Option",
    "Meta",
  ];
  return !modifiers.includes(parts[0]);
}

export function createBinding(action: ShortcutActionDefinition): ShortcutBinding {
  return {
    id:
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `${action.action}-${Date.now()}-${Math.random().toString(36).slice(2)}`,
    action: action.action,
    shortcut: "",
    trigger: action.recommended_trigger,
    enabled: true,
    allow_risky_combo: false,
  };
}

export function normalizeSettings(value: ShortcutSettings | null | undefined): ShortcutSettings {
  return {
    bindings: Array.isArray(value?.bindings) ? value.bindings : [],
  };
}

export function formatError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function shortcutComparisonKey(shortcut: string) {
  return normalizeShortcutKeys(shortcut).toLowerCase();
}

export const MOD_LABELS: Record<string, string> = {
  alt: "Option",
  meta: "Command",
  control: "Control",
  shift: "Shift",
};

// The primary recording trigger (toggle/hold) is configured in General Settings,
// not here. Exclude these actions from the custom-shortcuts list so the recording
// hotkey isn't shown/edited in two places and a duplicate recording binding can't
// be created from this page.
export const PRIMARY_RECORDING_ACTIONS: ReadonlySet<string> = new Set([
  "toggle_recording",
  "hold_to_record",
]);

export function formatBindingDisplay(binding: ShortcutBinding): string {
  const kind = binding.trigger_kind ?? "combo";
  const mod = binding.modifier;
  if (mod && (kind === "modifier_hold" || kind === "isolated_tap")) {
    const sideLabel = mod.side === "right" ? "Right " : mod.side === "left" ? "Left " : "";
    const modLabel = MOD_LABELS[mod.modifier] ?? mod.modifier;
    const verbLabel = kind === "modifier_hold" ? "Hold" : "Tap";
    return `${verbLabel} ${sideLabel}${modLabel}`;
  }
  return binding.shortcut || "No shortcut configured";
}

export type EditingCapture = {
  bindingId: string;
  /** Captured combo shortcut string (mutually exclusive with bareModifier). */
  combo: string;
  /** Captured bare modifier (mutually exclusive with combo). */
  bareModifier: BareModifierSpec | null;
  /** Mirrors allow_risky_combo for the draft being captured. */
  allowRiskyCombo: boolean;
};

export function groupActionsBySection(actions: ShortcutActionDefinition[]) {
  const groups = new Map<string, ShortcutActionDefinition[]>();

  for (const action of actions) {
    if (PRIMARY_RECORDING_ACTIONS.has(action.action)) {
      continue; // primary recording hotkey is managed in General Settings
    }
    const existing = groups.get(action.section);
    if (existing) {
      existing.push(action);
    } else {
      groups.set(action.section, [action]);
    }
  }

  return Array.from(groups.entries());
}

export function groupBindingsByAction(
  settingsBindings: ShortcutBinding[],
  draftBindings: ShortcutBinding[],
) {
  const groups = new Map<ShortcutAction, ShortcutBinding[]>();

  for (const binding of [...settingsBindings, ...draftBindings]) {
    const existing = groups.get(binding.action);
    if (existing) {
      existing.push(binding);
    } else {
      groups.set(binding.action, [binding]);
    }
  }

  return groups;
}

export function countEnabledSingleKeyBindings(
  settingsBindings: ShortcutBinding[],
  draftBindings: ShortcutBinding[],
) {
  return [...settingsBindings, ...draftBindings].filter(
    (b) => b.enabled && b.shortcut && isSingleKeyShortcut(b.shortcut),
  ).length;
}

export function findConflictingBinding(
  nextBinding: ShortcutBinding,
  settingsBindings: ShortcutBinding[],
  draftBindings: ShortcutBinding[],
) {
  const nextShortcut = nextBinding.shortcut.trim();
  if (!nextShortcut) return undefined;
  const nextShortcutKey = shortcutComparisonKey(nextShortcut);
  return [...settingsBindings, ...draftBindings].find(
    (binding) =>
      binding.id !== nextBinding.id &&
      binding.enabled &&
      binding.shortcut &&
      shortcutComparisonKey(binding.shortcut) === nextShortcutKey,
  );
}

export function bindingFromEditingCapture(
  originalBinding: ShortcutBinding,
  capture: EditingCapture,
  recommendedTrigger: ShortcutBinding["trigger"],
): ShortcutBinding {
  const { combo, bareModifier, allowRiskyCombo } = capture;

  if (bareModifier) {
    // Mode comes from the action row, not a separate toggle: Hold to Record =
    // push-to-talk (modifier_hold); Toggle Recording = tap (isolated_tap).
    const isPushToTalk = originalBinding.action === "hold_to_record";
    return {
      ...originalBinding,
      trigger_kind: isPushToTalk ? "modifier_hold" : "isolated_tap",
      trigger: isPushToTalk ? "hold" : "pressed",
      modifier: {
        modifier: bareModifier.modifier as ModifierKind,
        side: bareModifier.side as ModifierSide,
      },
      shortcut: "",
      enabled: true,
      allow_risky_combo: allowRiskyCombo,
    };
  }

  return {
    ...originalBinding,
    trigger_kind: "combo",
    trigger: recommendedTrigger,
    modifier: null,
    shortcut: combo,
    enabled: !!combo,
    allow_risky_combo: isSingleKeyShortcut(combo),
  };
}
