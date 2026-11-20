import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

const ESCAPE_AGAIN_MS = 2000;

export type DictationPhase = "idle" | "starting" | "listening" | "transcribing";

export interface FirstDictation {
  /** What the engine returned: no library rules or Polish ran on it. */
  heard: string;
  /** What built-in Polish made of it, or the same text when Polish didn't run. */
  typed: string;
  polished: boolean;
  words: number;
}

/** Whether this device may try dictation before a trial or license. */
export type TryAllowance = { kind: "access" } | { kind: "tries"; left: number } | { kind: "usedUp" };

export function readFirstDictation(heard: string, cleaned?: string | null): FirstDictation {
  const typed = cleaned?.trim() || heard;
  return { heard, typed, polished: Boolean(cleaned?.trim()), words: typed.split(/\s+/).filter(Boolean).length };
}

/**
 * The First try: recorded and transcribed for this window alone, never pasted
 * or saved. The user's shortcut reaches it while the step is on screen.
 */
export function useFirstDictation(active: boolean, holdToTalk: boolean, onResult: (result: FirstDictation) => void) {
  const [phase, setPhase] = useState<DictationPhase>("idle");
  const [allowance, setAllowance] = useState<TryAllowance | null>(null);
  const usedUp = useRef(false);
  const keepAllowance = useCallback((allowed: TryAllowance | null) => {
    usedUp.current = allowed?.kind === "usedUp";
    setAllowance(allowed);
  }, []);
  // `epoch` changes on blur and unmount, so a late reply from an ended try is ignored.
  const session = useRef({ id: null as string | null, stopQueued: false, cancelQueued: false, phase: "idle" as DictationPhase, epoch: 0, attempt: 0 });
  const onResultRef = useRef(onResult);
  useEffect(() => {
    onResultRef.current = onResult;
  });
  const move = useCallback((next: DictationPhase) => {
    session.current.phase = next;
    setPhase(next);
  }, []);
  const end = useCallback(() => {
    session.current.epoch++;
    session.current.id = null;
    session.current.stopQueued = false;
    session.current.cancelQueued = false;
    move("idle");
  }, [move]);

  const stop = useCallback(async () => {
    if (session.current.phase === "starting") {
      session.current.stopQueued = true;
      return;
    }
    const { id, epoch } = session.current;
    if (id === null || session.current.phase !== "listening") return;
    session.current.id = null;
    move("transcribing");
    try {
      const { heard, cleaned } = await invoke<{ heard: string; cleaned?: string | null }>("stop_onboarding_try", { id });
      if (epoch !== session.current.epoch) return;
      move("idle");
      if (heard) onResultRef.current(readFirstDictation(heard, cleaned));
      else toast.error("Voicetypr didn’t hear any words. Try again.");
    } catch (error) {
      if (epoch !== session.current.epoch) return;
      move("idle");
      toast.error(typeof error === "string" ? error : "Voicetypr could not transcribe that. Try again.");
    }
  }, [move]);

  const start = useCallback(async () => {
    if (session.current.phase !== "idle" || usedUp.current) return;
    const { epoch } = session.current;
    session.current.stopQueued = false;
    session.current.cancelQueued = false;
    session.current.attempt++;
    move("starting");
    try {
      const started = await invoke<{ id: string; left: number | null }>("start_onboarding_try");
      // A try this window no longer wants is thrown away, not left recording.
      if (epoch !== session.current.epoch || session.current.cancelQueued) {
        if (epoch === session.current.epoch) end();
        void invoke("cancel_onboarding_try", { id: started.id }).catch(() => undefined);
        return;
      }
      session.current.id = started.id;
      keepAllowance(started.left === null ? { kind: "access" } : { kind: "tries", left: started.left });
      move("listening");
      if (session.current.stopQueued) await stop();
    } catch (error) {
      if (epoch !== session.current.epoch) return;
      move("idle");
      const allowed = await invoke<TryAllowance>("get_onboarding_try").catch(() => null);
      if (epoch !== session.current.epoch) return;
      keepAllowance(allowed);
      if (allowed?.kind !== "usedUp") toast.error(typeof error === "string" ? error : "Could not start dictation. Try again.");
    }
  }, [move, stop, keepAllowance, end]);

  useEffect(() => {
    if (!active) return;
    let disposed = false;
    void invoke<TryAllowance>("get_onboarding_try")
      .then((allowed) => { if (!disposed) keepAllowance(allowed); })
      .catch(() => undefined);
    const subscriptions = [
      listen<boolean>("onboarding-try-key", ({ payload: pressed }) => {
        if (disposed) return;
        const current = session.current.phase;
        if (pressed && current === "idle") void start();
        // A second press also ends a hold.
        else if (pressed) void stop();
        else if (!pressed && holdToTalk) void stop();
      }),
      listen<string>("onboarding-try-capped", ({ payload }) => {
        if (!disposed && payload === session.current.id) void stop();
      }),
    ];
    // The shortcut is handed over only once both listeners are in place.
    void Promise.all(subscriptions)
      .then(() => (disposed ? undefined : invoke("arm_onboarding_try", { armed: true })))
      .catch(() => undefined);
    // Leaving the window ends the try in the app as well.
    const blur = () => {
      if (session.current.phase !== "idle") end();
    };
    // Esc twice within two seconds throws the try away, as it cancels a dictation.
    // A try still starting is cancelled once its start returns, so the start
    // cannot slip past the cancel.
    let lastEscape = { at: 0, attempt: -1 };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.repeat || session.current.phase === "idle") return;
      const now = Date.now();
      const { attempt } = session.current;
      if (lastEscape.attempt !== attempt || now - lastEscape.at > ESCAPE_AGAIN_MS) {
        lastEscape = { at: now, attempt };
        return;
      }
      lastEscape = { at: 0, attempt: -1 };
      if (session.current.phase === "starting") {
        session.current.cancelQueued = true;
        return;
      }
      const { id } = session.current;
      end();
      void invoke("cancel_onboarding_try", { id }).catch(() => undefined);
    };
    window.addEventListener("blur", blur);
    window.addEventListener("keydown", escape, true);
    return () => {
      disposed = true;
      window.removeEventListener("blur", blur);
      window.removeEventListener("keydown", escape, true);
      end();
      void invoke("arm_onboarding_try", { armed: false }).catch(() => undefined);
      for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [active, holdToTalk, start, stop, end]);

  const listening = phase === "listening" || phase === "starting";
  const working = phase === "transcribing";

  return { listening, working, allowance, start, stop };
}
