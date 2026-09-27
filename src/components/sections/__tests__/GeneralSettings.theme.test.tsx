import { render, screen, waitFor, act } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GeneralSettings } from "../GeneralSettings";

const mockUpdateSettings = vi.fn().mockResolvedValue(undefined);
const mockInvoke = vi.fn().mockResolvedValue(false);
const baseSettings = {
  recording_mode: "toggle",
  hotkey: "CommandOrControl+Shift+Space",
  theme: "system",
  keep_transcription_in_clipboard: false,
  play_sound_on_recording: true,
  pill_indicator_mode: "when_recording",
  pill_indicator_position: "bottom-center",
  pill_indicator_offset: 10,
};

let mockSettings: Record<string, unknown> = { ...baseSettings };

// Captures every rendered Select so tests can read values and drive changes.
const selectRegistry = vi.hoisted(
  () => [] as Array<{ value?: string; onValueChange?: (v: string) => void }>,
);

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

vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn(),
  disable: vi.fn(),
  isEnabled: vi.fn(),
}));

vi.mock("@/components/HotkeyInput", () => ({
  HotkeyInput: () => <div data-testid="hotkey-input" />,
}));

vi.mock("@/components/ui/scroll-area", () => ({
  ScrollArea: ({ children }: { children: ReactNode }) => <div>{children}</div>,
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
  ToggleGroup: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ToggleGroupItem: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock("@/components/ui/select", () => ({
  Select: ({
    children,
    value,
    onValueChange,
  }: {
    children: ReactNode;
    value?: string;
    onValueChange?: (v: string) => void;
  }) => {
    selectRegistry.push({ value, onValueChange });
    return <div data-testid="select">{children}</div>;
  },
  SelectTrigger: ({
    children,
    "aria-label": ariaLabel,
  }: {
    children: ReactNode;
    "aria-label"?: string;
  }) => (
    <div data-testid="select-trigger" aria-label={ariaLabel}>
      {children}
    </div>
  ),
  SelectContent: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  SelectItem: ({ children }: { children: ReactNode }) => <div>{children}</div>,
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

/** Locate the Theme select: each Select renders exactly one SelectTrigger. */
function getThemeSelect() {
  const triggers = screen.getAllByTestId("select-trigger");
  const themeIndex = triggers.findIndex(
    (trigger) => trigger.getAttribute("aria-label") === "Theme",
  );
  if (themeIndex < 0) {
    throw new Error("Theme select not found");
  }
  return selectRegistry[themeIndex];
}

describe("GeneralSettings theme select", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    selectRegistry.length = 0;
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue(false);
  });

  it("renders the current theme as the selected value", async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_autostart_status");
    });

    expect(getThemeSelect().value).toBe("system");
  });

  it("renders a stored dark theme as the selected value", async () => {
    mockSettings = { ...baseSettings, theme: "dark" };
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_autostart_status");
    });

    expect(getThemeSelect().value).toBe("dark");
  });

  it("guards unknown stored theme values to system", async () => {
    mockSettings = { ...baseSettings, theme: "neon" };
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_autostart_status");
    });

    expect(getThemeSelect().value).toBe("system");
  });

  it("calls updateSettings with the new theme on change", async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_autostart_status");
    });

    await act(async () => {
      getThemeSelect().onValueChange?.("dark");
    });

    expect(mockUpdateSettings).toHaveBeenCalledWith({ theme: "dark" });
  });
});
