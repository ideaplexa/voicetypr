import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings } from "@/types";
import type { KeptRecording } from "@/lib/home-needs";
import { invoke } from "@tauri-apps/api/core";
import { OverviewTab } from "./OverviewTab";

const mock = vi.hoisted(() => ({
  settings: { hotkey: "Alt+Space", current_model: "parakeet-tdt-0.6b-v3", current_model_engine: "parakeet", speech_language: "en", recording_mode: "push_to_talk", transcription_mode: "live_preview" } as Partial<AppSettings>,
  readiness: { hasAccessibilityPermission: true as boolean | null, canRecord: true, selectedModelAvailable: true as boolean | null, remoteSelected: false, remoteAvailable: true, licenseValid: true, licenseStatus: "licensed" as string | null, hasMicrophonePermission: true },
  downloadProgress: {} as Record<string, number>,
  shortcutBindings: [] as Array<{ id: string; action: string; shortcut: string; trigger: string; enabled: boolean; allow_risky_combo: boolean; trigger_kind: string; modifier: { modifier: string; side: string } }>,
  mac: true,
  polishOn: true,
  kept: [] as KeptRecording[],
  update: { available: null as null | { version: string }, checking: false },
  words: 3,
  promptInstall: vi.fn(),
  refreshKept: vi.fn(),
}));

vi.mock("@/contexts/ReadinessContext", () => ({ useReadiness: () => mock.readiness }));
vi.mock("@/contexts/SettingsContext", () => ({ useSettings: () => ({ settings: mock.settings }) }));
vi.mock("@/hooks/useKeptRecordings", () => ({ useKeptRecordings: () => ({ kept: mock.kept, refresh: mock.refreshKept }) }));
vi.mock("@/hooks/useUpdateStatus", () => ({ useUpdateStatus: () => ({ ...mock.update, lastChecked: null, result: "idle", checkNow: vi.fn(), promptInstall: mock.promptInstall }) }));
vi.mock("@/state/writingSettings", () => ({ useWritingSettings: (select: (state: unknown) => unknown) => select({ settings: { custom_words: Array.from({ length: mock.words }, () => ({})), replacements: [], snippets: [] }, loaded: true, load: vi.fn() }) }));
vi.mock("@/contexts/ModelManagementContext", () => ({ useDownloadProgress: (select?: (p: Record<string, number>) => unknown) => select ? select(mock.downloadProgress) : mock.downloadProgress }));
vi.mock("./overview/useActiveRemoteLabel", () => ({ useActiveRemoteLabel: () => null }));
vi.mock("@/lib/platform", () => ({ get isMacOS() { return mock.mac; } }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async (command: string) => command === "get_ai_settings" ? { enabled: mock.polishOn } : command === "retry_kept_dictation" ? undefined : command === "get_effective_primary_shortcut" ? (() => { const native = mock.settings.hotkey ? null : mock.shortcutBindings.find((binding) => binding.enabled && (binding.action === "hold_to_record" || binding.action === "toggle_recording")); return { binding: native ?? null, hotkey: mock.settings.hotkey || (native ? null : "CommandOrControl+Shift+Space"), mode: native?.action === "hold_to_record" || (!native && mock.settings.recording_mode === "push_to_talk") ? "hold" : "toggle" }; })() : { preset: "CleanDictation" }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

beforeEach(() => {
  mock.settings = { hotkey: "Alt+Space", current_model: "parakeet-tdt-0.6b-v3", current_model_engine: "parakeet", speech_language: "en", recording_mode: "push_to_talk", transcription_mode: "live_preview" };
  mock.readiness = { canRecord: true, selectedModelAvailable: true, remoteSelected: false, remoteAvailable: true, licenseValid: true, licenseStatus: "licensed", hasMicrophonePermission: true, hasAccessibilityPermission: true };
  mock.downloadProgress = {};
  mock.shortcutBindings = [];
  mock.mac = true;
  mock.polishOn = true;
  mock.kept = [];
  mock.update = { available: null, checking: false };
  mock.words = 3;
  mock.promptInstall.mockClear();
  mock.refreshKept.mockClear();
});

describe("Home", () => {
  it("shows the active local engine, key caps, recording mode and setup chips", async () => {
    const onNavigate = vi.fn();
    render(<OverviewTab onNavigate={onNavigate} />);
    expect(screen.getByText("Ready · Parakeet v3 runs on this Mac")).toHaveClass("text-sage");
    expect(screen.getByRole("heading", { name: /Press.*and start talking/ })).toBeInTheDocument();
    expect(screen.getByText("⌥ Space").tagName).toBe("KBD");
    expect(screen.getByText(/Hold to talk, release to paste/)).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: /PolishClean/ })).toBeInTheDocument());
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /EngineParakeet v3/ }));
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
    expect(screen.getByText("Ctrl + Alt + Space")).toBeInTheDocument();
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
      expect((await screen.findByText("⌘ ⇧ Space")).tagName).toBe("KBD");
    },
  );

  it.each([
    ["license", { licenseValid: false, licenseStatus: "expired", canRecord: false }, "License needs attention", "license", undefined],
    ["microphone mac", { hasMicrophonePermission: false, canRecord: false }, "Microphone access needed", undefined, "advanced"],
    ["remote", { remoteSelected: true, remoteAvailable: false, canRecord: false }, "Remote source unavailable", "transcription", undefined],
    ["missing model", { canRecord: false, selectedModelAvailable: false }, "No model yet", "transcription", undefined],
  ])("lists %s under Needs you and routes its fix", async (_case, readiness, label, screenId, pane) => {
    mock.readiness = { ...mock.readiness, ...readiness };
    const onNavigate = vi.fn(); const onNavigateSettingsPane = vi.fn();
    render(<OverviewTab onNavigate={onNavigate} onNavigateSettingsPane={onNavigateSettingsPane} />);
    const section = screen.getByRole("region", { name: "Needs you" });
    await userEvent.setup().click(within(section).getByRole("button", { name: new RegExp(label) }));
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

  it("puts the license problem ahead of an online remote source", () => {
    mock.readiness = { ...mock.readiness, remoteSelected: true, remoteAvailable: true, licenseValid: false, licenseStatus: "expired", canRecord: false };
    render(<OverviewTab />);
    expect(screen.getByRole("button", { name: /License needs attention/ })).toBeInTheDocument();
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

  it("asks for Accessibility on macOS only", () => {
    mock.readiness = { ...mock.readiness, hasAccessibilityPermission: false };
    const { unmount } = render(<OverviewTab />);
    expect(screen.getByText("Accessibility access needed")).toBeInTheDocument();
    unmount();
    mock.mac = false;
    render(<OverviewTab />);
    expect(screen.queryByText("Accessibility access needed")).not.toBeInTheDocument();
  });

  it("uses the selected model's context download state and clears it", async () => {
    mock.readiness = { ...mock.readiness, canRecord: false, selectedModelAvailable: false };
    mock.downloadProgress = { "parakeet-tdt-0.6b-v3": 42 };
    const { rerender } = render(<OverviewTab />);
    expect(screen.getByRole("button", { name: /Downloading Parakeet v3 · 42%/ })).toBeInTheDocument();
    mock.downloadProgress = {};
    rerender(<OverviewTab />);
    expect(screen.queryByRole("button", { name: /Downloading/ })).not.toBeInTheDocument();
    mock.settings = { ...mock.settings, current_model: "other-model" };
    rerender(<OverviewTab />);
    expect(screen.queryByRole("button", { name: /42%/ })).not.toBeInTheDocument();
  });

  it("shows only the hero when nothing needs attention", () => {
    mock.polishOn = true; mock.words = 3;
    render(<OverviewTab />);
    expect(screen.getByRole("heading", { name: /Press.*and start talking/ })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Needs you" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Get more out of Voicetypr" })).not.toBeInTheDocument();
  });

  it("drops Recent, This week and the test dictation from Home", () => {
    render(<OverviewTab />);
    expect(screen.queryByText("Recent")).not.toBeInTheDocument();
    expect(screen.queryByText("This week")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Try a test dictation" })).not.toBeInTheDocument();
  });

  it("follows the real recording mode in the helper line", () => {
    mock.settings = { ...mock.settings, recording_mode: "toggle" };
    render(<OverviewTab />);
    expect(screen.getByText(/Press once to start, again to paste/)).toBeInTheDocument();
    expect(screen.queryByText(/Hold to talk/)).not.toBeInTheDocument();
  });

  it("offers Retry for a kept recording and refreshes afterwards", async () => {
    mock.kept = [{ id: "k1", engine_short: "Soniox", alt_engine_short: "Parakeet v3", alt_engine_id: "parakeet-tdt-0.6b-v3", remaining_ms: 540_000, busy: false }];
    render(<OverviewTab />);
    const section = screen.getByRole("region", { name: "Needs you" });
    expect(within(section).getByText("A recording was kept")).toBeInTheDocument();
    expect(within(section).getByText(/Audio kept for 9 more minutes/)).toBeInTheDocument();
    await userEvent.setup().click(within(section).getByRole("button", { name: /Retry/ }));
    expect(invoke).toHaveBeenCalledWith("retry_kept_dictation", { id: "k1", engine: "parakeet-tdt-0.6b-v3" });
    await waitFor(() => expect(mock.refreshKept).toHaveBeenCalled());
  });

  it("shows a ready update with Install that opens the update dialog", async () => {
    mock.update = { available: { version: "2.1.0-beta.6" }, checking: false };
    render(<OverviewTab />);
    expect(screen.getByText("Update ready · 2.1.0-beta.6")).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: /Install/ }));
    expect(mock.promptInstall).toHaveBeenCalledOnce();
  });

  it("suggests Polish only while it is off and words only while the dictionary is empty", async () => {
    mock.polishOn = false; mock.words = 0;
    const onNavigate = vi.fn();
    const { unmount } = render(<OverviewTab onNavigate={onNavigate} />);
    const section = await screen.findByRole("region", { name: "Get more out of Voicetypr" });
    expect(within(section).getByText("Turn on Polish")).toBeInTheDocument();
    expect(within(section).getByText("Teach it your words")).toBeInTheDocument();
    const user = userEvent.setup();
    await user.click(within(section).getByRole("button", { name: /Set up/ }));
    await user.click(within(section).getByRole("button", { name: /Add words/ }));
    expect(onNavigate.mock.calls.map(([id]) => id)).toEqual(["polish", "dictionary"]);
    unmount();
    mock.polishOn = true; mock.words = 2;
    render(<OverviewTab />);
    await waitFor(() => expect(screen.getByRole("button", { name: /PolishClean/ })).toBeInTheDocument());
    expect(screen.queryByText("Turn on Polish")).not.toBeInTheDocument();
    expect(screen.queryByText("Teach it your words")).not.toBeInTheDocument();
  });

  it("hides a tip's section when only one applies", async () => {
    mock.polishOn = false; mock.words = 4;
    render(<OverviewTab />);
    expect(await screen.findByText("Turn on Polish")).toBeInTheDocument();
    expect(screen.queryByText("Teach it your words")).not.toBeInTheDocument();
  });
});
