import { ModelCard } from "@/components/ModelCard";
import { SettingsCard } from "@/components/settings/settings-ui";
import { CheckCircle, HardDrive, Star, Zap } from "lucide-react";
import type { LocalModelActions, ModelEntry } from "./types";

const modelScoreLegend = (
  <div className="mb-3 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[11px] text-muted-foreground">
    <span className="font-medium uppercase tracking-wide">Badges</span>
    <span className="flex items-center gap-1.5">
      <Zap className="size-3.5 text-emerald-600" />
      Speed
    </span>
    <span className="flex items-center gap-1.5">
      <CheckCircle className="size-3.5 text-blue-600" />
      Accuracy
    </span>
    <span className="flex items-center gap-1.5">
      <HardDrive className="size-3.5" />
      Size
    </span>
    <span className="flex items-center gap-1.5">
      <Star className="size-3.5 fill-amber-500 text-amber-500" />
      Recommended
    </span>
  </div>
);

interface LocalModelCardsProps extends LocalModelActions {
  models: ModelEntry[];
}

function LocalModelCards({
  models,
  downloadProgress,
  downloadPhases,
  verifyingModels,
  downloadErrors,
  onDownload,
  onDelete,
  onCancelDownload,
  onRepair,
  onSelect,
  currentModel,
  activeRemoteServer,
  clearActiveRemote,
  speedModeRecommended,
}: LocalModelCardsProps) {
  return (
    <div className="grid gap-3">
      {models.map(([name, model]) => (
        <ModelCard
          key={name}
          name={name}
          model={model}
          downloadProgress={downloadProgress[name]}
          downloadPhase={downloadPhases[name]}
          isVerifying={verifyingModels.has(name)}
          downloadError={downloadErrors[name]}
          onDownload={onDownload}
          onDelete={onDelete}
          onCancelDownload={onCancelDownload}
          onRepair={onRepair}
          onSelect={async (modelName) => {
            await clearActiveRemote();
            void onSelect(modelName);
          }}
          showSelectButton={model.downloaded}
          isSelected={!activeRemoteServer && currentModel === name}
          speedModeRecommended={speedModeRecommended && name === "large-v3-turbo"}
        />
      ))}
    </div>
  );
}

interface LocalModelsListProps extends LocalModelActions {
  readyLocalModels: ModelEntry[];
}

export function LocalModelsList({ readyLocalModels, ...actions }: LocalModelsListProps) {
  if (readyLocalModels.length === 0) return null;

  return (
    <SettingsCard
      icon={HardDrive}
      title={`Local models (${readyLocalModels.length})`}
      description="Offline transcription models stored on this machine."
    >
      <div className="mt-4">{modelScoreLegend}</div>
      <LocalModelCards models={readyLocalModels} {...actions} />
    </SettingsCard>
  );
}

interface LocalSetupGridProps extends LocalModelActions {
  setupLocalModels: ModelEntry[];
}

export function LocalSetupGrid({ setupLocalModels, ...actions }: LocalSetupGridProps) {
  if (setupLocalModels.length === 0) return null;

  return <LocalModelCards models={setupLocalModels} {...actions} />;
}
