import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { localDate } from "@/lib/first-week";
import type { FirstWeekProgress } from "@/types";

const mock = vi.hoisted(() => ({
  progress: null as unknown as FirstWeekProgress,
  mode: "toggle" as "toggle" | "hold",
  invoke: vi.fn(),
  refreshSettings: vi.fn(async () => undefined),
  changeRecordingMode: vi.fn(async () => undefined),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mock.invoke }));
vi.mock("@/contexts/SettingsContext", () => ({ useSettings: () => ({ settings: { current_model: "parakeet-v3", current_model_engine: "parakeet", first_week: mock.progress }, refreshSettings: mock.refreshSettings }) }));
vi.mock("@/contexts/ReadinessContext", () => ({ useReadiness: () => ({ licenseStatus: "trial", licenseValid: true, remoteSelected: false, selectedModelAvailable: true, hasAccessibilityPermission: true }) }));
vi.mock("@/state/writingSettings", () => {
  const state = { settings: { editing_commands: false }, loaded: true, update: vi.fn(), flush: vi.fn(async () => undefined) };
  const hook = (select: (s: typeof state) => unknown) => select(state);
  hook.getState = () => state;
  return { useWritingSettings: hook };
});
vi.mock("@/components/sections/models/useRealtimeWords", () => ({ useRealtimeWords: () => ({ streams: false, realtimeOn: false, setRealtime: vi.fn() }) }));
vi.mock("@/components/sections/recording/useRecordingHotkey", () => ({ useRecordingHotkey: () => ({ changeRecordingMode: mock.changeRecordingMode }) }));
vi.mock("@/lib/primary-shortcut", () => ({ loadEffectivePrimaryShortcut: async () => ({ mode: mock.mode, hotkey: "Alt+Space", binding: null }) }));
vi.mock("@/hooks/useTauriEvent", () => ({ useTauriEvent: () => undefined }));

import { FirstWeekCard } from "./FirstWeekCard";

const renderCard = (onNavigate = vi.fn()) => render(<FirstWeekCard sourceReady polishOn={false} polishReady={false} dictionaryEmpty holdToTalk={false} shortcut={["⌥", "Space"]} onNavigate={onNavigate} />);
const savedProgress = () => mock.invoke.mock.calls.filter(([command]) => command === "set_first_week_progress").map(([, args]) => (args as { progress: FirstWeekProgress }).progress);

describe("FirstWeekCard on Home", () => {
  beforeEach(() => {
    mock.progress = { day: 1, last_seen: localDate(new Date()), dismissed: [] };
    mock.mode = "toggle";
    mock.invoke.mockReset();
    mock.invoke.mockImplementation(async (command: string) => command === "get_sharing_status" ? { enabled: false } : undefined);
  });

  it("starts day 1 the first time Home opens, through its own command", async () => {
    mock.progress = { day: 0, last_seen: null, dismissed: [] };
    renderCard();
    await waitFor(() => expect(savedProgress()).toEqual([{ day: 1, last_seen: localDate(new Date()), dismissed: [] }]));
  });

  it("puts the card away when Hold to talk really turned on", async () => {
    mock.changeRecordingMode.mockImplementation(async () => { mock.mode = "hold"; });
    const onNavigate = vi.fn();
    renderCard(onNavigate);
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    await waitFor(() => expect(savedProgress()).toEqual([{ ...mock.progress, dismissed: ["hold"] }]));
    expect(onNavigate).toHaveBeenCalledWith("recording");
  });

  it("keeps the card when turning on Hold to talk failed", async () => {
    mock.changeRecordingMode.mockImplementation(async () => undefined);
    const onNavigate = vi.fn();
    renderCard(onNavigate);
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    await waitFor(() => expect(onNavigate).toHaveBeenCalledWith("recording"));
    expect(savedProgress()).toEqual([]);
  });

  it("can be dismissed without turning anything on", async () => {
    renderCard();
    fireEvent.click(screen.getByRole("button", { name: "Hide “Hold to talk”" }));
    await waitFor(() => expect(savedProgress()).toEqual([{ ...mock.progress, dismissed: ["hold"] }]));
    expect(mock.changeRecordingMode).not.toHaveBeenCalled();
  });
});
