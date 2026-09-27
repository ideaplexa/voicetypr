import { renderHook, act } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import { normalizeTheme, useTheme } from "./useTheme";

// Mutable settings snapshot the mocked context reads on every render.
const settingsRef: { current: { theme?: string } | null } = { current: null };

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({ settings: settingsRef.current }),
}));

type MediaChangeListener = (event: { matches: boolean }) => void;

interface MockMediaQueryList {
  matches: boolean;
  media: string;
  onchange: null;
  addEventListener: Mock<(type: string, listener: MediaChangeListener) => void>;
  removeEventListener: Mock<(type: string, listener: MediaChangeListener) => void>;
  addListener: Mock<() => void>;
  removeListener: Mock<() => void>;
  dispatchEvent: Mock<() => boolean>;
}

/**
 * Install a controllable matchMedia mock. Returns helpers to read the query
 * list and to emit a `change` event (updating `matches` first, like the real
 * MediaQueryList does).
 */
function installMatchMedia(initialMatches: boolean) {
  const listeners = new Set<MediaChangeListener>();
  const media: MockMediaQueryList = {
    matches: initialMatches,
    media: "(prefers-color-scheme: dark)",
    onchange: null,
    addEventListener: vi.fn((type: string, cb: MediaChangeListener) => {
      if (type === "change") listeners.add(cb);
    }),
    removeEventListener: vi.fn((type: string, cb: MediaChangeListener) => {
      if (type === "change") listeners.delete(cb);
    }),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  };
  window.matchMedia = vi.fn().mockReturnValue(media);

  return {
    media,
    emitChange: (nextMatches: boolean) => {
      media.matches = nextMatches;
      for (const cb of listeners) cb({ matches: nextMatches });
    },
  };
}

beforeEach(() => {
  settingsRef.current = null;
  document.documentElement.classList.remove("dark");
});

describe("normalizeTheme", () => {
  it("accepts the known theme values", () => {
    expect(normalizeTheme("system")).toBe("system");
    expect(normalizeTheme("light")).toBe("light");
    expect(normalizeTheme("dark")).toBe("dark");
  });

  it("guards unknown or missing values to system", () => {
    expect(normalizeTheme("neon")).toBe("system");
    expect(normalizeTheme("")).toBe("system");
    expect(normalizeTheme(undefined)).toBe("system");
    expect(normalizeTheme(null)).toBe("system");
  });
});

describe("useTheme", () => {
  it("adds .dark when theme is dark", () => {
    settingsRef.current = { theme: "dark" };
    renderHook(() => useTheme());
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("removes .dark when theme is light, even with a dark OS", () => {
    installMatchMedia(true);
    document.documentElement.classList.add("dark");

    settingsRef.current = { theme: "light" };
    renderHook(() => useTheme());
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("behaves as system when settings are missing (default)", () => {
    const { emitChange } = installMatchMedia(false);
    renderHook(() => useTheme());
    expect(document.documentElement.classList.contains("dark")).toBe(false);

    act(() => emitChange(true));
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("follows the OS in system mode and updates on media change", () => {
    const { emitChange } = installMatchMedia(false);
    settingsRef.current = { theme: "system" };
    renderHook(() => useTheme());
    expect(document.documentElement.classList.contains("dark")).toBe(false);

    act(() => emitChange(true));
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    act(() => emitChange(false));
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("ignores media changes when theme is explicit light", () => {
    const { emitChange } = installMatchMedia(false);
    settingsRef.current = { theme: "light" };
    renderHook(() => useTheme());
    expect(document.documentElement.classList.contains("dark")).toBe(false);

    act(() => emitChange(true));
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("re-evaluates when the theme preference changes and re-subscribes in system mode", () => {
    const { emitChange } = installMatchMedia(false);
    settingsRef.current = { theme: "system" };
    const { rerender } = renderHook(() => useTheme());
    expect(document.documentElement.classList.contains("dark")).toBe(false);

    act(() => emitChange(true));
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    // Switching to light removes the class and stops following the OS.
    settingsRef.current = { theme: "light" };
    rerender();
    expect(document.documentElement.classList.contains("dark")).toBe(false);

    act(() => emitChange(true));
    expect(document.documentElement.classList.contains("dark")).toBe(false);

    // Switching back to system re-subscribes and re-applies.
    settingsRef.current = { theme: "system" };
    rerender();
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    act(() => emitChange(false));
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("unsubscribes from matchMedia on unmount", () => {
    const { media, emitChange } = installMatchMedia(false);
    settingsRef.current = { theme: "system" };
    const { unmount } = renderHook(() => useTheme());
    expect(media.addEventListener).toHaveBeenCalledWith("change", expect.any(Function));

    unmount();
    expect(media.removeEventListener).toHaveBeenCalledWith(
      "change",
      media.addEventListener.mock.calls[0][1],
    );

    // No listeners remain, so the class must not change after unmount.
    act(() => emitChange(true));
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });
});
