import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RecordingSettings } from "../RecordingSettings";
import type { AppSettings } from "@/types";
import type { ShortcutBinding } from "@/types/shortcuts";

const mockUpdateSettings = vi.fn().mockResolvedValue(undefined);

// A bare-modifier primary is active: `hotkey` is intentionally empty.
const baseSettings: AppSettings = {
  recording_mode: "toggle",
  hotkey: "",
  current_model: "",
  speech_language: "en",
  theme: "system",
  keep_transcription_in_clipboard: false,
  play_sound_on_recording: true,
  pill_indicator_mode: "when_recording",
  pill_indicator_position: "bottom-center",
  pill_indicator_offset: 10,
};
let mockSettings: AppSettings = { ...baseSettings };

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({ settings: mockSettings, updateSettings: mockUpdateSettings }),
}));
vi.mock("@/contexts/ReadinessContext", () => ({ useCanAutoInsert: () => true }));
vi.mock("@/lib/platform", () => ({ isMacOS: true, isWindows: false }));

// Capture the inline HotkeyInput's onChange so we can drive a combo entry.
const hotkeyInput = vi.hoisted(() => ({
  onChange: null as null | ((v: string) => void),
}));
vi.mock("@/components/HotkeyInput", () => ({
  HotkeyInput: ({ onChange }: { onChange?: (v: string) => void }) => {
    hotkeyInput.onChange = onChange ?? null;
    return <div data-testid="hotkey-input" />;
  },
}));

const mockInvoke = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => mockInvoke(cmd, args),
}));
vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn(),
  disable: vi.fn(),
  isEnabled: vi.fn(),
}));

// Keep the DOM focused on the hotkey flow.
vi.mock("@/components/MicrophoneSelection", () => ({
  MicrophoneSelection: () => <div data-testid="microphone-selection" />,
}));
vi.mock("@/components/sections/NetworkSharingCard", () => ({
  NetworkSharingCard: () => <div data-testid="network-sharing-card" />,
}));
vi.mock("@/components/ui/scroll-area", () => ({
  ScrollArea: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));
vi.mock("@/components/ui/switch", () => ({
  Switch: (props: { checked?: boolean; onCheckedChange?: (v: boolean) => void; id?: string }) => (
    <button
      type="button"
      role="switch"
      aria-checked={props.checked}
      aria-label={props.id}
      data-testid={props.id}
      onClick={() => props.onCheckedChange?.(!props.checked)}
    />
  ),
}));
vi.mock("@/components/ui/toggle-group", () => ({
  ToggleGroup: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  ToggleGroupItem: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));
vi.mock("@/components/ui/select", () => ({
  Select: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectTrigger: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectItem: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectValue: () => <div />,
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() },
}));

const nativePrimary: ShortcutBinding = {
  id: "onboarding-primary-hold",
  action: "hold_to_record",
  shortcut: "",
  trigger: "hold",
  enabled: true,
  allow_risky_combo: false,
  trigger_kind: "modifier_hold",
  modifier: { modifier: "meta", side: "right" },
};
const cancelBinding: ShortcutBinding = {
  id: "cancel-recording",
  action: "cancel_recording",
  shortcut: "Escape",
  trigger: "pressed",
  enabled: true,
  allow_risky_combo: false,
  trigger_kind: "combo",
};

/** Runtime-narrow the `update_shortcut_settings` payload to its bindings list. */
function readBindings(args: unknown): ShortcutBinding[] {
  if (args && typeof args === "object" && "settings" in args) {
    const settings = args.settings;
    if (settings && typeof settings === "object" && "bindings" in settings) {
      const bindings = settings.bindings;
      if (Array.isArray(bindings)) {
        return bindings;
      }
    }
  }
  throw new Error("update_shortcut_settings was not called with { settings: { bindings } }");
}

describe("GeneralSettings combo-hotkey save", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    mockInvoke.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "get_autostart_status":
          return false;
        case "get_transcription_acceleration_status":
          return {
            message: "ok",
            effective_backend: "cpu",
            diagnostic_code: "ready",
            recommended_action: "none",
          };
        case "get_shortcut_settings":
          return { bindings: [nativePrimary, cancelBinding] };
        case "set_global_shortcut":
          return undefined;
        case "update_shortcut_settings":
          return { bindings: [nativePrimary, cancelBinding] };
        default:
          return undefined;
      }
    });
  });

  it("disables the existing native primary when saving a combo", async () => {
    render(<RecordingSettings />);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    });

    // Simulate the user entering a combo via the inline HotkeyInput.
    await act(async () => {
      hotkeyInput.onChange?.("CommandOrControl+Shift+Space");
    });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Save" }));
    });

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        hotkey: "CommandOrControl+Shift+Space",
      });
    });

    const calls = mockInvoke.mock.calls;
    const setGlobal = calls.find(([cmd]) => cmd === "set_global_shortcut");
    expect(setGlobal, "combo should be registered as the global shortcut").toBeDefined();
    expect(setGlobal?.[1]).toEqual({ shortcut: "CommandOrControl+Shift+Space" });

    const updateCall = calls.find(([cmd]) => cmd === "update_shortcut_settings");
    expect(
      updateCall,
      "saving a combo must replace the native primary so only one trigger fires",
    ).toBeDefined();

    const bindings = readBindings(updateCall?.[1]);
    const primary = bindings.find((b) => b.id === "onboarding-primary-hold");
    expect(primary?.enabled).toBe(false);
    // Other bindings must be left intact.
    const cancel = bindings.find((b) => b.id === "cancel-recording");
    expect(cancel?.enabled).toBe(true);
  });
});
