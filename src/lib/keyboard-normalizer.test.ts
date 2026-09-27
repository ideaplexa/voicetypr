import { describe, it, expect } from "vitest";
import {
  normalizeShortcutKeys,
  validateKeyCombination,
  isSingleModifierKey,
  formatKeyForDisplay,
} from "./keyboard-normalizer";

describe("keyboard-normalizer", () => {
  describe("normalizeShortcutKeys", () => {
    it("should preserve Super and Control as separate modifiers for macOS", () => {
      // This is the critical fix - Super (Command) and Control should remain separate
      expect(normalizeShortcutKeys("Super+Control+Alt+A")).toBe("Super+Control+Alt+A");
      expect(normalizeShortcutKeys("Super+Control+Shift+A")).toBe("Super+Control+Shift+A");
    });

    it("should handle case-insensitive modifiers", () => {
      expect(normalizeShortcutKeys("super+control+a")).toBe("Super+Control+a");
      expect(normalizeShortcutKeys("SUPER+CONTROL+A")).toBe("Super+Control+A");
    });

    it("should map Cmd and Ctrl to CommandOrControl for cross-platform compatibility", () => {
      expect(normalizeShortcutKeys("Cmd+A")).toBe("CommandOrControl+A");
      expect(normalizeShortcutKeys("Ctrl+A")).toBe("CommandOrControl+A");
      expect(normalizeShortcutKeys("cmd+a")).toBe("CommandOrControl+a");
      expect(normalizeShortcutKeys("ctrl+a")).toBe("CommandOrControl+a");
    });

    it("should keep Control as Control (not CommandOrControl)", () => {
      // This was the bug - Control should NOT become CommandOrControl
      expect(normalizeShortcutKeys("Control+A")).toBe("Control+A");
      expect(normalizeShortcutKeys("control+a")).toBe("Control+a");
    });

    it("should keep Super as Super (for macOS Command key)", () => {
      expect(normalizeShortcutKeys("Super+A")).toBe("Super+A");
      expect(normalizeShortcutKeys("super+a")).toBe("Super+a");
    });

    it("should normalize arrow keys", () => {
      expect(normalizeShortcutKeys("ArrowUp")).toBe("Up");
      expect(normalizeShortcutKeys("ArrowDown")).toBe("Down");
      expect(normalizeShortcutKeys("ArrowLeft")).toBe("Left");
      expect(normalizeShortcutKeys("ArrowRight")).toBe("Right");
    });

    it("should normalize Return to Enter", () => {
      expect(normalizeShortcutKeys("Return")).toBe("Enter");
      expect(normalizeShortcutKeys("Shift+Return")).toBe("Shift+Enter");
    });
  });

  describe("validateKeyCombination", () => {
    it("should accept valid modifier+key combinations", () => {
      expect(validateKeyCombination("CommandOrControl+A")).toEqual({ valid: true });
      expect(validateKeyCombination("Super+Control+A")).toEqual({ valid: true });
      expect(validateKeyCombination("Shift+Alt+F1")).toEqual({ valid: true });
    });

    it("should reject modifier-only combinations (library requirement)", () => {
      // This is the key requirement - at least one non-modifier key is required
      const result = validateKeyCombination("Super+Control+Alt");
      expect(result.valid).toBe(false);
      expect(result.error).toContain("at least one non-modifier key");
    });

    it("should reject single keys without modifiers", () => {
      const result = validateKeyCombination("A");
      expect(result.valid).toBe(false);
      expect(result.error).toContain("Minimum 2 key(s) required");
    });

    it("should reject non-modifier multi-key combinations", () => {
      const result = validateKeyCombination("A+B");
      expect(result.valid).toBe(false);
      // The error comes from the earlier check for requiring at least one modifier
      expect(result.error).toContain("At least one modifier key is required");
    });

    it("should reject combinations that do not start with a modifier", () => {
      const result = validateKeyCombination("A+CommandOrControl");
      expect(result.valid).toBe(false);
      expect(result.error).toContain("must start with a modifier key");
    });

    it("should accept complex valid combinations", () => {
      expect(validateKeyCombination("Super+Control+Alt+Shift+A")).toEqual({ valid: true });
      expect(validateKeyCombination("CommandOrControl+Shift+Space")).toEqual({ valid: true });
    });

    it("should reject too many keys", () => {
      const result = validateKeyCombination("Cmd+Shift+Alt+Ctrl+A+B");
      expect(result.valid).toBe(false);
      expect(result.error).toContain("Maximum 5 keys allowed");
    });
  });

  describe("isSingleModifierKey", () => {
    it("should identify single modifier keys", () => {
      expect(isSingleModifierKey("Alt")).toBe(true);
      expect(isSingleModifierKey("Shift")).toBe(true);
      expect(isSingleModifierKey("Control")).toBe(true);
      expect(isSingleModifierKey("Super")).toBe(true);
      expect(isSingleModifierKey("Command")).toBe(true);
      expect(isSingleModifierKey("Cmd")).toBe(true);
      expect(isSingleModifierKey("Ctrl")).toBe(true);
    });

    it("should handle case-insensitive matching", () => {
      expect(isSingleModifierKey("alt")).toBe(true);
      expect(isSingleModifierKey("SHIFT")).toBe(true);
      expect(isSingleModifierKey("control")).toBe(true);
    });

    it("should reject non-modifier keys", () => {
      expect(isSingleModifierKey("A")).toBe(false);
      expect(isSingleModifierKey("Space")).toBe(false);
      expect(isSingleModifierKey("Enter")).toBe(false);
      expect(isSingleModifierKey("F1")).toBe(false);
    });
  });

  describe("formatKeyForDisplay", () => {
    it("should show correct symbols for macOS", () => {
      expect(formatKeyForDisplay("CommandOrControl", true)).toBe("⌘");
      expect(formatKeyForDisplay("Super", true)).toBe("⌘");
      expect(formatKeyForDisplay("Control", true)).toBe("⌃");
      expect(formatKeyForDisplay("Shift", true)).toBe("⇧");
      expect(formatKeyForDisplay("Alt", true)).toBe("⌥");
    });

    it("should show correct text for Windows/Linux", () => {
      expect(formatKeyForDisplay("CommandOrControl", false)).toBe("Ctrl");
      expect(formatKeyForDisplay("Super", false)).toBe("Win");
      expect(formatKeyForDisplay("Control", false)).toBe("Ctrl");
      expect(formatKeyForDisplay("Shift", false)).toBe("Shift");
      expect(formatKeyForDisplay("Alt", false)).toBe("Alt");
    });

    it("should handle special keys", () => {
      expect(formatKeyForDisplay("Enter", true)).toBe("⏎");
      expect(formatKeyForDisplay("Enter", false)).toBe("Enter");
      expect(formatKeyForDisplay("Space", true)).toBe("␣");
      expect(formatKeyForDisplay("Space", false)).toBe("Space");
      expect(formatKeyForDisplay("Escape", true)).toBe("Esc");
      expect(formatKeyForDisplay("Escape", false)).toBe("Esc");
    });
  });
});
