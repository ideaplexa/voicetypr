import { Button } from "@/components/settings/SettingsButton";

export function OfflinePreparationStatus({ ready, verifying, progress, failed, onCancel, onRetry, onChoose }: {
  ready: boolean;
  verifying: boolean;
  progress: number | undefined;
  failed: boolean;
  onCancel: () => void;
  onRetry: () => void;
  onChoose: () => void;
}) {
  return (
    <div className="flex w-full flex-col items-center gap-2 text-sm text-muted-foreground">
      <p role="status">
        {ready ? "Offline dictation ready" : verifying ? "Verifying…" : failed
          ? "Offline preparation failed. Try again."
          : progress !== undefined ? `Preparing offline dictation · ${Math.round(progress)}%`
          : "Prepare offline dictation before you try it."}
      </p>
      <div className="flex flex-wrap justify-center gap-2">
        {!ready && (progress !== undefined || verifying
          ? <Button variant="ghost" onClick={onCancel}>Cancel download</Button>
          : <Button variant="outline" onClick={onRetry}>Retry</Button>)}
        <Button variant="ghost" onClick={onChoose}>Choose a different model</Button>
      </div>
    </div>
  );
}
