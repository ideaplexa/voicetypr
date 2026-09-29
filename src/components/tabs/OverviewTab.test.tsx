import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings, TranscriptionHistory } from "@/types";
import { OverviewTab } from "./OverviewTab";

const mock = vi.hoisted(() => ({
  settings: { hotkey: "Alt+Space", current_model: "parakeet-tdt-0.6b-v3", current_model_engine: "parakeet", speech_language: "en", recording_mode: "push_to_talk", transcription_mode: "live_preview" } as Partial<AppSettings>,
  readiness: { canRecord: true, selectedModelAvailable: true as boolean | null, remoteSelected: false },
  history: [] as TranscriptionHistory[],
  isLoading: false,
  loadError: null as string | null,
  mac: true,
  progress: null as ((payload: { payload: { model: string; progress: number } }) => void) | null,
}));

vi.mock("@/contexts/ReadinessContext", () => ({ useReadiness: () => mock.readiness }));
vi.mock("@/contexts/SettingsContext", () => ({ useSettings: () => ({ settings: mock.settings }) }));
vi.mock("@/hooks/useTranscriptionHistory", () => ({ useTranscriptionHistory: () => ({ history: mock.history, totalCount: mock.history.length, isLoading: mock.isLoading, loadError: mock.loadError, refreshHistory: vi.fn() }) }));
vi.mock("@/hooks/useActiveTrigger", () => ({ useActiveTrigger: (hotkey: string) => ({ hotkey, kbdLabel: hotkey || "Right ⌥" }) }));
vi.mock("./overview/useActiveRemoteLabel", () => ({ useActiveRemoteLabel: () => null }));
vi.mock("@/lib/platform", () => ({ get isMacOS() { return mock.mac; } }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async (command: string) => command === "get_ai_settings" ? { enabled: true } : { preset: "CleanDictation" }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (_name: string, handler: typeof mock.progress) => { mock.progress = handler; return () => {}; }) }));
vi.mock("@/components/ShareStatsModal", () => ({ ShareStatsModal: () => null }));

beforeEach(() => {
  mock.settings = { hotkey: "Alt+Space", current_model: "parakeet-tdt-0.6b-v3", current_model_engine: "parakeet", speech_language: "en", recording_mode: "push_to_talk", transcription_mode: "live_preview" };
  mock.readiness = { canRecord: true, selectedModelAvailable: true, remoteSelected: false };
  mock.history = [];
  mock.isLoading = false;
  mock.loadError = null;
  mock.mac = true;
  mock.progress = null;
});

describe("Home", () => {
  it("shows the active local engine, key caps, recording mode and setup chips", async () => {
    const onNavigate = vi.fn();
    render(<OverviewTab onNavigate={onNavigate} />);
    expect(screen.getByText("Ready · Parakeet V3 runs on this Mac")).toBeInTheDocument();
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

  it("shows live download progress", async () => {
    mock.readiness = { ...mock.readiness, canRecord: false, selectedModelAvailable: false };
    render(<OverviewTab />);
    await waitFor(() => expect(mock.progress).not.toBeNull());
    act(() => { mock.progress?.({ payload: { model: "parakeet-tdt-0.6b-v3", progress: 42 } }); });
    expect(await screen.findByRole("button", { name: /Downloading Parakeet V3 · 42%/ })).toBeInTheDocument();
  });

  it("shows an empty Recent state and the last four dictations with app and time", async () => {
    const { rerender } = render(<OverviewTab />);
    expect(screen.getByText("Your dictations will show up here.")).toBeInTheDocument();
    expect(screen.getByText("nothing yet this week")).toBeInTheDocument();
    expect(screen.getByText("—")).toBeInTheDocument();
    mock.history = Array.from({ length: 5 }, (_, index) => ({ id: String(index), text: `Dictation number ${index}`, timestamp: new Date(Date.now() - index * 60_000), model: "parakeet", writing: { context_hint: { app_name: "Notes" }, audio_duration_ms: 3000 } }));
    rerender(<OverviewTab />);
    expect(screen.getByText("Dictation number 0")).toBeInTheDocument();
    expect(screen.queryByText("Dictation number 4")).not.toBeInTheDocument();
    expect(screen.getAllByText("Notes")).toHaveLength(4);
    expect(screen.getByText("2m")).toBeInTheDocument();
    const onNavigate = vi.fn();
    rerender(<OverviewTab onNavigate={onNavigate} />);
    await userEvent.setup().click(screen.getByRole("button", { name: /View all history/ }));
    expect(onNavigate).toHaveBeenCalledWith("history");
  });

  it("autofocuses the try box and reports pasted words", async () => {
    render(<OverviewTab />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Try a test dictation" }));
    const textarea = screen.getByRole("textbox", { name: "Test dictation" });
    expect(textarea).toHaveFocus();
    await user.type(textarea, "This really works");
    expect(screen.getByText("Worked · 3 words")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("textbox", { name: "Test dictation" })).not.toBeInTheDocument());
  });
});
