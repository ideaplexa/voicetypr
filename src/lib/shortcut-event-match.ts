/**
 * Match a DOM KeyboardEvent against a stored Tauri shortcut string
 * (e.g. "CommandOrControl+Space").
 *
 * This is the IN-APP fallback path: when focus is inside one of our own
 * WebView2 text inputs, the OS-level keytrigger hook can be bypassed by the
 * browser/IME (notably Ctrl+Space, which Windows reserves as an input-language
 * toggle), so the global hotkey never fires. The frontend still receives the
 * DOM keydown, so we re-derive the same canonical shortcut string the hotkey
 * picker produces and compare it to the configured hotkey.
 *
 * The canonical form intentionally mirrors `HotkeyInput`'s keydown builder so a
 * string produced here equals the one persisted in settings.hotkey.
 */
import { mapCodeToKey } from "@/lib/keyboard-mapper";
import { normalizeShortcutKeys } from "@/lib/keyboard-normalizer";
import { isMacOS } from "@/lib/platform";

// Persisted-shortcut modifier order: CommandOrControl, Control, Alt, Shift.
const MODIFIER_ORDER = ["CommandOrControl", "Control", "Alt", "Shift"] as const;
const PURE_MODIFIER_KEYS: Record<string, true> = {
  Control: true,
  Shift: true,
  Alt: true,
  Meta: true,
};

/**
 * Apply the same per-key normalization the hotkey picker uses after mapping the
 * physical `e.code`: Tauri expects "Return" (not "Enter"), and single-character
 * keys are upper-cased so "a" and "A" produce the same shortcut.
 */
function normalizeMainKey(mapped: string): string {
  if (mapped === "Enter") return "Return";
  if (mapped.length === 1) return mapped.toUpperCase();
  return mapped;
}

/**
 * Derive the canonical shortcut string for a keydown event, or `null` when the
 * event is a bare modifier press (no main key) and therefore not a combo.
 */
export function eventToShortcut(event: KeyboardEvent): string | null {
  if (PURE_MODIFIER_KEYS[event.key]) return null;

  const modifiers = new Set<string>();
  if (isMacOS) {
    // macOS: Command → CommandOrControl, Control stays separate (Cmd+Ctrl combos).
    if (event.metaKey) modifiers.add("CommandOrControl");
    if (event.ctrlKey) modifiers.add("Control");
  } else {
    // Windows/Linux: Control → CommandOrControl.
    if (event.ctrlKey) modifiers.add("CommandOrControl");
  }
  if (event.altKey) modifiers.add("Alt");
  if (event.shiftKey) modifiers.add("Shift");

  const code = event.code || "";
  const mainKey = normalizeMainKey(code ? mapCodeToKey(code) : event.key);
  if (!mainKey) return null;

  const ordered = MODIFIER_ORDER.filter((mod) => modifiers.has(mod));
  return [...ordered, mainKey].join("+");
}

/**
 * Order-free form of a normalized shortcut. Off macOS, Control and
 * CommandOrControl name the same key: the Windows default primary is stored as
 * "Control+Space" while a Ctrl keydown derives "CommandOrControl".
 */
function canonicalShortcut(shortcut: string): string {
  return normalizeShortcutKeys(shortcut)
    .split("+")
    .map((part) => (!isMacOS && part === "Control" ? "CommandOrControl" : part))
    .sort()
    .join("+");
}

/**
 * True when the keydown event matches the configured hotkey string. Both sides
 * are normalized so stored variants (e.g. "Ctrl+Space") compare equal.
 */
export function eventMatchesShortcut(event: KeyboardEvent, hotkey: string): boolean {
  if (!hotkey || !hotkey.trim()) return false;
  const derived = eventToShortcut(event);
  if (!derived) return false;
  return canonicalShortcut(derived) === canonicalShortcut(hotkey);
}
