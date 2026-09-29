import type { Platform } from "@/lib/platform";
import { formatKeyForDisplay } from "@/lib/keyboard-normalizer";

// Mirror commands/key_normalizer.rs aliases for combo shortcuts. Native bare
// modifiers are display labels from their binding and never pass through here.
const BACKEND_ALIASES: Record<string, string> = {
  cmd: "CommandOrControl", ctrl: "CommandOrControl", command: "CommandOrControl",
  meta: "CommandOrControl", super: "CommandOrControl", option: "Alt",
  control: "Control", alt: "Alt", shift: "Shift", space: "Space",
};

/** Home's individual key labels. Other shortcut displays keep their own format. */
export function shortcutKeyCaps(hotkey: string | null | undefined, platform: Platform): string[] {
  if (!hotkey || hotkey === "Not set") return [];
  const mac = platform === "darwin";
  const modifiers: Record<string, { order: number; label: string }> = {
    commandorcontrol: { order: 0, label: mac ? "⌘" : "Ctrl" },
    control: { order: mac ? 2 : 0, label: mac ? "⌃" : "Ctrl" },
    alt: { order: 1, label: mac ? "⌥" : "Alt" },
    shift: { order: mac ? 3 : 2, label: mac ? "⇧" : "Shift" },
  };
  const parts = hotkey.split("+").map((part) => part.trim()).filter(Boolean);
  const modifierCaps: Array<{ order: number; label: string }> = [];
  const keyCaps: string[] = [];
  for (const part of parts) {
    const bare = part.replace(/^(Left|Right)\s+/i, "");
    const normalized = BACKEND_ALIASES[bare.toLowerCase()] ?? bare;
    const modifier = modifiers[normalized.toLowerCase()];
    if (modifier) {
      modifierCaps.push({ ...modifier, label: bare === part ? modifier.label : `${part.slice(0, part.length - bare.length).trim()} ${modifier.label}` });
    } else if (/^space$/i.test(normalized) || part === "␣") {
      keyCaps.push("Space");
    } else if (/^f\d{1,2}$/i.test(part)) {
      keyCaps.push(part.toUpperCase());
    } else {
      const display = formatKeyForDisplay(normalized, mac);
      keyCaps.push(display.length === 1 && /[a-z]/i.test(display) ? display.toUpperCase() : display);
    }
  }
  return [...new Set(modifierCaps.sort((a, b) => a.order - b.order).map((cap) => cap.label)), ...keyCaps];
}
