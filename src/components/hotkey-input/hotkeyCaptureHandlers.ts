import type { MutableRefObject } from "react";
import {
  normalizeShortcutKeys,
  validateKeyCombinationWithRules,
  formatKeyForDisplay,
  type KeyValidationRules,
} from "@/lib/keyboard-normalizer";
import { mapCodeToKey } from "@/lib/keyboard-mapper";
import { checkForSystemConflict, formatConflictMessage } from "@/lib/hotkey-conflicts";
import { isMacOS } from "@/lib/platform";
import { formatBareModifierDisplay } from "./formatBareModifierDisplay";
import type { BareModifierSpec } from "./types";

export interface HotkeyCaptureHandlerContext {
  keysRef: MutableRefObject<Set<string>>;
  validationRulesRef: MutableRefObject<KeyValidationRules>;
  handleCancelRef: MutableRefObject<() => void>;
  onChangeRef: MutableRefObject<(value: string) => void>;
  onBareModifierRef: MutableRefObject<((spec: BareModifierSpec) => void) | undefined>;
  allowBareModifier: boolean;
  inline: boolean;
  setKeys: (keys: Set<string>) => void;
  setValidationError: (error: string) => void;
  setPendingBareModifier: (spec: BareModifierSpec | null) => void;
  setPendingHotkey: (hotkey: string) => void;
  setCurrentKeysDisplay: (display: string) => void;
}

function addMappedKey(newKeys: Set<string>, mappedKey: string) {
  if (mappedKey === "Enter") {
    newKeys.add("Return"); // Tauri expects "Return" not "Enter"
  } else if (
    mappedKey === "Up" ||
    mappedKey === "Down" ||
    mappedKey === "Left" ||
    mappedKey === "Right"
  ) {
    newKeys.add(mappedKey); // Arrow keys
  } else if (mappedKey.startsWith("F") && mappedKey.length <= 3) {
    newKeys.add(mappedKey); // Function keys
  } else if (
    [
      "PageUp",
      "PageDown",
      "Home",
      "End",
      "Insert",
      "Delete",
      "Backspace",
      "Tab",
      "Space",
      "Escape",
    ].includes(mappedKey)
  ) {
    newKeys.add(mappedKey); // Special keys
  } else if (mappedKey.startsWith("Numpad")) {
    newKeys.add(mappedKey); // Numpad keys
  } else if (
    [
      "Comma",
      "Period",
      "Semicolon",
      "Quote",
      "BracketLeft",
      "BracketRight",
      "Backslash",
      "Slash",
      "Equal",
      "Minus",
      "Backquote",
    ].includes(mappedKey)
  ) {
    // Punctuation keys - use the physical position name
    newKeys.add(mappedKey);
  } else if (mappedKey.length === 1) {
    // Single character (letter or number)
    newKeys.add(mappedKey.toUpperCase());
  } else {
    // Fallback to the mapped key
    newKeys.add(mappedKey);
  }
}

function partitionKeys(keys: Iterable<string>) {
  const modifiers: string[] = [];
  const regularKeys: string[] = [];
  for (const k of keys) {
    if (["CommandOrControl", "Control", "Shift", "Alt"].includes(k)) {
      modifiers.push(k);
    } else {
      regularKeys.push(k);
    }
  }
  return { modifiers, regularKeys };
}

export function handleHotkeyKeyDown(e: KeyboardEvent, ctx: HotkeyCaptureHandlerContext) {
  e.preventDefault();
  e.stopPropagation();

  const key = e.key;
  const code = e.code || ""; // Get physical key code with fallback for older browsers

  // Handle Escape to cancel
  if (key === "Escape") {
    ctx.handleCancelRef.current();
    return;
  }

  const newKeys = new Set(ctx.keysRef.current);

  // Add modifier keys - handle platform differences correctly
  if (isMacOS) {
    // On macOS, Command maps to CommandOrControl and Control is tracked
    // separately, so both can coexist in a single combo (e.g. Cmd+Ctrl+K).
    // A lone Command → CommandOrControl; a lone Control → Control.
    if (e.metaKey) newKeys.add("CommandOrControl");
    if (e.ctrlKey) newKeys.add("Control");
  } else {
    // On Windows/Linux, Control key maps to CommandOrControl
    if (e.ctrlKey) newKeys.add("CommandOrControl");
  }
  if (e.shiftKey) newKeys.add("Shift");
  if (e.altKey) newKeys.add("Alt");

  // Add the actual key using physical position (e.code) for international keyboard support
  if (!["Control", "Shift", "Alt", "Meta"].includes(key)) {
    // Use physical key code when available, fallback to key for older browsers
    const mappedKey = code ? mapCodeToKey(code) : key;
    addMappedKey(newKeys, mappedKey);
  }

  // Check max keys limit
  if (newKeys.size > ctx.validationRulesRef.current.maxKeys) {
    ctx.setValidationError(`Maximum ${ctx.validationRulesRef.current.maxKeys} keys allowed`);
    return;
  }

  ctx.setKeys(newKeys);
  ctx.setValidationError("");

  // Update pending hotkey preview
  const { modifiers, regularKeys } = partitionKeys(newKeys);

  // ── Bare modifier path ────────────────────────────────────────────────
  // When the caller opts in (allowBareModifier), a single side-specific
  // modifier key pressed alone is a valid selection.  We read the
  // physical side from e.code (AltRight/AltLeft etc.).
  if (ctx.allowBareModifier && modifiers.length === 1 && regularKeys.length === 0) {
    const side: string = code.endsWith("Right")
      ? "right"
      : code.endsWith("Left")
        ? "left"
        : "either";
    const modMap: Record<string, string> = {
      Alt: "alt",
      Control: "control",
      Meta: "meta",
      Shift: "shift",
    };
    // key is the modifier being pressed here (already checked regularKeys.length===0)
    const modifierKind = modMap[key] ?? "alt";
    const spec: BareModifierSpec = { modifier: modifierKind, side };
    ctx.setPendingBareModifier(spec);
    ctx.setPendingHotkey("");
    ctx.setValidationError("");
    ctx.setCurrentKeysDisplay(formatBareModifierDisplay(spec));
    if (ctx.inline) {
      ctx.onBareModifierRef.current?.(spec);
    }
    return;
  }

  // Not a bare modifier (has a regular key, or multiple modifiers without key).
  // Clear any previously captured bare modifier so the paths stay exclusive.
  ctx.setPendingBareModifier(null);
  // ─────────────────────────────────────────────────────────────────────

  const orderedModifiers = ["CommandOrControl", "Control", "Alt", "Shift"].filter((mod) =>
    modifiers.includes(mod),
  );
  const shortcut = [...orderedModifiers, ...regularKeys].join("+");
  if (shortcut) {
    // Update current keys display
    const displayKeys = [];
    if (modifiers.includes("CommandOrControl"))
      displayKeys.push(formatKeyForDisplay("CommandOrControl", isMacOS));
    if (modifiers.includes("Control")) displayKeys.push(formatKeyForDisplay("Control", isMacOS));
    if (modifiers.includes("Alt")) displayKeys.push(formatKeyForDisplay("Alt", isMacOS));
    if (modifiers.includes("Shift")) displayKeys.push(formatKeyForDisplay("Shift", isMacOS));
    displayKeys.push(...regularKeys.map((k) => formatKeyForDisplay(k, isMacOS)));
    ctx.setCurrentKeysDisplay(displayKeys.join(" + "));

    // Validate with rules
    const validation = validateKeyCombinationWithRules(shortcut, ctx.validationRulesRef.current);
    if (!validation.valid) {
      ctx.setValidationError(validation.error || "Invalid key combination");
      ctx.setPendingHotkey("");
      if (ctx.inline) {
        ctx.onChangeRef.current("");
      }
    } else {
      // Check for system conflicts
      const conflict = checkForSystemConflict(shortcut);
      if (conflict) {
        ctx.setValidationError(formatConflictMessage(conflict));
        // Still show the hotkey but with warning for 'warning' severity
        if (ctx.inline) {
          ctx.onChangeRef.current("");
        }
        if (conflict.severity === "warning") {
          ctx.setPendingHotkey(shortcut);
        } else {
          ctx.setPendingHotkey("");
        }
      } else {
        ctx.setPendingHotkey(shortcut);
        ctx.setValidationError("");
        if (ctx.inline) {
          ctx.onChangeRef.current(normalizeShortcutKeys(shortcut));
        }
      }
    }
  }
}

export function handleHotkeyKeyUp(e: KeyboardEvent, ctx: HotkeyCaptureHandlerContext) {
  e.preventDefault();
  e.stopPropagation();

  if (ctx.keysRef.current.size > 0) {
    // Format the shortcut
    const { modifiers, regularKeys } = partitionKeys(ctx.keysRef.current);

    // ── Bare modifier release ─────────────────────────────────────────
    // When allowBareModifier is true and the released sequence has no
    // regular key, the bare modifier set during keydown is already the
    // selection.  Just clear the keys state so the display stays clean.
    if (ctx.allowBareModifier && regularKeys.length === 0) {
      ctx.setKeys(new Set());
      return;
    }
    // ─────────────────────────────────────────────────────────────────

    // Standard order: CommandOrControl+Control+Alt+Shift+Key
    const orderedModifiers = ["CommandOrControl", "Control", "Alt", "Shift"].filter((mod) =>
      modifiers.includes(mod),
    );
    const shortcut = [...orderedModifiers, ...regularKeys].join("+");

    const validation = validateKeyCombinationWithRules(shortcut, ctx.validationRulesRef.current);
    if (validation.valid) {
      // Check for system conflicts
      const conflict = checkForSystemConflict(shortcut);
      if (conflict) {
        ctx.setValidationError(formatConflictMessage(conflict));
        // Still allow setting it, but with warning
        if (ctx.inline) {
          ctx.onChangeRef.current("");
        }
        if (conflict.severity === "warning") {
          ctx.setPendingHotkey(shortcut);
          ctx.setKeys(new Set());
          ctx.setCurrentKeysDisplay("");
        }
      } else {
        ctx.setPendingHotkey(shortcut);
        ctx.setKeys(new Set());
        ctx.setCurrentKeysDisplay("");
        ctx.setValidationError("");
        if (ctx.inline) {
          ctx.onChangeRef.current(normalizeShortcutKeys(shortcut));
        }
      }
    } else {
      ctx.setValidationError(validation.error || "Invalid key combination");
      if (ctx.inline) {
        ctx.onChangeRef.current("");
      }
    }
  }
}
