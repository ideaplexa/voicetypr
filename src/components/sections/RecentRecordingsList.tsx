import { ScrollArea } from "@/components/ui/scroll-area";
import { Button } from "@/components/ui/button";
import { formatHotkey } from "@/lib/hotkey-utils";
import { TranscriptionHistory } from "@/types";
import { AlertCircle, AlertTriangle, Mic, Search, ShieldCheck } from "lucide-react";
import { isMacOS } from "@/lib/platform";
import { RecentRecordingRow } from "./RecentRecordingRow";

export interface RecentRecordingsListProps {
  historyLength: number;
  filteredHistory: TranscriptionHistory[];
  groupedHistory: Record<string, TranscriptionHistory[]>;
  visibleCount: number;
  onLoadMore: () => void;
  isLoading: boolean;
  loadError: string | null;
  onRetry?: () => void;
  hotkey: string;
  availability: {
    canRecord: boolean;
    canAutoInsert: boolean;
    unavailableMessage: string;
  };
  reTranscribingIds: Set<string>;
  reTranscribingModels: Map<string, string>;
  verifiedRecordings: Set<string>;
  checkedRecordings: Set<string>;
  showOriginalIds: Set<string>;
  onToggleOriginal: (id: string) => void;
  onReTranscribe: (item: TranscriptionHistory) => void;
  onShowInFolder: (item: TranscriptionHistory) => void;
  onDelete: (e: React.MouseEvent, id: string) => void;
}

export function RecentRecordingsList({
  historyLength,
  filteredHistory,
  groupedHistory,
  visibleCount,
  onLoadMore,
  isLoading,
  loadError,
  onRetry,
  hotkey,
  availability,
  reTranscribingIds,
  reTranscribingModels,
  verifiedRecordings,
  checkedRecordings,
  showOriginalIds,
  onToggleOriginal,
  onReTranscribe,
  onShowInFolder,
  onDelete,
}: RecentRecordingsListProps) {
  if (historyLength > 0) {
    if (filteredHistory.length === 0) {
      return (
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            <Search className="w-12 h-12 text-muted-foreground/30 mx-auto mb-4" />
            <p className="text-sm text-muted-foreground">No transcriptions found</p>
            <p className="text-xs text-muted-foreground/70 mt-2">
              Try adjusting your search or filters
            </p>
          </div>
        </div>
      );
    }

    return (
      <ScrollArea className="h-full">
        <div className="py-4 pl-2 pr-4 space-y-6">
          {Object.entries(groupedHistory).map(([date, items]) => (
            <div key={date} className="space-y-2.5">
              <p className="px-1 text-xs font-medium text-muted-foreground">
                {date} <span className="text-muted-foreground/50">· {items.length}</span>
              </p>
              <div className="overflow-hidden rounded-2xl border border-border bg-card">
                {items.map((item) => (
                  <RecentRecordingRow
                    key={item.id}
                    item={item}
                    reTranscribingIds={reTranscribingIds}
                    reTranscribingModels={reTranscribingModels}
                    verifiedRecordings={verifiedRecordings}
                    checkedRecordings={checkedRecordings}
                    showOriginal={showOriginalIds.has(item.id)}
                    onToggleOriginal={onToggleOriginal}
                    onReTranscribe={onReTranscribe}
                    onShowInFolder={onShowInFolder}
                    onDelete={onDelete}
                  />
                ))}
              </div>
            </div>
          ))}
          {filteredHistory.length > visibleCount && (
            <div className="flex justify-center pt-1">
              <Button variant="secondary" size="sm" onClick={onLoadMore}>
                Load more · showing {visibleCount} of {filteredHistory.length}
              </Button>
            </div>
          )}
          <p className="flex items-center gap-2 pt-1 text-xs text-muted-foreground">
            <ShieldCheck className="h-3.5 w-3.5 text-sage" />
            Every transcript stays on this {isMacOS ? "Mac" : "PC"}. Nothing syncs to a cloud.
          </p>
        </div>
      </ScrollArea>
    );
  }

  if (isLoading) {
    return (
      <div aria-hidden className="py-4 pl-2 pr-4 space-y-2.5">
        {[0, 1, 2].map((key) => (
          <div key={key} className="h-16 rounded-lg bg-muted/60 animate-pulse" />
        ))}
      </div>
    );
  }

  if (loadError) {
    return (
      <div className="py-4 pl-2 pr-4">
        <div className="flex items-center gap-3.5 rounded-2xl border border-border bg-amber-500/[0.04] px-5 py-4">
          <div className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-xl border border-amber-500/30 bg-amber-500/10 text-amber-500">
            <AlertTriangle className="h-4 w-4" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="text-xs font-medium text-amber-600 dark:text-amber-400">
              Couldn&apos;t load your history.
            </p>
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onRetry?.();
              }}
              className="mt-1 text-[11px] font-medium text-sage hover:underline"
            >
              Retry
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="text-center">
        {availability.canRecord ? (
          <>
            <Mic className="w-12 h-12 text-muted-foreground/30 mx-auto mb-4" />
            <p className="text-sm text-muted-foreground">No recordings yet</p>
            {availability.canAutoInsert ? (
              <p className="text-xs text-muted-foreground/70 mt-2">
                Press {formatHotkey(hotkey)} to record. Save recordings in Settings to enable
                re-transcription.
              </p>
            ) : (
              <p className="text-xs text-amber-600 mt-2">
                Recording available but accessibility permission needed for hotkeys
              </p>
            )}
          </>
        ) : (
          <>
            <AlertCircle className="w-12 h-12 text-amber-500/50 mx-auto mb-4" />
            <p className="text-sm text-muted-foreground">Cannot record yet</p>
            <p className="text-xs text-amber-600 mt-2">{availability.unavailableMessage}</p>
          </>
        )}
      </div>
    </div>
  );
}
