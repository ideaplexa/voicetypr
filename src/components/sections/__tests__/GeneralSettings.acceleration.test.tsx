import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GeneralSettings } from "../GeneralSettings";
import { RecordingSettings } from "../RecordingSettings";

const mockUpdateSettings = vi.fn().mockResolvedValue(undefined);

const baseSettings = {
  recording_mode: "toggle",
  hotkey: "CommandOrControl+Shift+Space",
  keep_transcription_in_clipboard: false,
  play_sound_on_recording: true,
  play_sound_on_transcription_complete: true,
  play_sound_on_paste_success: true,
  pill_indicator_mode: "when_recording",
  pill_indicator_position: "bottom-center",
  pill_indicator_offset: 10,
};

let mockSettings: Record<string, unknown> = { ...baseSettings };

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: mockSettings,
    updateSettings: mockUpdateSettings,
  }),
}));

vi.mock("@/contexts/ReadinessContext", () => ({
  useCanAutoInsert: () => true,
}));

// Use vi.hoisted so the object reference is stable and its properties can be
// mutated before each test without needing module reloads.
const platformMock = vi.hoisted(() => ({ isMacOS: false, isWindows: false }));

vi.mock("@/lib/platform", () => platformMock);

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn().mockResolvedValue(undefined),
  disable: vi.fn().mockResolvedValue(undefined),
  isEnabled: vi.fn().mockResolvedValue(false),
}));

vi.mock("@/components/HotkeyInput", () => ({
  HotkeyInput: () => <div data-testid="hotkey-input" />,
}));

vi.mock("@/components/ui/scroll-area", () => ({
  ScrollArea: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

vi.mock("@/components/ui/switch", () => ({
  Switch: ({
    id,
    checked,
    onCheckedChange,
    disabled,
  }: {
    id?: string;
    checked?: boolean;
    onCheckedChange?: (checked: boolean) => void;
    disabled?: boolean;
  }) => (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      data-testid={id ? `switch-${id}` : "switch"}
      disabled={disabled}
      onClick={() => onCheckedChange?.(!checked)}
    />
  ),
}));

vi.mock("@/components/ui/toggle-group", () => ({
  ToggleGroup: ({
    children,
    value,
  }: {
    children: React.ReactNode;
    value?: string;
    onValueChange?: (value: string) => void;
  }) => (
    <div data-testid="toggle-group" data-value={value}>
      {children}
    </div>
  ),
  ToggleGroupItem: ({ children, value }: { children: React.ReactNode; value: string }) => (
    <button type="button" data-testid={`toggle-item-${value}`}>
      {children}
    </button>
  ),
}));

// Select mock with controllable onValueChange; each Select gets a data-testid
// based on its data-value so tests can target the right one.
vi.mock("@/components/ui/select", () => ({
  Select: ({
    children,
    value,
    onValueChange,
  }: {
    children: React.ReactNode;
    value?: string;
    onValueChange?: (value: string) => void;
  }) => (
    <div
      data-testid="select"
      data-value={value}
      data-onvaluechange="true"
      onClick={(e) => {
        // Clicks on SelectItem children bubble up; read the value from the target.
        const target = e.target as HTMLElement;
        const itemValue = target.getAttribute("data-value");
        if (itemValue && onValueChange) onValueChange(itemValue);
      }}
    >
      {children}
    </div>
  ),
  SelectTrigger: ({ children, className }: { children: React.ReactNode; className?: string }) => (
    <button type="button" data-testid="select-trigger" className={className}>
      {children}
    </button>
  ),
  SelectContent: ({ children }: { children: React.ReactNode }) => (
    <div data-testid="select-content">{children}</div>
  ),
  SelectItem: ({ children, value }: { children: React.ReactNode; value: string }) => (
    <div data-testid={`select-item-${value}`} data-value={value}>
      {children}
    </div>
  ),
  SelectValue: () => <span data-testid="select-value" />,
}));

vi.mock("@/components/MicrophoneSelection", () => ({
  MicrophoneSelection: () => <div data-testid="microphone-selection" />,
}));

vi.mock("../NetworkSharingCard", () => ({
  NetworkSharingCard: () => <div data-testid="network-sharing-card" />,
}));

vi.mock("../TelemetrySection", () => ({
  TelemetrySection: () => <div>Privacy &amp; diagnostics</div>,
}));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "get_distribution_info") {
      return {
        channel: "direct",
        is_store_install: false,
        package_family_name: null,
      };
    }
    return undefined;
  });
});

// ============================================================================
// Transcription Acceleration — Windows
// ============================================================================

describe("GeneralSettings transcription acceleration — Windows", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    mockUpdateSettings.mockClear();
    platformMock.isWindows = true;
    platformMock.isMacOS = false;
  });

  it("renders the Transcription performance section header", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByText("Transcription performance")).toBeInTheDocument();
    });
  });

  it("renders Auto, GPU, and CPU select items", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByTestId("select-item-auto")).toBeInTheDocument();
      expect(screen.getByTestId("select-item-gpu")).toBeInTheDocument();
      expect(screen.getByTestId("select-item-cpu")).toBeInTheDocument();
    });
  });

  it("defaults to auto when transcription_acceleration is undefined", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      // The Select bound to acceleration should have data-value="auto"
      const selects = screen.getAllByTestId("select");
      const accelSelect = selects.find((el) => el.getAttribute("data-value") === "auto");
      expect(accelSelect).toBeTruthy();
    });
  });

  it("reflects a stored cpu value", async () => {
    mockSettings = { ...baseSettings, transcription_acceleration: "cpu" };
    render(<RecordingSettings />);
    await waitFor(() => {
      const selects = screen.getAllByTestId("select");
      const accelSelect = selects.find((el) => el.getAttribute("data-value") === "cpu");
      expect(accelSelect).toBeTruthy();
    });
  });

  it('calls updateSettings with transcription_acceleration: "cpu" when CPU is selected', async () => {
    render(<RecordingSettings />);

    const cpuItem = await screen.findByTestId("select-item-cpu");
    fireEvent.click(cpuItem);

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        transcription_acceleration: "cpu",
      });
    });
  });

  it('calls updateSettings with transcription_acceleration: "gpu" when GPU is selected', async () => {
    render(<RecordingSettings />);

    const gpuItem = await screen.findByTestId("select-item-gpu");
    fireEvent.click(gpuItem);

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        transcription_acceleration: "gpu",
      });
    });
  });

  it('calls updateSettings with transcription_acceleration: "auto" when Auto is selected', async () => {
    mockSettings = { ...baseSettings, transcription_acceleration: "cpu" };
    render(<RecordingSettings />);

    const autoItem = await screen.findByTestId("select-item-auto");
    fireEvent.click(autoItem);

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        transcription_acceleration: "auto",
      });
    });
  });
});

// ============================================================================
// Transcription Acceleration — non-Windows (macOS / Linux)
// ============================================================================

describe("GeneralSettings transcription acceleration — non-Windows", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    mockUpdateSettings.mockClear();
  });

  it("does NOT render the acceleration section on macOS", async () => {
    platformMock.isWindows = false;
    platformMock.isMacOS = true;
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.queryByText("Transcription performance")).not.toBeInTheDocument();
      expect(screen.queryByTestId("select-item-gpu")).not.toBeInTheDocument();
      expect(screen.queryByTestId("select-item-cpu")).not.toBeInTheDocument();
    });
  });

  it("does NOT render the acceleration section on Linux", async () => {
    platformMock.isWindows = false;
    platformMock.isMacOS = false;
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.queryByText("Transcription performance")).not.toBeInTheDocument();
      expect(screen.queryByTestId("select-item-gpu")).not.toBeInTheDocument();
    });
  });
});

describe("GeneralSettings update distribution controls", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings, update_channel: "stable" };
    mockUpdateSettings.mockClear();
    platformMock.isWindows = false;
    platformMock.isMacOS = true;
  });

  it("owns global privacy controls, not recording or reset controls", async () => {
    render(<GeneralSettings />);

    expect(await screen.findByText("Privacy & diagnostics")).toBeInTheDocument();
    expect(screen.queryByText("Reset app / start over")).not.toBeInTheDocument();
    expect(screen.queryByText("Recording workflow")).not.toBeInTheDocument();
  });

  it("hides direct update controls for Microsoft Store installs", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_distribution_info") {
        return {
          channel: "store_msix",
          is_store_install: true,
          package_family_name: "IdeaplexaLLC.Voicetypr_test",
        };
      }
      return undefined;
    });

    render(<GeneralSettings />);

    expect(await screen.findByText("Updates managed by Microsoft Store")).toBeInTheDocument();
    expect(screen.queryByText("Update channel")).not.toBeInTheDocument();
  });

  it("fails closed when distribution detection fails", async () => {
    invokeMock.mockImplementation((command: string) =>
      command === "get_distribution_info"
        ? Promise.reject(new Error("distribution unavailable"))
        : Promise.resolve(undefined),
    );

    render(<GeneralSettings />);

    expect(await screen.findByText("Update options unavailable")).toBeInTheDocument();
    expect(screen.queryByText("Update channel")).not.toBeInTheDocument();
  });

  it("shows channel controls only after confirming a direct install", async () => {
    render(<GeneralSettings />);

    expect(await screen.findByText("Update channel")).toBeInTheDocument();
    expect(screen.getByTestId("select-item-beta")).toBeInTheDocument();
  });

  it("shows Beta when the installed build defaults to the beta channel", async () => {
    mockSettings = { ...baseSettings, update_channel: "beta" };

    render(<GeneralSettings />);

    const channelField = (await screen.findByText("Update channel")).closest('[data-slot="field"]');
    expect(channelField).not.toBeNull();
    expect(within(channelField as HTMLElement).getByTestId("select")).toHaveAttribute(
      "data-value",
      "beta",
    );
  });
});
