import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
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
import { ModelInfo, isCloudModel } from "@/types";
import { CheckCircle, Zap } from "lucide-react";

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
  const requiresSetup = model.requires_setup;
  const isActive = currentModel === name && !activeRemoteServer;
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
  const providerDisplayName = provider?.displayName || provider?.providerName;
  const showModelSelector = availableModels.length > 1;

  return (
    <Card
      key={name}
      className={cn(
        "group rounded-xl border border-border bg-card p-4 transition-colors",
        requiresSetup ? "" : "cursor-pointer",
        isActive ? "border-sage/50 bg-sage-bg/40" : "hover:border-sage/40 hover:bg-muted/30",
      )}
      onClick={async () => {
        if (requiresSetup) {
          openCloudModal(name, "connect");
          return;
        }
        await clearActiveRemote();
        void onSelect(name);
      }}
    >
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3
              className={cn(
                "truncate text-sm font-semibold tracking-tight",
                isActive && "text-sage",
              )}
            >
              {modelDisplayName}
            </h3>
            {providerDisplayName && (
              <Badge variant="outline" className="gap-1 text-muted-foreground">
                {providerDisplayName}
              </Badge>
            )}
            {isActive && (
              <Badge className="gap-1 bg-sage text-sage-foreground">
                <CheckCircle className="size-3" />
                Active
              </Badge>
            )}
          </div>
          {provider?.description && (
            <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
              {provider.description}
            </p>
          )}
          {showModelSelector ? (
            <div
              className="mt-2"
              onClick={(event) => event.stopPropagation()}
              onPointerDown={(event) => event.stopPropagation()}
            >
              <Select
                items={availableModels.map((option) => ({
                  value: option.id,
                  label: option.display_name,
                }))}
                value={selectedModelId}
                onValueChange={(modelId) => {
                  if (modelId != null) {
                    void onModelChange(name, modelId, requiresSetup);
                  }
                }}
              >
                <SelectTrigger
                  size="sm"
                  className="h-8 w-full sm:w-[220px]"
                  aria-label={`${providerDisplayName ?? name} transcription model`}
                  onClick={(event) => event.stopPropagation()}
                >
                  <SelectValue placeholder={modelDisplayName} />
                </SelectTrigger>
                <SelectContent>
                  {availableModels.map((option) => (
                    <SelectItem key={option.id} value={option.id}>
                      {option.display_name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          ) : null}
          <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-xs text-muted-foreground">
            <span className="inline-flex items-center gap-1.5">
              <Zap className="size-3.5 text-sage" />
              Speed <span className="font-medium text-foreground">{model.speed_score ?? "—"}</span>
            </span>
            <span className="inline-flex items-center gap-1.5">
              <CheckCircle className="size-3.5 text-sage" />
              Accuracy{" "}
              <span className="font-medium text-foreground">{model.accuracy_score ?? "—"}</span>
            </span>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {requiresSetup ? (
            <Button
              size="sm"
              variant="outline"
              onClick={(event) => {
                event.stopPropagation();
                openCloudModal(name, "connect");
              }}
            >
              {provider?.setupCta ?? "Add API Key"}
            </Button>
          ) : (
            <Button
              size="sm"
              variant="ghost"
              className="text-muted-foreground hover:text-destructive"
              onClick={(event) => {
                event.stopPropagation();
                onDisconnect(name);
              }}
            >
              Remove API Key
            </Button>
          )}
        </div>
      </div>
    </Card>
  );
}
