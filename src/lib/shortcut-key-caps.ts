import type { Platform } from "@/lib/platform";

/** Home's individual key labels. Other shortcut displays keep their own format. */
export function shortcutKeyCaps(hotkey: string | null | undefined, platform: Platform): string[] {
  if (!hotkey || hotkey === "Not set") return [];
  const mac = platform === "darwin";
  const modifiers: Record<string, { order: number; label: string }> = {
    commandorcontrol: { order: 0, label: mac ? "⌘" : "Ctrl" },
    cmd: { order: 0, label: mac ? "⌘" : "Ctrl" },
    command: { order: 0, label: mac ? "⌘" : "Ctrl" },
    meta: { order: mac ? 0 : 3, label: mac ? "⌘" : "Win" },
    super: { order: mac ? 0 : 3, label: mac ? "⌘" : "Win" },
    win: { order: 3, label: "Win" },
    control: { order: mac ? 2 : 0, label: mac ? "⌃" : "Ctrl" },
    ctrl: { order: mac ? 2 : 0, label: mac ? "⌃" : "Ctrl" },
    alt: { order: 1, label: mac ? "⌥" : "Alt" },
    option: { order: 1, label: mac ? "⌥" : "Alt" },
    shift: { order: mac ? 3 : 2, label: mac ? "⇧" : "Shift" },
  };
  const parts = hotkey.split("+").map((part) => part.trim()).filter(Boolean);
  const modifierCaps: Array<{ order: number; label: string }> = [];
  const keyCaps: string[] = [];
  for (const part of parts) {
    const bare = part.replace(/^(Left|Right)\s+/i, "");
    const modifier = modifiers[bare.toLowerCase()];
    if (modifier) {
      modifierCaps.push({ ...modifier, label: bare === part ? modifier.label : `${part.slice(0, part.length - bare.length).trim()} ${modifier.label}` });
    } else if (/^space$/i.test(part) || part === "␣") {
      keyCaps.push("Space");
    } else if (/^f\d{1,2}$/i.test(part)) {
      keyCaps.push(part.toUpperCase());
    } else {
      keyCaps.push(part.length === 1 ? part.toUpperCase() : part);
    }
  }
  return [...modifierCaps.sort((a, b) => a.order - b.order).map((cap) => cap.label), ...keyCaps];
}
