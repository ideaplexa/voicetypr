import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings, TranscriptionHistory } from "@/types";
import { OverviewTab } from "./OverviewTab";

const mock = vi.hoisted(() => ({
  settings: { hotkey: "Alt+Space", current_model: "parakeet-tdt-0.6b-v3", current_model_engine: "parakeet", speech_language: "en", recording_mode: "push_to_talk", transcription_mode: "live_preview" } as Partial<AppSettings>,
  readiness: { canRecord: true, selectedModelAvailable: true as boolean | null, remoteSelected: false, remoteAvailable: true, licenseValid: true, licenseStatus: "licensed" as string | null, hasMicrophonePermission: true },
  downloadProgress: {} as Record<string, number>,
  shortcutBindings: [] as Array<{ id: string; action: string; shortcut: string; trigger: string; enabled: boolean; allow_risky_combo: boolean; trigger_kind: string; modifier: { modifier: string; side: string } }>,
  history: [] as TranscriptionHistory[],
  isLoading: false,
  loadError: null as string | null,
  mac: true,
  listeners: {} as Record<string, (event: { payload: unknown }) => void>,
}));

vi.mock("@/contexts/ReadinessContext", () => ({ useReadiness: () => mock.readiness }));
vi.mock("@/contexts/SettingsContext", () => ({ useSettings: () => ({ settings: mock.settings }) }));
vi.mock("@/hooks/useTranscriptionHistory", () => ({ useTranscriptionHistory: () => ({ history: mock.history, totalCount: mock.history.length, isLoading: mock.isLoading, loadError: mock.loadError, refreshHistory: vi.fn() }) }));
vi.mock("@/contexts/ModelManagementContext", () => ({ useModelManagementContext: () => ({ downloadProgress: mock.downloadProgress }) }));
vi.mock("./overview/useActiveRemoteLabel", () => ({ useActiveRemoteLabel: () => null }));
vi.mock("@/lib/platform", () => ({ get isMacOS() { return mock.mac; } }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async (command: string) => command === "get_ai_settings" ? { enabled: true } : command === "get_effective_primary_shortcut" ? (() => { const native = mock.settings.hotkey ? null : mock.shortcutBindings.find((binding) => binding.enabled && (binding.action === "hold_to_record" || binding.action === "toggle_recording")); return { binding: native ?? null, hotkey: mock.settings.hotkey || (native ? null : "CommandOrControl+Shift+Space"), mode: native?.action === "hold_to_record" || (!native && mock.settings.recording_mode === "push_to_talk") ? "hold" : "toggle" }; })() : { preset: "CleanDictation" }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => { mock.listeners[name] = handler; return () => { delete mock.listeners[name]; }; }) }));
vi.mock("@/components/ShareStatsModal", () => ({ ShareStatsModal: () => null }));

beforeEach(() => {
  mock.settings = { hotkey: "Alt+Space", current_model: "parakeet-tdt-0.6b-v3", current_model_engine: "parakeet", speech_language: "en", recording_mode: "push_to_talk", transcription_mode: "live_preview" };
  mock.readiness = { canRecord: true, selectedModelAvailable: true, remoteSelected: false, remoteAvailable: true, licenseValid: true, licenseStatus: "licensed", hasMicrophonePermission: true };
  mock.downloadProgress = {};
  mock.shortcutBindings = [];
  mock.history = [];
  mock.isLoading = false;
  mock.loadError = null;
  mock.mac = true;
  mock.listeners = {};
});

describe("Home", () => {
  it("shows the active local engine, key caps, recording mode and setup chips", async () => {
    const onNavigate = vi.fn();
    render(<OverviewTab onNavigate={onNavigate} />);
    expect(screen.getByText("Ready · Parakeet V3 runs on this Mac")).toHaveClass("text-foreground");
    expect(screen.getByRole("heading", { name: /Press.*and start talking/ })).toBeInTheDocument();
    expect(screen.getByText("⌥")).toBeInTheDocument();
    expect(screen.getByText("Space")).toBeInTheDocument();
    expect(screen.getByText(/Hold to talk, release to paste/)).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: /PolishClean/ })).toBeInTheDocument());
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /EngineParakeet V3/ }));
    await user.click(screen.getByRole("button", { name: /LanguageEnglish/ }));
    await user.click(screen.getByRole("button", { name: /PolishClean/ }));
    await user.click(screen.getByRole("button", { name: /Live previewOn/ }));
    expect(onNavigate.mock.calls.map(([screenId]) => screenId)).toEqual(["transcription", "transcription", "polish", "transcription"]);
  });

  it("shows cloud readiness and Windows copy", () => {
    mock.mac = false;
    mock.settings = { ...mock.settings, hotkey: "Control+Alt+Space", current_model: "soniox", current_model_engine: "soniox", recording_mode: "toggle" };
    render(<OverviewTab />);
    expect(screen.getByText("Ready · Soniox (cloud)")).toBeInTheDocument();
    expect(screen.getByText("Ctrl")).toBeInTheDocument();
    expect(screen.getByText(/Press once to start, again to paste/)).toBeInTheDocument();
  });

  it("shows the enabled native Hold shortcut and its effective mode despite a stale toggle setting and disabled preferred id", async () => {
    mock.settings = { ...mock.settings, hotkey: "", recording_mode: "toggle" };
    mock.shortcutBindings = [
      { id: "onboarding-primary-hold", action: "hold_to_record", shortcut: "", trigger: "hold", enabled: false, allow_risky_combo: false, trigger_kind: "modifier_hold", modifier: { modifier: "meta", side: "left" } },
      { id: "active-native", action: "hold_to_record", shortcut: "", trigger: "hold", enabled: true, allow_risky_combo: false, trigger_kind: "modifier_hold", modifier: { modifier: "alt", side: "right" } },
    ];
    render(<OverviewTab />);
    expect(await screen.findByText("Right ⌥")).toBeInTheDocument();
    expect(screen.getByText(/Hold to talk, release to paste/)).toBeInTheDocument();
    expect(screen.queryByText("Left ⌘")).not.toBeInTheDocument();
    expect(screen.queryByText("your recording shortcut")).not.toBeInTheDocument();
  });

  it.each([{ bindings: [] }, { bindings: [{ id: "onboarding-primary-hold", action: "hold_to_record", shortcut: "", trigger: "hold", enabled: false, allow_risky_combo: false, trigger_kind: "modifier_hold", modifier: { modifier: "alt", side: "right" } }] }])(
    "shows the backend fallback for empty or disabled-only native bindings",
    async ({ bindings }) => {
      mock.settings = { ...mock.settings, hotkey: "", recording_mode: "toggle" };
      mock.shortcutBindings = bindings;
      render(<OverviewTab />);
      expect(await screen.findByText("⌘")).toBeInTheDocument();
      expect(screen.getByText("⇧")).toBeInTheDocument();
      expect(screen.getByText("Space")).toBeInTheDocument();
    },
  );

  it.each([
    ["license", { licenseValid: false, licenseStatus: "expired", canRecord: false }, "License needs attention", "license", undefined],
    ["microphone mac", { hasMicrophonePermission: false, canRecord: false }, "Microphone access needed", undefined, "advanced"],
    ["remote", { remoteSelected: true, remoteAvailable: false, canRecord: false }, "Remote source unavailable", "transcription", undefined],
  ])("routes %s to its resolution", async (_case, readiness, label, screenId, pane) => {
    mock.readiness = { ...mock.readiness, ...readiness };
    const onNavigate = vi.fn(); const onNavigateSettingsPane = vi.fn();
    render(<OverviewTab onNavigate={onNavigate} onNavigateSettingsPane={onNavigateSettingsPane} />);
    const warning = screen.getByRole("button", { name: new RegExp(label) });
    expect(warning).toHaveClass("text-foreground");
    await userEvent.setup().click(warning);
    if (pane) expect(onNavigateSettingsPane).toHaveBeenCalledWith(pane);
    else expect(onNavigate).toHaveBeenCalledWith(screenId);
  });

  it("routes a denied Windows microphone to Recording", async () => {
    mock.mac = false; mock.readiness = { ...mock.readiness, hasMicrophonePermission: false, canRecord: false };
    const onNavigate = vi.fn();
    render(<OverviewTab onNavigate={onNavigate} />);
    await userEvent.setup().click(screen.getByRole("button", { name: /Microphone access needed/ }));
    expect(onNavigate).toHaveBeenCalledWith("recording");
  });

  it("prioritizes license and microphone blockers over an online remote source", () => {
    mock.readiness = { ...mock.readiness, remoteSelected: true, remoteAvailable: true, licenseValid: false, licenseStatus: "expired", canRecord: false };
    const { rerender } = render(<OverviewTab />);
    expect(screen.getByRole("button", { name: /License needs attention/ })).toBeInTheDocument();
    mock.readiness = { ...mock.readiness, licenseValid: true, licenseStatus: "licensed", hasMicrophonePermission: false };
    rerender(<OverviewTab />);
    expect(screen.getByRole("button", { name: /Microphone access needed/ })).toBeInTheDocument();
  });

  it.each([
    ["missing model", "", "parakeet", false, "No model yet"],
    ["missing cloud key", "soniox", "soniox", false, "Needs an API key"],
    ["unavailable local model", "parakeet-tdt-0.6b-v3", "parakeet", false, "No model yet"],
  ])("routes %s to Transcription", async (_case, model, engine, available, label) => {
    mock.settings = { ...mock.settings, current_model: model, current_model_engine: engine as AppSettings["current_model_engine"] };
    mock.readiness = { ...mock.readiness, canRecord: false, selectedModelAvailable: available };
    const onNavigate = vi.fn();
    render(<OverviewTab onNavigate={onNavigate} />);
    await userEvent.setup().click(screen.getByRole("button", { name: new RegExp(label) }));
    expect(onNavigate).toHaveBeenCalledWith("transcription");
  });

  it("opens the Cloud card for a missing API key", async () => {
    mock.settings = { ...mock.settings, current_model: "soniox", current_model_engine: "soniox" };
    mock.readiness = { ...mock.readiness, canRecord: false, selectedModelAvailable: false };
    const onNavigate = vi.fn(); const onSourceFilterChange = vi.fn();
    render(<OverviewTab onNavigate={onNavigate} onSourceFilterChange={onSourceFilterChange} />);
    await userEvent.setup().click(screen.getByRole("button", { name: /Needs an API key/ }));
    expect(onSourceFilterChange).toHaveBeenCalledWith("cloud");
    expect(onNavigate).toHaveBeenCalledWith("transcription");
  });

  it("uses the selected model's context download state and clears it", async () => {
    mock.readiness = { ...mock.readiness, canRecord: false, selectedModelAvailable: false };
    mock.downloadProgress = { "parakeet-tdt-0.6b-v3": 42 };
    const { rerender } = render(<OverviewTab />);
    expect(screen.getByRole("button", { name: /Downloading Parakeet V3 · 42%/ })).toBeInTheDocument();
    mock.downloadProgress = {};
    rerender(<OverviewTab />);
    expect(screen.queryByRole("button", { name: /Downloading/ })).not.toBeInTheDocument();
    mock.settings = { ...mock.settings, current_model: "other-model" };
    rerender(<OverviewTab />);
    expect(screen.queryByRole("button", { name: /42%/ })).not.toBeInTheDocument();
  });

  it("shows an empty Recent state and the last four dictations with app and time", async () => {
    const { rerender } = render(<OverviewTab />);
    expect(screen.getByText("Your dictations will show up here.")).toBeInTheDocument();
    expect(screen.getByText("nothing yet in the last 7 days")).toBeInTheDocument();
    expect(screen.getByText("—")).toBeInTheDocument();
    mock.history = Array.from({ length: 5 }, (_, index) => ({ id: String(index), text: `Dictation number ${index}`, timestamp: new Date(Date.now() - index * 60_000), model: "parakeet", writing: { context_hint: { app_name: "Notes" }, audio_duration_ms: 3000 } }));
    rerender(<OverviewTab />);
    expect(screen.getByText("Dictation number 0")).toBeInTheDocument();
    expect(screen.queryByText("Dictation number 4")).not.toBeInTheDocument();
    expect(screen.getAllByText("Notes")).toHaveLength(4);
    expect(screen.getByText("2m")).toBeInTheDocument();
    expect(screen.queryByText("nothing yet in the last 7 days")).not.toBeInTheDocument();
    expect(screen.getByRole("img", { name: /dictations on/ })).toHaveAccessibleName(/5 dictations on/);
    expect(screen.getAllByText("Notes")[0]).toHaveClass("text-muted-foreground");
    const onNavigate = vi.fn();
    rerender(<OverviewTab onNavigate={onNavigate} />);
    await userEvent.setup().click(screen.getByRole("button", { name: /View all history/ }));
    expect(onNavigate).toHaveBeenCalledWith("history");
  });

  it("describes a short dictation without treating zero rounded savings as empty", () => {
    mock.history = [{ id: "short", text: "hello", timestamp: new Date(), model: "parakeet", writing: { audio_duration_ms: 1000 } }];
    render(<OverviewTab />);
    expect(screen.getByText("less than a minute estimated saved")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Last 7 days" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /1 dictations on/ })).toBeInTheDocument();
  });

  it("keeps test dictation neutral through input before idle and cancellation", async () => {
    render(<OverviewTab />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Try a test dictation" }));
    const textarea = screen.getByRole("textbox", { name: "Test dictation" });
    expect(textarea).toHaveFocus();
    await user.type(textarea, "This really works");
    expect(screen.getByText("3 words")).toBeInTheDocument();
    fireEvent.paste(textarea, { clipboardData: { getData: () => "manually pasted" } });
    fireEvent.change(textarea, { target: { value: "This really works manually pasted" } });
    expect(screen.getByText("5 words")).toBeInTheDocument();
    fireEvent.change(textarea, { target: { value: "This really works manually pasted now" } });
    expect(screen.getByText("6 words")).toBeInTheDocument();
    expect(screen.queryByText(/Worked/)).not.toBeInTheDocument();
    expect(mock.listeners["recording-state-changed"]).toBeUndefined();
    expect(mock.listeners["transcription-complete"]).toBeUndefined();
    await user.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("textbox", { name: "Test dictation" })).not.toBeInTheDocument());
  });

});
