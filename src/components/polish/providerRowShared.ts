import { humanizeModelId } from "@/lib/model-display";
import type { AISettings } from "@/types/ai";
import type { AIProviderConfig, AIProviderModel, AgentCliProbe } from "@/types/providers";
import {
  AGENT_CLI_DEFAULT_LABEL,
  CLI_DEFAULT_MODEL_VALUE,
  fallbackAgentCliDefault,
  defaultAgentCliReasoning,
  getAgentCliProbeState,
  shortModelName,
  isAgentCliProvider,
  isDefaultAgentCliModel,
  modelMatchesQuery,
  toModelSelectValue,
  qualifyModelId,
  type ModelPickerGroup,
  type ModelPickerItem,
} from "./agentCli";

export const EMPTY_RECOMMENDED_ITEMS: ModelPickerItem[] = [];
export const EMPTY_OTHER_ITEMS: ModelPickerItem[] = [];

const RECOMMENDED_GROUP_LABEL = "Recommended";
const ALL_MODELS_GROUP_LABEL = "All models";

export interface ProviderRowProps {
  provider: AIProviderConfig;
  aiSettings: AISettings;
  providerApiKeys: Record<string, boolean>;
  agentCliStatus: Record<string, AgentCliProbe>;
  agentCliProbing: Record<string, boolean>;
  customModelName: string;
  guidedSetupProvider: string | null;
  setGuidedSetupProvider: (provider: string | null) => void;
  isModelsLoading: (providerId: string) => boolean;
  getModels: (providerId: string) => AIProviderModel[];
  fetchModels: (providerId: string) => void;
  getError: (providerId: string) => string | null;
  getDisplayModels: (providerId: string) => AIProviderModel[];
  providerQuery: string;
  showGuidedSetup: boolean;
  onSelectModel: (providerId: string, modelId: string) => Promise<void>;
  onSelectReasoning: (providerId: string, reasoning: string) => Promise<void>;
  onToggleFastMode: (providerId: string, enabled: boolean) => Promise<void>;
  onRefreshAgentCli: (provider: AIProviderConfig) => Promise<void>;
  onSetupApiKey: (providerId: string) => Promise<void>;
  onRemoveApiKey: (providerId: string) => Promise<void>;
}

export interface ProviderRowViewModel {
  agentCli: boolean;
  agentProbe: AgentCliProbe | undefined;
  agentCliReady: boolean;
  agentCliChecking: boolean;
  agentCliRefreshing: boolean;
  hasKey: boolean;
  isSelected: boolean;
  selectedModel: string;
  modelSelectValue: string | undefined;
  modelPickerGroups: ModelPickerGroup[];
  selectedModelItem: ModelPickerItem | null;
  statusCopy: string | null;
  reasoning: string;
  reasoningLevels: string[];
  fastModeEnabled: boolean;
}

export function toPickerItem(providerId: string, model: AIProviderModel): ModelPickerItem {
  return {
    value: toModelSelectValue(model.id),
    label: isDefaultAgentCliModel(model)
      ? AGENT_CLI_DEFAULT_LABEL
      : shortModelName(model.id, model.name),
    qualifiedId: qualifyModelId(providerId, model.id),
    model,
  };
}

export function buildModelPickerGroups(
  displayModels: AIProviderModel[],
  providerId: string,
): ModelPickerGroup[] {
  const recommendedItems = EMPTY_RECOMMENDED_ITEMS.slice();
  const otherItems = EMPTY_OTHER_ITEMS.slice();
  for (const model of displayModels) {
    (model.recommended ? recommendedItems : otherItems).push(toPickerItem(providerId, model));
  }
  return [
    { value: RECOMMENDED_GROUP_LABEL, items: recommendedItems },
    { value: ALL_MODELS_GROUP_LABEL, items: otherItems },
  ].filter((group) => group.items.length > 0);
}

export function buildProviderRowViewModel(props: ProviderRowProps): ProviderRowViewModel {
  const {
    provider,
    aiSettings,
    providerApiKeys,
    agentCliStatus,
    agentCliProbing,
    customModelName,
    getDisplayModels,
    providerQuery,
  } = props;

  const agentCli = isAgentCliProvider(provider.id);
  const agentProbe = agentCli ? agentCliStatus[provider.id] : undefined;
  const agentCliState = agentProbe ? getAgentCliProbeState(agentProbe) : null;
  const agentCliChecking = agentCli && !agentProbe && agentCliProbing[provider.id];
  const agentCliRefreshing = agentCli && Boolean(agentProbe) && agentCliProbing[provider.id];
  const agentCliReady = agentCliState === "ready";
  const hasKey = agentCli ? agentCliReady : Boolean(providerApiKeys[provider.id]);
  const isSelected = aiSettings.provider === provider.id;
  const hasSelectedProviderModel =
    Object.prototype.hasOwnProperty.call(aiSettings.modelsByProvider, provider.id) || isSelected;
  const selectedModel = provider.isCustom
    ? aiSettings.modelsByProvider.custom || customModelName
    : Object.prototype.hasOwnProperty.call(aiSettings.modelsByProvider, provider.id)
      ? aiSettings.modelsByProvider[provider.id]
      : aiSettings.provider === provider.id
        ? aiSettings.model
        : "";
  const discoveredModels = getDisplayModels(provider.id);
  const defaultAgentCliModel = agentCli
    ? (discoveredModels.find(isDefaultAgentCliModel) ?? fallbackAgentCliDefault(provider.id))
    : undefined;
  const models =
    defaultAgentCliModel && !discoveredModels.some(isDefaultAgentCliModel)
      ? [defaultAgentCliModel, ...discoveredModels]
      : discoveredModels;
  const modelSelectValue = hasSelectedProviderModel
    ? toModelSelectValue(selectedModel)
    : defaultAgentCliModel
      ? CLI_DEFAULT_MODEL_VALUE
      : undefined;
  const providerMatches = provider.name.toLowerCase().includes(providerQuery);
  const displayModels =
    providerQuery && !providerMatches
      ? models.filter((model) => modelMatchesQuery(model, providerQuery))
      : models;
  const modelPickerGroups = buildModelPickerGroups(displayModels, provider.id);
  const modelPickerItems = modelPickerGroups.flatMap((group) => group.items);
  const selectedModelItem =
    modelPickerItems.find((item) => item.value === modelSelectValue) ??
    (selectedModel
      ? toPickerItem(provider.id, {
          id: selectedModel,
          name: humanizeModelId(selectedModel),
          recommended: false,
        })
      : null);
  const statusCopy = agentCli
    ? agentCliChecking
      ? "Checking installation…"
      : agentCliRefreshing
        ? agentCliReady
          ? "Installed · Refreshing…"
          : "Refreshing installation…"
        : agentCliReady
          ? "Installed"
          : agentCliState === "missing"
            ? "Not installed in PATH"
            : "Installation status unavailable"
    : hasKey
      ? isSelected
        ? null
        : "Ready"
      : provider.isCustom
        ? "Endpoint not configured"
        : "API key required";

  return {
    agentCli,
    agentProbe,
    agentCliReady,
    agentCliChecking,
    agentCliRefreshing,
    hasKey,
    isSelected,
    selectedModel,
    modelSelectValue,
    modelPickerGroups,
    selectedModelItem,
    statusCopy,
    reasoning: aiSettings.reasoningByProvider[provider.id] ?? defaultAgentCliReasoning(provider.id),
    reasoningLevels: agentProbe?.reasoningLevels ?? [],
    fastModeEnabled: aiSettings.fastModeByProvider[provider.id] ?? false,
  };
}

export function providerRowShellClass(isSelected: boolean, clickable: boolean) {
  return `rounded-lg border p-3 transition-colors select-none ${
    clickable
      ? "cursor-pointer hover:border-sage/40 hover:bg-muted/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      : ""
  } ${isSelected ? "border-sage/50 bg-sage-bg/40" : "border-border/60 bg-background"}`;
}
