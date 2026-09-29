import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { ShortcutBinding, ShortcutSettings } from "@/types/shortcuts";
import {
  formatModifierLabel,
  formatPrimaryHotkeyLabel,
} from "@/lib/shortcut-display";
import { resolvePrimaryShortcut, type PrimaryMode } from "@/lib/primary-shortcut";

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
  const [bindings, setBindings] = useState<ShortcutBinding[]>([]);
  const hotkey = settings?.hotkey;

  useEffect(() => {
    if (hotkey) return;
    let cancelled = false;
    invoke<ShortcutSettings>("get_shortcut_settings")
      .then((result) => {
        if (!cancelled) setBindings(result.bindings);
      })
      .catch(() => {
        if (!cancelled) setBindings([]);
      });
    return () => {
      cancelled = true;
    };
  }, [hotkey]);

  // A combo primary (non-empty hotkey) wins; ignore any stale bare-modifier
  // binding still in state from a previous empty-hotkey load.
  const primary = resolvePrimaryShortcut(settings, bindings);
  const effectiveBinding = primary.binding;
  const kbdLabel =
    hotkey || (effectiveBinding?.modifier ? formatModifierLabel(effectiveBinding.modifier) : null);

  return {
    mode: primary.mode,
    label: formatPrimaryHotkeyLabel(effectiveBinding, hotkey),
    hotkey,
    kbdLabel: kbdLabel ?? formatPrimaryHotkeyLabel(effectiveBinding, hotkey),
  };
}
