import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { loadEffectivePrimaryShortcut, type EffectivePrimaryShortcut } from "@/lib/primary-shortcut";
import {
  formatModifierLabel,
  formatPrimaryHotkeyLabel,
} from "@/lib/shortcut-display";
import type { PrimaryMode } from "@/lib/primary-shortcut";

export interface ActiveTrigger {
  mode: PrimaryMode;
  /** Full descriptive label for the active primary trigger (source of truth). */
  label: string;
  /** Raw combo hotkey string, if the active primary is a combo (`settings.hotkey`). */
  hotkey: string | undefined;
  /**
   * Combo string or bare-modifier key token, suitable for kbd styling
   * (`formatHotkey`). For combos this is the `+`-delimited string; for a
   * bare-modifier primary it is the modifier token (e.g. "⌘"); otherwise the
   * descriptive `label`.
   */
  kbdLabel: string;
}

/**
 * Resolve the ACTIVE primary recording trigger reactively.
 *
 * A bare-modifier primary intentionally has an empty `settings.hotkey` (the real
 * trigger lives in `ShortcutSettings`), so callers MUST NOT fall back to a
 * default like `Cmd+Shift+Space` when `hotkey` is empty. This hook loads the
 * native primary binding in that case and resolves the label via the shared
 * `formatPrimaryHotkeyLabel`, keeping every display site consistent.
 */
export function useActiveTrigger(settings: { hotkey?: string; recording_mode?: PrimaryMode } | null | undefined): ActiveTrigger {
  const [effective, setEffective] = useState<EffectivePrimaryShortcut | null>(null);
  // Tray writes refresh SettingsContext even when only recording_mode changes.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const refresh = () => {
      void loadEffectivePrimaryShortcut()
        .then((result) => { if (!cancelled) setEffective(result); })
        .catch(() => undefined);
    };
    refresh();
    void listen("shortcut-settings-changed", refresh).then((dispose) => {
      if (cancelled) dispose(); else unlisten = dispose;
    }).catch(() => undefined);
    return () => { cancelled = true; unlisten?.(); };
  }, [settings?.hotkey, settings?.recording_mode]);

  const binding = effective?.binding ?? null;
  const hotkey = effective?.hotkey ?? (effective ? undefined : settings?.hotkey || undefined);
  const label = !effective && !settings?.hotkey ? "Loading shortcut…" : formatPrimaryHotkeyLabel(binding, hotkey);
  return {
    mode: effective ? (effective.mode === "hold" ? "push_to_talk" : "toggle") : settings?.recording_mode ?? "toggle",
    label,
    hotkey,
    kbdLabel: hotkey || (binding?.modifier ? formatModifierLabel(binding.modifier) : label),
  };
}
