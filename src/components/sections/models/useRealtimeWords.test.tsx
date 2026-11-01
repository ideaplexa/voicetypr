import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useRealtimeWords } from "@/components/sections/models/useRealtimeWords";
import type { ActiveStreamCapabilities } from "@/types";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn<(command: string) => Promise<ActiveStreamCapabilities>>(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
  unlisten: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, handler: (event: { payload: unknown }) => void) => {
    mocks.handlers.set(event, handler);
    return () => {
      mocks.handlers.delete(event);
      mocks.unlisten();
    };
  }),
}));
vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({ settings: { transcription_mode: "live_preview" } }),
}));

function capabilities(streams: boolean): ActiveStreamCapabilities {
  return {
    active_engine: "parakeet",
    transcription_mode: "live_preview",
    eou_model_downloaded: false,
    eou_model_path: null,
    eou_chunk_ms: 320,
    capabilities: {
      supports_streaming: streams,
      supports_committed_prefix: streams,
      supports_tentative_tail: streams,
      supports_endpointing: streams,
      final_only: !streams,
    },
  };
}
function sourceChanged() {
  act(() => mocks.handlers.get("sharing-status-changed")?.({ payload: { refresh: true } }));
}

describe("useRealtimeWords active source", () => {
  beforeEach(() => {
    mocks.handlers.clear();
    mocks.invoke.mockReset().mockResolvedValue(capabilities(true));
  });

  it("refreshes on remote selection and deselection without local model changes", async () => {
    const { result, unmount } = renderHook(() => useRealtimeWords("parakeet", "tdt"));
    await waitFor(() => expect(result.current.realtimeOn).toBe(true));
    mocks.invoke.mockResolvedValueOnce(capabilities(false));
    sourceChanged();
    expect(result.current.streams).toBe(false);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledTimes(2));
    expect(result.current.realtimeOn).toBe(false);
    sourceChanged();
    await waitFor(() => expect(result.current.realtimeOn).toBe(true));
    unmount();
    expect(mocks.unlisten).toHaveBeenCalledOnce();
  });

  it("ignores an older local response arriving after the remote response", async () => {
    let resolveLocal!: (value: ActiveStreamCapabilities) => void;
    mocks.invoke.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveLocal = resolve;
        }),
    );
    const { result } = renderHook(() => useRealtimeWords("parakeet", "tdt"));
    mocks.invoke.mockResolvedValueOnce(capabilities(false));
    sourceChanged();
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledTimes(2));
    await act(async () => resolveLocal(capabilities(true)));
    expect(result.current.streams).toBe(false);
  });

  it("ignores responses from the previous local engine/model", async () => {
    let resolveOld!: (value: ActiveStreamCapabilities) => void;
    mocks.invoke.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveOld = resolve;
        }),
    );
    const { result, rerender } = renderHook(({ model }) => useRealtimeWords("parakeet", model), {
      initialProps: { model: "tdt" },
    });
    mocks.invoke.mockResolvedValueOnce(capabilities(false));
    rerender({ model: "other" });
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledTimes(2));
    await act(async () => resolveOld(capabilities(true)));
    expect(result.current.streams).toBe(false);
  });
});
