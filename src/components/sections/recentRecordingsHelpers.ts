import { FileAudio, Globe, Mic, Terminal } from "lucide-react";
import { getModelDisplayName } from "@/lib/model-display";
import type { TranscriptionHistory } from "@/types";

// Pure helpers split from RecentRecordings.tsx so the component file only
// exports components.

/** Lucide fallback for transcripts without a captured application icon. */
export function sourceIcon(source: string | undefined) {
  switch (source) {
    case "audio_file":
    case "audio_bytes":
      return FileAudio;
    case "remote_server":
      return Globe;
    case "cli":
      return Terminal;
    default:
      return Mic;
  }
}

/** Build a plain-text export of transcript history. */
export function buildPlainHistory(items: TranscriptionHistory[]): string {
  return items
    .map((item) => {
      const when = new Date(item.timestamp).toLocaleString();
      const model = getModelDisplayName(item.model) ?? item.model ?? "";
      return `[${when}]${model ? ` ${model}` : ""}\n${item.text}\n`;
    })
    .join("\n");
}

/** Build a Markdown export of transcript history. */
export function buildMarkdownHistory(items: TranscriptionHistory[]): string {
  const lines: string[] = ["# Voicetypr transcript history", ""];
  for (const item of items) {
    const when = new Date(item.timestamp).toLocaleString();
    const model = getModelDisplayName(item.model) ?? item.model ?? "";
    lines.push(`## ${when}${model ? ` · ${model}` : ""}`, "", item.text, "");
  }
  return lines.join("\n");
}
/** Map raw source values to user-facing labels. */
export function sourceLabel(source: string | undefined): string {
  switch (source) {
    case "audio_file":
    case "audio_bytes":
      return "Upload";
    case "remote_server":
      return "Remote";
    case "cli":
      return "CLI";
    case "desktop_recording":
    default:
      return "This device";
  }
}

/** Format milliseconds as m:ss (e.g. 90 000 ms → "1:30"). */
export function formatDurationMs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return `${min}:${sec.toString().padStart(2, "0")}`;
}

/**
 * Structural filters for the history list (source, app, date).
 * Text search is handled separately in the component to support model display-name matching.
 * Under a specific source filter, only rows whose writing.source maps to that source pass;
 * rows with no/unknown source are excluded (they appear only under 'all').
 */
export function applyHistoryFilters(
  history: TranscriptionHistory[],
  sourceFilter: string,
  appFilter: string,
  dateFilter: string,
  now?: Date,
): TranscriptionHistory[] {
  const todayBase = now ? new Date(now) : new Date();
  todayBase.setHours(0, 0, 0, 0);
  const sevenDaysAgo = new Date(todayBase);
  sevenDaysAgo.setDate(sevenDaysAgo.getDate() - 6);

  return history.filter((item) => {
    // Source filter — requires an exact match; rows with no/unknown source are excluded
    if (sourceFilter !== "all") {
      const src = item.writing?.source;
      if (sourceFilter === "desktop_recording" && src !== "desktop_recording") return false;
      if (sourceFilter === "audio_file" && src !== "audio_file" && src !== "audio_bytes")
        return false;
      if (sourceFilter === "remote_server" && src !== "remote_server") return false;
      if (sourceFilter === "cli" && src !== "cli") return false;
    }

    // App filter
    if (appFilter !== "all" && item.writing?.context_hint?.app_name !== appFilter) return false;

    // Date filter
    if (dateFilter !== "all") {
      const itemDate = new Date(item.timestamp);
      itemDate.setHours(0, 0, 0, 0);
      if (dateFilter === "today" && itemDate.getTime() !== todayBase.getTime()) return false;
      if (dateFilter === "last7" && itemDate < sevenDaysAgo) return false;
    }

    return true;
  });
}
