import { useCallback, useEffect, useRef } from "react";
import type { KeyboardEvent } from "react";
import { AlertTriangle, Search, History } from "lucide-react";
import { KeyCaps } from "@/components/KeyCaps";
import { Button } from "@/components/settings/SettingsButton";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import { isMacOS } from "@/lib/platform";
import type { TranscriptionHistory } from "@/types";
import { RecentRecordingsListItem } from "./RecentRecordingsListItem";
import type { HomeStatus } from "@/lib/home-readiness";

export interface RecentRecordingsListProps {
  historyLength: number;
  filteredHistory: TranscriptionHistory[];
  groupedHistory: Record<string, TranscriptionHistory[]>;
  visibleCount: number;
  onLoadMore: () => void;
  selectedId: string | null;
  onSelect: (id: string) => void;
  isLoading: boolean;
  loadError: string | null;
  onRetry?: () => void;
  onTranscribeFile?: () => void;
  hotkey: string;
  readinessStatus: HomeStatus;
  onReadinessAction: () => void;
}

export function RecentRecordingsList({
  historyLength,
  filteredHistory,
  groupedHistory,
  visibleCount,
  onLoadMore,
  selectedId,
  onSelect,
  isLoading,
  loadError,
  onRetry,
  onTranscribeFile,
  hotkey,
  readinessStatus,
  onReadinessAction,
}: RecentRecordingsListProps) {
  const visible = filteredHistory.slice(0, visibleCount);
  // Keyboard handler reads the latest list/select through refs so its identity
  // never changes and memoised rows aren't invalidated by it.
  const visibleRef = useRef(visible);
  const onSelectRef = useRef(onSelect);
  useEffect(() => {
    visibleRef.current = visible;
    onSelectRef.current = onSelect;
  });
  const handleSelect = useCallback((id: string) => onSelectRef.current(id), []);
  const handleKeyDown = useCallback((event: KeyboardEvent<HTMLButtonElement>, id: string) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const list = visibleRef.current;
    const index = list.findIndex((item) => item.id === id);
    const next = list[index + (event.key === "ArrowDown" ? 1 : -1)];
    if (!next) return;
    onSelectRef.current(next.id);
    document.getElementById(`history-item-${next.id}`)?.focus();
  }, []);

  if (historyLength === 0 && isLoading)
    return (
      <div aria-hidden className="space-y-2 p-4">
        {[0, 1, 2].map((key) => (
          <div key={key} className="h-[77px] animate-pulse rounded-lg bg-muted" />
        ))}
      </div>
    );
  if (historyLength === 0 && loadError)
    return (
      <div className="p-4">
        <div className="flex items-center gap-3 rounded-[10px] bg-muted p-4">
          <AlertTriangle className="size-5 text-muted-foreground" />
          <div>
            <p className="text-sm text-muted-foreground">Couldn&apos;t load your history.</p>
            <button type="button" onClick={onRetry} className="mt-1 text-sm text-sage underline">
              Retry
            </button>
          </div>
        </div>
      </div>
    );
  if (filteredHistory.length === 0)
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        {historyLength === 0 ? <History className="size-7 text-muted-foreground" /> : <Search className="size-7 text-muted-foreground" />}
        {historyLength === 0 ? (
          readinessStatus.ready ? (
            <p className="text-sm text-muted-foreground">
              Your dictations will show up here. Press{" "}
              <span className="[&_kbd]:text-muted-foreground [&>span]:text-muted-foreground">
                <KeyCaps caps={shortcutKeyCaps(hotkey, isMacOS ? "darwin" : "windows")} />
              </span>{" "}
              to start.
            </p>
          ) : (
            <div className="space-y-2 text-sm">
              <p className="text-muted-foreground">Your dictations will show up here.</p>
              <button
                type="button"
                onClick={onReadinessAction}
                className="font-medium text-foreground underline"
              >
                {readinessStatus.label} —{" "}
                {readinessStatus.label === "No model yet" ? "Choose a model" : "Review settings"}
              </button>
            </div>
          )
        ) : (
          <p className="text-sm text-muted-foreground">No dictations match</p>
        )}
        {onTranscribeFile && (
          <Button type="button" variant="outline" onClick={onTranscribeFile}>
            Transcribe a file…
          </Button>
        )}
      </div>
    );

  return (
    <div className="h-full overflow-y-auto" aria-label="Dictations">
      {Object.entries(groupedHistory).map(([day, items]) => (
        <div key={day}>
          <div className="px-4 pb-[6px] pt-3 text-[10.5px] font-semibold tracking-[0.5px] text-muted-foreground">
            {day.toUpperCase()}
          </div>
          {items.map((item) => (
            <RecentRecordingsListItem
              key={item.id}
              item={item}
              selected={item.id === selectedId}
              onSelect={handleSelect}
              onKeyDown={handleKeyDown}
            />
          ))}
        </div>
      ))}
      {filteredHistory.length > visibleCount && (
        <div className="p-4 text-center">
          <Button type="button" variant="outline" onClick={onLoadMore}>
            Load more · showing {visibleCount} of {filteredHistory.length}
          </Button>
        </div>
      )}
    </div>
  );
}
