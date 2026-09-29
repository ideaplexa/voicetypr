import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RecordingSettings } from "../RecordingSettings";
import { invoke } from "@tauri-apps/api/core";
import { emitMockEvent } from "@/test/setup";

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
    "aria-label": ariaLabel,
  }: {
    id?: string;
    checked?: boolean;
    onCheckedChange?: (checked: boolean) => void;
    disabled?: boolean;
    "aria-label"?: string;
  }) => (
    <button
      type="button"
      role="switch"
      id={id}
      aria-label={ariaLabel}
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
      onClick={() => onValueChange?.(value === "compact" ? "full" : value === "when_recording" ? "always" : "top-center")}
    >
      {children}
    </div>
  ),
  SelectTrigger: ({ children, className, "aria-label": ariaLabel }: { children: React.ReactNode; className?: string; "aria-label"?: string }) => (
    <button type="button" data-testid="select-trigger" className={className} aria-label={ariaLabel}>
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

describe("Recording screen", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    setupDefaultInvoke();
  });

  it("shows the S3 cards and all remaining controls", () => {
    render(<RecordingSettings />);
    for (const title of [
      "Dictation shortcut",
      "Microphone",
      "Recording pill",
      "Paste automatically",
      "Keep in clipboard",
      "Sounds",
      "More",
      "Pause media during recording",
      "Transcript history cleanup",
      "Save recording audio",
      "Transcript ready",
      "Paste completed",
    ]) {
      expect(screen.getByText(title)).toBeInTheDocument();
    }
    expect(
      screen.getByText("Your shortcut, microphone and what happens after you speak."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Speak to test — start a dictation to see the level."),
    ).toBeInTheDocument();
  });

  it("switches the recording mode through the segmented control", () => {
    render(<RecordingSettings />);
    fireEvent.click(screen.getByRole("button", { name: "Hold to talk" }));
    expect(mockUpdateSettings).toHaveBeenCalledWith({ recording_mode: "push_to_talk" });
  });

  it("opens the existing hotkey capture from Change", () => {
    render(<RecordingSettings />);
    fireEvent.click(screen.getByRole("button", { name: "Change" }));
    expect(screen.getByTestId("hotkey-input")).toBeInTheDocument();
  });

  it("keeps the shortcut key caps visible", () => {
    render(<RecordingSettings />);
    expect(screen.getByLabelText("Current shortcut: Ctrl+Shift+Space")).toBeInTheDocument();
    expect(screen.getByText("Ctrl")).toBeInTheDocument();
    expect(screen.getByText("Shift")).toBeInTheDocument();
    const cap = screen.getByText("Ctrl");
    expect(cap.tagName).toBe("KBD");
    expect(cap).toHaveClass("font-sans", "bg-secondary", "rounded-[9px]", "border");
  });

  it("shows microphone level only during recording", async () => {
    render(<RecordingSettings />);
    expect(screen.queryByRole("meter", { name: "Microphone level" })).not.toBeInTheDocument();
    await act(async () => {
      emitMockEvent("recording-state-changed", { state: "recording" });
    });
    const meter = await screen.findByRole("meter", { name: "Microphone level" });
    await act(async () => {
      emitMockEvent("audio-level", 0.42);
    });
    expect(meter).toHaveAttribute("aria-valuenow", "42");
    await act(async () => {
      emitMockEvent("recording-state-changed", { state: "idle" });
    });
    expect(screen.queryByRole("meter", { name: "Microphone level" })).not.toBeInTheDocument();
  });

  it("hides pill position and detail when visibility is never", () => {
    mockSettings.pill_indicator_mode = "never";
    render(<RecordingSettings />);
    expect(screen.queryByText("Position")).not.toBeInTheDocument();
    expect(screen.queryByText("Detail")).not.toBeInTheDocument();
  });

  it("changes the pill position using the existing setting", async () => {
    render(<RecordingSettings />);
    const position = screen.getByText("Position").closest("div")?.parentElement;
    expect(position).not.toBeNull();
    fireEvent.click(position!.querySelector("[data-testid=select]")!);
    await waitFor(() =>
      expect(mockUpdateSettings).toHaveBeenCalledWith({ pill_indicator_position: "top-center" }),
    );
  });

  it("preserves each audio feedback setting", () => {
    render(<RecordingSettings />);
    fireEvent.click(screen.getByTestId("switch-sound-on-recording"));
    fireEvent.click(screen.getByTestId("switch-sound-on-transcription-complete"));
    fireEvent.click(screen.getByTestId("switch-sound-on-paste-success"));
    expect(mockUpdateSettings).toHaveBeenCalledWith({ play_sound_on_recording: false });
    expect(mockUpdateSettings).toHaveBeenCalledWith({
      play_sound_on_transcription_complete: false,
    });
    expect(mockUpdateSettings).toHaveBeenCalledWith({ play_sound_on_paste_success: false });
  });
});

describe("Recording pill behavior", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    setupDefaultInvoke();
  });

  it.each(["always", "when_recording"] as const)("shows position and detail when visibility is %s", (mode) => {
    mockSettings.pill_indicator_mode = mode;
    render(<RecordingSettings />);
    expect(screen.getByText("Position")).toBeInTheDocument();
    expect(screen.getByText("Detail")).toBeInTheDocument();
  });

  it("labels the recording pill and its visibility control", () => {
    render(<RecordingSettings />);
    expect(screen.getByText("Recording pill")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Pill visibility" })).toBeInTheDocument();
  });

  it.each(["never", "always", "when_recording"] as const)("offers visibility option %s", (mode) => {
    render(<RecordingSettings />);
    expect(screen.getByTestId(`select-item-${mode}`)).toBeInTheDocument();
  });

  it("updates the visibility setting", () => {
    render(<RecordingSettings />);
    fireEvent.click(screen.getAllByTestId("select").find((element) => element.getAttribute("data-value") === "when_recording")!);
    expect(mockUpdateSettings).toHaveBeenCalledWith({ pill_indicator_mode: "always" });
  });

  it.each(["compact", "full"] as const)("offers %s pill detail", (detail) => {
    render(<RecordingSettings />);
    expect(screen.getByTestId(`select-item-${detail}`)).toBeInTheDocument();
  });

  it("updates pill detail", () => {
    render(<RecordingSettings />);
    fireEvent.click(screen.getAllByTestId("select").find((element) => element.getAttribute("data-value") === "compact")!);
    expect(mockUpdateSettings).toHaveBeenCalledWith({ pill_indicator_style: "full" });
  });

  it.each(["top-left", "top-center", "top-right", "bottom-left", "bottom-center", "bottom-right"] as const)("offers pill position %s", (position) => {
    render(<RecordingSettings />);
    expect(screen.getByTestId(`select-item-${position}`)).toBeInTheDocument();
  });

  it("reflects the stored pill visibility and position", () => {
    mockSettings = { ...baseSettings, pill_indicator_mode: "always", pill_indicator_position: "top-left" };
    render(<RecordingSettings />);
    expect(screen.getAllByTestId("select").some((element) => element.getAttribute("data-value") === "always")).toBe(true);
    expect(screen.getAllByTestId("select").some((element) => element.getAttribute("data-value") === "top-left")).toBe(true);
  });
});

describe("Recording audio and clipboard behavior", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    setupDefaultInvoke();
  });

  it("describes the three exact sound events", () => {
    render(<RecordingSettings />);
    expect(screen.getByText("A soft tick when recording starts and ends.")).toBeInTheDocument();
    expect(screen.getByText("Play a sound after transcription and optional AI formatting finish.")).toBeInTheDocument();
    expect(screen.getByText("Play a sound after Voicetypr successfully sends the paste command.")).toBeInTheDocument();
  });

  it.each([
    ["sound-on-recording", "play_sound_on_recording"],
    ["sound-on-transcription-complete", "play_sound_on_transcription_complete"],
    ["sound-on-paste-success", "play_sound_on_paste_success"],
  ] as const)("updates %s independently", (id, setting) => {
    render(<RecordingSettings />);
    fireEvent.click(screen.getByTestId(`switch-${id}`));
    expect(mockUpdateSettings).toHaveBeenCalledWith({ [setting]: false });
  });

  it.each([
    ["sound-on-recording", "play_sound_on_recording"],
    ["sound-on-transcription-complete", "play_sound_on_transcription_complete"],
    ["sound-on-paste-success", "play_sound_on_paste_success"],
  ] as const)("reflects disabled %s", (id, setting) => {
    mockSettings = { ...baseSettings, [setting]: false };
    render(<RecordingSettings />);
    expect(screen.getByTestId(`switch-${id}`)).toHaveAttribute("aria-checked", "false");
  });

  it("describes clipboard retention and renders its switch", () => {
    render(<RecordingSettings />);
    expect(screen.getByText("Keep in clipboard")).toBeInTheDocument();
    expect(screen.getByText("Also copy it, so you can paste again.")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Keep in clipboard" })).toHaveAttribute("aria-checked", "false");
  });

  it("updates clipboard retention", () => {
    render(<RecordingSettings />);
    fireEvent.click(screen.getByRole("switch", { name: "Keep in clipboard" }));
    expect(mockUpdateSettings).toHaveBeenCalledWith({ keep_transcription_in_clipboard: true });
  });

  it("reflects enabled clipboard retention", () => {
    mockSettings = { ...baseSettings, keep_transcription_in_clipboard: true };
    render(<RecordingSettings />);
    expect(screen.getByRole("switch", { name: "Keep in clipboard" })).toHaveAttribute("aria-checked", "true");
  });

  it("renders safely with null settings", () => {
    mockSettings = null as unknown as typeof baseSettings;
    const { container } = render(<RecordingSettings />);
    expect(container).toBeEmptyDOMElement();
  });

  it("defaults undefined sound settings to enabled", () => {
    mockSettings = { ...baseSettings, play_sound_on_recording: undefined, play_sound_on_transcription_complete: undefined, play_sound_on_paste_success: undefined } as unknown as typeof baseSettings;
    render(<RecordingSettings />);
    for (const id of ["sound-on-recording", "sound-on-transcription-complete", "sound-on-paste-success"])
      expect(screen.getByTestId(`switch-${id}`)).toHaveAttribute("aria-checked", "true");
  });
});

describe("RecordingSettings hotkey editor", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    setupDefaultInvoke();
  });

  function edit() {
    fireEvent.click(screen.getByRole("button", { name: "Change" }));
    return screen.getByTestId("hotkey-input");
  }

  function nativeBinding(triggerKind: "isolated_tap" | "modifier_hold", id = "onboarding-primary-hold") {
    const hold = triggerKind === "modifier_hold";
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_shortcut_settings") return Promise.resolve({ bindings: [{ id, action: hold ? "hold_to_record" : "toggle_recording", shortcut: "", trigger: hold ? "hold" : "pressed", enabled: true, allow_risky_combo: false, trigger_kind: triggerKind, modifier: { modifier: "control", side: "left" } }] });
      return Promise.resolve(undefined);
    });
  }

  it("shows the current combo in view mode", () => {
    render(<RecordingSettings />);
    expect(screen.getByLabelText("Current shortcut: Ctrl+Shift+Space")).toBeInTheDocument();
  });

  it("shows Not set when no shortcut exists", async () => {
    mockSettings = { ...baseSettings, hotkey: "" };
    render(<RecordingSettings />);
    expect(screen.getByLabelText("Current shortcut: Not set")).toBeInTheDocument();
  });

  it("labels the shortcut field and enters capture on Change", () => {
    render(<RecordingSettings />);
    expect(screen.getByText("Dictation shortcut")).toBeInTheDocument();
    edit();
    expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  });

  it("Cancel returns to view without saving", () => {
    render(<RecordingSettings />);
    edit();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("hotkey-input")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Change" })).toBeInTheDocument();
    expect(mockUpdateSettings).not.toHaveBeenCalled();
  });

  it("disables Save until a new shortcut is captured", () => {
    mockSettings = { ...baseSettings, hotkey: "" };
    render(<RecordingSettings />);
    edit();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("saves a captured combo through the native shortcut and settings", async () => {
    render(<RecordingSettings />);
    edit();
    fireEvent.click(screen.getByTestId("mock-trigger-combo"));
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_global_shortcut", { shortcut: "Control+Space" });
      expect(mockUpdateSettings).toHaveBeenCalledWith({ hotkey: "Control+Space" });
    });
  });

  it("returns to view mode showing the saved combo", async () => {
    const { rerender } = render(<RecordingSettings />);
    edit();
    fireEvent.click(screen.getByTestId("mock-trigger-combo"));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByTestId("hotkey-input")).not.toBeInTheDocument());
    expect(mockUpdateSettings).toHaveBeenCalledWith({ hotkey: "Control+Space" });
    mockSettings = { ...baseSettings, hotkey: "Control+Space" };
    rerender(<RecordingSettings />);
    expect(screen.getByLabelText("Current shortcut: Ctrl+Space")).toBeInTheDocument();
  });

  it("shows Hold to talk after capturing a bare modifier", () => {
    render(<RecordingSettings />);
    edit();
    fireEvent.click(screen.getByTestId("mock-trigger-bare-modifier"));
    expect(screen.getByTestId("switch-hold-to-talk")).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText("Hold to talk (push-to-talk)")).toBeInTheDocument();
  });

  it("clears the combo before saving a bare modifier (regression #100)", async () => {
    render(<RecordingSettings />);
    edit();
    fireEvent.click(screen.getByTestId("mock-trigger-bare-modifier"));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", expect.anything()));
    const clear = mockUpdateSettings.mock.calls.findIndex(([value]) => value?.hotkey === "");
    const save = vi.mocked(invoke).mock.calls.findIndex(([cmd]) => cmd === "update_shortcut_settings");
    expect(clear).toBeGreaterThanOrEqual(0);
    expect(mockUpdateSettings.mock.invocationCallOrder[clear]).toBeLessThan(vi.mocked(invoke).mock.invocationCallOrder[save]);
  });

  it.each([
    [false, "isolated_tap", "toggle_recording", "pressed"],
    [true, "modifier_hold", "hold_to_record", "hold"],
  ] as const)("persists bare modifier with hold=%s", async (hold, triggerKind, action, trigger) => {
    render(<RecordingSettings />);
    edit();
    fireEvent.click(screen.getByTestId("mock-trigger-bare-modifier"));
    if (hold) fireEvent.click(screen.getByTestId("switch-hold-to-talk"));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", expect.objectContaining({ settings: expect.objectContaining({ bindings: expect.arrayContaining([expect.objectContaining({ trigger_kind: triggerKind, action, trigger, modifier: { modifier: "control", side: "left" } })]) }) })));
  });

  it("keeps the existing native binding id", async () => {
    nativeBinding("modifier_hold", "existing-primary");
    render(<RecordingSettings />);
    edit();
    fireEvent.click(screen.getByTestId("mock-trigger-bare-modifier"));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", expect.objectContaining({ settings: expect.objectContaining({ bindings: expect.arrayContaining([expect.objectContaining({ id: "existing-primary" })]) }) })));
  });

  it.each([
    ["isolated_tap", "Tap Left Control to toggle"],
    ["modifier_hold", "Hold Left Control to talk"],
  ] as const)("displays %s native binding", async (kind, label) => {
    mockSettings = { ...baseSettings, hotkey: "" };
    nativeBinding(kind);
    render(<RecordingSettings />);
    expect(await screen.findByLabelText(`Current shortcut: ${label}`)).toBeInTheDocument();
  });
});

describe("Recording screen structure", () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    setupDefaultInvoke();
  });

  it("shows the Recording heading", () => {
    render(<RecordingSettings />);
    expect(screen.getByRole("heading", { name: "Recording", level: 1 })).toBeInTheDocument();
  });

  it("separates shortcut capture from transcript handling", () => {
    render(<RecordingSettings />);
    expect(screen.getByText("Dictation shortcut")).toBeInTheDocument();
    expect(screen.getByText("Paste automatically")).toBeInTheDocument();
  });

  it("renders microphone selection", () => {
    render(<RecordingSettings />);
    expect(screen.getByTestId("microphone-selection")).toBeInTheDocument();
  });

  it("does not show global startup controls", () => {
    render(<RecordingSettings />);
    expect(screen.queryByText("Launch at Startup")).not.toBeInTheDocument();
  });
});
