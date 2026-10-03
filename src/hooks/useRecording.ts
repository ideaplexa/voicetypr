import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { updateService } from "@/services/updateService";
import { createLogger } from "@/lib/logger";

const log = createLogger("recording");

type RecordingState = "idle" | "starting" | "recording" | "stopping" | "transcribing" | "error";

interface UseRecordingReturn {
  state: RecordingState;
  error: string | null;
  startRecording: () => Promise<boolean>;
  stopRecording: () => Promise<void>;
  isActive: boolean;
}

const getCommandErrorMessage = (err: unknown, fallback: string) => {
  if (err instanceof Error && err.message.trim()) {
    return err.message;
  }
  if (typeof err === "string" && err.trim()) {
    return err;
  }
  return fallback;
};

export function useRecording(): UseRecordingReturn {
  const [state, setState] = useState<RecordingState>("idle");
  const [error, setError] = useState<string | null>(null);

  // Check initial state on mount by requesting current state
  useEffect(() => {
    const checkInitialState = async () => {
      try {
        const currentState = await invoke<{ state: RecordingState; error: string | null }>(
          "get_current_recording_state",
        );
        if (!currentState || typeof currentState.state !== "string") {
          return;
        }
        log.debug("[Recording Hook] Initial state:", currentState);
        setState(currentState.state);
        setError(currentState.error);
      } catch (err) {
        log.error("[Recording Hook] Failed to get initial state:", err);
      }
    };
    checkInitialState();
  }, []);

  // Listen to backend events - frontend is purely reactive
  useEffect(() => {
    const unsubscribers: Array<() => void> = [];

    const setupListeners = async () => {
      // Backend state changes
      unsubscribers.push(
        await listen("recording-state-changed", (event: any) => {
          log.debug("[Recording Hook] State changed:", event.payload);
          setState(event.payload.state);
          setError(event.payload.error || null);
        }),
      );

      // Legacy events for compatibility
      unsubscribers.push(
        await listen("recording-started", () => {
          log.info("[Recording Hook] Recording started");
          setState("recording");
          setError(null);
        }),
      );

      unsubscribers.push(
        await listen("recording-timeout", () => {
          log.debug("[Recording Hook] Recording timeout");
          setState("stopping");
        }),
      );

      unsubscribers.push(
        await listen("recording-stopped-silence", () => {
          log.debug("[Recording Hook] Recording stopped due to silence");
          // You could show a toast or notification here
          // For now, just log it
        }),
      );

      unsubscribers.push(
        await listen("transcription-started", () => {
          log.debug("[Recording Hook] Transcription started");
          setState("transcribing");
        }),
      );

      // NOTE: We don't listen to transcription-complete here anymore
      // The state will be set to idle by recording-state-changed event
      // after the pill finishes processing and calls transcription_processed

      // NOTE: Error events (transcription-error, recording-error) are handled
      // via island problem events in the backend.
      // State errors come through recording-state-changed event.
    };

    setupListeners();

    return () => {
      unsubscribers.forEach((unsub) => unsub());
    };
  }, []);

  // Sync session state with update service to prevent auto-updates during recording
  // Note: Only react to actual state changes from backend, not component lifecycle
  useEffect(() => {
    const isActive = state !== "idle" && state !== "error";
    updateService.setSessionActive(isActive);
  }, [state]);

  // Simple command invocations - let backend handle all state management
  const startRecording = useCallback(async (): Promise<boolean> => {
    try {
      log.debug("[Recording Hook] Invoking start_recording...");
      const started = await invoke<boolean>("start_recording");
      return started === true;
    } catch (err) {
      log.error("[Recording Hook] Failed to start recording:", err);
      setState("error");
      setError(
        `${getCommandErrorMessage(err, "Recording could not start")}. Try again in a moment.`,
      );
      return false;
    }
  }, []);

  const stopRecording = useCallback(async () => {
    try {
      log.debug("[Recording Hook] Invoking stop_recording...");
      await invoke("stop_recording");
    } catch (err) {
      log.error("[Recording Hook] Failed to stop recording:", err);
      setState("error");
      setError(
        `${getCommandErrorMessage(err, "Recording could not stop")}. Try again in a moment.`,
      );
    }
  }, []);

  return {
    state,
    error,
    startRecording,
    stopRecording,
    isActive: state !== "idle" && state !== "error",
  };
}
