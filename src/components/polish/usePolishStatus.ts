import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { PolishStatus } from "@/types";

/** `polish_status`, asked again whenever `refreshKey` changes, such as the licence status. */
export function usePolishStatus(refreshKey: unknown) {
  const [status, setStatus] = useState<PolishStatus | null>(null);
  const refresh = useCallback(() => {
    let cancelled = false;
    void invoke<PolishStatus | null>("polish_status")
      .then((next) => { if (!cancelled) setStatus(next ?? null); })
      .catch(() => { if (!cancelled) setStatus(null); });
    return () => { cancelled = true; };
  }, []);
  useEffect(() => refresh(), [refresh, refreshKey]);
  return { status, refresh };
}
