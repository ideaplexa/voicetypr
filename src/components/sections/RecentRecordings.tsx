import { TranscriptionHistory } from "@/types";
import { useCanAutoInsert, useReadiness } from "@/contexts/ReadinessContext";
import { isMacOS } from "@/lib/platform";
import { RecentRecordingsFilters } from "./RecentRecordingsFilters";
import { RecentRecordingsHeader } from "./RecentRecordingsHeader";
import { RecentRecordingsList } from "./RecentRecordingsList";
import { useRecentRecordings } from "./useRecentRecordings";

interface RecentRecordingsProps {
  history: TranscriptionHistory[];
  hotkey?: string;
  onHistoryUpdate?: () => void;
  isLoading?: boolean;
  loadError?: string | null;
}

export function RecentRecordings({
  history,
  hotkey = "Cmd+Shift+Space",
  onHistoryUpdate,
  isLoading = false,
  loadError = null,
}: RecentRecordingsProps) {
  const readiness = useReadiness();
  const canRecord = readiness.canRecord;
  const canAutoInsert = useCanAutoInsert();
  const unavailableMessage =
    readiness.licenseStatus === "expired" || readiness.licenseStatus === "none"
      ? "Activate a license to record again."
      : readiness.hasModels === false || readiness.selectedModelAvailable === false
        ? "Choose a ready local model, cloud provider, or remote Voicetypr source in Models."
        : isMacOS && readiness.hasMicrophonePermission === false
          ? "Allow microphone access in macOS Settings."
          : "Finish setup in Settings before recording.";

  const recordings = useRecentRecordings({ history, onHistoryUpdate });

  return (
    <div className="h-full flex flex-col">
      <RecentRecordingsHeader
        historyLength={history.length}
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
      <div className="flex-1 min-h-0 overflow-hidden">
        <RecentRecordingsList
          historyLength={history.length}
          filteredHistory={recordings.filteredHistory}
          groupedHistory={recordings.groupedHistory}
          visibleCount={recordings.visibleCount}
          onLoadMore={() => recordings.setVisibleCount((c) => c + 60)}
          isLoading={isLoading}
          loadError={loadError}
          onRetry={onHistoryUpdate}
          hotkey={hotkey}
          availability={{ canRecord, canAutoInsert, unavailableMessage }}
          reTranscribingIds={recordings.reTranscribingIds}
          reTranscribingModels={recordings.reTranscribingModels}
          verifiedRecordings={recordings.verifiedRecordings}
          checkedRecordings={recordings.checkedRecordings}
          showOriginalIds={recordings.showOriginalIds}
          onToggleOriginal={recordings.toggleShowOriginal}
          onReTranscribe={recordings.handleReTranscribe}
          onShowInFolder={recordings.handleShowInFolder}
          onDelete={recordings.handleDelete}
        />
      </div>
    </div>
  );
}
