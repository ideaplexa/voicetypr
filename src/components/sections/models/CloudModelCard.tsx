import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { getCloudProviderByModel, resolveCloudModelLabel } from "@/lib/cloudProviders";
import { getModelDisplayName } from "@/lib/model-display";
import { cn } from "@/lib/utils";
import { type ModelInfo, isCloudModel } from "@/types";

export interface CloudModelCardProps {
  name: string;
  model: ModelInfo;
  currentModel?: string;
  activeRemoteServer: string | null;
  onSelect: (modelName: string) => Promise<void> | void;
  clearActiveRemote: () => Promise<void>;
  openCloudModal: (providerId: string, mode: "connect" | "update") => void;
  onDisconnect: (modelName: string) => void;
  onModelChange: (providerId: string, modelId: string, requiresSetup: boolean) => void;
}

export function CloudModelCard({
  name,
  model,
  currentModel,
  activeRemoteServer,
  onSelect,
  clearActiveRemote,
  openCloudModal,
  onDisconnect,
  onModelChange,
}: CloudModelCardProps) {
  if (!isCloudModel(model)) return null;
  const provider = getCloudProviderByModel(name) ?? getCloudProviderByModel(model.engine);
  const ready = !!model.downloaded && !model.requires_setup;
  const selected = ready && currentModel === name && !activeRemoteServer;
  const availableModels = model.available_models ?? [];
  const selectedModelId =
    (model.underlying_model &&
      availableModels.some((option) => option.id === model.underlying_model) &&
      model.underlying_model) ||
    availableModels[0]?.id;
  const modelDisplayName =
    resolveCloudModelLabel(model) ||
    getModelDisplayName(name, { [name]: model }) ||
    provider?.displayName ||
    name;
  const providerName = provider?.displayName || provider?.providerName || name;
  return (
    <div className={cn("flex flex-wrap items-center gap-3 px-4 py-3", selected && "bg-sage-bg")}>
      <button
        type="button"
        role="radio"
        aria-checked={selected}
        aria-label={`Use ${providerName}`}
        disabled={!ready}
        onClick={() => {
          void clearActiveRemote().then(() => onSelect(name));
        }}
        className={cn(
          "size-4 shrink-0 rounded-full border border-border",
          selected && "border-[5px] border-sage",
        )}
      />
      <div className="min-w-36 flex-1">
        <p className="text-[13px] font-medium text-foreground">{providerName}</p>
        <p className="truncate text-xs text-muted-foreground">
          {modelDisplayName} · {ready ? "Key added" : "Needs a key"}
        </p>
      </div>
      {availableModels.length > 1 ? (
        <Select
          items={availableModels.map((option) => ({
            value: option.id,
            label: option.display_name,
          }))}
          value={selectedModelId}
          onValueChange={(modelId) => {
            if (modelId != null) void onModelChange(name, modelId, model.requires_setup);
          }}
        >
          <SelectTrigger
            size="sm"
            className="w-40"
            aria-label={`${providerName} transcription model`}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {availableModels.map((option) => (
              <SelectItem key={option.id} value={option.id}>
                {option.display_name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      ) : null}
      {selected ? <span className="text-xs font-medium text-sage">In use</span> : null}
      <Button
        size="sm"
        variant="ghost"
        onClick={() => openCloudModal(name, ready ? "update" : "connect")}
      >
        {ready ? "Replace key" : "Add key"}
      </Button>
      {ready ? (
        <Button
          size="sm"
          variant="ghost"
          className="text-muted-foreground"
          onClick={() => onDisconnect(name)}
        >
          Remove key
        </Button>
      ) : null}
    </div>
  );
}
