import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ShortcutBinding } from "@/types/shortcuts";

// Control the macOS flag read by `formatModifierLabel` without re-importing.
const platform = vi.hoisted(() => ({ isMacOS: true }));
vi.mock("@/lib/platform", () => ({
  get isMacOS() {
    return platform.isMacOS;
  },
}));

import {
  findActivePrimaryBinding,
  formatModifierLabel,
  formatPrimaryHotkeyLabel,
} from "@/lib/shortcut-display";

/** Build a native (engine-kind) binding with sensible defaults. */
function binding(over: Partial<ShortcutBinding> = {}): ShortcutBinding {
  return {
    id: "onboarding-primary-hold",
    action: "hold_to_record",
    shortcut: "",
    trigger: "hold",
    enabled: true,
    allow_risky_combo: false,
    trigger_kind: "modifier_hold",
    modifier: { modifier: "meta", side: "right" },
    ...over,
  };
}

describe("findActivePrimaryBinding", () => {
  it("returns the onboarding hold binding by id", () => {
    const primary = binding({ id: "onboarding-primary-hold" });
    const other = binding({
      id: "other-hold",
      action: "hold_to_record",
      trigger_kind: "modifier_hold",
    });
    expect(findActivePrimaryBinding([other, primary])).toBe(primary);
  });

  it("falls back to the first enabled engine-kind recording binding", () => {
    const hold = binding({
      id: "custom-hold",
      action: "hold_to_record",
      trigger_kind: "modifier_hold",
    });
    expect(findActivePrimaryBinding([hold])).toBe(hold);
  });

  it("ignores disabled bindings", () => {
    const disabled = binding({ id: "disabled-hold", enabled: false });
    expect(findActivePrimaryBinding([disabled])).toBeNull();
  });

  it("ignores combo bindings (those live in settings.hotkey)", () => {
    const combo = binding({
      id: "combo-trigger",
      action: "toggle_recording",
      trigger_kind: "combo",
      modifier: null,
    });
    expect(findActivePrimaryBinding([combo])).toBeNull();
  });

  it("returns null when nothing matches", () => {
    expect(findActivePrimaryBinding([])).toBeNull();
  });
});

describe("formatPrimaryHotkeyLabel", () => {
  beforeEach(() => {
    platform.isMacOS = true;
  });

  it("formats the combo string when a hotkey is set", () => {
    expect(formatPrimaryHotkeyLabel(null, "CommandOrControl+Shift+Space")).toBe("⌘+⇧+␣");
    platform.isMacOS = false;
    expect(formatPrimaryHotkeyLabel(null, "CommandOrControl+Shift+Space")).toBe("Ctrl+Shift+Space");
  });

  it("formats a modifier-hold primary as a hold-to-talk phrase", () => {
    expect(formatPrimaryHotkeyLabel(binding({ trigger_kind: "modifier_hold" }), undefined)).toBe(
      "Hold Right ⌘ to talk",
    );
  });

  it("formats an isolated-tap primary as a tap-to-toggle phrase", () => {
    expect(
      formatPrimaryHotkeyLabel(
        binding({ action: "toggle_recording", trigger: "pressed", trigger_kind: "isolated_tap" }),
        undefined,
      ),
    ).toBe("Tap Right ⌘ to toggle");
  });

  it("prefers the bare-modifier primary over a stale combo hotkey", () => {
    // A bare-modifier primary intentionally has an empty settings.hotkey; even
    // if a stray combo string were passed, the active native binding wins.
    expect(formatPrimaryHotkeyLabel(binding({ trigger_kind: "modifier_hold" }), "")).toBe(
      "Hold Right ⌘ to talk",
    );
  });

  it("returns 'Not set' when neither a primary nor a combo is configured", () => {
    expect(formatPrimaryHotkeyLabel(null, undefined)).toBe("Not set");
    expect(formatPrimaryHotkeyLabel(null, "")).toBe("Not set");
  });
});

describe("formatModifierLabel platform variants", () => {
  it("uses glyph symbols on macOS", () => {
    platform.isMacOS = true;
    expect(formatModifierLabel({ modifier: "alt", side: "either" })).toBe("⌥");
    expect(formatModifierLabel({ modifier: "shift", side: "left" })).toBe("Left ⇧");
  });

  it("uses capitalized words on non-macOS", () => {
    platform.isMacOS = false;
    expect(formatModifierLabel({ modifier: "alt", side: "either" })).toBe("Alt");
    expect(formatModifierLabel({ modifier: "control", side: "right" })).toBe("Right Control");
  });
});
