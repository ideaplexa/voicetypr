import type { KeyboardEvent } from "react";
import { AlertTriangle, Search } from "lucide-react";
import { KeyCaps } from "@/components/KeyCaps";
import { Button } from "@/components/ui/button";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import { isMacOS } from "@/lib/platform";
import type { TranscriptionHistory } from "@/types";
import { sourceLabel } from "./recentRecordingsHelpers";
import { RecentRecordingApplicationIcon } from "./RecentRecordingApplicationIcon";

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
}: RecentRecordingsListProps) {
  const visible = filteredHistory.slice(0, visibleCount);
  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>, id: string) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const index = visible.findIndex((item) => item.id === id);
    const next = visible[index + (event.key === "ArrowDown" ? 1 : -1)];
    if (!next) return;
    onSelect(next.id);
    document.getElementById(`history-item-${next.id}`)?.focus();
  };

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
        <Search className="size-7 text-muted-foreground" />
        {historyLength === 0 ? (
          <p className="text-sm text-muted-foreground">
            Your dictations will show up here. Press{" "}
            <span className="[&_kbd]:text-muted-foreground [&>span]:text-muted-foreground">
              <KeyCaps caps={shortcutKeyCaps(hotkey, isMacOS ? "darwin" : "windows")} />
            </span>{" "}
            to start.
          </p>
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
          {items.map((item) => {
            const app = item.writing?.context_hint?.app_name;
            const name = app && app !== "Other" ? app : sourceLabel(item.writing?.source);
            const selected = item.id === selectedId;
            return (
              <button
                id={`history-item-${item.id}`}
                key={item.id}
                type="button"
                aria-current={selected ? "true" : undefined}
                onClick={() => onSelect(item.id)}
                onKeyDown={(event) => handleKeyDown(event, item.id)}
                className={`block min-h-[77px] w-full border-b border-border px-4 py-[11px] text-left transition-colors focus-visible:outline-2 focus-visible:outline-sage ${selected ? "bg-sage-bg" : "hover:bg-muted/60"}`}
              >
                <span className="mb-1 flex items-start justify-between gap-2 text-[11.5px] font-semibold text-muted-foreground">
                  <span
                    className={`inline-flex min-w-0 items-center gap-1 truncate ${selected ? "text-[14px] text-sage" : ""}`}
                  >
                    {app && app !== "Other" && item.writing?.context_hint?.process_path ? (
                      <RecentRecordingApplicationIcon
                        appName={app}
                        processPath={item.writing.context_hint.process_path}
                      />
                    ) : null}
                    {name}
                  </span>
                  <time className="shrink-0 font-mono font-normal text-muted-foreground">
                    {new Date(item.timestamp).toLocaleTimeString([], {
                      hour: "2-digit",
                      minute: "2-digit",
                      hour12: false,
                    })}
                  </time>
                </span>
                <span className="line-clamp-2 text-[12.5px] leading-[18px] text-muted-foreground">
                  {item.text}
                </span>
              </button>
            );
          })}
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
