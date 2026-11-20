import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { PolishStatus } from "@/types";

/**
 * `works`: the next dictation will be polished (`polish_can_run`, the gate dictation uses), by the user's
 * own provider or by built-in Polish once Voicetypr's server confirmed it. `waiting`: built-in Polish is on
 * but not confirmed for this device yet. `needs_provider`: neither is set up. null until it loads.
 */
export type PolishReadiness = "works" | "waiting" | "needs_provider";

export function usePolishCanRun(): PolishReadiness | null {
  const [readiness, setReadiness] = useState<PolishReadiness | null>(null);
  useEffect(() => {
    let active = true;
    void (async () => {
      if (await invoke<boolean>("polish_can_run")) return "works" as const;
      const status = await invoke<PolishStatus>("polish_status");
      return status.builtin_enabled && !status.own_provider_ready ? "waiting" as const : "needs_provider" as const;
    })()
      .then((next) => { if (active) setReadiness(next); })
      .catch(() => { if (active) setReadiness(null); });
    return () => { active = false; };
  }, []);
  return readiness;
}
