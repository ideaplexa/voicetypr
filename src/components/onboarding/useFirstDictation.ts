import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import type { RecordingStatePayload, TranscriptionHistory } from "@/types";

export type DictationPhase = RecordingStatePayload["state"];

export interface FirstDictation {
  /** What the engine heard, before Polish. */
  heard: string;
  /** What Voicetypr typed; differs from `heard` only when Polish changed it. */
  typed: string;
  polished: boolean;
  words: number;
}

/** The text of the user's first dictation, read from the history entry it saved. */
export function readFirstDictation(entry: Pick<TranscriptionHistory, "text" | "writing">): FirstDictation {
  const original = entry.writing?.original_text;
  const polished = Boolean(entry.writing?.ai_applied && original && original !== entry.text);
  return {
    heard: polished && original ? original : entry.text,
    typed: entry.text,
    polished,
    words: entry.text.trim().split(/\s+/).filter(Boolean).length,
  };
}

export function useFirstDictation(active: boolean) {
  const [phase, setPhase] = useState<DictationPhase>("idle");
  const [result, setResult] = useState<FirstDictation | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!active) return;
    let disposed = false;
    let revision = 0;
    const subscriptions = [
      listen<RecordingStatePayload>("recording-state-changed", ({ payload }) => {
        revision++;
        if (disposed) return;
        setPhase(payload.state);
        if (payload.state === "starting") setResult(null);
      }),
      listen<Pick<TranscriptionHistory, "text" | "writing">>("transcription-added", ({ payload }) => {
        if (disposed || !payload?.text?.trim()) return;
        setResult(readFirstDictation(payload));
      }),
    ];
    void subscriptions[0]
      .then(async () => {
        const observed = revision;
        const current = await invoke<RecordingStatePayload | null>("get_current_recording_state");
        if (!disposed && current && revision === observed) setPhase(current.state);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      for (const subscription of subscriptions) void subscription.then((stop) => stop());
    };
  }, [active]);

  const listening = phase === "recording" || phase === "starting";
  const working = phase === "stopping" || phase === "transcribing";

  const talk = useCallback(async () => {
    setBusy(true);
    try {
      await invoke(listening ? "stop_recording" : "start_recording");
    } catch {
      toast.error(listening ? "Could not stop dictation. Try again." : "Could not start dictation. Try again.");
    } finally {
      setBusy(false);
    }
  }, [listening]);

  return { phase, listening, working, result, busy, talk, reset: () => setResult(null) };
}
