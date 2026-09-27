import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Combobox,
  ComboboxCollection,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxGroup,
  ComboboxInput,
  ComboboxItem,
  ComboboxLabel,
  ComboboxList,
} from "@/components/ui/combobox";
import { ask } from "@tauri-apps/plugin-dialog";
import { ExternalLink, Key, RefreshCw, Settings2, Trash2 } from "lucide-react";
import {
  formatModelCost,
  fromModelSelectValue,
  providerSupportsReasoning,
  type ModelPickerGroup,
  type ModelPickerItem,
} from "./agentCli";
import {
  providerRowShellClass,
  type ProviderRowProps,
  type ProviderRowViewModel,
} from "./providerRowShared";

export function CloudProviderRow({
  props,
  model,
}: {
  props: ProviderRowProps;
  model: ProviderRowViewModel;
}) {
  const {
    provider,
    isModelsLoading,
    fetchModels,
    getError,
    showGuidedSetup,
    setGuidedSetupProvider,
    onSelectModel,
    onSetupApiKey,
    onRemoveApiKey,
  } = props;
  const { hasKey, isSelected, modelPickerGroups, selectedModelItem, statusCopy } = model;

  return (
    <div className={providerRowShellClass(isSelected, false)}>
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
          {!provider.isCustom && hasKey && (
            <Combobox<ModelPickerItem>
              items={modelPickerGroups}
              value={selectedModelItem}
              onValueChange={(item) => {
                if (item) {
                  void onSelectModel(provider.id, fromModelSelectValue(item.value));
                }
              }}
              itemToStringLabel={(item) => item.label}
              itemToStringValue={(item) => item.value}
              isItemEqualToValue={(item, value) => item.value === value.value}
              filter={(item, query) => {
                const searchable = [item.label, item.qualifiedId, item.model.sourceProvider ?? ""]
                  .join(" ")
                  .toLowerCase();
                return searchable.includes(query.trim().toLowerCase());
              }}
              autoHighlight
            >
              <ComboboxInput
                className="w-full sm:w-64"
                placeholder="Search models"
                aria-label={`Model for ${provider.name}`}
              />
              <ComboboxContent className="min-w-80">
                <ComboboxEmpty>No models found.</ComboboxEmpty>
                <ComboboxList>
                  {(group: ModelPickerGroup) => (
                    <ComboboxGroup key={`${provider.id}-${group.value}`} items={group.items}>
                      <ComboboxLabel>{group.value}</ComboboxLabel>
                      <ComboboxCollection>
                        {(item: ModelPickerItem) => {
                          const cost = formatModelCost(item.model);
                          const detail = [
                            item.model.reasoning || providerSupportsReasoning(provider)
                              ? "reasoning"
                              : "",
                            cost,
                          ]
                            .filter(Boolean)
                            .join(" · ");
                          return (
                            <ComboboxItem key={item.value} value={item}>
                              <span className="flex min-w-0 flex-1 items-center justify-between gap-3">
                                <span className="min-w-0 truncate">
                                  {item.label}
                                  {detail ? ` · ${detail}` : ""}
                                </span>
                                <span className="max-w-40 shrink-0 truncate font-mono text-[11px] text-muted-foreground">
                                  {item.qualifiedId}
                                </span>
                              </span>
                            </ComboboxItem>
                          );
                        }}
                      </ComboboxCollection>
                    </ComboboxGroup>
                  )}
                </ComboboxList>
              </ComboboxContent>
            </Combobox>
          )}
          {hasKey ? (
            <>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                  provider.isCustom
                    ? void onSetupApiKey(provider.id)
                    : void fetchModels(provider.id)
                }
                disabled={!provider.isCustom && isModelsLoading(provider.id)}
              >
                {provider.isCustom ? (
                  <Settings2 className="h-3.5 w-3.5" />
                ) : (
                  <RefreshCw className="h-3.5 w-3.5" />
                )}
                {provider.isCustom ? "Configure" : "Refresh"}
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="text-muted-foreground hover:text-destructive"
                aria-label={`Remove ${provider.name} configuration`}
                onClick={async () => {
                  const confirmed = await ask(
                    provider.isCustom
                      ? `Remove configuration for ${provider.name}?`
                      : `Remove API key for ${provider.name}?`,
                    {
                      title: provider.isCustom ? "Remove Configuration" : "Remove API Key",
                      kind: "warning",
                    },
                  );
                  if (confirmed) void onRemoveApiKey(provider.id);
                }}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </>
          ) : (
            <>
              {!provider.isCustom && provider.apiKeyUrl && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => window.open(provider.apiKeyUrl, "_blank")}
                >
                  <ExternalLink className="h-3.5 w-3.5" />
                  Get key
                </Button>
              )}
              <Button
                type="button"
                variant="outline"
                size="sm"
                aria-label={
                  provider.isCustom ? `Configure ${provider.name}` : `Add ${provider.name} API key`
                }
                onClick={() => {
                  if (showGuidedSetup) setGuidedSetupProvider(provider.id);
                  void onSetupApiKey(provider.id);
                }}
              >
                {provider.isCustom ? (
                  <Settings2 className="h-3.5 w-3.5" />
                ) : (
                  <Key className="h-3.5 w-3.5" />
                )}
                {provider.isCustom ? "Configure" : "Add key"}
              </Button>
            </>
          )}
        </div>
      </div>

      {getError(provider.id) && (
        <p className="mt-2 text-xs text-destructive">{getError(provider.id)}</p>
      )}
    </div>
  );
}
