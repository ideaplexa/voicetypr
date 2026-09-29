import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defaultWritingSettings, type WritingSettings } from "@/types/writing";
import { toast } from "sonner";

let useWritingSettings: typeof import("./writingSettings").useWritingSettings;

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

describe("shared writing settings", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    vi.resetModules();
    ({ useWritingSettings } = await import("./writingSettings"));
  });

  it("merges edits from two consumers and serializes their full-blob saves", async () => {
    const saves: WritingSettings[] = [];
    let finishFirstSave: (() => void) | undefined;
    const firstSave = new Promise<void>((resolve) => {
      finishFirstSave = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_writing_settings") return defaultWritingSettings;
      if (command === "update_writing_settings") {
        saves.push((args as { settings: WritingSettings }).settings);
        if (saves.length === 1) await firstSave;
        return;
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const wordsConsumer = renderHook(() => useWritingSettings());
    const snippetsConsumer = renderHook(() => useWritingSettings());
    await act(async () => {
      expect(await wordsConsumer.result.current.load()).toBe(true);
    });

    act(() => {
      wordsConsumer.result.current.update({
        custom_words: [{ phrase: "Voicetypr", enabled: true }],
      });
      snippetsConsumer.result.current.update({
        snippets: [{ trigger: "sig", body: "Regards", enabled: true, preserve_literal: false }],
      });
    });

    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0].custom_words).toHaveLength(1);
    expect(saves[0].snippets).toHaveLength(0);
    expect(snippetsConsumer.result.current.settings.snippets).toHaveLength(1);
    finishFirstSave?.();

    await waitFor(() => expect(saves).toHaveLength(2));
    expect(saves[1].custom_words).toHaveLength(1);
    expect(saves[1].snippets).toHaveLength(1);
  });

  it("restores the last persisted snapshot after two consecutive saves fail", async () => {
    const persisted = {
      ...defaultWritingSettings,
      custom_words: [{ phrase: "On disk", enabled: true }],
    };
    let saveCount = 0;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_writing_settings") return persisted;
      if (command === "update_writing_settings") {
        saveCount += 1;
        throw new Error(`Save ${saveCount} failed`);
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const consumer = renderHook(() => useWritingSettings());
    await act(async () => expect(await consumer.result.current.load()).toBe(true));
    act(() => {
      consumer.result.current.update({ custom_words: [{ phrase: "A", enabled: true }] });
      consumer.result.current.update({ snippets: [{ trigger: "B", body: "B", enabled: true, preserve_literal: false }] });
    });

    await waitFor(() => expect(saveCount).toBe(2));
    await waitFor(() => expect(consumer.result.current.settings).toEqual(persisted));
    expect(toast.error).toHaveBeenCalledTimes(1);
  });

  it("keeps a shared load when a second consumer cancels", async () => {
    let finishLoad: ((settings: WritingSettings) => void) | undefined;
    const response = new Promise<WritingSettings>((resolve) => { finishLoad = resolve; });
    const saved: WritingSettings[] = [];
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_writing_settings") return response;
      if (command === "update_writing_settings") {
        saved.push((args as { settings: WritingSettings }).settings);
        return;
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const first = renderHook(() => useWritingSettings());
    const second = renderHook(() => useWritingSettings());
    const firstAbort = new AbortController();
    const secondAbort = new AbortController();
    const firstLoad = first.result.current.load(firstAbort.signal);
    const secondLoad = second.result.current.load(secondAbort.signal);
    act(() => {
      first.result.current.update({ custom_words: [{ phrase: "Pending", enabled: true }] });
      secondAbort.abort();
    });
    await act(async () => finishLoad?.(defaultWritingSettings));

    expect(await firstLoad).toBe(true);
    expect(await secondLoad).toBe(false);
    expect(first.result.current.loaded).toBe(true);
    await waitFor(() => expect(saved[0].custom_words[0].phrase).toBe("Pending"));
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "get_writing_settings")).toHaveLength(1);
  });
});
