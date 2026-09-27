import { useSettings } from "@/contexts/SettingsContext";
import { getCloudProviderByModel } from "@/lib/cloudProviders";
import { createLogger } from "@/lib/logger";
import { getErrorMessage } from "@/utils/error";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import { toast } from "sonner";
import type { CloudProviderDefinition } from "@/lib/cloudProviders";
import type { CloudModalMode, CloudModalState } from "./types";

const log = createLogger("models");

export function useCloudProviders({
  onSelect,
  refreshModels,
  clearActiveRemote,
}: {
  onSelect: (modelName: string) => Promise<void> | void;
  refreshModels: () => Promise<void>;
  clearActiveRemote: () => Promise<void>;
}): CloudProvidersApi {
  const { settings, updateSettings } = useSettings();
  const [cloudModal, setCloudModal] = useState<CloudModalState | null>(null);
  const [cloudModalLoading, setCloudModalLoading] = useState(false);

  const openCloudModal = useCallback((providerId: string, mode: CloudModalMode) => {
    setCloudModal({ providerId, mode });
  }, []);

  const closeCloudModal = useCallback(() => {
    if (cloudModalLoading) return;
    setCloudModal(null);
  }, [cloudModalLoading]);

  const handleCloudKeySubmit = useCallback(
    async (apiKey: string) => {
      if (!cloudModal) return;
      const provider = getCloudProviderByModel(cloudModal.providerId);
      if (!provider) {
        toast.error("Unknown cloud provider");
        return;
      }

      setCloudModalLoading(true);
      try {
        await provider.addKey(apiKey);
        await refreshModels();
        toast.success(
          `${provider.providerName} key ${cloudModal.mode === "update" ? "updated" : "saved"}`,
        );
        setCloudModal(null);
        if (cloudModal.mode === "connect") {
          await clearActiveRemote();
          await Promise.resolve(onSelect(provider.modelName));
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        toast.error(`Failed to save ${provider.providerName} key: ${message}`);
      } finally {
        setCloudModalLoading(false);
      }
    },
    [cloudModal, onSelect, refreshModels, clearActiveRemote],
  );

  const handleCloudDisconnect = useCallback(
    async (modelName: string) => {
      const provider = getCloudProviderByModel(modelName);
      if (!provider) {
        toast.error("Unknown cloud provider");
        return;
      }

      try {
        await provider.removeKey();
        toast.success(`${provider.providerName} disconnected`);
        if (settings?.current_model === provider.modelName) {
          await updateSettings({
            current_model: "",
            current_model_engine: "whisper",
          });
        }
        await refreshModels();
        // Ensure tray menu reflects removal immediately even if selection unchanged
        try {
          await invoke("update_tray_menu");
        } catch (e) {
          log.warn("[ModelsSection] Failed to refresh tray menu after disconnect:", e);
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        toast.error(`Failed to disconnect ${provider.providerName}: ${message}`);
      }
    },
    [refreshModels, settings, updateSettings],
  );

  const handleCloudModelChange = useCallback(
    async (providerId: string, modelId: string, requiresSetup: boolean) => {
      try {
        await invoke("set_cloud_stt_model", { providerId, modelId });
        await refreshModels();
        if (!requiresSetup) {
          await clearActiveRemote();
          await Promise.resolve(onSelect(providerId));
        }
      } catch (error) {
        toast.error(getErrorMessage(error, "Failed to update cloud model"));
      }
    },
    [clearActiveRemote, onSelect, refreshModels],
  );

  const activeProvider = cloudModal ? getCloudProviderByModel(cloudModal.providerId) : undefined;
  const isModalOpen = !!cloudModal && !!activeProvider;

  return {
    cloudModal,
    cloudModalLoading,
    openCloudModal,
    closeCloudModal,
    handleCloudKeySubmit,
    handleCloudDisconnect,
    handleCloudModelChange,
    activeProvider,
    isModalOpen,
  };
}

export interface CloudProvidersApi {
  cloudModal: CloudModalState | null;
  cloudModalLoading: boolean;
  openCloudModal: (providerId: string, mode: CloudModalMode) => void;
  closeCloudModal: () => void;
  handleCloudKeySubmit: (apiKey: string) => Promise<void>;
  handleCloudDisconnect: (modelName: string) => Promise<void>;
  handleCloudModelChange: (
    providerId: string,
    modelId: string,
    requiresSetup: boolean,
  ) => Promise<void>;
  activeProvider: CloudProviderDefinition | undefined;
  isModalOpen: boolean;
}
