import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { PolishStatus } from "@/types";

/**
 * `works`: the next dictation will be polished, by the user's own provider or by built-in Polish once
 * Voicetypr's server confirmed it. `waiting`: built-in Polish is on but not confirmed for this device.
 * `turn_on`: the user's own provider is set up but switched off. `needs_provider`: neither is set up.
 */
export type PolishReadiness = "works" | "waiting" | "turn_on" | "needs_provider";

/**
 * The gate dictation uses: `polish_status`'s `will_run` is what `polish_can_run` returns, read
 * once so Voicetypr's server is asked once. null until it loads.
 */
export function usePolishCanRun(): PolishReadiness | null {
  const [readiness, setReadiness] = useState<PolishReadiness | null>(null);
  useEffect(() => {
    let active = true;
    void invoke<PolishStatus>("polish_status")
      .then((status): PolishReadiness =>
        status.will_run ? "works"
          : status.own_provider_ready ? "turn_on"
          : status.builtin_enabled ? "waiting"
          : "needs_provider")
      .then((next) => { if (active) setReadiness(next); })
      .catch(() => { if (active) setReadiness(null); });
    return () => { active = false; };
  }, []);
  return readiness;
}
