import type { MouseEvent } from "react";
import { Copy, FolderOpen, Loader2, RotateCcw, Sparkles, Trash2, X } from "lucide-react";
import { toast } from "sonner";
import { getModelDisplayName } from "@/lib/model-display";
import type { TranscriptionHistory } from "@/types";
import { formatDurationMs, sourceLabel } from "./recentRecordingsHelpers";

interface Props {
  item: TranscriptionHistory;
  verified: boolean;
  checked: boolean;
  reTranscribing: boolean;
  reTranscribingModel?: string;
  onReTranscribe: (item: TranscriptionHistory) => void;
  onShowInFolder: (item: TranscriptionHistory) => void;
  onDelete: (event: MouseEvent, id: string) => void;
  onClose: () => void;
}

function formatHistoryDateTime(timestamp: Date): string {
  const date = new Date(timestamp);
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const day = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const dayOffset = Math.round((today.getTime() - day.getTime()) / 86_400_000);
  const label =
    dayOffset === 0
      ? "Today"
      : dayOffset === 1
        ? "Yesterday"
        : date.toLocaleDateString([], { month: "short", day: "numeric" });
  const time = date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  return `${label} ${time}`;
}

export function RecentRecordingDetail({
  item,
  verified,
  checked,
  reTranscribing,
  reTranscribingModel,
  onReTranscribe,
  onShowInFolder,
  onDelete,
  onClose,
}: Props) {
  const failed = item.status === "failed";
  const inProgress = item.status === "in_progress" || reTranscribing;
  const app = item.writing?.context_hint?.app_name;
  const name = app && app !== "Other" ? app : sourceLabel(item.writing?.source);
  const words = item.text.trim() ? item.text.trim().split(/\s+/).length : 0;
  const original =
    item.writing?.original_text && item.writing.original_text !== item.text
      ? item.writing.original_text
      : null;
  const dateTime = formatHistoryDateTime(item.timestamp);
  const duration = item.writing?.audio_duration_ms;
  const meta = [
    name,
    dateTime,
    `${words} word${words === 1 ? "" : "s"}`,
    duration != null
      ? duration < 60_000
        ? `${(duration / 1000).toFixed(1)} s`
        : formatDurationMs(duration)
      : null,
  ]
    .filter(Boolean)
    .join(" · ");
  const copy = (text: string) => {
    void navigator.clipboard.writeText(text);
    toast.success("Copied to clipboard");
  };
  return (
    <div className="h-full overflow-y-auto p-[22px]">
      <div className="flex items-start justify-between gap-2">
        <p className="min-w-0 pt-[9px] text-[12px] text-muted-foreground">{meta}</p>
        <div className="flex shrink-0 items-center gap-[2px]">
          {!inProgress && (
            <button
              type="button"
              aria-label="Copy"
              title="Copy"
              onClick={() => copy(item.text)}
              className="grid size-[33px] place-items-center rounded-[10px] text-muted-foreground hover:bg-muted"
            >
              <Copy className="size-[15px]" />
            </button>
          )}
          {verified && (
            <button
              type="button"
              aria-label="Show recording in folder"
              title="Show recording in folder"
              onClick={() => onShowInFolder(item)}
              className="grid size-[33px] place-items-center rounded-[10px] text-muted-foreground hover:bg-muted"
            >
              <FolderOpen className="size-[15px]" />
            </button>
          )}
          {verified && (
            <button
              type="button"
              aria-label="Re-transcribe with current source"
              title="Re-transcribe with current source"
              disabled={inProgress}
              onClick={() => void onReTranscribe(item)}
              className="grid size-[33px] place-items-center rounded-[10px] text-muted-foreground hover:bg-muted disabled:opacity-50"
            >
              {inProgress ? (
                <Loader2 className="size-[15px] animate-spin" />
              ) : (
                <RotateCcw className="size-[15px]" />
              )}
            </button>
          )}
          <button
            type="button"
            aria-label="Delete"
            title="Delete"
            onClick={(event) => onDelete(event, item.id)}
            className="grid size-[33px] place-items-center rounded-[10px] text-muted-foreground hover:bg-muted hover:text-destructive"
          >
            <Trash2 className="size-[15px]" />
          </button>
          <button
            type="button"
            aria-label="Close detail"
            title="Close detail"
            onClick={onClose}
            className="grid size-[33px] place-items-center rounded-[10px] text-muted-foreground hover:bg-muted @min-[688px]:hidden"
          >
            <X className="size-[15px]" />
          </button>
        </div>
      </div>
      {inProgress && (
        <p className="mt-3 text-sm text-muted-foreground">
          {item.status === "in_progress" && !reTranscribing
            ? `Re-transcription in progress with ${getModelDisplayName(item.model) ?? item.model}...`
            : `Re-transcribing with ${reTranscribingModel}...`}
        </p>
      )}
      {failed && (
        <p className="mt-3 text-sm text-muted-foreground">
          {verified
            ? "Transcription failed - recording preserved"
            : checked
              ? "Transcription failed - recording unavailable for retry"
              : "Transcription failed"}
        </p>
      )}
      {item.writing?.translation_failed && !failed && !inProgress && (
        <p className="mt-3 text-sm text-muted-foreground">
          {item.writing.target_language
            ? `Translation to ${item.writing.target_language} failed - saved untranslated`
            : "Translation failed - saved untranslated"}
        </p>
      )}
      <p className="mt-[14px] whitespace-pre-wrap break-words text-[17px] font-medium leading-[26px] text-foreground">
        {item.text}
      </p>
      {original && (
        <section aria-label="Before polish" className="mt-[14px] rounded-[10px] bg-muted p-[14px]">
          <div className="flex items-center justify-between gap-2">
            <h2 className="inline-flex items-center gap-[6px] text-[10.5px] font-semibold uppercase tracking-[0.5px] text-muted-foreground">
              <Sparkles className="size-3" />
              Before polish
            </h2>
            <button
              type="button"
              title="Copy original transcript"
              aria-label="Copy original transcript"
              onClick={() => copy(original)}
              className="text-xs text-muted-foreground hover:text-sage"
            >
              Copy original
            </button>
          </div>
          <p className="mt-[6px] whitespace-pre-wrap break-words text-[13px] leading-[19px] text-muted-foreground">
            {original}
          </p>
        </section>
      )}
      <div className="mt-[14px] flex flex-wrap gap-2 text-[11.5px] text-muted-foreground">
        {item.model && (
          <span className="rounded-[7px] border border-border px-[9px] py-1">
            {getModelDisplayName(item.model) ?? item.model}
          </span>
        )}
        {item.writing?.mode && (
          <span className="rounded-[7px] border border-border px-[9px] py-1">
            Polish: {item.writing.mode}
          </span>
        )}
        {item.writing?.ai_provider && (
          <span className="rounded-[7px] border border-border px-[9px] py-1">
            {item.writing.ai_model
              ? (getModelDisplayName(item.writing.ai_model) ?? item.writing.ai_model)
              : item.writing.ai_provider}
          </span>
        )}
        {item.writing?.diarized && (
          <span className="rounded-[7px] border border-border px-[9px] py-1">Speakers</span>
        )}
      </div>
    </div>
  );
}
