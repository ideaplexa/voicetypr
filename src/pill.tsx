import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { TRANSCRIPTION_STREAM_EVENT, type TranscriptionStreamEvent } from "@/types/streaming";
import "./pill.css";

type BackendRecordingState =
  | "idle"
  | "starting"
  | "recording"
  | "stopping"
  | "transcribing"
  | "error";

type PillState = "idle" | "listening" | "transcribing" | "formatting";
type VisibleState = PillState | "error";
type PillIndicatorMode = "never" | "always" | "when_recording";
type PillIndicatorStyle = "compact" | "full";
type PillIndicatorPosition =
  | "top-left"
  | "top-center"
  | "top-right"
  | "bottom-left"
  | "bottom-center"
  | "bottom-right";

interface SettingsPayload {
  pill_indicator_mode?: PillIndicatorMode;
  pill_indicator_style?: PillIndicatorStyle;
  pill_indicator_position?: PillIndicatorPosition;
  transcription_mode?: "regular" | "live_preview";
  streaming_preview_enabled?: boolean;
  streaming_preview_demo?: boolean;
}

interface RecordingStatePayload {
  state: BackendRecordingState;
  error: string | null;
}

interface TauriEvent<T> {
  payload: T;
}

type UnlistenFn = () => void;
type ListenFn = <T>(event: string, handler: (event: TauriEvent<T>) => void) => Promise<UnlistenFn>;
type InvokeFn = <T = unknown>(command: string) => Promise<T>;
type TimeoutHandle = number | ReturnType<typeof setTimeout>;
type TimeoutFn = (handler: () => void, timeout: number) => TimeoutHandle;
type ClearTimeoutFn = (timeout: TimeoutHandle) => void;

export interface RecordingPillController {
  destroy: () => void;
}

interface RecordingPillDeps {
  invoke?: InvokeFn;
  listen?: ListenFn;
  setTimeout?: TimeoutFn;
  clearTimeout?: ClearTimeoutFn;
}

interface PillDom {
  root: HTMLDivElement;
  surface: HTMLDivElement;
  idle: HTMLDivElement;
  bars: HTMLDivElement;
  barSpans: HTMLSpanElement[];
  listening: HTMLDivElement;
  listeningControls: HTMLDivElement;
  listeningLabel: HTMLSpanElement;
  preview: HTMLDivElement;
  committed: HTMLSpanElement;
  tentative: HTMLSpanElement;
  timer: HTMLSpanElement;
  cancel: HTMLButtonElement;
  transcribing: HTMLDivElement;
  transcribingPrimary: HTMLSpanElement;
  transcribingSecondary: HTMLSpanElement;
  formatting: HTMLDivElement;
  formattingPrimary: HTMLSpanElement;
  formattingSecondary: HTMLSpanElement;
  error: HTMLDivElement;
  errorPrimary: HTMLSpanElement;
  errorSecondary: HTMLSpanElement;
}

const ERROR_FLASH_MS = 1500;
const LEVEL_BAR_COUNT = 9;
const LEVEL_ENVELOPE = Array.from({ length: LEVEL_BAR_COUNT }, (_, index) => {
  const center = (LEVEL_BAR_COUNT - 1) / 2;
  const distance = Math.abs(index - center) / center;
  return 0.42 + (1 - distance * distance) * 0.58;
});

function normalizeMode(mode: SettingsPayload["pill_indicator_mode"]): PillIndicatorMode {
  if (mode === "never" || mode === "always" || mode === "when_recording") {
    return mode;
  }
  return "when_recording";
}

function formatElapsed(seconds: number) {
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}:${remainingSeconds.toString().padStart(2, "0")}`;
}

function stateFromBackend(state: BackendRecordingState): PillState {
  if (state === "recording" || state === "starting") return "listening";
  if (state === "stopping" || state === "transcribing") return "transcribing";
  return "idle";
}

function createEl<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  if (className) element.className = className;
  return element;
}

function createTextPair(primaryText: string) {
  const wrapper = createEl("div", "pill-text-pair");
  const primary = createEl("span", "pill-text-primary");
  const secondary = createEl("span", "pill-text-secondary");
  primary.textContent = primaryText;
  secondary.textContent = "";
  wrapper.append(primary, secondary);
  return { wrapper, primary, secondary };
}

function createProcessingActivity(state: "transcribing" | "formatting") {
  const activity = createEl("div", "pill-activity");
  activity.dataset.testid = "pill-activity";
  activity.dataset.state = state;
  activity.setAttribute("aria-hidden", "true");

  if (state === "transcribing") {
    const scan = createEl("div", "pill-scan");
    scan.dataset.visual = "transcribing";
    scan.append(createEl("span", "pill-scan-track"), createEl("span", "pill-scan-dot"));
    activity.append(scan);
  } else {
    const sparkles = createEl("div", "pill-sparkles");
    sparkles.dataset.visual = "formatting";
    for (let index = 0; index < 3; index += 1) {
      sparkles.append(createEl("span", `pill-spark pill-spark-${index + 1}`));
    }
    activity.append(sparkles);
  }

  return activity;
}

function createPillDom(rootElement: HTMLElement): PillDom {
  const root = createEl("div", "pill-root");
  const surface = createEl("div", "pill-surface");

  const idle = createEl("div", "pill-dots");
  idle.setAttribute("aria-label", "Recording idle");
  idle.dataset.testid = "pill-dots";
  idle.append(createEl("span"), createEl("span"), createEl("span"));

  const bars = createEl("div", "pill-bars");
  bars.dataset.testid = "pill-bars";
  const barSpans = LEVEL_ENVELOPE.map(() => {
    const bar = createEl("span", "pill-bar");
    bars.append(bar);
    return bar;
  });

  const listening = createEl("div", "pill-status pill-status-listening");
  const preview = createEl("div", "pill-preview");
  preview.dataset.testid = "pill-preview";
  const committed = createEl("span", "pill-committed");
  committed.dataset.testid = "pill-committed";
  const tentative = createEl("span", "pill-tentative");
  tentative.dataset.testid = "pill-tentative";
  preview.append(committed, tentative);
  const listeningControls = createEl("div", "pill-listening-controls");
  const listeningLabel = createEl("span", "pill-text-primary");
  listeningLabel.textContent = "Listening";
  const timer = createEl("span", "pill-timer");
  timer.setAttribute("aria-label", "Recording elapsed time");
  const cancel = createEl("button", "pill-cancel");
  cancel.type = "button";
  cancel.setAttribute("aria-label", "Cancel recording");
  cancel.textContent = "×";
  listeningControls.append(bars, listeningLabel, timer, cancel);
  listening.append(preview, listeningControls);

  const transcribing = createEl("div", "pill-status pill-status-label");
  transcribing.setAttribute("role", "status");
  const transcribingText = createTextPair("Transcribing…");
  transcribing.append(createProcessingActivity("transcribing"), transcribingText.wrapper);

  const formatting = createEl("div", "pill-status pill-status-label");
  formatting.setAttribute("role", "status");
  const formattingText = createTextPair("Polishing…");
  formatting.append(createProcessingActivity("formatting"), formattingText.wrapper);

  const error = createEl("div", "pill-status pill-status-error");
  error.setAttribute("role", "status");
  const errorText = createTextPair("");
  error.append(errorText.wrapper);

  surface.append(idle, listening, transcribing, formatting, error);
  root.append(surface);
  rootElement.replaceChildren(root);

  return {
    root,
    surface,
    idle,
    bars,
    barSpans,
    listening,
    listeningControls,
    listeningLabel,
    preview,
    committed,
    tentative,
    timer,
    cancel,
    transcribing,
    transcribingPrimary: transcribingText.primary,
    transcribingSecondary: transcribingText.secondary,
    formatting,
    formattingPrimary: formattingText.primary,
    formattingSecondary: formattingText.secondary,
    error,
    errorPrimary: errorText.primary,
    errorSecondary: errorText.secondary,
  };
}

function setHidden(element: HTMLElement, hidden: boolean) {
  element.hidden = hidden;
}

function setBars(dom: PillDom, level: number, state: PillState) {
  const clampedLevel = Math.max(0, Math.min(1, level));
  dom.bars.dataset.state = state;

  dom.barSpans.forEach((bar, index) => {
    const envelope = LEVEL_ENVELOPE[index] ?? 0.42;
    const scale =
      state === "listening" ? 0.22 + clampedLevel * envelope * 0.78 : 0.2 + envelope * 0.12;
    bar.style.transform = `scaleY(${scale.toFixed(3)})`;
  });
}

export function createRecordingPill(
  rootElement: HTMLElement,
  deps: RecordingPillDeps = {},
): RecordingPillController {
  const tauriInvoke = deps.invoke ?? invoke;
  const tauriListen = deps.listen ?? listen;
  const scheduleTimeout = deps.setTimeout ?? setTimeout;
  const cancelTimeout = deps.clearTimeout ?? clearTimeout;
  const dom = createPillDom(rootElement);

  let mode: PillIndicatorMode = "when_recording";
  let style: PillIndicatorStyle = "compact";
  let position: PillIndicatorPosition = "bottom-center";
  let streamingPreviewEnabled = false;
  let streamingPreviewDemo = false;
  let pillState: PillState = "idle";
  let isFormatting = false;
  let audioLevel = 0;
  let elapsedSeconds = 0;
  let errorMessage: string | null = null;
  let isCancelling = false;
  let isDestroyed = false;
  let errorTimeout: TimeoutHandle | undefined;
  let timerInterval: ReturnType<typeof setInterval> | undefined;
  let audioUnlisten: UnlistenFn | undefined;
  let audioListenPending = false;
  let streamUnlisten: UnlistenFn | undefined;
  let streamListenPending = false;
  let activeStreamSessionId: number | null = null;
  let lastStreamRevision = -1;
  let streamPreviewVisible = false;
  let committedWarned = false;
  let demoRunId = 0;
  let demoTimeouts: TimeoutHandle[] = [];
  const unlisteners: UnlistenFn[] = [];

  const visibleState = (): VisibleState =>
    errorMessage ? "error" : isFormatting ? "formatting" : pillState;

  const isVisible = (state: VisibleState) =>
    errorMessage !== null || mode === "always" || (mode === "when_recording" && state !== "idle");

  const render = () => {
    if (isDestroyed) return;

    const state = visibleState();
    const visible = isVisible(state);
    dom.root.dataset.state = state;
    dom.root.dataset.visible = String(visible);
    dom.root.dataset.pillStyle = style;
    dom.root.dataset.pillPosition = position;
    setHidden(dom.surface, !visible);

    setHidden(dom.idle, state !== "idle");
    setHidden(dom.listening, state !== "listening");
    setHidden(dom.transcribing, state !== "transcribing");
    setHidden(dom.formatting, state !== "formatting");
    setHidden(dom.error, state !== "error");
    setHidden(dom.transcribingPrimary.parentElement as HTMLElement, style !== "full");
    setHidden(dom.formattingPrimary.parentElement as HTMLElement, style !== "full");
    setHidden(dom.preview, state !== "listening" || !streamPreviewVisible);

    dom.timer.textContent = formatElapsed(elapsedSeconds);
    setHidden(dom.timer, style !== "full" || state !== "listening");
    setHidden(dom.listeningLabel, style !== "full" || state !== "listening");
    dom.cancel.disabled = isCancelling;
    dom.errorPrimary.textContent = errorMessage ?? "";
    setBars(
      dom,
      state === "listening" ? audioLevel : 0,
      state === "listening" ? "listening" : "idle",
    );
  };

  const stopAudioListener = () => {
    audioUnlisten?.();
    audioUnlisten = undefined;
  };

  const startAudioListener = () => {
    if (audioUnlisten || audioListenPending) return;

    audioListenPending = true;
    void tauriListen<number>("audio-level", (event) => {
      if (isDestroyed || visibleState() !== "listening") return;
      audioLevel = event.payload;
      render();
    })
      .then((unlisten) => {
        audioListenPending = false;
        if (audioUnlisten || isDestroyed || visibleState() !== "listening") {
          unlisten();
        } else {
          audioUnlisten = unlisten;
        }
      })
      .catch(() => {
        audioListenPending = false;
      });
  };

  const stopTimer = () => {
    if (timerInterval) {
      clearInterval(timerInterval);
      timerInterval = undefined;
    }
  };

  const clearDemo = () => {
    demoRunId += 1;
    demoTimeouts.forEach((timeout) => cancelTimeout(timeout));
    demoTimeouts = [];
  };

  const resetStreamPreview = () => {
    activeStreamSessionId = null;
    lastStreamRevision = -1;
    streamPreviewVisible = false;
    dom.committed.textContent = "";
    dom.tentative.textContent = "";
    clearDemo();
  };

  const applyStreamText = (committed: string, tentative: string) => {
    const previousCommitted = dom.committed.textContent ?? "";
    if (committed.startsWith(previousCommitted)) {
      dom.committed.textContent = previousCommitted + committed.slice(previousCommitted.length);
    } else {
      if (!committedWarned) {
        committedWarned = true;
        console.warn("Streaming preview committed text was non-monotonic; replacing text.");
      }
      dom.committed.textContent = committed;
    }
    dom.tentative.textContent = tentative;
    streamPreviewVisible = committed.length > 0 || tentative.length > 0;
  };

  const isFreshStreamEvent = (event: TranscriptionStreamEvent) => {
    if (activeStreamSessionId !== null && event.session_id !== activeStreamSessionId) {
      return false;
    }
    if (event.revision <= lastStreamRevision) {
      return false;
    }
    return true;
  };

  const handleStreamEvent = (event: TranscriptionStreamEvent) => {
    if (isDestroyed || !streamingPreviewEnabled || visibleState() !== "listening") return;
    if (!isFreshStreamEvent(event)) return;

    activeStreamSessionId = event.session_id;
    lastStreamRevision = event.revision;

    if (event.type === "partial") {
      applyStreamText(event.committed, event.tentative);
      render();
      return;
    }

    if (event.type === "started") {
      streamPreviewVisible = false;
      dom.committed.textContent = "";
      dom.tentative.textContent = "";
      render();
      return;
    }

    if (event.type === "final" || event.type === "cancelled" || event.type === "error") {
      resetStreamPreview();
      render();
    }
  };

  const startDemo = () => {
    if (!streamingPreviewEnabled || !streamingPreviewDemo || visibleState() !== "listening") return;

    clearDemo();
    const runId = demoRunId;
    const sessionId = Date.now();
    const staleSessionId = sessionId + 1;
    const steps: Array<{ delay: number; event: TranscriptionStreamEvent }> = [
      { delay: 0, event: { type: "started", session_id: sessionId, engine: "demo", revision: 0 } },
      {
        delay: 90,
        event: {
          type: "partial",
          session_id: sessionId,
          revision: 1,
          committed: "Launch",
          tentative: "ing",
        },
      },
      {
        delay: 180,
        event: {
          type: "partial",
          session_id: sessionId,
          revision: 2,
          committed: "Launching ",
          tentative: "the",
        },
      },
      {
        delay: 270,
        event: {
          type: "partial",
          session_id: sessionId,
          revision: 4,
          committed: "Launching the ",
          tentative: "stream",
        },
      },
      {
        delay: 360,
        event: {
          type: "partial",
          session_id: sessionId,
          revision: 3,
          committed: "ignored",
          tentative: "stale",
        },
      },
      {
        delay: 450,
        event: {
          type: "partial",
          session_id: staleSessionId,
          revision: 5,
          committed: "ignored session",
          tentative: "",
        },
      },
      {
        delay: 540,
        event: {
          type: "partial",
          session_id: sessionId,
          revision: 5,
          committed: "Launching the stream ",
          tentative: "preview",
        },
      },
      {
        delay: 630,
        event: {
          type: "partial",
          session_id: sessionId,
          revision: 6,
          committed: "Launching the stream preview",
          tentative: "",
        },
      },
      {
        delay: 720,
        event: {
          type: "final",
          session_id: sessionId,
          revision: 7,
          text: "Launching the stream preview",
        },
      },
    ];

    demoTimeouts = steps.map(({ delay, event }) =>
      scheduleTimeout(() => {
        if (runId === demoRunId) handleStreamEvent(event);
      }, delay),
    );
  };

  const stopStreamListener = () => {
    streamUnlisten?.();
    streamUnlisten = undefined;
  };

  const syncStreamListener = () => {
    if (!streamingPreviewEnabled) {
      stopStreamListener();
      resetStreamPreview();
      render();
      return;
    }
    if (streamUnlisten || streamListenPending) return;

    streamListenPending = true;
    void tauriListen<TranscriptionStreamEvent>(TRANSCRIPTION_STREAM_EVENT, (event) => {
      handleStreamEvent(event.payload);
    })
      .then((unlisten) => {
        streamListenPending = false;
        if (streamUnlisten || isDestroyed || !streamingPreviewEnabled) {
          unlisten();
        } else {
          streamUnlisten = unlisten;
        }
      })
      .catch(() => {
        streamListenPending = false;
      });
  };

  const startTimer = () => {
    if (timerInterval) return;

    timerInterval = setInterval(() => {
      elapsedSeconds += 1;
      render();
    }, 1000);
  };

  const syncListeningEffects = () => {
    if (visibleState() === "listening") {
      startAudioListener();
      startTimer();
      startDemo();
      return;
    }

    stopAudioListener();
    stopTimer();
    resetStreamPreview();
  };

  const resetActiveState = () => {
    isFormatting = false;
    isCancelling = false;
    audioLevel = 0;
    elapsedSeconds = 0;
  };

  const setPillState = (nextState: PillState) => {
    const wasListening = visibleState() === "listening";
    pillState = nextState;
    const nowListening = visibleState() === "listening";
    if (wasListening !== nowListening) syncListeningEffects();
    render();
  };

  const flashError = (message: string) => {
    if (errorTimeout) cancelTimeout(errorTimeout);
    errorMessage = message;
    syncListeningEffects();
    render();

    errorTimeout = scheduleTimeout(() => {
      errorMessage = null;
      errorTimeout = undefined;
      syncListeningEffects();
      render();
    }, ERROR_FLASH_MS);
  };

  const readSettings = async () => {
    try {
      const settings = await tauriInvoke<SettingsPayload>("get_settings");
      if (isDestroyed) return;
      mode = normalizeMode(settings.pill_indicator_mode);
      style = settings.pill_indicator_style === "full" ? "full" : "compact";
      position = settings.pill_indicator_position ?? "bottom-center";
      streamingPreviewEnabled =
        settings.transcription_mode === "live_preview" ||
        settings.streaming_preview_enabled === true;
      streamingPreviewDemo = settings.streaming_preview_demo === true;
    } catch {
      if (isDestroyed) return;
      mode = "when_recording";
      style = "compact";
      position = "bottom-center";
      streamingPreviewEnabled = false;
      streamingPreviewDemo = false;
    }
    syncStreamListener();
    if (visibleState() === "listening") startDemo();
    render();
  };

  const applyRecordingState = (payload: RecordingStatePayload) => {
    resetActiveState();
    setPillState(stateFromBackend(payload.state));

    if (payload.state === "error") {
      flashError(payload.error || "Recording failed");
    }
  };

  const readInitialRecordingState = async () => {
    try {
      const currentState = await tauriInvoke<RecordingStatePayload>("get_current_recording_state");
      if (isDestroyed || !currentState || typeof currentState.state !== "string") return;
      applyRecordingState(currentState);
    } catch {
      // The pill can still recover from subsequent recording-state events.
    }
  };

  const subscribe = <T,>(event: string, handler: (event: TauriEvent<T>) => void) => {
    void tauriListen<T>(event, handler).then((unlisten) => {
      if (isDestroyed) {
        unlisten();
      } else {
        unlisteners.push(unlisten);
      }
    });
  };

  dom.cancel.addEventListener("click", () => {
    if (isCancelling) return;
    isCancelling = true;
    render();
    void tauriInvoke("cancel_recording").catch(() => {
      if (isDestroyed) return;
      isCancelling = false;
      render();
    });
  });

  subscribe("settings-changed", () => {
    void readSettings();
  });

  subscribe<RecordingStatePayload>("recording-state-changed", (event) => {
    if (isDestroyed) return;
    applyRecordingState(event.payload);
  });

  subscribe("recording-started", () => {
    if (isDestroyed) return;
    resetActiveState();
    setPillState("listening");
  });

  subscribe("transcription-started", () => {
    if (isDestroyed) return;
    resetActiveState();
    setPillState("transcribing");
  });

  subscribe("enhancing-started", () => {
    if (isDestroyed) return;
    isFormatting = true;
    syncListeningEffects();
    render();
  });

  subscribe("enhancing-completed", () => {
    if (isDestroyed) return;
    isFormatting = false;
    syncListeningEffects();
    render();
  });

  subscribe("enhancing-failed", () => {
    if (isDestroyed) return;
    isFormatting = false;
    syncListeningEffects();
    render();
  });

  subscribe<string>("recording-too-short", (event) => {
    if (isDestroyed) return;
    resetActiveState();
    setPillState("idle");
    flashError(event.payload || "Recording too short");
  });

  render();
  void Promise.resolve().then(() => {
    void readSettings();
    void readInitialRecordingState();
  });

  return {
    destroy: () => {
      isDestroyed = true;
      if (errorTimeout) cancelTimeout(errorTimeout);
      stopTimer();
      stopAudioListener();
      stopStreamListener();
      clearDemo();
      unlisteners.forEach((unlisten) => unlisten());
      rootElement.replaceChildren();
    },
  };
}

const root = document.getElementById("root");

if (root) {
  createRecordingPill(root);
}
