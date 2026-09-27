import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RecordingSettings } from "../RecordingSettings";
import type { AppSettings } from "@/types";

const mockUpdateSettings = vi.fn().mockResolvedValue(undefined);
const mockInvoke = vi.fn().mockResolvedValue(false);

const baseSettings: AppSettings = {
  recording_mode: "toggle",
  hotkey: "CommandOrControl+Shift+Space",
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

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

describe("GeneralSettings auto-paste-transcription switch", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue(false);
  });

  it("defaults to enabled when setting is missing", async () => {
    // baseSettings has no auto_paste_transcription field
    render(<RecordingSettings />);

    const sw = screen.getByRole("switch", {
      name: "auto-paste-transcription",
    });
    expect(sw).toHaveAttribute("aria-checked", "true");
  });

  it("reflects explicit false value", async () => {
    mockSettings = { ...baseSettings, auto_paste_transcription: false };

    render(<RecordingSettings />);

    const sw = screen.getByRole("switch", {
      name: "auto-paste-transcription",
    });
    expect(sw).toHaveAttribute("aria-checked", "false");
  });

  it("calls updateSettings with false when toggled off", async () => {
    // Missing field → defaults true → toggle sends false
    render(<RecordingSettings />);

    const sw = screen.getByRole("switch", {
      name: "auto-paste-transcription",
    });
    fireEvent.click(sw);

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        auto_paste_transcription: false,
      });
    });
  });

  it("calls updateSettings with true when toggled on from explicit false", async () => {
    mockSettings = { ...baseSettings, auto_paste_transcription: false };

    render(<RecordingSettings />);

    const sw = screen.getByRole("switch", {
      name: "auto-paste-transcription",
    });
    fireEvent.click(sw);

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        auto_paste_transcription: true,
      });
    });
  });
});
