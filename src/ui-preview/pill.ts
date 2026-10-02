import { installPreviewPlatform } from "./platform";
import type { TranscriptionStreamEvent } from "@/types/streaming";

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Handler[]>();
const params = new URLSearchParams(location.search);
const state = params.get("state") ?? "idle";
const background = params.get("theme") === "dark" ? "#141415" : "#e9e9e5";
document.documentElement.style.setProperty("background", background, "important");
document.body.style.setProperty("background", background, "important");
installPreviewPlatform(params.get("platform") === "windows" ? "windows" : "macos");
const { createRecordingPill } = await import("@/pill");
const root = document.getElementById("preview-root");
if (root) {
  createRecordingPill(root, {
    invoke: async <T>(command: string): Promise<T> => {
      const value: unknown =
        command === "get_settings"
          ? {
              pill_indicator_mode: "always",
              pill_indicator_style: params.get("style") ?? "full",
              transcription_mode: "live_preview",
              pill_indicator_position: params.get("position") ?? "bottom-center",
            }
          : command === "pill_get_geometry"
            ? { anchor: params.get("position") ?? "bottom-center", anchorX: params.get("position")?.endsWith("-left") ? 0 : params.get("position")?.endsWith("-right") ? 440 : 220, anchorY: params.get("position")?.startsWith("top-") ? 6 : 414 }
            : { state: "idle", error: null };
      return value as T;
    },
    listen: async <T>(event: string, handler: (event: { payload: T }) => void) => {
      const wrapped: Handler = ({ payload }) => handler({ payload: payload as T });
      handlers.set(event, [...(handlers.get(event) ?? []), wrapped]);
      return () => {
        handlers.set(
          event,
          (handlers.get(event) ?? []).filter((entry) => entry !== wrapped),
        );
      };
    },
    // Freeze transient feedback for reproducible screenshots.
    setTimeout: () => 0,
    clearTimeout: () => {},
  });
  const emit = (event: string, payload?: unknown) =>
    handlers.get(event)?.forEach((handler) => handler({ payload }));
  await new Promise<void>((resolve) => window.setTimeout(resolve, 50));
  if (state === "listening" || state === "preview") emit("recording-started");
  if (state === "preview") {
    const partial: TranscriptionStreamEvent = {
      type: "partial",
      session_id: 1,
      revision: 1,
      committed: "…and I'll send the notes before the call ",
      tentative: "tomorrow",
    };
    emit("transcription-stream", partial);
  }
  if (state === "transcribing") emit("transcription-started");
  if (state === "formatting") emit("enhancing-started");
  if (state === "error")
    emit("recording-state-changed", { state: "error", error: "Microphone unavailable" });
  if (state === "too_short") emit("recording-too-short");
  if (["pasted", "copied", "no_permission"].includes(state))
    emit("paste-outcome", { outcome: state, words: 38 });
  document.documentElement.dataset.pillPreviewReady = "true";
}
