import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GeneralSettings } from "../GeneralSettings";

const mockUpdateSettings = vi.fn().mockResolvedValue(undefined);
const mockInvoke = vi.fn().mockResolvedValue(false);
const baseSettings = {
  recording_mode: "toggle",
  hotkey: "CommandOrControl+Shift+Space",
  keep_transcription_in_clipboard: false,
  play_sound_on_recording: true,
  pill_indicator_mode: "when_recording",
  pill_indicator_position: "bottom-center",
  pill_indicator_offset: 10,
};

let mockSettings = { ...baseSettings };

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: mockSettings,
    updateSettings: mockUpdateSettings,
  }),
}));

vi.mock("@/contexts/ReadinessContext", () => ({
  useCanAutoInsert: () => true,
}));

vi.mock("@/lib/platform", () => ({
  isMacOS: false,
  isWindows: false,
}));

// Mock invoke — the new backend commands replace the autostart plugin
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// The autostart plugin should NO LONGER be imported by the component.
// We still mock it here so that if it is accidentally imported,
// the import won't crash — but our tests assert it is never called.
vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn(),
  disable: vi.fn(),
  isEnabled: vi.fn(),
}));

vi.mock("@/components/HotkeyInput", () => ({
  HotkeyInput: () => <div data-testid="hotkey-input" />,
}));

vi.mock("@/components/ui/scroll-area", () => ({
  ScrollArea: ({ children }: { children: any }) => <div>{children}</div>,
}));

vi.mock("@/components/ui/switch", () => ({
  Switch: ({
    checked,
    onCheckedChange,
    disabled,
    id,
  }: {
    checked?: boolean;
    onCheckedChange?: (v: boolean) => void;
    disabled?: boolean;
    id?: string;
  }) => (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={id}
      data-testid={id}
      data-disabled={disabled}
      onClick={() => onCheckedChange?.(!checked)}
    />
  ),
}));

vi.mock("@/components/ui/toggle-group", () => ({
  ToggleGroup: ({ children }: { children: any }) => <div>{children}</div>,
  ToggleGroupItem: ({ children }: { children: any }) => <div>{children}</div>,
}));

vi.mock("@/components/ui/select", () => ({
  Select: ({
    children,
    value,
    onValueChange,
  }: {
    children: any;
    value?: string;
    onValueChange?: (v: string) => void;
  }) => (
    <div data-testid="select" data-value={value} onClick={() => onValueChange?.("top-center")}>
      {children}
    </div>
  ),
  SelectTrigger: ({ children }: { children: any }) => (
    <div data-testid="select-trigger">{children}</div>
  ),
  SelectContent: ({ children }: { children: any }) => <div>{children}</div>,
  SelectItem: ({ children, value }: { children: any; value: string }) => (
    <div data-testid={`select-item-${value}`}>{children}</div>
  ),
  SelectValue: () => <div data-testid="select-value" />,
}));

vi.mock("@/components/MicrophoneSelection", () => ({
  MicrophoneSelection: () => <div data-testid="microphone-selection" />,
}));

vi.mock("../NetworkSharingCard", () => ({
  NetworkSharingCard: () => <div data-testid="network-sharing-card" />,
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

describe("GeneralSettings autostart via backend commands", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    // Default: backend reports autostart disabled
    mockInvoke.mockResolvedValue(false);
  });

  it("calls get_autostart_status on mount and renders switch off", async () => {
    mockInvoke.mockResolvedValue(false);

    render(<GeneralSettings />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_autostart_status");
    });

    const autostartSwitch = screen.getByRole("switch", { name: /autostart/i });
    expect(autostartSwitch).toHaveAttribute("aria-checked", "false");
  });

  it("renders switch on when backend reports autostart enabled", async () => {
    mockInvoke.mockResolvedValue(true);

    render(<GeneralSettings />);

    await waitFor(() => {
      const autostartSwitch = screen.getByRole("switch", { name: /autostart/i });
      expect(autostartSwitch).toHaveAttribute("aria-checked", "true");
    });
  });

  it("calls set_autostart when toggled and updates UI from response", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_autostart_status") {
        return Promise.resolve(false);
      }
      if (command === "set_autostart") {
        return Promise.resolve(true);
      }
      return Promise.resolve(false);
    });

    render(<GeneralSettings />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_autostart_status");
    });

    const autostartSwitch = screen.getByRole("switch", { name: /autostart/i });
    await act(async () => {
      fireEvent.click(autostartSwitch);
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_autostart", {
        enabled: true,
      });
    });

    // UI should reflect the backend response (true)
    await waitFor(() => {
      expect(screen.getByRole("switch", { name: /autostart/i })).toHaveAttribute(
        "aria-checked",
        "true",
      );
    });

    // Settings should be updated with the actual backend state
    expect(mockUpdateSettings).toHaveBeenCalledWith({
      launch_at_startup: true,
    });
  });

  it("shows correct UI when backend returns different state than requested", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_autostart_status") {
        return Promise.resolve(false);
      }
      if (command === "set_autostart") {
        // User requests enable, but backend returns false (OS failure)
        return Promise.resolve(false);
      }
      return Promise.resolve(false);
    });

    render(<GeneralSettings />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_autostart_status");
    });

    const autostartSwitch = screen.getByRole("switch", { name: /autostart/i });
    await act(async () => {
      fireEvent.click(autostartSwitch);
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_autostart", {
        enabled: true,
      });
    });

    // UI should show the actual backend state (false), not the requested state (true)
    await waitFor(() => {
      expect(screen.getByRole("switch", { name: /autostart/i })).toHaveAttribute(
        "aria-checked",
        "false",
      );
    });

    // Settings should reflect actual state
    expect(mockUpdateSettings).toHaveBeenCalledWith({
      launch_at_startup: false,
    });
  });

  it("does not call autostart plugin directly", async () => {
    // Import the mocked plugin to verify it was never called
    const autostartPlugin = await import("@tauri-apps/plugin-autostart");

    render(<GeneralSettings />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_autostart_status");
    });

    expect(autostartPlugin.enable).not.toHaveBeenCalled();
    expect(autostartPlugin.disable).not.toHaveBeenCalled();
    expect(autostartPlugin.isEnabled).not.toHaveBeenCalled();
  });
});
