import type {
  AIProviderConfig,
  AIProviderModel,
  AgentCliProbe,
  AgentCliProbeState,
} from "@/types/providers";
import type { AISettings } from "@/types/ai";
import { humanizeModelId } from "@/lib/model-display";
export type AISettingsResponse = Omit<
  AISettings,
  "modelsByProvider" | "reasoningByProvider" | "fastModeByProvider"
> & {
  modelsByProvider?: Record<string, string>;
  reasoningByProvider?: Record<string, string>;
  fastModeByProvider?: Record<string, boolean>;
  aiModelNeedsReselection?: boolean;
};

export const normalizeAISettings = (settings: AISettingsResponse): AISettings => ({
  ...settings,
  modelsByProvider: settings.modelsByProvider ?? {},
  reasoningByProvider: settings.reasoningByProvider ?? {},
  fastModeByProvider: settings.fastModeByProvider ?? {},
});

export const providerSupportsReasoning = (provider: AIProviderConfig) =>
  provider.supports_reasoning ?? provider.supportsReasoning ?? false;

export const formatModelCost = (model: AIProviderModel) => {
  if (model.costInput == null && model.costOutput == null) {
    return null;
  }
  const input = model.costInput == null ? "?" : `$${model.costInput}`;
  const output = model.costOutput == null ? "?" : `$${model.costOutput}`;
  return `${input}/${output}`;
};

export const modelMatchesQuery = (model: AIProviderModel, query: string) =>
  model.id.toLowerCase().includes(query) ||
  model.name.toLowerCase().includes(query) ||
  Boolean(model.sourceProvider?.toLowerCase().includes(query));

/// Subscription-authenticated local coding CLIs.
const AGENT_CLI_PROVIDER_IDS = [
  "claude-code",
  "pi",
  "omp",
  "codex",
  "droid",
  "grok",
  "opencode",
  "cline",
] as const;

const CLAUDE_CODE_MODELS: AIProviderModel[] = [
  {
    id: "",
    name: "Default",
    recommended: true,
    sourceProvider: null,
    cliDefault: true,
  },
  {
    id: "haiku",
    name: "Haiku",
    recommended: false,
    sourceProvider: null,
    cliDefault: false,
  },
  {
    id: "sonnet",
    name: "Sonnet",
    recommended: false,
    sourceProvider: null,
    cliDefault: false,
  },
  {
    id: "opus",
    name: "Opus",
    recommended: false,
    sourceProvider: null,
    cliDefault: false,
  },
];

export const AGENT_CLI_DEFAULT_LABEL = "Default";

const fallbackAgentCliDefault = (_providerId: string): AIProviderModel => ({
  id: "",
  name: AGENT_CLI_DEFAULT_LABEL,
  recommended: true,
  cliDefault: true,
});

export const isAgentCliProvider = (providerId: string): boolean =>
  (AGENT_CLI_PROVIDER_IDS as readonly string[]).includes(providerId);

export const CLI_DEFAULT_MODEL_VALUE = "__voicetypr_cli_default__";

export const toModelSelectValue = (modelId: string) =>
  modelId === "" ? CLI_DEFAULT_MODEL_VALUE : modelId;

export const fromModelSelectValue = (modelId: string) =>
  modelId === CLI_DEFAULT_MODEL_VALUE ? "" : modelId;

export interface ModelPickerItem {
  value: string;
  label: string;
  qualifiedId: string;
  model: AIProviderModel;
}

export interface ModelPickerGroup {
  value: string;
  items: ModelPickerItem[];
}

export const qualifyModelId = (providerId: string, modelId: string) =>
  modelId
    ? modelId.includes("/")
      ? modelId
      : `${providerId}/${modelId}`
    : `${providerId}/default`;

export const shortModelName = (modelId: string, fallbackName?: string) => {
  if (fallbackName && !fallbackName.includes("/")) {
    return fallbackName;
  }
  const shortId = modelId.includes("/") ? modelId.slice(modelId.lastIndexOf("/") + 1) : modelId;
  return humanizeModelId(shortId || modelId);
};

export const defaultAgentCliReasoning = (providerId: string) =>
  providerId === "pi" || providerId === "omp" ? "off" : "low";

export const formatAgentCliReasoning = (providerId: string, reasoning: string) =>
  providerId === "claude-code" ? `Effort ${reasoning}` : `Thinking ${reasoning}`;

export const formatReasoningLevel = (reasoning: string) =>
  reasoning.charAt(0).toUpperCase() + reasoning.slice(1);

export const getAgentCliProbeState = (probe?: AgentCliProbe): AgentCliProbeState =>
  probe?.state ?? "missing";

export const isAgentCliReady = (probe?: AgentCliProbe): boolean =>
  getAgentCliProbeState(probe) === "ready";

export const isDefaultAgentCliModel = (model: AIProviderModel) =>
  Boolean(model.cliDefault) || model.id === "";

export { CLAUDE_CODE_MODELS, fallbackAgentCliDefault };
