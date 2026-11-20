import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

export type DictationPhase = "idle" | "starting" | "listening" | "transcribing";

export interface FirstDictation {
  /** What the engine returned: no library rules or Polish ran on it. */
  heard: string;
  /** The same text: Polish does not run on the First try yet. */
  typed: string;
  polished: false;
  words: number;
}

/** Whether this device may try dictation before a trial or license. */
export type TryAllowance = { kind: "access" } | { kind: "tries"; left: number } | { kind: "usedUp" };

export function readFirstDictation(heard: string): FirstDictation {
  return { heard, typed: heard, polished: false, words: heard.split(/\s+/).filter(Boolean).length };
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
  const session = useRef({ id: null as string | null, stopQueued: false, phase: "idle" as DictationPhase, epoch: 0 });
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
      const { heard } = await invoke<{ heard: string }>("stop_onboarding_try", { id });
      if (epoch !== session.current.epoch) return;
      move("idle");
      if (heard) onResultRef.current(readFirstDictation(heard));
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
    move("starting");
    try {
      const started = await invoke<{ id: string; left: number | null }>("start_onboarding_try");
      if (epoch !== session.current.epoch) return;
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
  }, [move, stop, keepAllowance]);

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
        // The in-app fallback (Windows, focused window) sends presses only, so a
        // second press also ends a hold.
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
    window.addEventListener("blur", blur);
    return () => {
      disposed = true;
      window.removeEventListener("blur", blur);
      end();
      void invoke("arm_onboarding_try", { armed: false }).catch(() => undefined);
      for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [active, holdToTalk, start, stop, end]);

  const listening = phase === "listening" || phase === "starting";
  const working = phase === "transcribing";

  return { listening, working, allowance, start, stop };
}
