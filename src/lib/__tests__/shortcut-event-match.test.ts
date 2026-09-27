import { describe, expect, it, vi } from "vitest";

// Toggle the macOS flag read by the matcher without re-importing.
const platform = vi.hoisted(() => ({ isMacOS: false }));
vi.mock("@/lib/platform", () => ({
  get isMacOS() {
    return platform.isMacOS;
  },
  get isWindows() {
    return !platform.isMacOS;
  },
}));

import { eventMatchesShortcut, eventToShortcut } from "@/lib/shortcut-event-match";

function keydown(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent("keydown", init);
}

describe("eventToShortcut", () => {
  it("builds the canonical combo string on Windows (Ctrl → CommandOrControl)", () => {
    platform.isMacOS = false;
    expect(eventToShortcut(keydown({ ctrlKey: true, code: "Space", key: " " }))).toBe(
      "CommandOrControl+Space",
    );
  });

  it("orders modifiers as CommandOrControl+Alt+Shift+Key", () => {
    platform.isMacOS = false;
    expect(
      eventToShortcut(
        keydown({ ctrlKey: true, shiftKey: true, altKey: true, code: "KeyK", key: "k" }),
      ),
    ).toBe("CommandOrControl+Alt+Shift+K");
  });

  it("maps macOS Command to CommandOrControl and Control separately", () => {
    platform.isMacOS = true;
    expect(eventToShortcut(keydown({ metaKey: true, code: "Space", key: " " }))).toBe(
      "CommandOrControl+Space",
    );
    expect(eventToShortcut(keydown({ metaKey: true, ctrlKey: true, code: "KeyK", key: "k" }))).toBe(
      "CommandOrControl+Control+K",
    );
    platform.isMacOS = false;
  });

  it("uppercases single-character keys via physical code", () => {
    expect(eventToShortcut(keydown({ ctrlKey: true, code: "KeyA", key: "a" }))).toBe(
      "CommandOrControl+A",
    );
  });

  it("returns null for a bare modifier press", () => {
    expect(
      eventToShortcut(keydown({ ctrlKey: true, key: "Control", code: "ControlLeft" })),
    ).toBeNull();
  });
});

describe("eventMatchesShortcut", () => {
  it("matches Ctrl+Space against the stored CommandOrControl+Space", () => {
    platform.isMacOS = false;
    expect(
      eventMatchesShortcut(
        keydown({ ctrlKey: true, code: "Space", key: " " }),
        "CommandOrControl+Space",
      ),
    ).toBe(true);
  });

  it("does not match a bare Space (missing the modifier)", () => {
    expect(
      eventMatchesShortcut(keydown({ code: "Space", key: " " }), "CommandOrControl+Space"),
    ).toBe(false);
  });

  it("requires the exact modifier set (Ctrl+Space != Ctrl+Shift+Space)", () => {
    expect(
      eventMatchesShortcut(
        keydown({ ctrlKey: true, code: "Space", key: " " }),
        "CommandOrControl+Shift+Space",
      ),
    ).toBe(false);
  });

  it("normalizes Enter/Return so they compare equal", () => {
    expect(
      eventMatchesShortcut(
        keydown({ ctrlKey: true, code: "Enter", key: "Enter" }),
        "CommandOrControl+Return",
      ),
    ).toBe(true);
  });

  it("returns false for an empty configured hotkey", () => {
    expect(eventMatchesShortcut(keydown({ ctrlKey: true, code: "Space", key: " " }), "")).toBe(
      false,
    );
  });
});
