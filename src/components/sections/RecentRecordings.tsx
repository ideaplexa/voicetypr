import { useEffect, useRef, useState } from "react";
import type { TranscriptionHistory } from "@/types";
import { RecentRecordingsFilters } from "./RecentRecordingsFilters";
import { RecentRecordingsHeader } from "./RecentRecordingsHeader";
import { RecentRecordingsList } from "./RecentRecordingsList";
import { RecentRecordingDetail } from "./RecentRecordingDetail";
import { useRecentRecordings } from "./useRecentRecordings";
import { useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { homeStatus } from "@/lib/home-readiness";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import type { SourceFilter } from "./models/types";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";

interface RecentRecordingsProps {
  history: TranscriptionHistory[];
  onTranscribeFile?: () => void;
  hotkey?: string;
  onHistoryUpdate?: () => void;
  isLoading?: boolean;
  loadError?: string | null;
  onNavigate?: (screen: ScreenId) => void;
  onNavigateSettingsPane?: (pane: SettingsPane) => void;
  onSourceFilterChange?: (source: SourceFilter) => void;
}

export function RecentRecordings({
  history,
  onTranscribeFile,
  hotkey = "Cmd+Shift+Space",
  onHistoryUpdate,
  isLoading = false,
  loadError = null,
  onNavigate,
  onNavigateSettingsPane,
  onSourceFilterChange,
}: RecentRecordingsProps) {
  const readiness = useReadiness();
  const { settings } = useSettings();
  const emptyStatus = homeStatus({
    model: settings?.current_model ?? "",
    engine: settings?.current_model_engine ?? "whisper",
    modelAvailable: readiness.selectedModelAvailable,
    canRecord: readiness.canRecord,
    remoteSelected: readiness.remoteSelected,
    remoteAvailable: readiness.remoteAvailable,
    remoteLabel: null,
    downloadProgress: null,
    licenseValid: readiness.licenseValid,
    licenseStatus: readiness.licenseStatus,
    hasMicrophonePermission: readiness.hasMicrophonePermission,
  });
  const recordings = useRecentRecordings({ history, onHistoryUpdate });
  const [selectedId, setSelectedId] = useState<string | null>(history[0]?.id ?? null);
  const [previousHistory, setPreviousHistory] = useState(history);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const [narrow, setNarrow] = useState(false);

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const measure = () => setNarrow((element.clientWidth || window.innerWidth) <= 688);
    measure();
    const observer = typeof ResizeObserver !== "undefined" ? new ResizeObserver(measure) : null;
    observer?.observe(element);
    window.addEventListener("resize", measure);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, []);

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
  const detail = selected ? (
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
  ) : null;

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
      <div
        ref={containerRef}
        inert={narrow && drawerOpen}
        className="relative flex min-h-0 w-full flex-1 overflow-hidden rounded-t-[14px] border border-b-0 border-border bg-card"
      >
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
            readinessStatus={emptyStatus}
            onReadinessAction={() => {
              if (emptyStatus.pane) onNavigateSettingsPane?.(emptyStatus.pane);
              else if (emptyStatus.screen) {
                if (emptyStatus.source) onSourceFilterChange?.(emptyStatus.source);
                onNavigate?.(emptyStatus.screen);
              }
            }}
          />
        </div>
        {selected && !narrow && (
          <div className="min-w-0 flex-1 bg-card" role="region" aria-label="Dictation detail">
            {detail}
          </div>
        )}
      </div>
      {narrow && selected && (
        <Dialog open={drawerOpen} onOpenChange={setDrawerOpen}>
          <DialogContent
            showCloseButton={false}
            finalFocus={() => document.getElementById(`history-item-${selected.id}`)}
            className="inset-x-0 bottom-0 top-auto left-0 h-[min(85dvh,720px)] max-w-none translate-x-0 translate-y-0 overflow-hidden rounded-t-[14px] rounded-b-none bg-card p-0 ring-1 ring-border"
          >
            <DialogTitle className="sr-only">Dictation detail</DialogTitle>
            {detail}
          </DialogContent>
        </Dialog>
      )}
    </div>
  );
}
