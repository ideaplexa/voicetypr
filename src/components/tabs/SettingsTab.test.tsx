import { render } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { SettingsTab } from "./SettingsTab";

// Mock sonner
vi.mock("sonner", () => ({
  toast: {
    warning: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

// Mock contexts
vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: {
      hotkey: "Cmd+Shift+Space",
      current_language: "en",
      auto_launch: true,
    },
    updateSettings: vi.fn().mockResolvedValue(true),
    refreshSettings: vi.fn(),
  }),
}));

// Mock hooks
vi.mock("@/hooks/useEventCoordinator", () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn((event: string, callback: any) => {
      (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
      (window as any).__testEventCallbacks[event] = callback;
      return vi.fn();
    }),
  }),
}));

// Mock GeneralSettings component
vi.mock("@/components/sections/GeneralSettings", () => ({
  GeneralSettings: () => (
    <div data-testid="general-settings">
      <div>Hotkey: Cmd+Shift+Space</div>
      <div>Language: en</div>
    </div>
  ),
}));

describe("SettingsTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
  });

  it("shows warning toast for no-speech-detected event", async () => {
    const { toast } = await import("sonner");
    render(<SettingsTab />);

    const callback = (window as any).__testEventCallbacks["no-speech-detected"];
    callback({ severity: "warning", message: "No speech detected" });

    expect(toast.warning).toHaveBeenCalledWith(
      "No Speech Detected",
      expect.objectContaining({
        description: expect.stringContaining("No speech detected"),
      }),
    );
  });

  it("shows error toast for hotkey registration failure", async () => {
    const { toast } = await import("sonner");
    render(<SettingsTab />);

    const callback = (window as any).__testEventCallbacks["hotkey-registration-failed"];
    callback({
      hotkey: "Cmd+Shift+X",
      suggestion: "The hotkey is already in use",
      error: "Conflict detected",
    });

    expect(toast.error).toHaveBeenCalledWith(
      "Hotkey Registration Failed",
      expect.objectContaining({
        description: "The hotkey is already in use",
      }),
    );
  });
});
