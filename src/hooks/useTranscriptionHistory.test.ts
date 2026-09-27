import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useTranscriptionHistory } from "./useTranscriptionHistory";

const eventHarness = vi.hoisted(() => {
  const callbacks = new Map<string, (payload?: unknown) => void>();
  const registerEvent = vi.fn((event: string, callback: (payload?: unknown) => void) => {
    callbacks.set(event, callback);
    return vi.fn();
  });

  return { callbacks, registerEvent };
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@/hooks/useEventCoordinator", () => ({
  useEventCoordinator: () => ({
    registerEvent: eventHarness.registerEvent,
  }),
}));

describe("useTranscriptionHistory", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventHarness.callbacks.clear();
  });

  it("loads history and total count when requested", async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((command: string) => {
      if (command === "get_transcription_history") {
        return Promise.resolve([
          {
            timestamp: "2026-05-18T10:00:00.000Z",
            text: "hello world",
            model: "base.en",
            recording_file: "one.wav",
            source_recording_id: "source-1",
            status: "completed",
          },
        ]);
      }
      if (command === "get_transcription_count") {
        return Promise.resolve(7);
      }
      return Promise.reject(new Error(`Unknown command: ${command}`));
    });

    const { result } = renderHook(() =>
      useTranscriptionHistory({ limit: 500, includeTotalCount: true }),
    );

    expect(result.current.isLoading).toBe(true);
    expect(result.current.loadError).toBeNull();

    await waitFor(() => {
      expect(result.current.history).toHaveLength(1);
    });

    expect(invoke).toHaveBeenCalledWith("get_transcription_history", {
      limit: 500,
    });
    expect(invoke).toHaveBeenCalledWith("get_transcription_count");
    expect(result.current.totalCount).toBe(7);
    expect(result.current.isLoading).toBe(false);
    expect(result.current.loadError).toBeNull();
    expect(result.current.history[0]).toMatchObject({
      id: "2026-05-18T10:00:00.000Z",
      text: "hello world",
      model: "base.en",
      recording_file: "one.wav",
      source_recording_id: "source-1",
      status: "completed",
    });
  });

  it("threads translation-failed writing metadata through to history items", async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((command: string) => {
      if (command === "get_transcription_history") {
        return Promise.resolve([
          {
            timestamp: "2026-05-18T10:00:00.000Z",
            text: "hola mundo",
            model: "base.en",
            writing: { translation_failed: true, target_language: "es" },
          },
        ]);
      }
      if (command === "get_transcription_count") {
        return Promise.resolve(1);
      }
      return Promise.reject(new Error(`Unknown command: ${command}`));
    });

    const { result } = renderHook(() => useTranscriptionHistory({ limit: 50 }));

    await waitFor(() => {
      expect(result.current.history).toHaveLength(1);
    });

    expect(result.current.history[0]?.writing).toEqual({
      translation_failed: true,
      target_language: "es",
    });
  });

  it("prepends unique transcription events and respects the limit", async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((command: string) => {
      if (command === "get_transcription_history") {
        return Promise.resolve([
          {
            timestamp: "2026-05-18T10:00:00.000Z",
            text: "first",
            model: "base.en",
          },
        ]);
      }
      if (command === "get_transcription_count") {
        return Promise.resolve(1);
      }
      return Promise.reject(new Error(`Unknown command: ${command}`));
    });

    const { result } = renderHook(() =>
      useTranscriptionHistory({ limit: 1, includeTotalCount: true }),
    );

    await waitFor(() => {
      expect(result.current.history).toHaveLength(1);
    });

    await act(async () => {
      eventHarness.callbacks.get("transcription-added")?.({
        timestamp: "2026-05-18T10:01:00.000Z",
        text: "second",
        model: "large-v3-turbo",
        status: "completed",
      });
    });

    expect(result.current.history).toHaveLength(1);
    expect(result.current.history[0].text).toBe("second");
    expect(result.current.totalCount).toBe(2);

    await act(async () => {
      eventHarness.callbacks.get("transcription-added")?.({
        timestamp: "2026-05-18T10:01:00.000Z",
        text: "second duplicate",
        model: "large-v3-turbo",
      });
    });

    expect(result.current.history).toHaveLength(1);
    expect(result.current.history[0].text).toBe("second");
    expect(result.current.totalCount).toBe(2);
  });

  it("reloads history on update events", async () => {
    let text = "before";
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((command: string) => {
      if (command === "get_transcription_history") {
        return Promise.resolve([
          {
            timestamp: "2026-05-18T10:00:00.000Z",
            text,
            model: "base.en",
          },
        ]);
      }
      return Promise.reject(new Error(`Unknown command: ${command}`));
    });

    const { result } = renderHook(() => useTranscriptionHistory({ limit: 50 }));

    await waitFor(() => {
      expect(result.current.history[0]?.text).toBe("before");
    });

    text = "after";
    await act(async () => {
      eventHarness.callbacks.get("history-updated")?.();
    });

    await waitFor(() => {
      expect(result.current.history[0]?.text).toBe("after");
    });
  });

  it("surfaces load errors and clears them on retry", async () => {
    let shouldFail = true;
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((command: string) => {
      if (command === "get_transcription_history") {
        if (shouldFail) {
          return Promise.reject(new Error("db locked"));
        }
        return Promise.resolve([]);
      }
      return Promise.reject(new Error(`Unknown command: ${command}`));
    });

    const { result } = renderHook(() => useTranscriptionHistory({ limit: 50 }));

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.loadError).toBe("Couldn't load history");
    expect(result.current.history).toHaveLength(0);

    shouldFail = false;
    await act(async () => {
      await result.current.refreshHistory();
    });

    expect(result.current.loadError).toBeNull();
    expect(result.current.isLoading).toBe(false);
    expect(result.current.history).toHaveLength(0);
  });

  it("a slow stale failure cannot re-set the error after a newer retry succeeds", async () => {
    let rejectSlow!: (reason: Error) => void;
    const slowFail = new Promise<void>((_, reject) => {
      rejectSlow = reject;
    });
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((command: string) => {
      if (command === "get_transcription_history") {
        // First call: slow, will fail after the retry already resolved.
        if (callCount === 0) {
          callCount += 1;
          return slowFail.then(() => {
            throw new Error("slow failure");
          });
        }
        return Promise.resolve([]);
      }
      return Promise.reject(new Error(`Unknown command: ${command}`));
    });
    let callCount = 0;

    const { result } = renderHook(() => useTranscriptionHistory({ limit: 50 }));

    // Fire the retry while the first request is still pending.
    await act(async () => {
      void result.current.refreshHistory();
    });
    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
      expect(result.current.loadError).toBeNull();
    });

    // The stale request finally fails — it must not resurrect the error.
    await act(async () => {
      rejectSlow(new Error("slow failure"));
      await Promise.resolve();
    });

    expect(result.current.loadError).toBeNull();
    expect(result.current.history).toHaveLength(0);
  });
});
