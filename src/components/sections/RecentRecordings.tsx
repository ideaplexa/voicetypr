import { useState } from "react";
import type { TranscriptionHistory } from "@/types";
import { RecentRecordingsFilters } from "./RecentRecordingsFilters";
import { RecentRecordingsHeader } from "./RecentRecordingsHeader";
import { RecentRecordingsList } from "./RecentRecordingsList";
import { RecentRecordingDetail } from "./RecentRecordingDetail";
import { useRecentRecordings } from "./useRecentRecordings";

interface RecentRecordingsProps {
  history: TranscriptionHistory[];
  onTranscribeFile?: () => void;
  hotkey?: string;
  onHistoryUpdate?: () => void;
  isLoading?: boolean;
  loadError?: string | null;
}

export function RecentRecordings({
  history,
  onTranscribeFile,
  hotkey = "Cmd+Shift+Space",
  onHistoryUpdate,
  isLoading = false,
  loadError = null,
}: RecentRecordingsProps) {
  const recordings = useRecentRecordings({ history, onHistoryUpdate });
  const [selectedId, setSelectedId] = useState<string | null>(history[0]?.id ?? null);
  const [previousHistory, setPreviousHistory] = useState(history);
  const [drawerOpen, setDrawerOpen] = useState(false);

  // The existing transcription-added flow prepends completed uploads. Select any
  // newly prepended entry, including one arriving while the upload dialog is open.
  if (previousHistory !== history) {
    setPreviousHistory(history);
    const newest = history[0];
    if (newest && !previousHistory.some((item) => item.id === newest.id)) {
      setSelectedId(newest.id);
    } else if (selectedId && !history.some((item) => item.id === selectedId)) {
      setSelectedId(null);
    }
  }
  if (selectedId && !recordings.filteredHistory.some((item) => item.id === selectedId)) {
    setSelectedId(null);
  }
  const selected = recordings.filteredHistory.find((item) => item.id === selectedId);

  const handleDelete = async (event: React.MouseEvent, id: string) => {
    const index = recordings.filteredHistory.findIndex((item) => item.id === id);
    const next = recordings.filteredHistory[index + 1] ?? recordings.filteredHistory[index - 1];
    if (await recordings.handleDelete(event, id)) {
      setSelectedId(next?.id ?? null);
      if (!next) setDrawerOpen(false);
    }
  };

  return (
    <div className="@container flex h-full min-h-0 flex-col gap-4 overflow-hidden px-9 pt-10 [&>header]:items-end">
      <RecentRecordingsHeader
        historyLength={history.length}
        onTranscribeFile={onTranscribeFile}
        onExport={recordings.handleExport}
        onExportText={recordings.handleExportText}
        onClearAll={recordings.handleClearAll}
      />
      {history.length > 0 && (
        <RecentRecordingsFilters
          searchQuery={recordings.searchQuery}
          onSearchQueryChange={recordings.setSearchQuery}
          sourceFilter={recordings.sourceFilter}
          onSourceFilterChange={recordings.setSourceFilter}
          dateFilter={recordings.dateFilter}
          onDateFilterChange={recordings.setDateFilter}
          appFilter={recordings.appFilter}
          onAppFilterChange={recordings.setAppFilter}
          distinctAppNames={recordings.distinctAppNames}
          resultCount={recordings.filteredHistory.length}
          onClearFilters={recordings.clearFilters}
        />
      )}
      <div className="relative flex min-h-0 w-full flex-1 overflow-hidden rounded-t-[14px] border border-b-0 border-border bg-card">
        <div
          className={`h-full shrink-0 @max-[688px]:w-full @max-[688px]:border-r-0 ${recordings.filteredHistory.length === 0 ? "w-full" : "w-[330px] border-r border-border"}`}
        >
          <RecentRecordingsList
            historyLength={history.length}
            filteredHistory={recordings.filteredHistory}
            groupedHistory={recordings.groupedHistory}
            visibleCount={recordings.visibleCount}
            onLoadMore={() => recordings.setVisibleCount((count) => count + 60)}
            selectedId={selectedId}
            onSelect={(id) => {
              setSelectedId(id);
              setDrawerOpen(true);
            }}
            isLoading={isLoading}
            loadError={loadError}
            onRetry={onHistoryUpdate}
            onTranscribeFile={onTranscribeFile}
            hotkey={hotkey}
          />
        </div>
        {selected && (
          <div
            className={`min-w-0 flex-1 bg-card @max-[688px]:absolute @max-[688px]:inset-0 @max-[688px]:z-10 @max-[688px]:border-l @max-[688px]:border-border ${drawerOpen ? "" : "@max-[688px]:hidden"}`}
            role="region"
            aria-label="Dictation detail"
          >
            <RecentRecordingDetail
              item={selected}
              verified={recordings.verifiedRecordings.has(selected.id)}
              checked={recordings.checkedRecordings.has(selected.id)}
              reTranscribing={recordings.reTranscribingIds.has(selected.id)}
              reTranscribingModel={recordings.reTranscribingModels.get(selected.id)}
              onReTranscribe={recordings.handleReTranscribe}
              onShowInFolder={recordings.handleShowInFolder}
              onDelete={handleDelete}
              onClose={() => setDrawerOpen(false)}
            />
          </div>
        )}
      </div>
    </div>
  );
}
