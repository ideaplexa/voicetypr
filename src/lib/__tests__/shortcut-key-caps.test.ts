import { describe, expect, it } from "vitest";
import { shortcutKeyCaps } from "../shortcut-key-caps";

describe("shortcutKeyCaps", () => {
  it("orders Mac modifiers and spells out Space", () => {
    expect(shortcutKeyCaps("Shift+Alt+CommandOrControl+Control+Space", "darwin")).toEqual(["⌘", "⌥", "⌃", "⇧", "Space"]);
  });
  it("uses Windows words and retains letters and function keys", () => {
    expect(shortcutKeyCaps("Shift+Super+Alt+Control+q", "windows")).toEqual(["Ctrl", "Alt", "Shift", "Q"]);
    expect(shortcutKeyCaps("Control+F12", "windows")).toEqual(["Ctrl", "F12"]);
  });
  it("supports bare modifiers and no configured shortcut", () => {
    expect(shortcutKeyCaps("Right Alt", "darwin")).toEqual(["Right ⌥"]);
    expect(shortcutKeyCaps("Alt", "windows")).toEqual(["Alt"]);
    expect(shortcutKeyCaps("", "darwin")).toEqual([]);
  });
  it("formats backend aliases as the keys actually registered", () => {
    expect(shortcutKeyCaps("Super+K", "windows")).toEqual(["Ctrl", "K"]);
    expect(shortcutKeyCaps("Meta+K", "windows")).toEqual(["Ctrl", "K"]);
    expect(shortcutKeyCaps("Ctrl+K", "darwin")).toEqual(["⌘", "K"]);
    expect(shortcutKeyCaps("Control+K", "darwin")).toEqual(["⌃", "K"]);
  });
  it("uses established display labels for punctuation and numpad keys", () => {
    expect(shortcutKeyCaps("Control+Slash", "windows")).toEqual(["Ctrl", "/"]);
    expect(shortcutKeyCaps("Control+NumpadAdd", "windows")).toEqual(["Ctrl", "Num+"]);
  });
});
