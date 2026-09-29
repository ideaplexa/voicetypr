import { EmptyState, LoadingState } from "@/components/onboarding/OnboardingChrome";
import { SettingsPaneCard } from "@/components/settings/settings-ui";
import { Button } from "@/components/settings/SettingsButton";
import { Progress } from "@/components/ui/progress";
import { Switch } from "@/components/settings/SettingsSwitch";
import { isWindows } from "@/lib/platform";
import type { ModelInfo, TranscriptionAcceleration } from "@/types";

export interface ReadinessLocalPanelProps {
  localModelNames: string[];
  models: Record<string, ModelInfo>;
  downloadProgress: Record<string, number>;
  verifyingModels: Set<string>;
  downloadErrors: Record<string, string>;
  currentModel: string | undefined;
  isLoading: boolean;
  hasDownloadedLocalModel: boolean;
  localReady: boolean;
  transcriptionAcceleration: TranscriptionAcceleration | undefined;
  onDownload: (modelName: string) => void;
  onSelectLocal: (modelName: string) => void;
  onCancelDownload: (modelName: string) => void;
  onDelete: (modelName: string) => void | Promise<void>;
  onRepair: (modelName: string) => void;
  isModelReady: (name: string) => boolean;
  onGpuToggle: (checked: boolean) => void;
}

export function ReadinessLocalPanel({
  localModelNames,
  models,
  downloadProgress,
  verifyingModels,
  downloadErrors,
  currentModel,
  isLoading,
  hasDownloadedLocalModel,
  localReady,
  transcriptionAcceleration,
  onDownload,
  onSelectLocal,
  onCancelDownload,
  onDelete,
  onRepair,
  isModelReady,
  onGpuToggle,
}: ReadinessLocalPanelProps) {
  return (
    <div className="flex flex-col gap-4">
      <SettingsPaneCard className="!p-[18px]">
        {localModelNames.map((name) => {
          const model = models[name];
          if (!model) return null;
          const progress = downloadProgress[name];
          const ready = isModelReady(name);
          return (
            <div
              key={name}
              className="flex flex-col gap-2 border-b border-border py-3 first:pt-0 last:border-0 last:pb-0"
            >
              <div className="flex items-center gap-3">
                <button
                  type="button"
                  disabled={!ready}
                  onClick={() => onSelectLocal(name)}
                  className="flex flex-1 flex-col items-start text-left disabled:opacity-100"
                >
                  <span className="text-sm font-semibold">{model.display_name}</span>
                  <span className="text-xs text-muted-foreground">
                    {currentModel === name && ready
                      ? "Active"
                      : ready
                        ? "Downloaded"
                        : `${Math.round((model.size ?? 0) / (1024 * 1024))} MB`}
                    {model.recommended ? " · Recommended" : ""}
                  </span>
                </button>
                {verifyingModels.has(name) ? (
                  <span className="text-xs text-muted-foreground">Verifying…</span>
                ) : progress !== undefined ? (
                  <Button
                    variant="outline"
                    className="text-[13px] text-muted-foreground hover:text-muted-foreground"
                    onClick={() => onCancelDownload(name)}
                  >
                    Cancel download
                  </Button>
                ) : ready ? (
                  <>
                    <Button
                      variant="outline"
                      className="text-[13px] text-muted-foreground hover:text-muted-foreground"
                      onClick={() => onRepair(name)}
                    >
                      Repair
                    </Button>
                    <Button
                      variant="outline"
                      className="text-[13px] text-muted-foreground hover:text-muted-foreground"
                      onClick={() => void onDelete(name)}
                    >
                      Delete
                    </Button>
                  </>
                ) : (
                  <Button className="text-sm" onClick={() => onDownload(name)}>
                    Download
                  </Button>
                )}
              </div>
              {progress !== undefined && (
                <>
                  <Progress value={progress} aria-label={`Downloading ${model.display_name}`} />
                  <p className="text-xs text-muted-foreground">
                    Downloading {Math.round(progress)}%
                  </p>
                </>
              )}
              {downloadErrors[name] && (
                <p role="alert" className="text-sm text-destructive">
                  {downloadErrors[name]}
                </p>
              )}
            </div>
          );
        })}
        {isLoading && localModelNames.length === 0 && <LoadingState label="Loading local models" />}
        {!isLoading && localModelNames.length === 0 && (
          <EmptyState
            title="No local models available"
            description="Choose Cloud or Another computer to continue without a local model."
          />
        )}
        {hasDownloadedLocalModel && !localReady && (
          <div className="mt-3 text-sm">
            <p className="font-medium">Select a downloaded model</p>
            <p className="text-muted-foreground">
              Downloaded models are ready to use, but onboarding needs one selected before
              continuing.
            </p>
          </div>
        )}
      </SettingsPaneCard>
      {isWindows && (
        <SettingsPaneCard className="!p-[18px]">
          <div className="flex items-center justify-between gap-3">
            <div>
              <p className="text-sm font-medium">Use GPU acceleration</p>
              <p className="text-xs text-muted-foreground">
                Recommended — uses your graphics card for faster transcription.
              </p>
            </div>
            <Switch
              checked={(transcriptionAcceleration ?? "auto") !== "cpu"}
              onCheckedChange={onGpuToggle}
              aria-label="Use GPU acceleration"
            />
          </div>
        </SettingsPaneCard>
      )}
    </div>
  );
}
