import { createElement } from "react";
import {
  AlertTriangle,
  Copy,
  FolderOpen,
  Loader2,
  RotateCcw,
  Sparkles,
  ChevronDown,
  Trash2,
} from "lucide-react";
import { TranscriptionHistory } from "@/types";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { getModelDisplayName } from "@/lib/model-display";
import { formatDurationMs, sourceIcon, sourceLabel } from "./recentRecordingsHelpers";
import { RecentRecordingApplicationIcon } from "./RecentRecordingApplicationIcon";

function copyTextToClipboard(text: string) {
  navigator.clipboard.writeText(text);
  toast.success("Copied to clipboard");
}

export interface RecentRecordingRowProps {
  item: TranscriptionHistory;
  reTranscribingIds: Set<string>;
  reTranscribingModels: Map<string, string>;
  verifiedRecordings: Set<string>;
  checkedRecordings: Set<string>;
  showOriginal: boolean;
  onToggleOriginal: (id: string) => void;
  onReTranscribe: (item: TranscriptionHistory) => void;
  onShowInFolder: (item: TranscriptionHistory) => void;
  onDelete: (e: React.MouseEvent, id: string) => void;
}

export function RecentRecordingRow({
  item,
  reTranscribingIds,
  reTranscribingModels,
  verifiedRecordings,
  checkedRecordings,
  showOriginal,
  onToggleOriginal,
  onReTranscribe,
  onShowInFolder,
  onDelete,
}: RecentRecordingRowProps) {
  const isFailed = item.status === "failed";
  const isPersistedInProgress = item.status === "in_progress";
  const isInProgress = reTranscribingIds.has(item.id) || isPersistedInProgress;
  const hasOriginal = Boolean(
    item.writing?.ai_applied &&
    item.writing?.original_text &&
    item.writing.original_text !== item.text,
  );
  const originalText = item.writing?.original_text;
  const displayText = item.text;
  const wordCount = displayText.trim() ? displayText.trim().split(/\s+/).length : 0;
  const appContext = item.writing?.context_hint;
  const usesDesktopApp =
    (!item.writing?.source || item.writing.source === "desktop_recording") &&
    Boolean(appContext?.app_name);
  const sourceDisplayName =
    usesDesktopApp && appContext?.app_name
      ? appContext.app_name
      : sourceLabel(item.writing?.source);
  const canCopyRow = !isFailed && !isInProgress;

  return (
    <div
      className={cn(
        "group relative flex cursor-pointer gap-3.5 border-t border-border px-5 py-4 transition-colors first:border-t-0",
        isFailed ? "bg-amber-500/[0.04]" : "hover:bg-muted/40",
      )}
    >
      {(isInProgress || isFailed) && (
        <div
          className={cn(
            "mt-0.5 grid size-9 shrink-0 place-items-center rounded-xl border",
            isInProgress
              ? "border-sage/30 bg-sage-bg text-sage"
              : "border-amber-500/30 bg-amber-500/10 text-amber-500",
          )}
          title={sourceDisplayName}
        >
          {isInProgress ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <AlertTriangle className="h-4 w-4" />
          )}
        </div>
      )}
      <div className="min-w-0 flex-1">
        {isInProgress && (
          <p className="mb-1 text-xs font-medium text-sage">
            {isPersistedInProgress && !reTranscribingIds.has(item.id)
              ? `Re-transcription in progress with ${getModelDisplayName(item.model) ?? item.model}...`
              : `Re-transcribing with ${reTranscribingModels.get(item.id)}...`}
          </p>
        )}
        {isFailed && !isInProgress && (
          <div className="mb-1 flex flex-wrap items-center gap-2">
            <span className="text-xs font-medium text-amber-600 dark:text-amber-400">
              {verifiedRecordings.has(item.id)
                ? "Transcription failed - recording preserved"
                : checkedRecordings.has(item.id)
                  ? "Transcription failed - recording unavailable for retry"
                  : "Transcription failed"}
            </span>
            {verifiedRecordings.has(item.id) && (
              <button
                type="button"
                onClick={() => {
                  void onReTranscribe(item);
                }}
                className="inline-flex items-center gap-1 rounded-md bg-amber-500/15 px-2 py-0.5 text-xs font-medium text-amber-600 transition-colors hover:bg-amber-500/25 dark:text-amber-400"
              >
                <RotateCcw className="h-3 w-3" /> Re-transcribe
              </button>
            )}
          </div>
        )}
        {item.writing?.translation_failed && !isFailed && !isInProgress && (
          <p className="mb-1 text-xs font-medium text-amber-600 dark:text-amber-400">
            {item.writing.target_language
              ? `Translation to ${item.writing.target_language} failed - saved untranslated`
              : "Translation failed - saved untranslated"}
          </p>
        )}

        {canCopyRow ? (
          <button
            type="button"
            onClick={() => copyTextToClipboard(displayText)}
            onKeyDown={(e) => {
              if (e.key !== "Enter" && e.key !== " ") return;
              e.preventDefault();
              copyTextToClipboard(displayText);
            }}
            className="w-full cursor-pointer bg-transparent p-0 text-left text-sm leading-relaxed text-foreground line-clamp-3"
          >
            {displayText}
          </button>
        ) : (
          <p
            className={cn(
              "text-sm leading-relaxed line-clamp-3",
              isFailed ? "italic text-muted-foreground" : "text-foreground",
            )}
          >
            {displayText}
          </p>
        )}

        <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
          <span className="inline-flex items-center gap-1.5 font-medium text-foreground/80">
            {usesDesktopApp && appContext?.app_name ? (
              <RecentRecordingApplicationIcon
                appName={appContext.app_name}
                processPath={appContext.process_path}
              />
            ) : (
              createElement(sourceIcon(item.writing?.source), {
                "aria-hidden": true,
                className: "size-3.5",
              })
            )}
            {sourceDisplayName}
          </span>
          {item.model && (
            <>
              <span className="text-muted-foreground/40">·</span>
              <span>{getModelDisplayName(item.model) ?? item.model}</span>
            </>
          )}
          {item.writing?.ai_provider && (
            <>
              <span className="text-muted-foreground/40">·</span>
              <span className="inline-flex items-center gap-1 text-foreground/70">
                <Sparkles className="h-3 w-3" />
                {item.writing.ai_model
                  ? (getModelDisplayName(item.writing.ai_model) ?? item.writing.ai_model)
                  : item.writing.ai_provider}
              </span>
            </>
          )}
          <span className="text-muted-foreground/40">·</span>
          <span>
            {new Date(item.timestamp).toLocaleTimeString([], {
              hour: "numeric",
              minute: "2-digit",
            })}
          </span>
          {wordCount > 0 && (
            <>
              <span className="text-muted-foreground/40">·</span>
              <span>{wordCount} words</span>
            </>
          )}
          {item.writing?.audio_duration_ms != null && (
            <>
              <span className="text-muted-foreground/40">·</span>
              <span>{formatDurationMs(item.writing.audio_duration_ms)}</span>
            </>
          )}
          {item.writing?.diarized && (
            <span className="rounded-md bg-muted px-1.5 py-0.5 text-[11px] font-medium">
              Speakers
            </span>
          )}
          {hasOriginal && (
            <button
              type="button"
              onClick={() => onToggleOriginal(item.id)}
              className="inline-flex items-center gap-1 text-[11px] font-medium text-sage hover:underline"
              title={showOriginal ? "Hide original transcript" : "Show original text before Polish"}
            >
              <ChevronDown
                className={cn("h-3 w-3 transition-transform", showOriginal && "rotate-180")}
              />
              {showOriginal ? "Hide original" : "Show original"}
            </button>
          )}
        </div>

        {hasOriginal && showOriginal && originalText && (
          <div className="mt-2 rounded-lg border border-border/60 bg-muted/40 px-3 py-2.5">
            <div className="flex items-center justify-between gap-2">
              <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                Original · before Polish
              </p>
              <button
                type="button"
                onClick={() => copyTextToClipboard(originalText)}
                className="text-[11px] font-medium text-sage hover:underline"
                title="Copy original transcript"
              >
                Copy
              </button>
            </div>
            <p className="mt-1 whitespace-pre-wrap text-sm leading-relaxed text-muted-foreground">
              {originalText}
            </p>
          </div>
        )}
      </div>

      <div className="flex shrink-0 items-start gap-1 opacity-0 transition-opacity group-hover:opacity-100">
        {!isInProgress && (
          <button
            type="button"
            onClick={() => copyTextToClipboard(displayText)}
            className="grid size-7 place-items-center rounded-md border border-border bg-card text-muted-foreground transition-colors hover:text-foreground"
            title="Copy"
          >
            <Copy className="h-3.5 w-3.5" />
          </button>
        )}
        {verifiedRecordings.has(item.id) && (
          <button
            type="button"
            onClick={() => onShowInFolder(item)}
            className="grid size-7 place-items-center rounded-md border border-border bg-card text-muted-foreground transition-colors hover:text-foreground"
            title="Show recording in folder"
          >
            <FolderOpen className="h-3.5 w-3.5" />
          </button>
        )}
        {verifiedRecordings.has(item.id) && (
          <button
            type="button"
            onClick={() => {
              void onReTranscribe(item);
            }}
            disabled={isInProgress}
            className={cn(
              "grid size-7 place-items-center rounded-md border border-border bg-card text-muted-foreground transition-colors hover:text-foreground",
              isInProgress && "pointer-events-none",
            )}
            title="Re-transcribe with current source"
          >
            {isInProgress ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RotateCcw className="h-3.5 w-3.5" />
            )}
          </button>
        )}
        <button
          type="button"
          onClick={(e) => onDelete(e, item.id)}
          className="grid size-7 place-items-center rounded-md border border-border bg-card text-muted-foreground transition-colors hover:border-destructive/40 hover:text-destructive"
          title="Delete"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}
