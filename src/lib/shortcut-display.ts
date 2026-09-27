import { isMacOS } from "@/lib/platform";
import type { ModifierSpec, ShortcutBinding } from "@/types/shortcuts";
import { formatKeyForDisplay } from "@/lib/keyboard-normalizer";

const BARE_MOD_ICONS: Record<string, string> = {
  alt: "⌥",
  meta: "⌘",
  control: "⌃",
  shift: "⇧",
};

/**
 * Locate the active native "primary" recording trigger among shortcut bindings.
 *
 * The primary is the onboarding hold binding, or — failing that — the first
 * enabled engine-kind binding (`modifier_hold` / `isolated_tap`)
 * that drives recording (`hold_to_record` / `toggle_recording`). Combo shortcuts
 * live in `settings.hotkey` (the `global_shortcut` path) and are intentionally
 * not considered here.
 *
 * Returns `null` when the active primary is a combo, or nothing is set.
 */
export function findActivePrimaryBinding(bindings: ShortcutBinding[]): ShortcutBinding | null {
  return (
    bindings.find((b) => b.id === "onboarding-primary-hold") ??
    bindings.find(
      (b) =>
        b.enabled &&
        (b.action === "hold_to_record" || b.action === "toggle_recording") &&
        (b.trigger_kind === "modifier_hold" || b.trigger_kind === "isolated_tap"),
    ) ??
    null
  );
}

/** Format a side-specific bare modifier as a compact label, e.g. "Right ⌥". */
export function formatModifierLabel(mod: ModifierSpec): string {
  const sideLabel = mod.side === "right" ? "Right " : mod.side === "left" ? "Left " : "";
  const modLabel = isMacOS
    ? (BARE_MOD_ICONS[mod.modifier] ?? mod.modifier)
    : mod.modifier.charAt(0).toUpperCase() + mod.modifier.slice(1);
  return `${sideLabel}${modLabel}`;
}

/**
 * Resolve the ACTIVE primary recording trigger to a single display string.
 *
 * This is the single source of truth for "what trigger starts recording":
 * - Bare-modifier primary (stored in `ShortcutSettings`; `hotkey` is empty) → a
 *   human label such as "Hold ⌘ to talk" or "Tap ⌥ to toggle".
 * - Combo primary (stored in `settings.hotkey`) → the combo formatted for humans
 *   (e.g. `⌘+⇧+␣` on macOS, `Ctrl+Shift+Space` elsewhere).
 * - Nothing configured → "Not set".
 *
 * Never falls back to a hardcoded default like `Cmd+Shift+Space`.
 */
export function formatPrimaryHotkeyLabel(
  binding: ShortcutBinding | null,
  hotkey: string | undefined,
): string {
  const modSpec = binding?.modifier;
  if (modSpec) {
    const mod = formatModifierLabel(modSpec);
    if (binding?.trigger_kind === "modifier_hold") return `Hold ${mod} to talk`;
    if (binding?.trigger_kind === "isolated_tap") return `Tap ${mod} to toggle`;
    return mod;
  }
  if (hotkey) {
    // Format the combo for humans (CommandOrControl -> ⌘/Ctrl, Shift -> ⇧, ...)
    // instead of leaking the raw accelerator string into the UI.
    return hotkey
      .split("+")
      .filter(Boolean)
      .map((key) => formatKeyForDisplay(key, isMacOS))
      .join("+");
  }
  return "Not set";
}
