import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { KeptRecording } from "@/lib/home-needs";

const REFRESH_MS = 5_000;

/**
 * Failed dictations whose audio Voicetypr still holds. The backend owns their bounded lifetime,
 * including restart-safe failed takes. Home refreshes while it is on screen.
 */
export function useKeptRecordings() {
  const [kept, setKept] = useState<KeptRecording[]>([]);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let active = true;
    void invoke<KeptRecording[]>("list_kept_dictations")
      .then((next) => { if (active) setKept(next); })
      .catch(() => { if (active) setKept([]); });
    const timer = window.setTimeout(() => setTick((value) => value + 1), REFRESH_MS);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [tick]);
  const refresh = useCallback(() => setTick((value) => value + 1), []);
  return { kept, refresh };
}
