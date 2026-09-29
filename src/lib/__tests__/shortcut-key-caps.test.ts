import { describe, expect, it } from "vitest";
import { shortcutKeyCaps } from "../shortcut-key-caps";

describe("shortcutKeyCaps", () => {
  it("orders Mac modifiers and spells out Space", () => {
    expect(shortcutKeyCaps("Shift+Alt+CommandOrControl+Control+Space", "darwin")).toEqual(["⌘", "⌥", "⌃", "⇧", "Space"]);
  });
  it("uses Windows words and retains letters and function keys", () => {
    expect(shortcutKeyCaps("Shift+Super+Alt+Control+q", "windows")).toEqual(["Ctrl", "Alt", "Shift", "Win", "Q"]);
    expect(shortcutKeyCaps("Control+F12", "windows")).toEqual(["Ctrl", "F12"]);
  });
  it("supports bare modifiers and no configured shortcut", () => {
    expect(shortcutKeyCaps("Right Alt", "darwin")).toEqual(["Right ⌥"]);
    expect(shortcutKeyCaps("Alt", "windows")).toEqual(["Alt"]);
    expect(shortcutKeyCaps("", "darwin")).toEqual([]);
  });
});
