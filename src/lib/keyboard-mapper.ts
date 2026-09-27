/**
 * Keyboard code-to-key mapper for physical key position support
 * Maps e.code values to normalized key names for cross-layout compatibility
 */

// Physical key code to normalized key mapping
// Using e.code ensures the same physical key works on all keyboard layouts
export const CODE_TO_KEY_MAP: Record<string, string> = {
  // Letters (physical positions - same on all keyboards!)
  KeyA: "A",
  KeyB: "B",
  KeyC: "C",
  KeyD: "D",
  KeyE: "E",
  KeyF: "F",
  KeyG: "G",
  KeyH: "H",
  KeyI: "I",
  KeyJ: "J",
  KeyK: "K",
  KeyL: "L",
  KeyM: "M",
  KeyN: "N",
  KeyO: "O",
  KeyP: "P",
  KeyQ: "Q",
  KeyR: "R",
  KeyS: "S",
  KeyT: "T",
  KeyU: "U",
  KeyV: "V",
  KeyW: "W",
  KeyX: "X",
  KeyY: "Y",
  KeyZ: "Z",

  // Numbers (top row)
  Digit0: "0",
  Digit1: "1",
  Digit2: "2",
  Digit3: "3",
  Digit4: "4",
  Digit5: "5",
  Digit6: "6",
  Digit7: "7",
  Digit8: "8",
  Digit9: "9",

  // Punctuation (physical positions - same on all keyboards!)
  Comma: "Comma", // Where "," and "<" are on US keyboard
  Period: "Period", // Where "." and ">" are
  Semicolon: "Semicolon", // Where ";" and ":" are
  Quote: "Quote", // Where "'" and '"' are
  BracketLeft: "BracketLeft", // Where "[" and "{" are
  BracketRight: "BracketRight", // Where "]" and "}" are
  Backslash: "Backslash", // Where "\\" and "|" are
  Slash: "Slash", // Where "/" and "?" are
  Equal: "Equal", // Where "=" and "+" are
  Minus: "Minus", // Where "-" and "_" are
  Backquote: "Backquote", // Where "`" and "~" are

  // Special keys
  Space: "Space",
  Enter: "Enter",
  Tab: "Tab",
  Backspace: "Backspace",
  Delete: "Delete",
  Escape: "Escape",
  CapsLock: "CapsLock",
  NumLock: "NumLock",
  ScrollLock: "ScrollLock",
  PrintScreen: "PrintScreen",
  Pause: "Pause",
  Insert: "Insert",

  // Function keys
  F1: "F1",
  F2: "F2",
  F3: "F3",
  F4: "F4",
  F5: "F5",
  F6: "F6",
  F7: "F7",
  F8: "F8",
  F9: "F9",
  F10: "F10",
  F11: "F11",
  F12: "F12",
  F13: "F13",
  F14: "F14",
  F15: "F15",
  F16: "F16",
  F17: "F17",
  F18: "F18",
  F19: "F19",
  F20: "F20",
  F21: "F21",
  F22: "F22",
  F23: "F23",
  F24: "F24",

  // Navigation
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",

  // Numpad
  Numpad0: "Numpad0",
  Numpad1: "Numpad1",
  Numpad2: "Numpad2",
  Numpad3: "Numpad3",
  Numpad4: "Numpad4",
  Numpad5: "Numpad5",
  Numpad6: "Numpad6",
  Numpad7: "Numpad7",
  Numpad8: "Numpad8",
  Numpad9: "Numpad9",
  NumpadAdd: "NumpadAdd",
  NumpadSubtract: "NumpadSubtract",
  NumpadMultiply: "NumpadMultiply",
  NumpadDivide: "NumpadDivide",
  NumpadDecimal: "NumpadDecimal",
  NumpadEnter: "NumpadEnter",
  NumpadEqual: "NumpadEqual",

  // Media keys
  AudioVolumeUp: "AudioVolumeUp",
  AudioVolumeDown: "AudioVolumeDown",
  AudioVolumeMute: "AudioVolumeMute",
  MediaPlayPause: "MediaPlayPause",
  MediaStop: "MediaStop",
  MediaTrackNext: "MediaTrackNext",
  MediaTrackPrevious: "MediaTrackPrevious",

  // Modifier keys (usually not used as standalone keys in hotkeys)
  ShiftLeft: "Shift",
  ShiftRight: "Shift",
  ControlLeft: "Control",
  ControlRight: "Control",
  AltLeft: "Alt",
  AltRight: "Alt",
  MetaLeft: "Super",
  MetaRight: "Super",
};

/**
 * Convert a physical key code to normalized key name
 * @param code - The e.code value from KeyboardEvent
 * @returns Normalized key name for Tauri
 */
export function mapCodeToKey(code: string): string {
  return CODE_TO_KEY_MAP[code] || code; // Fallback to code if not mapped
}
