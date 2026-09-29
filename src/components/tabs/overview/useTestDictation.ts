import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { isMacOS } from "@/lib/platform";
import { TRANSCRIPTION_STREAM_EVENT, type TranscriptionStreamEvent } from "@/types/streaming";
import type { PasteOutcomePayload } from "@/types/paste-outcome";

// Correlate delivery and a real input change in either order, within three seconds.
export function useTestDictation(open: boolean) {
  const [feedback, setFeedback] = useState<string | null>(null);
  const changedAt = useRef<number | null>(null);
  const pasted = useRef<{ at: number; words: number } | null>(null);
  useEffect(() => {
    changedAt.current = null;
    pasted.current = null;
    if (!open) return;
    let active = true;
    const subscriptions = [
      listen<PasteOutcomePayload>("paste-outcome", ({ payload }) => {
        if (!active) return;
        const now = Date.now();
        if (payload.outcome === "pasted") {
          pasted.current = { at: now, words: payload.words };
          setFeedback(
            changedAt.current !== null && now - changedAt.current <= 3000
              ? `Worked · ${payload.words} words`
              : null,
          );
        } else {
          pasted.current = null;
          changedAt.current = null;
          setFeedback(`Copied — press ${isMacOS ? "⌘V" : "Ctrl+V"} to paste here`);
        }
      }),
      listen<TranscriptionStreamEvent>(TRANSCRIPTION_STREAM_EVENT, ({ payload }) => {
        if (!active || payload.type !== "cancelled") return;
        changedAt.current = null;
        pasted.current = null;
        setFeedback(null);
      }),
      listen("recording-started", () => {
        if (!active) return;
        changedAt.current = null;
        pasted.current = null;
        setFeedback(null);
      }),
    ];
    return () => {
      active = false;
      for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten());
    };
  }, [open]);

  const contentChanged = () => {
    const now = Date.now();
    changedAt.current = now;
    if (pasted.current && now - pasted.current.at <= 3000) {
      setFeedback(`Worked · ${pasted.current.words} words`);
      pasted.current = null;
    } else {
      setFeedback(null);
    }
  };
  const reset = () => {
    changedAt.current = null;
    pasted.current = null;
    setFeedback(null);
  };
  return { feedback, contentChanged, reset };
}
