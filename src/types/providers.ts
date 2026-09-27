/**
 * AI provider DTOs.
 * Models are fetched dynamically from provider APIs.
 */

export type AiProviderStatus = "production" | "experimental" | "hidden";

export interface AIProviderModel {
  id: string;
  name: string;
  recommended: boolean;
  reasoning?: boolean;
  contextWindow?: number | null;
  sourceProvider?: string | null;
  cliDefault?: boolean;
  costInput?: number | null;
  costOutput?: number | null;
}

export type AgentCliProbeState = "ready" | "missing" | "unsafe_launcher";

export interface AgentCliProbe {
  state: AgentCliProbeState;
  /** Static reasoning levels supported by VoiceTypr's provider adapter. */
  reasoningLevels: string[];
  /** Whether VoiceTypr can invoke this CLI's native fast service mode. */
  supportsFastMode: boolean;
}

// Mirrors Rust `crate::ai::contract::AiProvider`.
// The existing Tauri command wire DTO keeps `{ id, name }` compatibility and maps
// Rust `label` to `name`; contract fields may arrive as snake_case or camelCase.
export interface AiProvider {
  id: string;
  label?: string;
  name?: string;
  status: AiProviderStatus;
  requires_api_key?: boolean;
  requiresApiKey?: boolean;
  supports_base_url?: boolean;
  supportsBaseUrl?: boolean;
  supports_reasoning?: boolean;
  supportsReasoning?: boolean;
}

export interface AIProviderConfig extends AiProvider {
  name: string;
  color: string;
  apiKeyUrl: string;
  /** Hint shown when the provider needs setup that isn't an API key (e.g.
   * "Install Claude Code" for CLI providers). Empty for key-based providers. */
  installHint: string;
  isCustom: boolean;
}

const PROVIDER_UI_METADATA: Record<
  string,
  { color: string; apiKeyUrl: string; installHint: string }
> = {
  openai: {
    color: "text-green-600",
    apiKeyUrl: "https://platform.openai.com/api-keys",
    installHint: "",
  },
  gemini: {
    color: "text-blue-600",
    apiKeyUrl: "https://aistudio.google.com/apikey",
    installHint: "",
  },
  anthropic: {
    color: "text-orange-600",
    apiKeyUrl: "https://console.anthropic.com/settings/keys",
    installHint: "",
  },
  openrouter: {
    color: "text-indigo-600",
    apiKeyUrl: "https://openrouter.ai/keys",
    installHint: "",
  },
  custom: {
    color: "text-purple-600",
    apiKeyUrl: "",
    installHint: "",
  },
  "claude-code": {
    // Claude Code is a local CLI authenticated by subscription, not an API
    // key. The card shows an install hint (no "Get a key" link); availability
    // comes from probe_agent_cli, not hasApiKey.
    color: "text-orange-600",
    apiKeyUrl: "",
    installHint: "Install the Claude Code CLI, then run `claude` to sign in.",
  },
  pi: {
    color: "text-pink-600",
    apiKeyUrl: "",
    installHint: "Install pi and sign in to a provider.",
  },
  omp: {
    color: "text-cyan-600",
    apiKeyUrl: "",
    installHint: "Install oh-my-pi (omp) and sign in.",
  },
};

export function toProviderConfig(provider: AiProvider): AIProviderConfig {
  const metadata = PROVIDER_UI_METADATA[provider.id] ?? {
    color: "text-foreground",
    apiKeyUrl: "",
    installHint: "",
  };
  const supportsBaseUrl = provider.supports_base_url ?? provider.supportsBaseUrl ?? false;

  return {
    ...provider,
    status: provider.status ?? "production",
    name: provider.name ?? provider.label ?? provider.id,
    color: metadata.color,
    apiKeyUrl: metadata.apiKeyUrl,
    installHint: metadata.installHint,
    isCustom: provider.id === "custom" || supportsBaseUrl,
  };
}
