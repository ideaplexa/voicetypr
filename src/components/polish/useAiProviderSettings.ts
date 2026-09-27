import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { useCallback, useEffect, useRef, useState } from "react";
import type { AISettings } from "@/types/ai";
import type {
  AiProvider,
  AIProviderConfig,
  AIProviderModel,
  AgentCliProbe,
} from "@/types/providers";
import { toProviderConfig } from "@/types/providers";
import { useAllProviderModels } from "@/hooks/useProviderModels";
import { getApiKey, hasApiKey, removeApiKey, saveApiKey } from "@/utils/keyring";
import { getErrorMessage } from "@/utils/error";
import { createLogger } from "@/lib/logger";
import {
  CLAUDE_CODE_MODELS,
  fallbackAgentCliDefault,
  getAgentCliProbeState,
  isAgentCliProvider,
  isAgentCliReady,
  normalizeAISettings,
  type AISettingsResponse,
} from "./agentCli";

const log = createLogger("enhancements");

export type ProviderTab = "cloud" | "local";

export interface UseAiProviderSettingsOptions {
  readinessAiReady: boolean | undefined;
  settingsLoaded: boolean;
  /** Flip the formatting preset after Polish turns on (preset → CleanDictation). */
  onPolishEnabled: () => Promise<void>;
  /** Keep the provider picker expanded after a model is selected. */
  onModelSelected: () => void;
  /** Reload formatting options after a model switch while Polish is enabled. */
  onEnabledModelSelected: () => Promise<void>;
  /** Handle preset + final-language side effects after the enabled toggle settles. */
  onPolishToggled: (enabled: boolean) => Promise<void>;
  /** Reset presets + language after the active provider's configuration is removed. */
  onActiveProviderCleared: () => Promise<void>;
}

export function useAiProviderSettings({
  readinessAiReady,
  settingsLoaded,
  onPolishEnabled,
  onModelSelected,
  onEnabledModelSelected,
  onPolishToggled,
  onActiveProviderCleared,
}: UseAiProviderSettingsOptions) {
  const {
    fetchModels,
    getModels,
    isLoading: isModelsLoading,
    getError,
    clearModels,
  } = useAllProviderModels();

  const [aiSettings, setAISettings] = useState<AISettings>({
    enabled: false,
    provider: "",
    model: "",
    hasApiKey: false,
    modelsByProvider: {},
    reasoningByProvider: {},
    fastModeByProvider: {},
  });
  const [aiModelNeedsReselection, setAiModelNeedsReselection] = useState(false);
  const [providers, setProviders] = useState<AIProviderConfig[]>([]);
  const [providerApiKeys, setProviderApiKeys] = useState<Record<string, boolean>>({});
  // Per-provider executable-resolution result for local agent CLIs.
  const [agentCliStatus, setAgentCliStatus] = useState<Record<string, AgentCliProbe>>({});
  const [agentCliProbing, setAgentCliProbing] = useState<Record<string, boolean>>({});
  const agentCliProbingRef = useRef<Map<string, object>>(new Map());

  const [showApiKeyModal, setShowApiKeyModal] = useState(false);
  const [showOpenAIConfig, setShowOpenAIConfig] = useState(false);
  const [openAIDefaultBaseUrl, setOpenAIDefaultBaseUrl] = useState("https://api.openai.com/v1");
  const [customModelName, setCustomModelName] = useState<string>("");
  const [selectedProvider, setSelectedProvider] = useState<string>("");
  const [guidedSetupProvider, setGuidedSetupProvider] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const probeAgentCli = useCallback(
    async (
      providerId: string,
      refresh: boolean,
      signal?: AbortSignal,
    ): Promise<AgentCliProbe | null> => {
      if (signal?.aborted) return null;
      if (agentCliProbingRef.current.has(providerId)) return null;
      const attempt = {};
      agentCliProbingRef.current.set(providerId, attempt);
      setAgentCliProbing((prev) => ({ ...prev, [providerId]: true }));
      const release = () => {
        if (agentCliProbingRef.current.get(providerId) !== attempt) return;
        agentCliProbingRef.current.delete(providerId);
        setAgentCliProbing((prev) => ({ ...prev, [providerId]: false }));
      };
      signal?.addEventListener("abort", release, { once: true });
      try {
        const probe = await invoke<AgentCliProbe>("probe_agent_cli", {
          provider: providerId,
          refresh,
        });
        if (signal?.aborted) return null;
        const ready = isAgentCliReady(probe);
        setAgentCliStatus((prev) => ({ ...prev, [providerId]: probe }));
        setProviderApiKeys((prev) => ({ ...prev, [providerId]: ready }));
        return probe;
      } catch (error) {
        if (signal?.aborted) return null;
        log.error(`Failed to probe ${providerId} CLI:`, error);
        return null;
      } finally {
        signal?.removeEventListener("abort", release);
        release();
      }
    },
    [],
  );

  const loadAISettings = useCallback(
    async (signal?: AbortSignal) => {
      if (signal?.aborted) return null;
      try {
        const listedProviders = (await invoke<AiProvider[]>("list_ai_providers")).map(
          toProviderConfig,
        );
        if (signal?.aborted) return null;
        setProviders(listedProviders);

        const loadedAISettingsResponse = await invoke<AISettingsResponse>("get_ai_settings");
        if (signal?.aborted) return null;
        const loadedAISettings = normalizeAISettings(loadedAISettingsResponse);
        const customModel =
          loadedAISettings.modelsByProvider.custom ||
          (loadedAISettings.provider === "custom" ? loadedAISettings.model : "");
        if (customModel) {
          setCustomModelName(customModel);
        }
        setAiModelNeedsReselection(Boolean(loadedAISettingsResponse.aiModelNeedsReselection));
        setAISettings(loadedAISettings);

        if (isAgentCliProvider(loadedAISettings.provider)) {
          void probeAgentCli(loadedAISettings.provider, false, signal);
        }

        const keyStatus: Record<string, boolean> = {};
        const keyTasks: Promise<void>[] = [];
        for (const { id: providerId } of listedProviders) {
          if (isAgentCliProvider(providerId)) continue;
          keyTasks.push(
            (async () => {
              let isConfigured = await hasApiKey(providerId);
              if (signal?.aborted) return;

              if ((providerId === "custom" || providerId === "openai") && !isConfigured) {
                try {
                  const providerSettings = normalizeAISettings(
                    await invoke<AISettingsResponse>("get_ai_settings_for_provider", {
                      provider: providerId,
                    }),
                  );
                  isConfigured = providerSettings.hasApiKey;
                } catch (error) {
                  if (signal?.aborted) return;
                  log.error(`Failed to resolve ${providerId} provider readiness:`, error);
                }
              }

              if (signal?.aborted) return;
              keyStatus[providerId] = isConfigured;
              if (isConfigured) {
                try {
                  const apiKey = await getApiKey(providerId);
                  if (signal?.aborted) return;
                  if (apiKey) {
                    await invoke("cache_ai_api_key", {
                      args: { provider: providerId, apiKey },
                    });
                  }
                } catch (error) {
                  if (signal?.aborted) return;
                  log.error(`Failed to cache ${providerId} API key:`, error);
                }
              }
            })(),
          );
        }
        await Promise.all(keyTasks);
        if (signal?.aborted) return null;
        setProviderApiKeys((prev) => ({ ...prev, ...keyStatus }));

        if (
          readinessAiReady &&
          loadedAISettings.provider &&
          !isAgentCliProvider(loadedAISettings.provider)
        ) {
          setProviderApiKeys((prev) => ({
            ...prev,
            [loadedAISettings.provider]: true,
          }));
        }

        try {
          const customConfig = await invoke<{ baseUrl: string }>("get_openai_config");
          if (signal?.aborted) return null;
          setOpenAIDefaultBaseUrl(customConfig.baseUrl || "https://api.openai.com/v1");
        } catch (error) {
          if (signal?.aborted) return null;
          log.error("Failed to load custom config:", error);
        }

        if (signal?.aborted) return null;
        for (const provider of listedProviders) {
          if (provider.isCustom || isAgentCliProvider(provider.id)) continue;
          void fetchModels(provider.id, signal);
        }

        return loadedAISettings;
      } catch (error) {
        if (signal?.aborted) return null;
        log.error("Failed to load AI settings:", error);
        return null;
      }
    },
    [readinessAiReady, fetchModels, probeAgentCli],
  );

  const handleRefreshAgentCli = useCallback(
    async (provider: AIProviderConfig) => {
      const probe = await probeAgentCli(provider.id, true);
      if (!probe) {
        toast.info(`${provider.name}: status refresh failed. Your previous selection was kept.`);
        return;
      }

      const state = getAgentCliProbeState(probe);
      if (state === "ready") {
        // The CLI account behind the binary can change outside the app, so the
        // picker must reload models (populated or default-only cache) from the
        // probed installation instead of trusting the pre-refresh cache.
        await fetchModels(provider.id, undefined, { force: true });
        toast.success(`${provider.name}: installed`);
      } else if (state === "missing") {
        toast.info(
          `${provider.name}: install it in an existing PATH directory, then Refresh. Restart only if PATH itself changed.`,
        );
      } else if (state === "unsafe_launcher") {
        toast.info(
          `${provider.name}: install a compatible launcher in an existing PATH directory, then Refresh.`,
        );
      } else {
        toast.info(`${provider.name}: status unavailable. Try Refresh again.`);
      }
    },
    [probeAgentCli, fetchModels],
  );

  useEffect(() => {
    const unlistenReady = listen("ai-ready", async () => {
      if (!settingsLoaded) return;
      const loadedAISettings = normalizeAISettings(
        await invoke<AISettingsResponse>("get_ai_settings"),
      );
      setAISettings(loadedAISettings);
      if (loadedAISettings.provider && !isAgentCliProvider(loadedAISettings.provider)) {
        setProviderApiKeys((prev) => ({
          ...prev,
          [loadedAISettings.provider]: true,
        }));
      }
    });

    const unlistenApiKey = listen("api-key-saved", async (event) => {
      const loadedAISettings = normalizeAISettings(
        await invoke<AISettingsResponse>("get_ai_settings"),
      );
      const provider = (event.payload as { provider?: string }).provider;
      if (!provider || provider === loadedAISettings.provider) {
        setAISettings(loadedAISettings);
      } else {
        const rememberedModel = loadedAISettings.modelsByProvider[provider] || "";
        setAISettings({
          ...loadedAISettings,
          provider,
          enabled: false,
          model: rememberedModel,
          hasApiKey: true,
        });
      }

      if (provider) {
        setProviderApiKeys((prev) => ({ ...prev, [provider]: true }));
      }
    });

    const unlistenApiKeyRemoved = listen<{ provider: string }>("api-key-removed", async (event) => {
      let providerStillConfigured = false;

      if (event.payload.provider === "custom" || event.payload.provider === "openai") {
        try {
          const providerSettings = normalizeAISettings(
            await invoke<AISettingsResponse>("get_ai_settings_for_provider", {
              provider: event.payload.provider,
            }),
          );
          providerStillConfigured = providerSettings.hasApiKey;
          setProviderApiKeys((prev) => ({
            ...prev,
            [event.payload.provider]: providerStillConfigured,
          }));
        } catch (error) {
          log.error(
            `Failed to refresh ${event.payload.provider} provider readiness after key removal:`,
            error,
          );
          setProviderApiKeys((prev) => ({
            ...prev,
            [event.payload.provider]: false,
          }));
        }
      } else {
        setProviderApiKeys((prev) => ({
          ...prev,
          [event.payload.provider]: false,
        }));
      }

      clearModels(event.payload.provider);

      const isCurrentProviderRemoved =
        aiSettings.provider === event.payload.provider && !providerStillConfigured;

      if (isCurrentProviderRemoved) {
        setAISettings((prev) => ({
          ...prev,
          enabled: false,
          provider: "",
          model: "",
          hasApiKey: false,
        }));

        await invoke("update_ai_settings", {
          enabled: false,
          provider: "",
          model: "",
        });
        await onActiveProviderCleared();
      }
    });

    const unlistenAiEnabledChanged = listen<boolean>("ai-enabled-changed", (event) => {
      setAISettings((prev) => ({ ...prev, enabled: event.payload }));
    });

    return () => {
      Promise.all([
        unlistenReady,
        unlistenApiKey,
        unlistenApiKeyRemoved,
        unlistenAiEnabledChanged,
      ]).then((fns) => {
        fns.forEach((fn) => fn());
      });
    };
  }, [settingsLoaded, aiSettings.provider, clearModels, onActiveProviderCleared]);

  const getDisplayModels = useCallback(
    (providerId: string): AIProviderModel[] => {
      const models = getModels(providerId);
      if (isAgentCliReady(agentCliStatus[providerId]) && models.length === 0) {
        if (providerId === "claude-code") {
          return CLAUDE_CODE_MODELS;
        }
        if (isAgentCliProvider(providerId)) {
          return [fallbackAgentCliDefault(providerId)];
        }
      }
      return models;
    },
    [agentCliStatus, getModels],
  );

  const resolveRecommendedModel = async (providerId: string) => {
    const cachedModels = getDisplayModels(providerId);
    let models = cachedModels;

    if (!cachedModels.some((model) => model.recommended)) {
      const fetchedModels = await fetchModels(providerId);
      models = fetchedModels?.length > 0 ? fetchedModels : getDisplayModels(providerId);
    }
    if (isAgentCliProvider(providerId)) {
      return models.find((model) => model.cliDefault) ?? fallbackAgentCliDefault(providerId);
    }

    return models.find((model) => model.recommended) ?? null;
  };

  const enablePolishForProviderModel = async (
    providerId: string,
    modelId: string,
    modelsByProvider: Record<string, string>,
  ) => {
    await invoke("update_ai_settings", {
      enabled: true,
      provider: providerId,
      model: modelId,
    });

    setAISettings((prev) => {
      const nextModelsByProvider = {
        ...prev.modelsByProvider,
        ...modelsByProvider,
      };
      if (modelId) {
        nextModelsByProvider[providerId] = modelId;
      } else {
        delete nextModelsByProvider[providerId];
      }

      return {
        ...prev,
        enabled: true,
        provider: providerId,
        model: modelId,
        hasApiKey: true,
        modelsByProvider: nextModelsByProvider,
      };
    });
    setProviderApiKeys((prev) => ({ ...prev, [providerId]: true }));
    setAiModelNeedsReselection(false);

    await onPolishEnabled();
  };

  const enableGuidedProvider = async (
    providerId: string,
    modelsByProvider: Record<string, string>,
  ) => {
    const recommendedModel = await resolveRecommendedModel(providerId);
    if (!recommendedModel) {
      toast.error("No recommended model was available. Choose a model below.");
      return false;
    }

    await enablePolishForProviderModel(providerId, recommendedModel.id, modelsByProvider);
    toast.success("Polish on");
    return true;
  };

  const handleToggleEnabled = async (enabled: boolean) => {
    const hasActiveProviderKey = Boolean(providerApiKeys[aiSettings.provider]);

    if (
      enabled &&
      (!hasActiveProviderKey || (!aiSettings.model && !isAgentCliProvider(aiSettings.provider)))
    ) {
      toast.error("Polish is not set up yet. Connect an AI to turn it on.");
      return;
    }

    try {
      await invoke("update_ai_settings", {
        enabled,
        provider: aiSettings.provider,
        model: aiSettings.model,
      });

      setAISettings((prev) => ({ ...prev, enabled }));

      await onPolishToggled(enabled);

      toast.success(enabled ? "Polish on" : "Polish off");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to update Polish");
      toast.error(message);
    }
  };

  const handleSetupApiKey = async (providerId: string) => {
    setSelectedProvider(providerId);

    if (providerId === "custom") {
      try {
        const [savedConfig, providerSettingsResponse] = await Promise.all([
          invoke<{ baseUrl: string }>("get_openai_config"),
          invoke<AISettingsResponse>("get_ai_settings_for_provider", {
            provider: providerId,
          }),
        ]);
        const providerSettings = normalizeAISettings(providerSettingsResponse);
        setOpenAIDefaultBaseUrl(savedConfig.baseUrl || "https://api.openai.com/v1");
        if (providerSettings.model) {
          setCustomModelName(providerSettings.model);
        }
      } catch (error) {
        log.error("Failed to load custom config:", error);
      }
      setShowOpenAIConfig(true);
    } else {
      setShowApiKeyModal(true);
    }
  };

  const handleApiKeySubmit = async (apiKey: string) => {
    setIsLoading(true);
    try {
      const trimmedKey = apiKey.trim();
      await saveApiKey(selectedProvider, trimmedKey);
      const providerSettings = normalizeAISettings(
        await invoke<AISettingsResponse>("get_ai_settings_for_provider", {
          provider: selectedProvider,
        }),
      );
      const rememberedModel = providerSettings.model || "";
      const modelsByProvider = {
        ...providerSettings.modelsByProvider,
        ...(rememberedModel ? { [selectedProvider]: rememberedModel } : {}),
      };
      const shouldAutoEnable = guidedSetupProvider === selectedProvider;

      setProviderApiKeys((prev) => ({ ...prev, [selectedProvider]: true }));

      if (shouldAutoEnable) {
        const didEnable = await enableGuidedProvider(selectedProvider, modelsByProvider);
        if (!didEnable) {
          setAISettings((prev) => ({
            ...prev,
            provider: selectedProvider,
            enabled: false,
            model: rememberedModel,
            hasApiKey: true,
            modelsByProvider: {
              ...prev.modelsByProvider,
              ...modelsByProvider,
            },
          }));
        }
      } else {
        setAISettings((prev) => ({
          ...prev,
          provider: selectedProvider,
          enabled: prev.provider === selectedProvider ? prev.enabled : false,
          model: rememberedModel,
          hasApiKey: true,
          modelsByProvider: {
            ...prev.modelsByProvider,
            ...modelsByProvider,
          },
        }));
        toast.success("API key saved securely");
      }

      setShowApiKeyModal(false);
      setGuidedSetupProvider(null);
    } catch (error) {
      const message = getErrorMessage(error, "Failed to save API key");
      toast.error(message);
    } finally {
      setIsLoading(false);
    }
  };

  const handleRemoveApiKey = async (providerId: string) => {
    try {
      await removeApiKey(providerId);
      clearModels(providerId);
      toast.success("API key removed");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to remove API key");
      toast.error(message);
    }
  };

  const handleSelectModel = async (providerId: string, modelId: string) => {
    try {
      const hasKey = providerApiKeys[providerId];
      const shouldEnable = hasKey ? aiSettings.enabled : false;

      await invoke("update_ai_settings", {
        enabled: shouldEnable,
        provider: providerId,
        model: modelId,
      });

      setAISettings((prev) => {
        const nextModelsByProvider = { ...prev.modelsByProvider };
        if (modelId) {
          nextModelsByProvider[providerId] = modelId;
        } else {
          delete nextModelsByProvider[providerId];
        }

        return {
          ...prev,
          enabled: shouldEnable,
          provider: providerId,
          model: modelId,
          hasApiKey: hasKey,
          modelsByProvider: nextModelsByProvider,
        };
      });
      setAiModelNeedsReselection(false);
      onModelSelected();
      if (shouldEnable) {
        await onEnabledModelSelected();
      }

      toast.success("Model selected");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to select model");
      toast.error(message);
    }
  };

  const handleSelectReasoning = async (providerId: string, reasoning: string) => {
    try {
      await invoke("update_agent_cli_reasoning", {
        provider: providerId,
        reasoning,
      });
      setAISettings((prev) => ({
        ...prev,
        reasoningByProvider: {
          ...prev.reasoningByProvider,
          [providerId]: reasoning,
        },
      }));
      toast.success("Thinking setting saved");
    } catch (error) {
      toast.error(getErrorMessage(error, "Failed to save thinking setting"));
    }
  };

  const handleToggleFastMode = async (providerId: string, enabled: boolean) => {
    try {
      await invoke("update_agent_cli_fast_mode", {
        provider: providerId,
        enabled,
      });
      setAISettings((prev) => ({
        ...prev,
        fastModeByProvider: {
          ...prev.fastModeByProvider,
          [providerId]: enabled,
        },
      }));
      toast.success(`Fast mode ${enabled ? "on" : "off"}`);
    } catch (error) {
      toast.error(getErrorMessage(error, "Failed to save fast mode"));
    }
  };
  return {
    // model discovery
    fetchModels,
    getModels,
    isModelsLoading,
    getError,
    // state
    aiSettings,
    aiModelNeedsReselection,
    providers,
    providerApiKeys,
    agentCliStatus,
    agentCliProbing,
    showApiKeyModal,
    setShowApiKeyModal,
    showOpenAIConfig,
    setShowOpenAIConfig,
    openAIDefaultBaseUrl,
    setOpenAIDefaultBaseUrl,
    customModelName,
    setCustomModelName,
    selectedProvider,
    guidedSetupProvider,
    setGuidedSetupProvider,
    isLoading,
    setIsLoading,
    setAiModelNeedsReselection,
    setProviderApiKeys,
    setAISettings,
    // actions
    loadAISettings,
    probeAgentCli,
    handleRefreshAgentCli,
    handleToggleEnabled,
    handleSetupApiKey,
    handleApiKeySubmit,
    handleRemoveApiKey,
    handleSelectModel,
    handleSelectReasoning,
    handleToggleFastMode,
    getDisplayModels,
  };
}
