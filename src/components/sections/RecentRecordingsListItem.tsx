import { memo } from "react";
import type { KeyboardEvent } from "react";
import type { TranscriptionHistory } from "@/types";
import { sourceLabel } from "./recentRecordingsHelpers";
import { parseHistoryDate } from "@/lib/history-date";
import { RecentRecordingApplicationIcon } from "./RecentRecordingApplicationIcon";

// Hoisted: building an Intl formatter per row per render is the expensive part.
const TIME_FORMAT = new Intl.DateTimeFormat([], {
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});

function formatTime(timestamp: Date | string): string {
  const date = parseHistoryDate(timestamp);
  // Intl.format throws on an invalid date where toLocaleTimeString did not.
  return date ? TIME_FORMAT.format(date) : "Unknown date";
}

interface RecentRecordingsListItemProps {
  item: TranscriptionHistory;
  selected: boolean;
  onSelect: (id: string) => void;
  onKeyDown: (event: KeyboardEvent<HTMLButtonElement>, id: string) => void;
}

/**
 * One history row. Memoised: `item` objects are stable between history
 * updates and the handlers are id-based, so selecting a row re-renders only the
 * previously and newly selected rows. `content-visibility: auto` lets the
 * browser skip layout and paint for rows far off-screen; the intrinsic height
 * matches the row's min height so the scrollbar doesn't jump.
 */
export const RecentRecordingsListItem = memo(function RecentRecordingsListItem({
  item,
  selected,
  onSelect,
  onKeyDown,
}: RecentRecordingsListItemProps) {
  const app = item.writing?.context_hint?.app_name;
  const name = app && app !== "Other" ? app : sourceLabel(item.writing?.source);
  return (
    <button
      id={`history-item-${item.id}`}
      type="button"
      aria-current={selected ? "true" : undefined}
      onClick={() => onSelect(item.id)}
      onKeyDown={(event) => onKeyDown(event, item.id)}
      className={`block min-h-[77px] w-full border-b border-border px-4 py-[11px] text-left transition-colors [contain-intrinsic-size:auto_77px] [content-visibility:auto] focus-visible:outline-2 focus-visible:outline-sage ${selected ? "bg-sage-bg" : "hover:bg-muted/60"}`}
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
          {formatTime(item.timestamp)}
        </time>
      </span>
      <span className="line-clamp-2 text-[12.5px] leading-[18px] text-muted-foreground">
        {item.text}
      </span>
    </button>
  );
});
