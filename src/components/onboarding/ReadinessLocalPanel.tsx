import { failureReason } from "@/components/onboarding/OfflinePreparationStatus";
import { supportsSpokenLanguage } from "@/components/onboarding/useOfflinePreparation";
import { Radio, Tag } from "@/components/onboarding/Frame";
import { Switch } from "@/components/settings/SettingsSwitch";
import { cn } from "@/lib/utils";
import { isMacOS } from "@/lib/platform";
import { isLocalModel, type ModelInfo, type TranscriptionAcceleration } from "@/types";
import { Download } from "lucide-react";

export interface ReadinessLocalPanelProps {
  localModelNames: string[];
  recommendedModel: string | undefined;
  /** The model being prepared for this computer, while it downloads. */
  preparing: string | null | undefined;
  /** The spoken language: models that don't support it say so. */
  language: string;
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

/** The frames' words for each shipped model. */
const ABOUT: Record<string, string> = {
  "Parakeet V3": "Many languages",
  "Parakeet Unified (English)": "English, live words",
  "Nemotron 3.5": "Many languages, live words",
  "Parakeet V2 (English)": "English",
  "Large v3 Turbo": "Many languages",
  "Large v3": "Many languages, largest",
  "Small (English)": "English",
  "Base (English)": "English, smallest",
};

const size = (bytes: number | undefined) => {
  const mb = (bytes ?? 0) / (1024 * 1024);
  return mb >= 1000 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
};

/** 04 Choose a model: every on-device model, recommended first, its download as the control. */
export function ReadinessLocalPanel({
  localModelNames,
  recommendedModel,
  preparing,
  language,
  models,
  downloadProgress,
  verifyingModels,
  downloadErrors,
  currentModel,
  isLoading,
  localReady,
  transcriptionAcceleration,
  onSelectLocal,
  isModelReady,
  onGpuToggle,
}: ReadinessLocalPanelProps) {
  const names = [
    ...localModelNames.filter((name) => name === recommendedModel),
    ...localModelNames.filter((name) => name !== recommendedModel),
  ].filter((name) => models[name] && isLocalModel(models[name]) && !(isMacOS && models[name].display_name === "Small (English)"));
  return (
    <div className="flex flex-col gap-3 self-stretch">
      <div role="radiogroup" aria-label="Model" className="flex flex-col overflow-hidden rounded-[14px] border border-border bg-card">
        {names.map((name) => {
          const model = models[name];
          const ready = isModelReady(name);
          const error = ready ? undefined : downloadErrors[name];
          const verifying = !error && verifyingModels.has(name);
          const progress = error ? undefined : downloadProgress[name];
          const on = localReady ? currentModel === name : name === preparing;
          const about = ABOUT[model.display_name] ??
            ((model.supported_languages?.length ?? 0) > 1 ? "Many languages" : "English");
          return (
            <div key={name} className={cn("flex flex-col border-b border-border px-[18px] last:border-b-0", on && "bg-sage-bg")}>
              <div className="flex h-[52px] items-center gap-3.5">
                <button
                  type="button"
                  role="radio"
                  aria-checked={on}
                  aria-label={model.display_name}
                  onClick={() => onSelectLocal(name)}
                  className="flex min-w-0 flex-1 items-center gap-3.5 text-left"
                >
                  <Radio on={on} />
                  <span className="flex min-w-0 flex-1 items-center gap-2.5">
                    <span className="truncate text-[15px] font-semibold text-foreground">{model.display_name}</span>
                    {name === recommendedModel ? <Tag>Recommended</Tag> : null}
                    <span className="truncate text-sm text-muted-foreground">
                      {supportsSpokenLanguage(model, language) ? about : `${about} · not your language`}
                    </span>
                  </span>
                </button>
                <span className="text-sm text-muted-foreground tabular-nums">{size(model.size)}</span>
                <span className="flex w-[116px] items-center justify-end gap-2">
                  {ready ? (
                    <span className="text-[13.5px] text-muted-foreground">{currentModel === name ? "In use" : "Downloaded"}</span>
                  ) : progress !== undefined || verifying ? (
                    <>
                      <span
                        role="progressbar"
                        aria-label={`Downloading ${model.display_name}`}
                        aria-valuenow={verifying ? 100 : Math.round(progress ?? 0)}
                        aria-valuemin={0}
                        aria-valuemax={100}
                        className={cn("relative h-1.5 w-16 overflow-hidden rounded-[3px] bg-border", verifying && "animate-pulse")}
                      >
                        <span className="absolute inset-y-0 left-0 rounded-[3px] bg-sage" style={{ width: `${verifying ? 100 : Math.round(progress ?? 0)}%` }} />
                      </span>
                      <span className="text-[13.5px] text-muted-foreground tabular-nums">{verifying ? "100%" : `${Math.round(progress ?? 0)}%`}</span>
                    </>
                  ) : (
                    <button
                      type="button"
                      aria-label={`${error ? "Retry" : "Download"} ${model.display_name}`}
                      onClick={() => onSelectLocal(name)}
                      className="flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-[13.5px] font-medium text-foreground hover:bg-accent"
                    >
                      <Download aria-hidden="true" className="size-3.5" />
                      {error ? "Retry" : "Download"}
                    </button>
                  )}
                </span>
              </div>
              {error ? <p role="alert" className="pb-3 text-[13px] text-destructive">The download stopped. {failureReason(error)}</p> : null}
            </div>
          );
        })}
        {isLoading && names.length === 0 ? <p className="px-[18px] py-4 text-sm text-muted-foreground">Loading local models…</p> : null}
        {!isLoading && names.length === 0 ? (
          <p className="px-[18px] py-4 text-sm text-muted-foreground">No local models available. Go back to choose Cloud or Another computer.</p>
        ) : null}
      </div>
      {models[currentModel ?? ""]?.gpu_controls_available === true && (
        <label className="flex items-center justify-between gap-3 rounded-[14px] border border-border bg-card px-[18px] py-3">
          <span className="text-sm font-medium">Use GPU acceleration</span>
          <Switch
            checked={(transcriptionAcceleration ?? "auto") !== "cpu"}
            onCheckedChange={onGpuToggle}
            aria-label="Use GPU acceleration"
          />
        </label>
      )}
    </div>
  );
}
