import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { AISettings } from "@/types/ai";

/** Whether Polish can run for this user, from the same backend gate dictation uses; null until it loads. */
export function usePolishCanRun(): boolean | null {
  const [ready, setReady] = useState<boolean | null>(null);
  useEffect(() => {
    let active = true;
    void invoke<AISettings>("get_ai_settings")
      .then((ai) => { if (active) setReady(ai.polishReady ?? false); })
      .catch(() => { if (active) setReady(null); });
    return () => { active = false; };
  }, []);
  return ready;
}
