import { useEffect, useRef, type MutableRefObject } from "react";
import { createLogger } from "@/lib/logger";
import type { AISettings } from "@/types/ai";

const log = createLogger("enhancements");

export function usePolishSettingsLoad({
  settingsLoaded,
  setSettingsLoaded,
  loadAISettings,
  loadEnhancementOptionsRef,
  loadWritingSettingsRef,
}: {
  settingsLoaded: boolean;
  setSettingsLoaded: (loaded: boolean) => void;
  loadAISettings: (signal?: AbortSignal) => Promise<AISettings | null | undefined>;
  loadEnhancementOptionsRef: MutableRefObject<
    (aiEnabled: boolean, signal?: AbortSignal) => Promise<void>
  >;
  loadWritingSettingsRef: MutableRefObject<(signal?: AbortSignal) => Promise<boolean>>;
}) {
  const settingsLoadStartedRef = useRef(false);
  const controllerRef = useRef<AbortController | null>(null);

  useEffect(() => () => controllerRef.current?.abort(), []);

  useEffect(() => {
    if (settingsLoaded || settingsLoadStartedRef.current) {
      return;
    }

    settingsLoadStartedRef.current = true;
    controllerRef.current?.abort();
    const controller = new AbortController();
    controllerRef.current = controller;
    let completed = false;
    const { signal } = controller;
    void (async () => {
      try {
        const loadedAISettings = await loadAISettings(signal);
        // A superseded attempt applies nothing and leaves the start guard
        // alone: the effect that replaced it owns the guard now.
        if (signal.aborted) return;
        await loadEnhancementOptionsRef.current(loadedAISettings?.enabled ?? false, signal);
        if (signal.aborted) return;
        const writingSettingsLoaded = await loadWritingSettingsRef.current(signal);
        if (signal.aborted) return;
        completed = writingSettingsLoaded;
        setSettingsLoaded(writingSettingsLoaded);
        if (!writingSettingsLoaded) {
          settingsLoadStartedRef.current = false;
        }
      } catch (error) {
        if (signal.aborted) return;
        settingsLoadStartedRef.current = false;
        log.error("Failed to load Polish settings:", error);
      }
    })();

    return () => {
      // Lower the guard so the replacement effect (StrictMode's
      // setup-cleanup-setup cycle) can start its own attempt instead of being
      // blocked by the canceled one.
      // Completion reruns this effect too. Its background model/probe work
      // remains owned until unmount or a new attempt starts.
      if (!completed) controller.abort();
      settingsLoadStartedRef.current = false;
    };
  }, [
    settingsLoaded,
    loadAISettings,
    loadEnhancementOptionsRef,
    loadWritingSettingsRef,
    setSettingsLoaded,
  ]);
}
