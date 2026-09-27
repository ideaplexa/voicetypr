import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Brain, RefreshCw } from "lucide-react";
import { formatReasoningLevel } from "./agentCli";
import { AgentModelPickerDialog } from "./AgentModelPickerDialog";
import {
  providerRowShellClass,
  type ProviderRowProps,
  type ProviderRowViewModel,
} from "./providerRowShared";

export function AgentProviderRow({
  props,
  model,
}: {
  props: ProviderRowProps;
  model: ProviderRowViewModel;
}) {
  const {
    provider,
    agentCliProbing,
    isModelsLoading,
    getModels,
    fetchModels,
    getError,
    onSelectModel,
    onSelectReasoning,
    onToggleFastMode,
    onRefreshAgentCli,
  } = props;
  const {
    hasKey,
    isSelected,
    selectedModel,
    modelPickerGroups,
    selectedModelItem,
    statusCopy,
    reasoning,
    reasoningLevels,
    fastModeEnabled,
    agentCliReady,
    agentProbe,
  } = model;

  return (
    <div
      role={hasKey ? "button" : undefined}
      tabIndex={hasKey ? 0 : undefined}
      aria-pressed={hasKey ? isSelected : undefined}
      aria-label={hasKey ? `Select ${provider.name}` : undefined}
      className={providerRowShellClass(isSelected, hasKey)}
      onClick={(event) => {
        if (!hasKey) return;
        if (
          event.target instanceof Element &&
          event.target.closest(
            "button, input, [role='combobox'], [role='listbox'], [role='option'], [role='switch'], [role='dialog']",
          )
        ) {
          return;
        }
        void onSelectModel(provider.id, selectedModel);
      }}
      onKeyDown={(event) => {
        if (!hasKey) return;
        if (event.key !== "Enter" && event.key !== " ") return;
        if (event.target instanceof Element && event.target !== event.currentTarget) {
          return;
        }
        event.preventDefault();
        void onSelectModel(provider.id, selectedModel);
      }}
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className={`pointer-events-none text-sm font-semibold ${provider.color}`}>
              {provider.name}
            </h3>
            {isSelected && (
              <Badge variant="secondary" className="text-sage">
                Active
              </Badge>
            )}
            {provider.status === "experimental" && <Badge variant="outline">Experimental</Badge>}
            {statusCopy && <span className="text-xs text-muted-foreground">{statusCopy}</span>}
          </div>
        </div>

        <div className="flex min-w-0 flex-col gap-2 sm:flex-row sm:flex-nowrap sm:items-center sm:justify-end">
          <AgentModelPickerDialog
            provider={provider}
            groups={modelPickerGroups}
            selectedItem={selectedModelItem}
            disabled={!agentCliReady}
            loading={isModelsLoading(provider.id)}
            onOpen={() => {
              if (getModels(provider.id).length === 0) {
                void fetchModels(provider.id);
              }
            }}
            onSelect={(modelId) => void onSelectModel(provider.id, modelId)}
          />
          {reasoningLevels.length > 0 && (
            <Select
              value={reasoning}
              disabled={!agentCliReady}
              onValueChange={(value) => value != null && void onSelectReasoning(provider.id, value)}
            >
              <SelectTrigger
                className="h-9 w-full justify-between gap-2 px-2.5 sm:w-24"
                aria-label={`${provider.id === "claude-code" ? "Effort" : "Thinking"} for ${provider.name}`}
              >
                <SelectValue>
                  <span className="inline-flex items-center gap-1.5">
                    <Brain className="h-3.5 w-3.5 shrink-0" />
                    {formatReasoningLevel(reasoning)}
                  </span>
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                {reasoningLevels.map((level) => (
                  <SelectItem key={level} value={level}>
                    <span className="inline-flex items-center gap-1.5">
                      <Brain className="h-3.5 w-3.5 shrink-0" />
                      {formatReasoningLevel(level)}
                    </span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
          {agentProbe?.supportsFastMode && (
            <label
              className="flex h-9 items-center gap-2 rounded-md border border-input bg-background px-3 text-xs font-medium"
              title="Use this CLI's native fast service mode. Provider pricing may be higher."
            >
              Fast
              <Switch
                aria-label={`Fast mode for ${provider.name}`}
                checked={fastModeEnabled}
                disabled={!agentCliReady}
                onCheckedChange={(enabled) => void onToggleFastMode(provider.id, enabled)}
              />
            </label>
          )}
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            aria-label={`Refresh ${provider.name} status`}
            title={`Refresh ${provider.name} status`}
            onClick={() => void onRefreshAgentCli(provider)}
            disabled={agentCliProbing[provider.id]}
          >
            <RefreshCw
              className={`h-3.5 w-3.5 ${agentCliProbing[provider.id] ? "animate-spin" : ""}`}
            />
          </Button>
        </div>
      </div>

      {getError(provider.id) && (
        <p className="mt-2 text-xs text-destructive">{getError(provider.id)}</p>
      )}
    </div>
  );
}
