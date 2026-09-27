import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import type { TranscriptionHistory } from "@/types";
import { createLogger } from "@/lib/logger";

const log = createLogger("history");

interface RawTranscriptionHistoryItem {
  timestamp?: string;
  text: string;
  model: string;
  recording_file?: string;
  source_recording_id?: string;
  status?: TranscriptionHistory["status"];
  writing?: TranscriptionHistory["writing"];
}

interface TranscriptionAddedEvent {
  timestamp: string;
  text: string;
  model: string;
  recording_file?: string;
  source_recording_id?: string;
  status?: TranscriptionHistory["status"];
  writing?: TranscriptionHistory["writing"];
}

interface UseTranscriptionHistoryOptions {
  limit: number;
  includeTotalCount?: boolean;
}

interface UseTranscriptionHistoryResult {
  history: TranscriptionHistory[];
  totalCount: number;
  isLoading: boolean;
  loadError: string | null;
  refreshHistory: () => Promise<void>;
}

function toHistoryItem(item: RawTranscriptionHistoryItem): TranscriptionHistory {
  const timestamp = item.timestamp ?? Date.now().toString();

  return {
    id: timestamp,
    text: item.text,
    timestamp: new Date(timestamp),
    model: item.model,
    recording_file: item.recording_file,
    source_recording_id: item.source_recording_id,
    status: item.status,
    writing: item.writing,
  };
}

function fromAddedEvent(item: TranscriptionAddedEvent): TranscriptionHistory {
  return {
    id: item.timestamp,
    text: item.text,
    timestamp: new Date(item.timestamp),
    model: item.model,
    recording_file: item.recording_file,
    source_recording_id: item.source_recording_id,
    status: item.status,
    writing: item.writing,
  };
}

export function useTranscriptionHistory({
  limit,
  includeTotalCount = false,
}: UseTranscriptionHistoryOptions): UseTranscriptionHistoryResult {
  const { registerEvent } = useEventCoordinator("main");
  const [history, setHistory] = useState<TranscriptionHistory[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const requestIdRef = useRef(0);
  // Mirrors `history` synchronously so event handlers can check for duplicates
  // without reading stale closure state or causing side effects inside a
  // setState updater.
  const historyRef = useRef<TranscriptionHistory[]>([]);

  const refreshHistory = useCallback(async () => {
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    try {
      const historyPromise = invoke<RawTranscriptionHistoryItem[]>("get_transcription_history", {
        limit,
      });
      const countPromise = includeTotalCount
        ? invoke<number>("get_transcription_count")
        : Promise.resolve<number | null>(null);

      const [storedHistory, count] = await Promise.all([historyPromise, countPromise]);

      if (requestIdRef.current !== requestId) return;
      const formattedHistory = storedHistory.map(toHistoryItem);
      historyRef.current = formattedHistory;
      setHistory(formattedHistory);
      setTotalCount(count ?? formattedHistory.length);
      setLoadError(null);
    } catch (error) {
      if (requestIdRef.current !== requestId) return;
      log.error("Failed to load transcription history:", error);
      setLoadError("Couldn't load history");
    } finally {
      if (requestIdRef.current === requestId) {
        setIsLoading(false);
      }
    }
  }, [includeTotalCount, limit]);

  useEffect(() => {
    let isMounted = true;
    const unlisteners: Array<() => void> = [];

    const register = async <T>(
      eventName: string,
      handler: (payload: T) => void | Promise<void>,
    ) => {
      const unlisten = await registerEvent<T>(eventName, handler);
      if (typeof unlisten !== "function") return;
      if (!isMounted) {
        unlisten();
        return;
      }
      unlisteners.push(unlisten);
    };

    const setup = async () => {
      await refreshHistory();

      await register<TranscriptionAddedEvent>("transcription-added", (data) => {
        const newItem = fromAddedEvent(data);
        setLoadError(null);
        const previous = historyRef.current;
        if (previous.some((item) => item.id === newItem.id)) {
          return;
        }
        const next = [newItem, ...previous].slice(0, limit);
        historyRef.current = next;
        setHistory(next);
        if (includeTotalCount) {
          setTotalCount((count) => count + 1);
        }
      });

      await register("history-updated", () => {
        void refreshHistory();
      });

      await register("transcription-updated", () => {
        void refreshHistory();
      });
    };

    void setup();

    return () => {
      isMounted = false;
      unlisteners.forEach((unlisten) => {
        if (typeof unlisten === "function") {
          unlisten();
        }
      });
    };
  }, [includeTotalCount, limit, refreshHistory, registerEvent]);

  return {
    history,
    totalCount,
    isLoading,
    loadError,
    refreshHistory,
  };
}
