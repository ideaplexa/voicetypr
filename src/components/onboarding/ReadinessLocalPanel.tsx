import { EmptyState, LoadingState, ModelLegend } from "@/components/onboarding/OnboardingChrome";
import { ModelCard } from "@/components/ModelCard";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Card } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import { isWindows } from "@/lib/platform";
import type { ModelInfo, TranscriptionAcceleration } from "@/types";
import { Info } from "lucide-react";

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
      <ModelLegend />
      <Card className="rounded-2xl border border-border bg-card py-0 shadow-sm">
        <ScrollArea className="h-[320px]">
          <div className="flex flex-col gap-3 p-4">
            {localModelNames.map((name: string) => {
              const model = models[name];
              if (!model) return null;
              const progressValue = downloadProgress[name];
              return (
                <ModelCard
                  key={name}
                  name={name}
                  model={model}
                  downloadProgress={progressValue}
                  isVerifying={verifyingModels.has(name)}
                  downloadError={downloadErrors[name]}
                  isSelected={currentModel === name}
                  onDownload={onDownload}
                  onSelect={(modelName) => void onSelectLocal(modelName)}
                  onCancelDownload={onCancelDownload}
                  onDelete={onDelete}
                  onRepair={onRepair}
                  showSelectButton={isModelReady(name)}
                />
              );
            })}
            {isLoading && localModelNames.length === 0 ? (
              <LoadingState label="Loading local models" />
            ) : null}
            {!isLoading && localModelNames.length === 0 ? (
              <EmptyState
                title="No local models available"
                description="Choose Cloud or Remote Voicetypr to continue without a local model."
              />
            ) : null}
            {hasDownloadedLocalModel && !localReady ? (
              <Alert>
                <Info className="size-4" />
                <AlertTitle>Select a downloaded model</AlertTitle>
                <AlertDescription>
                  Downloaded models are ready to use, but onboarding needs one selected before
                  continuing.
                </AlertDescription>
              </Alert>
            ) : null}
          </div>
        </ScrollArea>
      </Card>
      {isWindows && (
        <div className="flex items-center justify-between gap-3 rounded-2xl border border-border bg-card px-4 py-3 shadow-sm">
          <div>
            <p className="text-sm font-medium">Use GPU acceleration</p>
            <p className="text-xs text-muted-foreground">
              Recommended — uses your graphics card for faster transcription.
            </p>
          </div>
          <Switch
            checked={(transcriptionAcceleration ?? "auto") !== "cpu"}
            onCheckedChange={(checked) => void onGpuToggle(checked)}
            aria-label="Use GPU acceleration"
          />
        </div>
      )}
    </div>
  );
}
