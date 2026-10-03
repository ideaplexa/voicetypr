import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { localDateKey, shiftDay, type UsagePeriod } from "@/components/insights/stats";
import type { UsageStats, UsageStatsArgs } from "@/types/usage";

export function useUsageStats(period: UsagePeriod = "all", enabled = true) {
  const [stats, setStats] = useState<UsageStats | null>(null);
  const [loading, setLoading] = useState(enabled);
  const [error, setError] = useState(false);
  const generation = useRef(0);
  const [previousInputs, setPreviousInputs] = useState({ period, enabled });
  if (previousInputs.period !== period || previousInputs.enabled !== enabled) {
    setPreviousInputs({ period, enabled });
    setLoading(enabled);
    setError(false);
  }
  const refresh = useCallback((): Promise<void> => {
    if (!enabled) return Promise.resolve();
    const request = ++generation.current;
    const args: UsageStatsArgs = {
      since:
        period === "all" ? null : localDateKey(shiftDay(new Date(), period === "week" ? -6 : -29)),
    };
    return invoke<UsageStats>("get_usage_stats", { ...args }).then(
      (next) => {
        if (request !== generation.current) return;
        setStats(next);
        setError(false);
        setLoading(false);
      },
      () => {
        if (request !== generation.current) return;
        setError(true);
        setLoading(false);
      },
    );
  }, [period, enabled]);
  useEffect(() => {
    void refresh();
    return () => {
      generation.current++;
    };
  }, [refresh]);
  useTauriEvent("transcription-added", refresh);
  useTauriEvent("transcription-updated", refresh);
  useTauriEvent("history-updated", refresh);
  return { stats, loading, error, refresh };
}
