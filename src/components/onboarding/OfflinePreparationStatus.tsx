import { Button } from "@/components/settings/SettingsButton";
import { Progress } from "@/components/ui/progress";
import { isMacOS } from "@/lib/platform";

/** The download error in words a user can act on, never the raw message. */
export function failureReason(error: string) {
  const text = error.toLowerCase();
  if (/no space left|not enough (free )?(disk )?space|insufficient (disk )?space|disk (is )?full/.test(text)) {
    return "There isn’t enough free disk space for it.";
  }
  if (/(checksum|hash|sha-?256) mismatch|corrupt/.test(text)) return "The file didn’t download correctly.";
  if (/timed out|timeout|connection (reset|refused|closed|aborted)|error sending request|failed to connect|could not connect|dns|network is unreachable|offline/.test(text)) {
    return "The connection dropped. Check your internet and try again.";
  }
  return "Try again.";
}

/** Where the on-device engine is: downloading, checking, or stopped. */
export function OfflinePreparationStatus({ compact, ready, verifying, progress, size, error, onRetry, onChoose }: {
  /** One small line, for the steps after the download started. */
  compact?: boolean;
  /** Downloaded, checked and selected. */
  ready: boolean;
  verifying: boolean;
  /** Percent downloaded, while a download runs. */
  progress: number | undefined;
  /** The engine's download size, such as "477 MB". */
  size: string | null;
  /** Why the last download failed, if it did. */
  error: string | undefined;
  onRetry: () => void;
  onChoose: () => void;
}) {
  const of = size ? ` of ${size}` : "";
  const idle = !ready && !error && !verifying && progress === undefined;
  const line = ready ? `The dictation engine${size ? ` (${size})` : ""} is ready on this ${isMacOS ? "Mac" : "PC"}.`
    : error ? `The download stopped. ${failureReason(error)}`
    : verifying ? "Checking the dictation engine. This happens once."
    : idle ? `The dictation engine${size ? ` (${size})` : ""} isn’t downloaded yet.`
    : `Getting the dictation engine ready · ${Math.round(progress ?? 0)}%${of}. This happens once.`;
  if (compact) {
    const short = error ? "Dictation engine download stopped."
      : verifying ? "Dictation engine: checking."
      : idle ? "Dictation engine isn’t downloaded yet."
      : `Dictation engine · ${Math.round(progress ?? 0)}%${of}`;
    return (
      <p role={error ? "alert" : "status"} className="flex items-center gap-2 text-[12.5px] text-muted-foreground">
        {short}
        {(error || idle) && (
          <Button variant="link" size="sm" className="h-auto p-0" onClick={onRetry}>{error ? "Retry" : "Download"}</Button>
        )}
      </p>
    );
  }
  return (
    <div className="flex w-full max-w-[420px] flex-col items-center gap-3 text-sm text-muted-foreground">
      {!ready && !error && !idle && (
        <Progress
          aria-label="Dictation engine download"
          value={verifying ? 100 : progress ?? 0}
          className="w-full"
        />
      )}
      <p role={error ? "alert" : "status"}>{line}</p>
      {(error || idle) && <Button variant="outline" onClick={onRetry}>{error ? "Retry" : "Download"}</Button>}
      <Button variant="link" size="sm" className="h-auto p-0 text-muted-foreground" onClick={onChoose}>
        Choose a different model
      </Button>
    </div>
  );
}
