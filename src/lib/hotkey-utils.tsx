import { cn } from "@/lib/utils";
import { isMacOS } from "@/lib/platform";

/**
 * Formats a hotkey string into styled keyboard keys
 * e.g., "cmd+shift+space" -> styled keyboard keys
 */
export function formatHotkey(hotkey: string): React.ReactNode {
  if (!hotkey) return null;

  // Normalize the hotkey string based on platform
  const normalized = hotkey
    .toLowerCase()
    .replace("commandorcontrol", isMacOS ? "cmd" : "ctrl")
    .replace("super", isMacOS ? "cmd" : "win") // Super is Command on Mac
    .replace("command", "cmd")
    .replace("control", "ctrl")
    .replace("option", "alt")
    .replace("meta", "cmd")
    .replace("return", "enter")
    .replace("arrowup", "up")
    .replace("arrowdown", "down")
    .replace("arrowleft", "left")
    .replace("arrowright", "right");

  // Split by + and filter out empty strings
  const keys = normalized.split("+").filter(Boolean);

  // Map common key names to display names
  const keyDisplayMap: Record<string, string> = {
    cmd: isMacOS ? "⌘" : "Ctrl",
    ctrl: isMacOS ? "⌃" : "Ctrl", // Use proper Control symbol on Mac
    super: isMacOS ? "⌘" : "Win", // Super is Command on Mac
    win: "Win",
    commandorcontrol: isMacOS ? "⌘" : "Ctrl",
    command: isMacOS ? "⌘" : "Ctrl",
    shift: isMacOS ? "⇧" : "Shift",
    alt: isMacOS ? "⌥" : "Alt",
    option: isMacOS ? "⌥" : "Alt",
    meta: isMacOS ? "⌘" : "Ctrl",
    space: "Space",
    enter: isMacOS ? "⏎" : "Enter",
    return: isMacOS ? "⏎" : "Enter",
    tab: isMacOS ? "⇥" : "Tab",
    escape: "Esc",
    esc: "Esc",
    delete: isMacOS ? "⌦" : "Del",
    backspace: isMacOS ? "⌫" : "Backspace",
    up: "↑",
    down: "↓",
    left: "←",
    right: "→",
    arrowup: "↑",
    arrowdown: "↓",
    arrowleft: "←",
    arrowright: "→",
    pageup: isMacOS ? "⇞" : "PgUp",
    pagedown: isMacOS ? "⇟" : "PgDn",
    home: isMacOS ? "⇱" : "Home",
    end: isMacOS ? "⇲" : "End",
    f1: "F1",
    f2: "F2",
    f3: "F3",
    f4: "F4",
    f5: "F5",
    f6: "F6",
    f7: "F7",
    f8: "F8",
    f9: "F9",
    f10: "F10",
    f11: "F11",
    f12: "F12",
  };

  return (
    <span className="inline-flex items-center gap-1">
      {keys.map((key, index) => (
        <span key={index} className="inline-flex items-center gap-1">
          <kbd
            className={cn(
              "inline-flex items-center justify-center",
              "min-w-[28px] h-7 px-2 rounded",
              "bg-muted text-muted-foreground",
              "text-sm font-medium",
              "border border-b-2 border-muted-foreground/20",
              "shadow-sm",
            )}
          >
            {keyDisplayMap[key] || key}
          </kbd>
          {index < keys.length - 1 && <span className="text-muted-foreground text-sm">+</span>}
        </span>
      ))}
    </span>
  );
}
