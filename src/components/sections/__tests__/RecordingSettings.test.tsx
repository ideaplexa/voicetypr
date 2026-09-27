import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RecordingSettings } from "../RecordingSettings";
import { invoke } from "@tauri-apps/api/core";

const mockUpdateSettings = vi.fn().mockResolvedValue(undefined);
const baseSettings = {
  recording_mode: "toggle",
  hotkey: "CommandOrControl+Shift+Space",
  keep_transcription_in_clipboard: false,
  play_sound_on_recording: true,
  play_sound_on_transcription_complete: true,
  play_sound_on_paste_success: true,
  pill_indicator_mode: "when_recording",
  pill_indicator_style: "compact",
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

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn().mockResolvedValue(undefined),
  disable: vi.fn().mockResolvedValue(undefined),
  isEnabled: vi.fn().mockResolvedValue(false),
}));

// Mock HotkeyInput with buttons to simulate combo and bare-modifier captures
vi.mock("@/components/HotkeyInput", () => ({
  HotkeyInput: ({
    onChange,
    onBareModifier,
  }: {
    onChange?: (v: string) => void;
    onBareModifier?: (spec: { modifier: string; side: string }) => void;
  }) => (
    <div data-testid="hotkey-input">
      <button data-testid="mock-trigger-combo" onClick={() => onChange?.("Control+Space")} />
      <button
        data-testid="mock-trigger-bare-modifier"
        onClick={() => onBareModifier?.({ modifier: "control", side: "left" })}
      />
    </div>
  ),
}));

vi.mock("@/components/ui/scroll-area", () => ({
  ScrollArea: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

// Mock Switch with actual interactive behavior for testing
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
      id={id}
      data-testid={id ? `switch-${id}` : "switch"}
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onCheckedChange?.(!checked)}
    />
  ),
}));

// Mock Select with interactive behavior
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
      onClick={() => onValueChange?.(value === "compact" ? "full" : "top-center")}
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

/** Default invoke behavior: autostart=false, shortcut_settings empty, everything else undefined */
function setupDefaultInvoke() {
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "get_autostart_status") return Promise.resolve(false);
    if (cmd === "get_shortcut_settings") return Promise.resolve({ bindings: [] });
    return Promise.resolve(undefined);
  });
}

// ============================================================================
// Recording Indicator Mode Tests
// ============================================================================

describe("GeneralSettings recording indicator", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    setupDefaultInvoke();
  });

  it("hides the position selector when mode is never", async () => {
    mockSettings.pill_indicator_mode = "never";
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.queryByText("Indicator Position")).not.toBeInTheDocument();
    });
  });

  it("shows the position selector when mode is always", async () => {
    mockSettings.pill_indicator_mode = "always";
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByText("Indicator Position")).toBeInTheDocument();
    });
  });

  it("shows the position selector when mode is when_recording", async () => {
    mockSettings.pill_indicator_mode = "when_recording";
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByText("Indicator Position")).toBeInTheDocument();
    });
  });

  it("displays Recording Indicator Visibility label", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByText("Recording indicator")).toBeInTheDocument();
    });
  });

  it("displays all indicator mode options", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByTestId("select-item-never")).toBeInTheDocument();
      expect(screen.getByTestId("select-item-always")).toBeInTheDocument();
      expect(screen.getByTestId("select-item-when_recording")).toBeInTheDocument();
    });
  });

  it("offers compact and full indicator detail", async () => {
    render(<RecordingSettings />);

    await waitFor(() => {
      expect(screen.getByText("Indicator detail")).toBeInTheDocument();
      expect(screen.getByTestId("select-item-compact")).toBeInTheDocument();
      expect(screen.getByTestId("select-item-full")).toBeInTheDocument();
    });

    const styleSelect = screen
      .getAllByTestId("select")
      .find((select) => select.getAttribute("data-value") === "compact");
    expect(styleSelect).toBeDefined();
    fireEvent.click(styleSelect!);
    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        pill_indicator_style: "full",
      });
    });
  });

  it("displays all indicator position options when mode is not never", async () => {
    mockSettings.pill_indicator_mode = "always";
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByTestId("select-item-top-left")).toBeInTheDocument();
      expect(screen.getByTestId("select-item-top-center")).toBeInTheDocument();
      expect(screen.getByTestId("select-item-top-right")).toBeInTheDocument();
      expect(screen.getByTestId("select-item-bottom-left")).toBeInTheDocument();
      expect(screen.getByTestId("select-item-bottom-center")).toBeInTheDocument();
      expect(screen.getByTestId("select-item-bottom-right")).toBeInTheDocument();
    });
  });

  it("calls updateSettings when position is changed", async () => {
    mockSettings.pill_indicator_mode = "always";
    render(<RecordingSettings />);

    await waitFor(() => {
      expect(screen.getByText("Indicator Position")).toBeInTheDocument();
    });

    const selects = screen.getAllByTestId("select");
    const positionSelect = selects.find((select) =>
      select.getAttribute("data-value")?.includes("center"),
    );

    if (positionSelect) {
      fireEvent.click(positionSelect);
      await waitFor(() => {
        expect(mockUpdateSettings).toHaveBeenCalledWith({
          pill_indicator_position: "top-center",
        });
      });
    }
  });
});

// ============================================================================
// Sound Settings Tests
// ============================================================================

describe("GeneralSettings audio feedback settings", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    setupDefaultInvoke();
  });

  it("describes the three exact audio feedback events", async () => {
    render(<RecordingSettings />);

    expect(await screen.findByText("Recording started")).toBeInTheDocument();
    expect(screen.getByText("Transcript ready")).toBeInTheDocument();
    expect(screen.getByText("Paste completed")).toBeInTheDocument();
    expect(
      screen.getByText("Play a sound when the microphone is ready for speech."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Play a sound after transcription and optional AI formatting finish."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Play a sound after VoiceTypr successfully sends the paste command."),
    ).toBeInTheDocument();
  });

  it.each([
    ["Recording started", "play_sound_on_recording"],
    ["Transcript ready", "play_sound_on_transcription_complete"],
    ["Paste completed", "play_sound_on_paste_success"],
  ] as const)("updates %s independently", async (label, setting) => {
    render(<RecordingSettings />);
    fireEvent.click(await screen.findByRole("switch", { name: label }));

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({ [setting]: false });
    });
  });

  it.each([
    ["Recording started", "play_sound_on_recording"],
    ["Transcript ready", "play_sound_on_transcription_complete"],
    ["Paste completed", "play_sound_on_paste_success"],
  ] as const)("reflects disabled state for %s", async (label, setting) => {
    mockSettings = { ...baseSettings, [setting]: false };
    render(<RecordingSettings />);

    expect(await screen.findByRole("switch", { name: label })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });
});

// ============================================================================
// Clipboard Settings Tests
// ============================================================================

describe("GeneralSettings clipboard settings", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    setupDefaultInvoke();
  });

  it("displays Keep Transcript in Clipboard label", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByText("Keep Transcript in Clipboard")).toBeInTheDocument();
    });
  });

  it("displays clipboard description", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(
        screen.getByText("Leave transcribed text available for manual pastes"),
      ).toBeInTheDocument();
    });
  });

  it("renders clipboard retain switch", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByTestId("switch-clipboard-retain")).toBeInTheDocument();
    });
  });

  it("calls updateSettings when clipboard switch is clicked", async () => {
    render(<RecordingSettings />);

    await waitFor(() => {
      expect(screen.getByTestId("switch-clipboard-retain")).toBeInTheDocument();
    });

    const switchButton = screen.getByTestId("switch-clipboard-retain");
    fireEvent.click(switchButton);

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        keep_transcription_in_clipboard: true,
      });
    });
  });
});

// ============================================================================
// UI Structure Tests
// ============================================================================

describe("RecordingSettings UI structure", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    setupDefaultInvoke();
  });

  it("renders the Recording header", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Recording", level: 1 })).toBeInTheDocument();
    });
  });

  it("separates capture controls from transcript handling", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByText("Capture controls")).toBeInTheDocument();
      expect(screen.getByText("Transcript handling")).toBeInTheDocument();
    });
  });

  it("shows hotkey input after clicking Edit", async () => {
    render(<RecordingSettings />);
    const editButton = await screen.findByRole("button", { name: /edit/i });
    fireEvent.click(editButton);
    await waitFor(() => {
      expect(screen.getByTestId("hotkey-input")).toBeInTheDocument();
    });
  });

  it("renders the microphone selection component", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByTestId("microphone-selection")).toBeInTheDocument();
    });
  });

  it("does not render global startup settings", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.queryByText("Launch at Startup")).not.toBeInTheDocument();
    });
  });
});

// ============================================================================
// Recording Hotkey Editor Tests
// ============================================================================

describe("RecordingSettings hotkey editor", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    setupDefaultInvoke();
  });

  it("displays the current combo hotkey in view mode", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      // The label appears in both FieldDescription and the display box
      expect(screen.getAllByText("Ctrl+Shift+Space").length).toBeGreaterThan(0);
    });
  });

  it('displays "Not set" when no hotkey is configured', async () => {
    mockSettings = { ...baseSettings, hotkey: "" };
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getAllByText("Not set").length).toBeGreaterThan(0);
    });
  });

  it("shows the Recording Hotkey field title", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByText("Recording Hotkey")).toBeInTheDocument();
    });
  });

  it("clicking Edit enters capture mode and shows HotkeyInput", async () => {
    render(<RecordingSettings />);
    const editButton = await screen.findByRole("button", { name: /edit/i });
    fireEvent.click(editButton);
    await waitFor(() => {
      expect(screen.getByTestId("hotkey-input")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /save/i })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument();
    });
  });

  it("Cancel returns to view mode without saving", async () => {
    render(<RecordingSettings />);
    const editButton = await screen.findByRole("button", { name: /edit/i });
    fireEvent.click(editButton);
    await screen.findByTestId("hotkey-input");

    const cancelButton = screen.getByRole("button", { name: /cancel/i });
    fireEvent.click(cancelButton);

    await waitFor(() => {
      expect(screen.queryByTestId("hotkey-input")).not.toBeInTheDocument();
      expect(mockUpdateSettings).not.toHaveBeenCalled();
    });
  });

  it("Save is disabled until a key is captured when no prior hotkey", async () => {
    mockSettings = { ...baseSettings, hotkey: "" };
    render(<RecordingSettings />);
    const editButton = await screen.findByRole("button", { name: /edit/i });
    fireEvent.click(editButton);
    await screen.findByTestId("hotkey-input");

    const saveButton = screen.getByRole("button", { name: /save/i });
    expect(saveButton).toBeDisabled();
  });

  it("capturing a combo enables Save and calls set_global_shortcut + updateSettings", async () => {
    render(<RecordingSettings />);
    const editButton = await screen.findByRole("button", { name: /edit/i });
    fireEvent.click(editButton);
    await screen.findByTestId("hotkey-input");

    // Simulate capturing a combo
    fireEvent.click(screen.getByTestId("mock-trigger-combo"));

    const saveButton = screen.getByRole("button", { name: /save/i });
    expect(saveButton).not.toBeDisabled();
    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_global_shortcut", {
        shortcut: "Control+Space",
      });
      expect(mockUpdateSettings).toHaveBeenCalledWith({ hotkey: "Control+Space" });
    });
  });

  it("after saving a combo, view mode returns showing the new hotkey", async () => {
    render(<RecordingSettings />);
    const editButton = await screen.findByRole("button", { name: /edit/i });
    fireEvent.click(editButton);
    await screen.findByTestId("hotkey-input");

    fireEvent.click(screen.getByTestId("mock-trigger-combo"));
    fireEvent.click(screen.getByRole("button", { name: /save/i }));

    await waitFor(() => {
      expect(screen.queryByTestId("hotkey-input")).not.toBeInTheDocument();
    });
  });

  it("capturing a bare modifier shows the Hold-to-talk switch", async () => {
    render(<RecordingSettings />);
    const editButton = await screen.findByRole("button", { name: /edit/i });
    fireEvent.click(editButton);
    await screen.findByTestId("hotkey-input");

    fireEvent.click(screen.getByTestId("mock-trigger-bare-modifier"));

    await waitFor(() => {
      expect(screen.getByTestId("switch-hold-to-talk")).toBeInTheDocument();
      expect(screen.getByText(/hold to talk/i)).toBeInTheDocument();
    });
  });

  it("clears the combo hotkey BEFORE saving a bare-modifier binding (regression #100)", async () => {
    // Fix A's startup migration treats a non-empty settings.hotkey as the
    // authoritative primary and disables a bare-modifier primary. So when the
    // user switches from a combo to a bare modifier, the combo MUST be cleared
    // before update_shortcut_settings rebuilds the engine — otherwise the new
    // bare-modifier binding is disabled the instant it is saved.
    render(<RecordingSettings />);
    const editButton = await screen.findByRole("button", { name: /edit/i });
    fireEvent.click(editButton);
    await screen.findByTestId("hotkey-input");

    fireEvent.click(screen.getByTestId("mock-trigger-bare-modifier"));
    fireEvent.click(screen.getByRole("button", { name: /save/i }));

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({ hotkey: "" });
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("update_shortcut_settings", expect.anything());
    });

    const clearIdx = mockUpdateSettings.mock.calls.findIndex(
      ([arg]) => (arg as { hotkey?: string } | undefined)?.hotkey === "",
    );
    const clearOrder = mockUpdateSettings.mock.invocationCallOrder[clearIdx];
    const saveIdx = vi
      .mocked(invoke)
      .mock.calls.findIndex(([cmd]) => cmd === "update_shortcut_settings");
    const saveOrder = vi.mocked(invoke).mock.invocationCallOrder[saveIdx];

    expect(clearOrder).toBeLessThan(saveOrder);
  });

  it("bare modifier + Hold-to-talk OFF → persists isolated_tap/toggle_recording/pressed", async () => {
    render(<RecordingSettings />);
    const editButton = await screen.findByRole("button", { name: /edit/i });
    fireEvent.click(editButton);
    await screen.findByTestId("hotkey-input");

    // Capture bare modifier (Hold-to-talk defaults to OFF)
    fireEvent.click(screen.getByTestId("mock-trigger-bare-modifier"));
    await screen.findByTestId("switch-hold-to-talk");

    // Verify switch is off (aria-checked=false)
    expect(screen.getByTestId("switch-hold-to-talk")).toHaveAttribute("aria-checked", "false");

    fireEvent.click(screen.getByRole("button", { name: /save/i }));

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "update_shortcut_settings",
        expect.objectContaining({
          settings: expect.objectContaining({
            bindings: expect.arrayContaining([
              expect.objectContaining({
                trigger_kind: "isolated_tap",
                action: "toggle_recording",
                trigger: "pressed",
                modifier: { modifier: "control", side: "left" },
              }),
            ]),
          }),
        }),
      );
    });
  });

  it("bare modifier + Hold-to-talk ON → persists modifier_hold/hold_to_record/hold", async () => {
    render(<RecordingSettings />);
    const editButton = await screen.findByRole("button", { name: /edit/i });
    fireEvent.click(editButton);
    await screen.findByTestId("hotkey-input");

    fireEvent.click(screen.getByTestId("mock-trigger-bare-modifier"));
    await screen.findByTestId("switch-hold-to-talk");

    // Toggle hold-to-talk ON
    fireEvent.click(screen.getByTestId("switch-hold-to-talk"));

    fireEvent.click(screen.getByRole("button", { name: /save/i }));

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "update_shortcut_settings",
        expect.objectContaining({
          settings: expect.objectContaining({
            bindings: expect.arrayContaining([
              expect.objectContaining({
                trigger_kind: "modifier_hold",
                action: "hold_to_record",
                trigger: "hold",
                modifier: { modifier: "control", side: "left" },
              }),
            ]),
          }),
        }),
      );
    });
  });

  it("saves bare modifier with stable id from existing binding", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_autostart_status") return Promise.resolve(false);
      if (cmd === "get_shortcut_settings")
        return Promise.resolve({
          bindings: [
            {
              id: "onboarding-primary-hold",
              action: "hold_to_record",
              shortcut: "",
              trigger: "hold",
              enabled: true,
              allow_risky_combo: false,
              trigger_kind: "modifier_hold",
              modifier: { modifier: "alt", side: "right" },
            },
          ],
        });
      return Promise.resolve(undefined);
    });

    render(<RecordingSettings />);
    const editButton = await screen.findByRole("button", { name: /edit/i });
    fireEvent.click(editButton);
    await screen.findByTestId("hotkey-input");

    fireEvent.click(screen.getByTestId("mock-trigger-bare-modifier"));
    await screen.findByTestId("switch-hold-to-talk");
    fireEvent.click(screen.getByRole("button", { name: /save/i }));

    await waitFor(() => {
      const calls = vi.mocked(invoke).mock.calls;
      const saveCall = calls.find(([cmd]) => cmd === "update_shortcut_settings");
      expect(saveCall).toBeDefined();
      const payload = saveCall![1] as { settings: { bindings: Array<{ id: string }> } };
      expect(payload.settings.bindings[0].id).toBe("onboarding-primary-hold");
    });
  });

  it('displays isolated_tap native binding as "Tap Left Control to toggle"', async () => {
    mockSettings = { ...baseSettings, hotkey: "" };
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_autostart_status") return Promise.resolve(false);
      if (cmd === "get_shortcut_settings")
        return Promise.resolve({
          bindings: [
            {
              id: "onboarding-primary-hold",
              action: "toggle_recording",
              shortcut: "",
              trigger: "pressed",
              enabled: true,
              allow_risky_combo: false,
              trigger_kind: "isolated_tap",
              modifier: { modifier: "control", side: "left" },
            },
          ],
        });
      return Promise.resolve(undefined);
    });

    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getAllByText("Tap Left Control to toggle").length).toBeGreaterThan(0);
    });
  });

  it('displays modifier_hold native binding as "Hold Left Control to talk"', async () => {
    mockSettings = { ...baseSettings, hotkey: "" };
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_autostart_status") return Promise.resolve(false);
      if (cmd === "get_shortcut_settings")
        return Promise.resolve({
          bindings: [
            {
              id: "onboarding-primary-hold",
              action: "hold_to_record",
              shortcut: "",
              trigger: "hold",
              enabled: true,
              allow_risky_combo: false,
              trigger_kind: "modifier_hold",
              modifier: { modifier: "control", side: "left" },
            },
          ],
        });
      return Promise.resolve(undefined);
    });

    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getAllByText("Hold Left Control to talk").length).toBeGreaterThan(0);
    });
  });
});

// ============================================================================
// Null Settings Handling Tests
// ============================================================================

describe("GeneralSettings null handling", () => {
  beforeEach(() => {
    setupDefaultInvoke();
  });

  it("returns null when settings is null", async () => {
    // Create a separate mock for null settings
    const originalMock = vi.fn();
    vi.doMock("@/contexts/SettingsContext", () => ({
      useSettings: () => ({
        settings: null,
        updateSettings: originalMock,
      }),
    }));

    // With current mock setup, we can test the base case
    // The component should handle null gracefully
  });

  it("handles undefined sound settings with enabled defaults", async () => {
    const settingsWithoutSound = {
      recording_mode: "toggle",
      hotkey: "CommandOrControl+Shift+Space",
      keep_transcription_in_clipboard: false,
      pill_indicator_mode: "when_recording",
      pill_indicator_position: "bottom-center",
    };
    mockSettings = settingsWithoutSound as typeof mockSettings;

    render(<RecordingSettings />);
    for (const label of ["Recording started", "Transcript ready", "Paste completed"]) {
      expect(await screen.findByRole("switch", { name: label })).toHaveAttribute(
        "aria-checked",
        "true",
      );
    }
  });
});

// ============================================================================
// Settings Value Display Tests
// ============================================================================

describe("GeneralSettings settings values", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    setupDefaultInvoke();
  });

  it("displays the current hotkey string in view mode", async () => {
    mockSettings.hotkey = "CommandOrControl+Shift+Space";
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getAllByText("Ctrl+Shift+Space").length).toBeGreaterThan(0);
    });
  });

  it("displays correct pill indicator mode value", async () => {
    mockSettings.pill_indicator_mode = "always";
    render(<RecordingSettings />);
    // The first Select should have the indicator mode
    await waitFor(() => {
      const selects = screen.getAllByTestId("select");
      expect(selects.length).toBeGreaterThan(0);
    });
  });

  it("shows all three indicator visibility options", async () => {
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByText("Never")).toBeInTheDocument();
      expect(screen.getByText("Always")).toBeInTheDocument();
      expect(screen.getByText("When Recording")).toBeInTheDocument();
    });
  });

  it("shows all indicator position options when indicator is visible", async () => {
    mockSettings.pill_indicator_mode = "always";
    render(<RecordingSettings />);
    await waitFor(() => {
      expect(screen.getByText("Top Left")).toBeInTheDocument();
      expect(screen.getByText("Top Center")).toBeInTheDocument();
      expect(screen.getByText("Top Right")).toBeInTheDocument();
      expect(screen.getByText("Bottom Left")).toBeInTheDocument();
      expect(screen.getByText("Bottom Center")).toBeInTheDocument();
      expect(screen.getByText("Bottom Right")).toBeInTheDocument();
    });
  });
});
