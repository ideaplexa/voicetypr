import { ApiKeyModal } from "@/components/ApiKeyModal";
import { OpenAICompatConfigModal } from "@/components/OpenAICompatConfigModal";
import type { AISettings } from "@/types/ai";
import { invoke } from "@tauri-apps/api/core";
import type { Dispatch, SetStateAction } from "react";
import { toast } from "sonner";
import { getErrorMessage } from "@/utils/error";
import { getApiKey, saveApiKey } from "@/utils/keyring";

export function EnhancementsProviderModals({
  showApiKeyModal,
  setShowApiKeyModal,
  setGuidedSetupProvider,
  handleApiKeySubmit,
  selectedProvider,
  isLoading,
  setIsLoading,
  showOpenAIConfig,
  setShowOpenAIConfig,
  openAIDefaultBaseUrl,
  setOpenAIDefaultBaseUrl,
  customModelName,
  setCustomModelName,
  aiSettings,
  setAISettings,
  setProviderApiKeys,
}: {
  showApiKeyModal: boolean;
  setShowApiKeyModal: (open: boolean) => void;
  setGuidedSetupProvider: (provider: string | null) => void;
  handleApiKeySubmit: (apiKey: string) => Promise<void>;
  selectedProvider: string;
  isLoading: boolean;
  setIsLoading: (loading: boolean) => void;
  showOpenAIConfig: boolean;
  setShowOpenAIConfig: (open: boolean) => void;
  openAIDefaultBaseUrl: string;
  setOpenAIDefaultBaseUrl: (url: string) => void;
  customModelName: string;
  setCustomModelName: (name: string) => void;
  aiSettings: AISettings;
  setAISettings: Dispatch<SetStateAction<AISettings>>;
  setProviderApiKeys: Dispatch<SetStateAction<Record<string, boolean>>>;
}) {
  return (
    <>
      <ApiKeyModal
        isOpen={showApiKeyModal}
        onClose={() => {
          setShowApiKeyModal(false);
          setGuidedSetupProvider(null);
        }}
        onSubmit={handleApiKeySubmit}
        providerName={selectedProvider}
        isLoading={isLoading}
      />

      <OpenAICompatConfigModal
        isOpen={showOpenAIConfig}
        defaultBaseUrl={openAIDefaultBaseUrl}
        defaultModel={customModelName || ""}
        onClose={() => setShowOpenAIConfig(false)}
        onSubmit={async ({ baseUrl, model, apiKey }) => {
          try {
            setIsLoading(true);
            const trimmedBase = baseUrl.trim();
            const trimmedModel = model.trim();
            const trimmedKey = apiKey?.trim() || "";

            const existingKey = trimmedKey ? "" : await getApiKey("custom");
            const validationKey = trimmedKey || existingKey || "";
            const noAuth = !validationKey;

            if (trimmedKey) {
              await saveApiKey("custom", trimmedKey, {
                baseUrl: trimmedBase,
                model: trimmedModel,
                noAuth: false,
              });
            } else {
              await invoke("validate_ai_api_key", {
                args: {
                  provider: "custom",
                  apiKey: validationKey,
                  baseUrl: trimmedBase,
                  model: trimmedModel,
                  noAuth,
                },
              });
              if (validationKey) {
                await invoke("cache_ai_api_key", {
                  args: { provider: "custom", apiKey: validationKey },
                });
              }
            }

            await invoke("set_openai_config", {
              args: { baseUrl: trimmedBase, noAuth },
            });

            await invoke("update_ai_settings", {
              enabled: aiSettings.enabled,
              provider: "custom",
              model: trimmedModel,
            });

            setCustomModelName(trimmedModel);
            setOpenAIDefaultBaseUrl(trimmedBase);
            setAISettings((prev) => ({
              ...prev,
              provider: "custom",
              model: trimmedModel,
              hasApiKey: true,
              modelsByProvider: {
                ...prev.modelsByProvider,
                custom: trimmedModel,
              },
            }));
            setProviderApiKeys((prev) => ({ ...prev, custom: true }));

            toast.success("Custom provider configured");
            setShowOpenAIConfig(false);
          } catch (error) {
            const message = getErrorMessage(error, "Failed to save configuration");
            toast.error(message);
          } finally {
            setIsLoading(false);
          }
        }}
      />
    </>
  );
}
