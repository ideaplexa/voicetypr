import type { PrimaryMode } from "@/lib/primary-shortcut";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  settings: { hotkey: "Control+Space", recording_mode: "toggle" as PrimaryMode },
  invoke: vi.fn(),
}));
vi.mock("@/contexts/SettingsContext", () => ({ useSettings: () => ({ settings: mocks.settings, refreshSettings: vi.fn() }) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
import { useActiveTrigger } from "@/hooks/useActiveTrigger";
import { useRecordingHotkey } from "@/components/sections/recording/useRecordingHotkey";

beforeEach(() => {
  mocks.settings = { hotkey: "Control+Space", recording_mode: "toggle" };
  mocks.invoke.mockReset();
  mocks.invoke.mockResolvedValue({ binding: null, hotkey: "Control+Space", mode: "toggle" });
});

it("refreshes Home instructions and Recording selection after a tray settings refresh changes only mode", async () => {
  const { result, rerender } = renderHook(() => ({
    home: useActiveTrigger(mocks.settings),
    recording: useRecordingHotkey(),
  }));
  await waitFor(() => expect(result.current.recording.effective?.mode).toBe("toggle"));
  mocks.invoke.mockResolvedValue({ binding: null, hotkey: "Control+Space", mode: "hold" });
  mocks.settings = { ...mocks.settings, recording_mode: "push_to_talk" };
  rerender();
  await waitFor(() => {
    expect(result.current.home.mode).toBe("push_to_talk");
    expect(result.current.recording.effective?.mode).toBe("hold");
  });
  act(() => result.current.recording.startEditing());
  expect(result.current.recording.holdToTalk).toBe(true);
});
