import { getByTestId, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createRecordingPill, type RecordingPillController } from "@/pill";
import {
  TRANSCRIPTION_STREAM_EVENT,
  type TranscriptionStreamEvent,
} from "@/types/streaming";

type Handler = (event: { payload: unknown }) => void;

let root: HTMLDivElement;
let controller: RecordingPillController | undefined;
let listeners: Map<string, Set<Handler>>;
let invokeMock: ReturnType<typeof vi.fn>;
let streamingPreviewEnabled: boolean;
let streamingPreviewDemo: boolean;

function createTestPill(options: { injectTimers?: boolean } = {}) {
  const deps = {
    invoke: invokeMock as never,
    listen: vi.fn((event: string, handler: Handler) => {
      if (!listeners.has(event)) {
        listeners.set(event, new Set());
      }
      listeners.get(event)?.add(handler);

      return Promise.resolve(() => {
        listeners.get(event)?.delete(handler);
      });
    }) as never,
    ...(options.injectTimers
      ? {
          setTimeout: vi.fn((handler: () => void, timeout: number) => setTimeout(handler, timeout)),
          clearTimeout: vi.fn((timeout: number | ReturnType<typeof setTimeout>) => clearTimeout(timeout)),
        }
      : {}),
  };

  controller = createRecordingPill(root, deps);
  return deps;
}

function emitMockEvent(event: string, payload?: unknown) {
  listeners.get(event)?.forEach((handler) => {
    handler({ payload });
  });
}

function emitStream(payload: TranscriptionStreamEvent) {
  emitMockEvent(TRANSCRIPTION_STREAM_EVENT, payload);
}

function preview() {
  return getByTestId(root, "pill-preview");
}

function committed() {
  return getByTestId(root, "pill-committed");
}

function tentative() {
  return getByTestId(root, "pill-tentative");
}

async function waitForSettings() {
  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("get_settings");
  });
}

async function waitForStreamListener() {
  await waitFor(() => {
    expect(listeners.get(TRANSCRIPTION_STREAM_EVENT)?.size).toBe(1);
  });
}

async function flushPromises() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

describe("RecordingPill streaming preview", () => {
  beforeEach(() => {
    vi.useRealTimers();
    listeners = new Map();
    streamingPreviewEnabled = true;
    streamingPreviewDemo = false;
    invokeMock = vi.fn((command: string) => {
      if (command === "get_settings") {
        return Promise.resolve({
          pill_indicator_mode: "when_recording",
          streaming_preview_enabled: streamingPreviewEnabled,
          streaming_preview_demo: streamingPreviewDemo,
        });
      }

      return Promise.resolve(null);
    });

    root = document.createElement("div");
    document.body.append(root);
  });

  afterEach(() => {
    controller?.destroy();
    controller = undefined;
    root.remove();
    vi.clearAllTimers();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("does not subscribe to the stream when the setting is off", async () => {
    streamingPreviewEnabled = false;
    createTestPill();

    await waitForSettings();
    emitMockEvent("recording-started");

    expect(listeners.has(TRANSCRIPTION_STREAM_EVENT)).toBe(false);
    expect(preview()).not.toBeVisible();
  });

  it("does not double-subscribe to stream events while listen is pending", async () => {
    let resolveStreamListen: ((unlisten: () => void) => void) | undefined;
    const listenMock = vi.fn((event: string, handler: Handler) => {
      if (event === TRANSCRIPTION_STREAM_EVENT) {
        return new Promise<() => void>((resolve) => {
          resolveStreamListen = (unlisten) => {
            if (!listeners.has(event)) {
              listeners.set(event, new Set());
            }
            listeners.get(event)?.add(handler);
            resolve(unlisten);
          };
        });
      }

      if (!listeners.has(event)) {
        listeners.set(event, new Set());
      }
      listeners.get(event)?.add(handler);

      return Promise.resolve(() => {
        listeners.get(event)?.delete(handler);
      });
    });

    controller = createRecordingPill(root, {
      invoke: invokeMock as never,
      listen: listenMock as never,
    });

    await waitForSettings();
    emitMockEvent("settings-changed");
    emitMockEvent("settings-changed");

    expect(
      listenMock.mock.calls.filter(([event]) => event === TRANSCRIPTION_STREAM_EVENT),
    ).toHaveLength(1);
    expect(resolveStreamListen).toBeDefined();

    const unlisten = vi.fn();
    resolveStreamListen?.(unlisten);

    await waitForStreamListener();
    expect(unlisten).not.toHaveBeenCalled();
  });

  it("keeps the committed span mounted and appends committed text across partials", async () => {
    createTestPill();
    await waitForSettings();
    await waitForStreamListener();
    emitMockEvent("recording-started");

    emitStream({ type: "started", session_id: 1, engine: "test", revision: 0 });
    emitStream({ type: "partial", session_id: 1, revision: 1, committed: "hello", tentative: " wor" });
    const committedNode = committed();

    emitStream({ type: "partial", session_id: 1, revision: 2, committed: "hello world", tentative: "" });

    expect(committed()).toBe(committedNode);
    expect(committed()).toHaveTextContent("hello world");
    expect(tentative()).toHaveTextContent("");
    expect(preview()).toBeVisible();
  });

  it("allows tentative rewrites and warns once before replacing non-monotonic committed text", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    createTestPill();
    await waitForSettings();
    await waitForStreamListener();
    emitMockEvent("recording-started");

    emitStream({ type: "started", session_id: 1, engine: "test", revision: 0 });
    emitStream({ type: "partial", session_id: 1, revision: 1, committed: "the quick", tentative: " borwn" });
    emitStream({ type: "partial", session_id: 1, revision: 2, committed: "the quick", tentative: " brown" });
    emitStream({ type: "partial", session_id: 1, revision: 3, committed: "rewritten", tentative: " tail" });
    emitStream({ type: "partial", session_id: 1, revision: 4, committed: "again", tentative: "" });

    expect(committed()).toHaveTextContent("again");
    expect(tentative()).toHaveTextContent("");
    expect(warnSpy).toHaveBeenCalledTimes(1);
  });

  it("ignores stale sessions and stale or duplicate revisions", async () => {
    createTestPill();
    await waitForSettings();
    await waitForStreamListener();
    emitMockEvent("recording-started");

    emitStream({ type: "started", session_id: 1, engine: "test", revision: 0 });
    emitStream({ type: "partial", session_id: 1, revision: 1, committed: "one", tentative: " two" });
    emitStream({ type: "partial", session_id: 2, revision: 2, committed: "wrong", tentative: "" });
    emitStream({ type: "partial", session_id: 1, revision: 1, committed: "duplicate", tentative: "" });
    emitStream({ type: "partial", session_id: 1, revision: 3, committed: "one two", tentative: "" });

    expect(committed()).toHaveTextContent("one two");
    expect(tentative()).toHaveTextContent("");
  });

  it("hides on final, cancelled, error, and state exit", async () => {
    createTestPill();
    await waitForSettings();
    await waitForStreamListener();
    emitMockEvent("recording-started");

    emitStream({ type: "started", session_id: 1, engine: "test", revision: 0 });
    emitStream({ type: "partial", session_id: 1, revision: 1, committed: "visible", tentative: "" });
    expect(preview()).toBeVisible();

    emitStream({ type: "final", session_id: 1, revision: 2, text: "visible" });
    expect(preview()).not.toBeVisible();

    emitStream({ type: "started", session_id: 2, engine: "test", revision: 0 });
    emitStream({ type: "partial", session_id: 2, revision: 1, committed: "visible", tentative: "" });
    emitStream({ type: "cancelled", session_id: 2, revision: 2 });
    expect(preview()).not.toBeVisible();

    emitStream({ type: "started", session_id: 3, engine: "test", revision: 0 });
    emitStream({ type: "partial", session_id: 3, revision: 1, committed: "visible", tentative: "" });
    emitStream({ type: "error", session_id: 3, revision: 2, error: "failed" });
    expect(preview()).not.toBeVisible();

    emitStream({ type: "started", session_id: 4, engine: "test", revision: 0 });
    emitStream({ type: "partial", session_id: 4, revision: 1, committed: "visible", tentative: "" });
    emitMockEvent("transcription-started");
    expect(preview()).not.toBeVisible();
  });

  it("runs the synthetic demo through gaps, stale events, and final under injected timers", async () => {
    vi.useFakeTimers();
    streamingPreviewDemo = true;
    const deps = createTestPill({ injectTimers: true });
    emitMockEvent("recording-started");

    await flushPromises();
    expect(invokeMock).toHaveBeenCalledWith("get_settings");
    expect(listeners.get(TRANSCRIPTION_STREAM_EVENT)?.size).toBe(1);

    await vi.advanceTimersByTimeAsync(540);
    expect(committed().textContent).toBe("Launching the stream ");
    expect(tentative()).toHaveTextContent("preview");

    await vi.advanceTimersByTimeAsync(90);
    expect(committed()).toHaveTextContent("Launching the stream preview");
    expect(tentative()).toHaveTextContent("");

    await vi.advanceTimersByTimeAsync(90);
    expect(preview()).not.toBeVisible();
    expect(deps.setTimeout).toHaveBeenCalled();
  });
});
