/**
 * Normalize keyboard keys for cross-platform compatibility
 * This should match the Rust key_normalizer implementation
 */

const MODIFIER_KEYS: Record<string, true> = {
  CommandOrControl: true,
  Super: true,
  Shift: true,
  Alt: true,
  Control: true,
  Command: true,
  Cmd: true,
  Ctrl: true,
  Option: true,
  Meta: true,
};

/**
 * Normalize a complete shortcut string (e.g., "Cmd+Shift+A")
 */
export function normalizeShortcutKeys(shortcut: string): string {
  return shortcut
    .split("+")
    .map((key) => normalizeSingleKey(key))
    .join("+");
}

/**
 * Normalize a single key string
 */
function normalizeSingleKey(key: string): string {
  // Handle special key mappings (case-insensitive for common modifiers)
  const keyMapLower: Record<string, string> = {
    return: "Enter",
    arrowup: "Up",
    arrowdown: "Down",
    arrowleft: "Left",
    arrowright: "Right",
    cmd: "CommandOrControl",
    ctrl: "CommandOrControl",
    control: "Control", // Keep Control as Control for macOS Cmd+Ctrl support
    command: "CommandOrControl",
    option: "Alt",
    meta: "CommandOrControl",
    alt: "Alt",
    shift: "Shift",
    space: "Space",
    super: "Super", // Keep Super as Super for macOS Command key
  };

  // Check lowercase version first for common keys
  const lowerKey = key.toLowerCase();
  if (keyMapLower[lowerKey]) {
    return keyMapLower[lowerKey];
  }

  // Handle exact case matches for special formatting
  const keyMap: Record<string, string> = {
    Return: "Enter",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    Cmd: "CommandOrControl",
    Ctrl: "CommandOrControl",
    Control: "Control", // Keep Control as Control for macOS Cmd+Ctrl support
    Command: "CommandOrControl",
    Super: "Super", // Keep Super as Super for macOS Command key
    Option: "Alt",
    Meta: "CommandOrControl",
  };

  return keyMap[key] || key;
}

/**
 * Validation rules for key combinations
 */
export interface KeyValidationRules {
  minKeys: number;
  maxKeys: number;
  requireModifier: boolean;
  requireModifierForMultiKey: boolean;
}

/**
 * Preset validation rules
 */
export const ValidationPresets = {
  /** Standard hotkey rules (2-5 keys, must include at least one modifier) */
  standard: (): KeyValidationRules => ({
    minKeys: 2,
    maxKeys: 5,
    requireModifier: true,
    requireModifierForMultiKey: true,
  }),

  /** Custom rules */
  custom: (rules: Partial<KeyValidationRules>): KeyValidationRules => ({
    minKeys: 1,
    maxKeys: 5,
    requireModifier: false,
    requireModifierForMultiKey: false,
    ...rules,
  }),
};

/**
 * Validate that a key combination is allowed with default rules
 */
export function validateKeyCombination(shortcut: string): { valid: boolean; error?: string } {
  return validateKeyCombinationWithRules(shortcut, ValidationPresets.standard());
}

/**
 * Validate that a key combination is allowed with custom rules
 */
export function validateKeyCombinationWithRules(
  shortcut: string,
  rules: KeyValidationRules,
): { valid: boolean; error?: string } {
  const parts = shortcut.split("+").filter(Boolean);

  // Check minimum keys
  if (parts.length < rules.minKeys) {
    return { valid: false, error: `Minimum ${rules.minKeys} key(s) required` };
  }

  // Check maximum keys
  if (parts.length > rules.maxKeys) {
    return { valid: false, error: `Maximum ${rules.maxKeys} keys allowed in combination` };
  }

  // Check modifier requirements
  const hasModifier = parts.some((key) => MODIFIER_KEYS[key]);

  if (rules.requireModifier && !hasModifier) {
    return { valid: false, error: "At least one modifier key is required" };
  }

  // Check that the shortcut starts with a modifier
  if (rules.requireModifier && parts.length > 0 && !MODIFIER_KEYS[parts[0]]) {
    return {
      valid: false,
      error: "Keyboard shortcuts must start with a modifier key (Cmd/Ctrl, Alt, or Shift)",
    };
  }

  if (rules.requireModifierForMultiKey && !hasModifier && parts.length > 1) {
    return { valid: false, error: "Multi-key shortcuts must include at least one modifier key" };
  }

  // Check that at least one key is NOT a modifier (library requirement)
  const hasNonModifier = parts.some((key) => !MODIFIER_KEYS[key]);
  if (!hasNonModifier) {
    return {
      valid: false,
      error:
        "Hotkey must include at least one non-modifier key (e.g., a letter, number, or function key)",
    };
  }

  // Validate each key
  for (const key of parts) {
    if (!isValidKey(key)) {
      return { valid: false, error: `Invalid key: ${key}` };
    }
  }

  return { valid: true };
}

/**
 * Check if a key string is valid
 */
function isValidKey(key: string): boolean {
  // Empty keys are invalid
  if (!key || key.length === 0) {
    return false;
  }

  // ESC/Escape key is allowed (backend handles validation for recording cancellation)
  const normalizedKey = key.toLowerCase();
  if (normalizedKey === "escape" || normalizedKey === "esc") {
    return true; // Let backend handle the validation context
  }

  // Allow any non-empty string as a potential key
  // The actual validation will happen when we try to register the shortcut
  // This allows for maximum flexibility with different keyboard layouts,
  // media keys, numpad keys, international keys, etc.
  return true;
}

/**
 * Check if a key is a single modifier key (not supported by OS for global shortcuts)
 */
export function isSingleModifierKey(key: string): boolean {
  const singleModifiers = [
    "Alt",
    "Shift",
    "Control",
    "Ctrl",
    "Command",
    "Cmd",
    "Option",
    "Meta",
    "Super",
    "LeftAlt",
    "RightAlt",
    "LeftShift",
    "RightShift",
    "LeftControl",
    "RightControl",
    "LeftMeta",
    "RightMeta",
  ];

  return singleModifiers.some((m) => m.toLowerCase() === key.toLowerCase());
}

/**
 * Format a key for display (returns the display symbol)
 * Note: This function now accepts platform as parameter for consistent behavior
 */
export function formatKeyForDisplay(key: string, isMac: boolean = false): string {
  const displayMap: Record<string, string> = {
    CommandOrControl: isMac ? "⌘" : "Ctrl",
    Super: isMac ? "⌘" : "Win", // Super is Command on Mac, Windows key on PC
    Cmd: isMac ? "⌘" : "Ctrl",
    Ctrl: isMac ? "⌃" : "Ctrl", // Use proper Control symbol on Mac
    Control: isMac ? "⌃" : "Ctrl",
    Command: isMac ? "⌘" : "Ctrl",
    Shift: isMac ? "⇧" : "Shift",
    Alt: isMac ? "⌥" : "Alt",
    LeftAlt: isMac ? "⌥" : "Alt",
    RightAlt: isMac ? "⌥" : "Alt",
    Option: isMac ? "⌥" : "Alt",
    Meta: isMac ? "⌘" : "Ctrl",
    Enter: isMac ? "⏎" : "Enter",
    Return: isMac ? "⏎" : "Enter",
    Tab: isMac ? "⇥" : "Tab",
    Backspace: isMac ? "⌫" : "Backspace",
    Delete: isMac ? "⌦" : "Delete",
    Space: isMac ? "␣" : "Space",
    Escape: "Esc",
    ArrowUp: "↑",
    ArrowDown: "↓",
    ArrowLeft: "←",
    ArrowRight: "→",
    Up: "↑",
    Down: "↓",
    Left: "←",
    Right: "→",
    PageUp: "⇞",
    PageDown: "⇟",
    Home: "⇱",
    End: "⇲",
    // Punctuation and symbols
    Slash: "/",
    Backslash: "\\",
    Period: ".",
    Comma: ",",
    Semicolon: ";",
    Quote: "'",
    BracketLeft: "[",
    BracketRight: "]",
    Minus: "-",
    Equal: "=",
    Plus: "+",
    Backquote: "`",
    // Numpad keys
    Numpad0: "Num0",
    Numpad1: "Num1",
    Numpad2: "Num2",
    Numpad3: "Num3",
    Numpad4: "Num4",
    Numpad5: "Num5",
    Numpad6: "Num6",
    Numpad7: "Num7",
    Numpad8: "Num8",
    Numpad9: "Num9",
    NumpadMultiply: "Num*",
    NumpadAdd: "Num+",
    NumpadSubtract: "Num-",
    NumpadDecimal: "Num.",
    NumpadDivide: "Num/",
    NumpadEnter: "NumEnter",
    // Lock keys
    CapsLock: "CapsLock",
    NumLock: "NumLock",
    ScrollLock: "ScrollLock",
    // Media keys
    AudioVolumeUp: "Vol+",
    AudioVolumeDown: "Vol-",
    AudioVolumeMute: "Mute",
    MediaPlayPause: "Play/Pause",
    MediaStop: "Stop",
    MediaTrackNext: "Next",
    MediaTrackPrevious: "Previous",
    // System keys
    PrintScreen: "PrtScn",
    Insert: "Ins",
    Pause: "Pause",
    ContextMenu: "Menu",
  };

  return displayMap[key] || key;
}
