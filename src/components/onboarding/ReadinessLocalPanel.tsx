import { EmptyState, LoadingState } from "@/components/onboarding/OnboardingChrome";
import { failureReason } from "@/components/onboarding/OfflinePreparationStatus";
import { SettingsPaneCard } from "@/components/settings/settings-ui";
import { Button } from "@/components/settings/SettingsButton";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { Switch } from "@/components/settings/SettingsSwitch";
import { cn } from "@/lib/utils";
import { isLocalModel, type ModelInfo, type TranscriptionAcceleration } from "@/types";
import { CircleCheck } from "lucide-react";

export interface ReadinessLocalPanelProps {
  localModelNames: string[];
  recommendedModel: string | undefined;
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
  isModelReady: (name: string) => boolean;
  onGpuToggle: (checked: boolean) => void;
}

/** What a model is good for, in the app's own terms: its scores, size and languages. */
function strengths(model: ModelInfo, others: ModelInfo[]) {
  const best = (score: (m: ModelInfo) => number) => others.every((other) => score(model) >= score(other));
  const words = [
    best((m) => m.accuracy_score ?? 0) ? "Most accurate" : null,
    best((m) => m.speed_score ?? 0) ? "Fastest" : null,
    best((m) => -(m.size ?? 0)) ? "Smallest" : null,
    (model.supported_languages?.length ?? 0) > 1 ? "Many languages" : /\.en$/i.test(model.name) ? "English only" : null,
  ].filter(Boolean);
  return words.join(" · ");
}

/** The on-device models, recommended first, each with its download as the control. */
export function ReadinessLocalPanel({
  localModelNames,
  recommendedModel,
  models,
  downloadProgress,
  verifyingModels,
  downloadErrors,
  currentModel,
  isLoading,
  hasDownloadedLocalModel,
  localReady,
  transcriptionAcceleration,
  onSelectLocal,
  onCancelDownload,
  isModelReady,
  onGpuToggle,
}: ReadinessLocalPanelProps) {
  const names = [
    ...localModelNames.filter((name) => name === recommendedModel),
    ...localModelNames.filter((name) => name !== recommendedModel),
  ].filter((name) => models[name]);
  const list = names.map((name) => models[name]).filter(isLocalModel);
  return (
    <div className="flex flex-col gap-3">
      <SettingsPaneCard className="!px-[18px] !py-1">
        {names.map((name) => {
          const model = models[name];
          const ready = isModelReady(name);
          const inUse = ready && currentModel === name;
          const error = ready ? undefined : downloadErrors[name];
          const verifying = !error && verifyingModels.has(name);
          const progress = error ? undefined : downloadProgress[name];
          return (
            <div key={name} className="flex flex-col gap-2 border-b border-border py-3.5 last:border-0">
              <div className="flex items-center gap-3">
                <button
                  type="button"
                  aria-pressed={currentModel === name}
                  onClick={() => onSelectLocal(name)}
                  className="flex min-w-0 flex-1 flex-col items-start gap-0.5 rounded-md text-left focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                >
                  <span className="flex items-center gap-2 text-sm font-semibold">
                    {model.display_name}
                    {name === recommendedModel ? (
                      <Badge variant="secondary" className="bg-sage-bg text-sage">Recommended</Badge>
                    ) : null}
                  </span>
                  <span className="text-[13px] text-muted-foreground">
                    {[strengths(model, list), `${Math.round((model.size ?? 0) / (1024 * 1024))} MB`].filter(Boolean).join(" · ")}
                  </span>
                </button>
                {inUse ? (
                  <span className="flex items-center gap-1.5 text-[13px] font-medium text-sage">
                    <CircleCheck className="size-4" />In use
                  </span>
                ) : ready ? (
                  <span className="flex items-center gap-3 text-[13px] text-muted-foreground">
                    Downloaded
                    <Button variant="outline" size="sm" onClick={() => onSelectLocal(name)}>Use</Button>
                  </span>
                ) : verifying ? (
                  <span className="text-[13px] text-muted-foreground">Checking…</span>
                ) : progress !== undefined ? (
                  <span className="flex items-center gap-3 text-[13px] tabular-nums text-muted-foreground">
                    <span>{Math.round(progress)}%</span>
                    <Button variant="ghost" size="sm" onClick={() => onCancelDownload(name)}>Cancel</Button>
                  </span>
                ) : (
                  <Button size="sm" variant={name === recommendedModel ? "default" : "outline"} onClick={() => onSelectLocal(name)}>
                    {error ? "Retry" : "Download"}
                  </Button>
                )}
              </div>
              {progress !== undefined || verifying ? (
                <Progress
                  value={verifying ? 100 : progress ?? null}
                  aria-label={`Downloading ${model.display_name}`}
                  className={cn(verifying && "animate-pulse")}
                />
              ) : null}
              {error ? <p role="alert" className="text-[13px] text-destructive">The download stopped. {failureReason(error)}</p> : null}
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
        {hasDownloadedLocalModel && !localReady && !names.some((name) => downloadProgress[name] !== undefined) && (
          <p className="pb-3 text-[13px] text-muted-foreground">Pick a downloaded model to use it.</p>
        )}
      </SettingsPaneCard>
      {models[currentModel ?? ""]?.gpu_controls_available === true && (
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
