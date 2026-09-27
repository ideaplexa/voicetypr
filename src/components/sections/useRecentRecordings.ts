import { TranscriptionHistory } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import { useState, useMemo, useCallback, useEffect } from "react";
import { getModelDisplayName } from "@/lib/model-display";
import { createLogger } from "@/lib/logger";
import { applyHistoryFilters } from "./recentRecordingsHelpers";
import { useRecentRecordingsActions } from "./useRecentRecordingsActions";

const log = createLogger("recordings");

export function useRecentRecordings({
  history,
  onHistoryUpdate,
}: {
  history: TranscriptionHistory[];
  onHistoryUpdate?: () => void;
}) {
  const [searchQuery, setSearchQuery] = useState("");
  const [verifiedRecordings, setVerifiedRecordings] = useState<Set<string>>(new Set());
  const [checkedRecordings, setCheckedRecordings] = useState<Set<string>>(new Set());
  const [sourceFilter, setSourceFilter] = useState("all");
  const [appFilter, setAppFilter] = useState("all");
  const [dateFilter, setDateFilter] = useState("all");
  const [showOriginalIds, setShowOriginalIds] = useState<Set<string>>(new Set());
  const [visibleCount, setVisibleCount] = useState(60);
  const actions = useRecentRecordingsActions({ history, onHistoryUpdate });

  // Reset the visible page whenever the result set changes — adjusted during
  // render instead of a synchronous setState in an effect.
  const filterKey = `${searchQuery}\u0000${sourceFilter}\u0000${dateFilter}\u0000${appFilter}`;
  const [lastFilterKey, setLastFilterKey] = useState(filterKey);
  if (lastFilterKey !== filterKey) {
    setLastFilterKey(filterKey);
    setVisibleCount(60);
  }

  useEffect(() => {
    let cancelled = false;
    const verifyRecordings = async () => {
      log.debug("[RecentRecordings] Starting verification for", history.length, "items");
      const candidates = history.filter((item) => item.recording_file);
      const results = await Promise.all(
        candidates.map(async (item) => {
          log.debug(
            "[RecentRecordings] Checking recording:",
            item.recording_file,
            "for item:",
            item.id,
          );
          try {
            const exists = await invoke<boolean>("check_recording_exists", {
              filename: item.recording_file,
            });
            log.debug("[RecentRecordings] Recording", item.recording_file, "exists:", exists);
            return { id: item.id, exists };
          } catch (error) {
            log.error(`Failed to verify recording ${item.recording_file}:`, error);
            return null;
          }
        }),
      );
      if (cancelled) return;
      const verified = new Set<string>();
      const checked = new Set<string>();
      for (const result of results) {
        if (!result) continue;
        checked.add(result.id);
        if (result.exists) {
          verified.add(result.id);
        }
      }
      log.debug(
        "[RecentRecordings] Verification complete. Items with recording_file:",
        candidates.length,
        "Verified:",
        verified.size,
      );
      setCheckedRecordings(checked);
      setVerifiedRecordings(verified);
    };
    void verifyRecordings();
    return () => {
      cancelled = true;
    };
  }, [history]);

  const distinctAppNames = useMemo(() => {
    const names = new Set<string>();
    for (const item of history) {
      const app = item.writing?.context_hint?.app_name;
      if (app) names.add(app);
    }
    return [...names].sort();
  }, [history]);

  const filteredHistory = useMemo(() => {
    const structural = applyHistoryFilters(history, sourceFilter, appFilter, dateFilter);
    if (!searchQuery.trim()) return structural;
    const q = searchQuery.trim().toLowerCase();
    return structural.filter(
      (item) =>
        item.text.toLowerCase().includes(q) ||
        (item.model && item.model.toLowerCase().includes(q)) ||
        (item.model && (getModelDisplayName(item.model) ?? "").toLowerCase().includes(q)),
    );
  }, [history, searchQuery, sourceFilter, appFilter, dateFilter]);

  const groupedHistory = useMemo(() => {
    const groups: Record<string, TranscriptionHistory[]> = {};
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    filteredHistory.slice(0, visibleCount).forEach((item) => {
      const itemDate = new Date(item.timestamp);
      itemDate.setHours(0, 0, 0, 0);

      let groupKey: string;
      if (itemDate.getTime() === today.getTime()) {
        groupKey = "Today";
      } else if (itemDate.getTime() === yesterday.getTime()) {
        groupKey = "Yesterday";
      } else {
        groupKey = itemDate.toLocaleDateString("en-US", {
          weekday: "long",
          month: "short",
          day: "numeric",
          year: itemDate.getFullYear() !== today.getFullYear() ? "numeric" : undefined,
        });
      }
      if (!groups[groupKey]) {
        groups[groupKey] = [];
      }
      groups[groupKey].push(item);
    });

    return groups;
  }, [filteredHistory, visibleCount]);

  const toggleShowOriginal = useCallback((id: string) => {
    setShowOriginalIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const clearFilters = useCallback(() => {
    setSearchQuery("");
    setSourceFilter("all");
    setAppFilter("all");
    setDateFilter("all");
  }, []);

  return {
    searchQuery,
    setSearchQuery,
    sourceFilter,
    setSourceFilter,
    appFilter,
    setAppFilter,
    dateFilter,
    setDateFilter,
    distinctAppNames,
    filteredHistory,
    groupedHistory,
    visibleCount,
    setVisibleCount,
    reTranscribingIds: actions.reTranscribingIds,
    reTranscribingModels: actions.reTranscribingModels,
    verifiedRecordings,
    checkedRecordings,
    showOriginalIds,
    toggleShowOriginal,
    clearFilters,
    handleShowInFolder: actions.handleShowInFolder,
    handleReTranscribe: actions.handleReTranscribe,
    handleDelete: actions.handleDelete,
    handleClearAll: actions.handleClearAll,
    handleExport: actions.handleExport,
    handleExportText: actions.handleExportText,
  };
}
