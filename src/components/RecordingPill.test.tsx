import {
  fireEvent,
  getByRole,
  getByTestId,
  getByText,
  queryByText,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createRecordingPill, type RecordingPillController } from "@/pill";

const platform = vi.hoisted(() => ({ mac: true }));
vi.mock("@/lib/platform", () => ({
  get isMacOS() {
    return platform.mac;
  },
}));

type PillIndicatorMode = "never" | "always" | "when_recording";
type Handler = (event: { payload: unknown }) => void;

let root: HTMLDivElement;
let controller: RecordingPillController | undefined;
let pillIndicatorMode: PillIndicatorMode;
let pillIndicatorStyle: "compact" | "full";
let pillIndicatorPosition: "top-left" | "bottom-center" | "bottom-right";
let listeners: Map<string, Set<Handler>>;
let invokeMock: ReturnType<typeof vi.fn>;
let currentRecordingState: { state: string; error: string | null };

function createTestPill() {
  controller = createRecordingPill(root, {
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
  });
}

function emitMockEvent(event: string, payload?: unknown) {
  listeners.get(event)?.forEach((handler) => {
    handler({ payload });
  });
}

function pillRoot() {
  return root.querySelector(".pill-root") as HTMLDivElement;
}

function pillSurface() {
  return root.querySelector(".pill-surface") as HTMLDivElement;
}

describe("RecordingPill", () => {
  beforeEach(() => {
    vi.useRealTimers();
    platform.mac = true;
    listeners = new Map();
    pillIndicatorMode = "when_recording";
    pillIndicatorStyle = "compact";
    pillIndicatorPosition = "bottom-center";
    currentRecordingState = { state: "idle", error: null };
    invokeMock = vi.fn((command: string) => {
      if (command === "get_settings") {
        return Promise.resolve({
          pill_indicator_mode: pillIndicatorMode,
          pill_indicator_style: pillIndicatorStyle,
          pill_indicator_position: pillIndicatorPosition,
        });
      }

      if (command === "get_current_recording_state") {
        return Promise.resolve(currentRecordingState);
      }

      if (command === "cancel_recording") {
        return Promise.resolve(true);
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
  });

  it("hides while idle when the indicator mode is when_recording", async () => {
    createTestPill();

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_settings");
    });

    expect(pillRoot()).toHaveAttribute("data-visible", "false");
    expect(pillSurface()).not.toBeVisible();
  });

  it("shows idle dots when the indicator mode is always", async () => {
    pillIndicatorMode = "always";
    createTestPill();

    await waitFor(() => {
      expect(getByTestId(root, "pill-dots")).toBeVisible();
    });
    expect(pillRoot()).toHaveAttribute("data-state", "idle");
  });

  it("re-reads indicator mode on settings-changed", async () => {
    createTestPill();

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_settings");
    });

    pillIndicatorMode = "always";
    emitMockEvent("settings-changed");

    await waitFor(() => {
      expect(getByTestId(root, "pill-dots")).toBeVisible();
    });
  });

  it("shows listening bars, timer, and cancel button while recording", async () => {
    pillIndicatorStyle = "full";
    vi.useFakeTimers();
    createTestPill();
    await Promise.resolve();
    await Promise.resolve();

    emitMockEvent("recording-state-changed", { state: "recording", error: null });

    expect(getByTestId(root, "pill-bars")).toHaveAttribute("data-state", "listening");
    expect(getByText(root, "0:00")).toBeVisible();
    expect(getByRole(root, "button", { name: "Cancel recording" })).toHaveTextContent("×");

    vi.advanceTimersByTime(3000);

    expect(getByText(root, "0:03")).toBeVisible();
  });

  it("keeps compact listening content icon-only", async () => {
    createTestPill();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_settings"));
    emitMockEvent("recording-state-changed", { state: "recording", error: null });
    expect(getByTestId(root, "pill-bars")).toBeVisible();
    expect(root.querySelector(".pill-timer")).not.toBeVisible();
    expect(root.querySelector(".pill-listening-controls .pill-text-primary")).not.toBeVisible();
  });

  it("reads the configured style and edge position", async () => {
    pillIndicatorStyle = "full";
    pillIndicatorPosition = "top-left";
    createTestPill();
    await waitFor(() => expect(pillRoot()).toHaveAttribute("data-pill-position", "top-left"));
    expect(pillRoot()).toHaveAttribute("data-pill-style", "full");
  });

  it("hydrates the current recording state on startup", async () => {
    currentRecordingState = { state: "recording", error: null };
    createTestPill();

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_current_recording_state");
      expect(getByTestId(root, "pill-bars")).toHaveAttribute("data-state", "listening");
    });
  });

  it("listens to audio-level only while listening", () => {
    createTestPill();

    emitMockEvent("audio-level", 0.75);
    expect(pillRoot()).toHaveAttribute("data-state", "idle");

    emitMockEvent("recording-started");
    emitMockEvent("audio-level", 0.75);

    const firstBar = getByTestId(root, "pill-bars").querySelector("span");
    expect(firstBar).toHaveStyle({ transform: "scaleY(0.466)" });
  });

  it("does not double-register audio-level while the first listen is pending", async () => {
    currentRecordingState = { state: "recording", error: null };
    const audioResolvers: Array<(unlisten: () => void) => void> = [];
    const listenMock = vi.fn((event: string, handler: Handler) => {
      if (event === "audio-level") {
        return new Promise<() => void>((resolve) => {
          audioResolvers.push((unlisten) => {
            if (!listeners.has(event)) {
              listeners.set(event, new Set());
            }
            listeners.get(event)?.add(handler);
            resolve(unlisten);
          });
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

    emitMockEvent("recording-started");
    emitMockEvent("transcription-started");
    emitMockEvent("recording-started");

    expect(listenMock.mock.calls.filter(([event]) => event === "audio-level")).toHaveLength(1);
    expect(audioResolvers).toHaveLength(1);

    const unlisten = vi.fn();
    audioResolvers[0]?.(unlisten);

    await waitFor(() => {
      expect(listeners.get("audio-level")?.size).toBe(1);
    });
    expect(unlisten).not.toHaveBeenCalled();
  });

  it("invokes cancel_recording once until state changes", async () => {
    createTestPill();
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_settings");
    });
    invokeMock.mockClear();

    emitMockEvent("recording-started");

    const cancelButton = getByRole(root, "button", { name: "Cancel recording" });
    fireEvent.click(cancelButton);
    fireEvent.click(cancelButton);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("cancel_recording");
    expect(cancelButton).toBeDisabled();
  });

  it("maps stopping and transcribing to the transcribing label", async () => {
    pillIndicatorStyle = "full";
    createTestPill();
    await waitFor(() => expect(pillRoot()).toHaveAttribute("data-pill-style", "full"));

    emitMockEvent("recording-state-changed", { state: "stopping", error: null });

    expect(getByText(root, "Transcribing")).toBeVisible();
    expect(root.querySelector(".pill-status-label .pill-text-primary")).toHaveTextContent(
      "Transcribing",
    );
    expect(root.querySelector(".pill-text-secondary")).toHaveTextContent("");

    emitMockEvent("recording-state-changed", { state: "transcribing", error: null });

    expect(getByText(root, "Transcribing")).toBeVisible();
  });

  it.each(["compact", "full"] as const)(
    "renders distinct processing activity in %s style",
    async (indicatorStyle) => {
      pillIndicatorStyle = indicatorStyle;
      createTestPill();
      await waitFor(() => expect(pillRoot()).toHaveAttribute("data-pill-style", indicatorStyle));

      emitMockEvent("transcription-started");
      const transcribing = root.querySelector(
        '.pill-status:not([hidden]) [data-testid="pill-activity"]',
      );
      expect(transcribing).toHaveAttribute("data-state", "transcribing");
      expect(
        transcribing?.querySelector('[data-visual="transcribing"] .pill-scan-dot'),
      ).toBeVisible();
      expect(transcribing?.querySelector(".pill-spark")).not.toBeInTheDocument();

      emitMockEvent("enhancing-started");
      const formatting = root.querySelector(
        '.pill-status:not([hidden]) [data-testid="pill-activity"]',
      );
      expect(formatting).toHaveAttribute("data-state", "formatting");
      expect(formatting?.querySelector('[data-visual="formatting"]')).toBeVisible();
      expect(formatting?.querySelector('[data-icon="sparkles"]')).toBeVisible();
      expect(formatting?.querySelector(".pill-scan-dot")).not.toBeInTheDocument();

      if (indicatorStyle === "full") {
        expect(getByText(root, "Polishing")).toBeVisible();
      } else {
        expect(getByText(root, "Polishing")).not.toBeVisible();
      }
    },
  );

  it("gives formatting feedback precedence until enhancement completes", async () => {
    pillIndicatorStyle = "full";
    createTestPill();
    await waitFor(() => expect(pillRoot()).toHaveAttribute("data-pill-style", "full"));

    emitMockEvent("transcription-started");
    expect(getByText(root, "Transcribing")).toBeVisible();

    emitMockEvent("enhancing-started");
    expect(getByText(root, "Polishing")).toBeVisible();

    emitMockEvent("enhancing-completed");
    expect(getByText(root, "Transcribing")).toBeVisible();
  });

  it("flashes recording-too-short errors briefly", () => {
    vi.useFakeTimers();
    createTestPill();

    emitMockEvent("recording-too-short", "Too short — hold a bit longer");

    expect(getByText(root, "Too short — hold a bit longer")).toBeVisible();
    expect(root.querySelector('.pill-status-error [data-icon="timer"]')).toBeVisible();

    vi.advanceTimersByTime(1500);

    expect(queryByText(root, "Too short — hold a bit longer")).not.toBeInTheDocument();
  });

  it("flashes recording-state error messages briefly", () => {
    vi.useFakeTimers();
    createTestPill();

    emitMockEvent("recording-state-changed", { state: "error", error: "Mic unavailable" });

    expect(getByText(root, "Mic unavailable")).toBeVisible();
    expect(root.querySelector('.pill-status-error [data-icon="mic-off"]')).toBeVisible();

    vi.advanceTimersByTime(1500);

    expect(queryByText(root, "Mic unavailable")).not.toBeInTheDocument();
  });
  it.each([
    ["pasted", "Pasted · 38 words", "check", 1200],
    ["copied", "Copied — press ⌘V", "clipboard-check", 1600],
    ["no_permission", "Copied — allow Accessibility to paste", "clipboard-check", 2500],
  ] as const)("renders %s feedback until its timeout", async (outcome, label, icon, duration) => {
    vi.useFakeTimers();
    createTestPill();
    await vi.advanceTimersByTimeAsync(0);
    emitMockEvent("transcription-started");
    emitMockEvent("paste-outcome", { outcome, words: 38 });
    emitMockEvent("recording-state-changed", { state: "idle", error: null });
    expect(getByText(root, label)).toBeVisible();
    expect(root.querySelector(`.pill-status-terminal [data-icon="${icon}"]`)).toBeVisible();
    vi.advanceTimersByTime(duration - 1);
    expect(getByText(root, label)).toBeVisible();
    vi.advanceTimersByTime(1);
    expect(pillSurface()).not.toBeVisible();
  });

  it.each(["pasted", "copied", "no_permission"])(
    "new recording immediately interrupts %s feedback",
    async (outcome) => {
      vi.useFakeTimers();
      createTestPill();
      await vi.advanceTimersByTimeAsync(0);
      emitMockEvent("paste-outcome", { outcome, words: 2 });
      emitMockEvent("recording-started");
      expect(getByTestId(root, "pill-bars")).toBeVisible();
      vi.advanceTimersByTime(2600);
      expect(pillRoot()).toHaveAttribute("data-state", "listening");
      expect(getByTestId(root, "pill-bars")).toBeVisible();
    },
  );

  it("never mode hides recording, errors and all terminal feedback", async () => {
    pillIndicatorMode = "never";
    createTestPill();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_settings"));
    emitMockEvent("recording-started");
    expect(pillSurface()).not.toBeVisible();
    emitMockEvent("recording-state-changed", { state: "error", error: "Mic unavailable" });
    expect(pillSurface()).not.toBeVisible();
    for (const outcome of ["pasted", "copied", "no_permission"]) {
      emitMockEvent("paste-outcome", { outcome, words: 2 });
      expect(pillSurface()).not.toBeVisible();
    }
  });

  it("returns to ready dots after feedback in always mode", async () => {
    vi.useFakeTimers();
    pillIndicatorMode = "always";
    createTestPill();
    await vi.advanceTimersByTimeAsync(0);
    emitMockEvent("paste-outcome", { outcome: "pasted", words: 2 });
    vi.advanceTimersByTime(1200);
    expect(getByTestId(root, "pill-dots")).toBeVisible();
  });

  it("uses Ctrl+V for Windows copied feedback", async () => {
    platform.mac = false;
    createTestPill();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_settings"));
    emitMockEvent("paste-outcome", { outcome: "copied", words: 2 });
    expect(getByText(root, "Copied — press Ctrl+V")).toBeVisible();
  });

  it("keeps permission feedback through the subsequent backend error", async () => {
    createTestPill();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_settings"));
    emitMockEvent("paste-outcome", { outcome: "no_permission", words: 2 });
    emitMockEvent("recording-state-changed", {
      state: "error",
      error: "No accessibility permission",
    });
    expect(getByText(root, "Copied — allow Accessibility to paste")).toBeVisible();
  });
});
