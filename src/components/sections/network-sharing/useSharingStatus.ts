import { useSettings } from "@/contexts/SettingsContext";
import { getModelDisplayName } from "@/lib/model-display";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";
import type { AppSettings } from "@/types";
import {
  MAX_SHARING_PORT,
  MIN_SHARING_PORT,
  bindingResultsFromLocalIps,
  isShareableEngine,
  isShareableModel,
  parseSharingPort,
} from "./sharingUtils";
import type { FirewallStatus, ModelStatusResponse, SharingStatus } from "./types";

const log = createLogger("network");

const INITIAL_STATUS: SharingStatus = {
  enabled: false,
  port: null,
  model_name: null,
  server_name: null,
  active_connections: 0,
  password_configured: false,
  binding_results: [],
  allow_model_control: false,
};

export function useSharingAutoRestart({
  settings,
  currentModel,
  currentEngine,
  statusEnabled,
  statusModelName,
  port,
  fetchStatus,
  modelDisplayName,
}: {
  settings: AppSettings | null;
  currentModel: string | undefined;
  currentEngine: string;
  statusEnabled: boolean;
  statusModelName: string | null;
  port: string;
  fetchStatus: () => Promise<void>;
  modelDisplayName: string | null;
}) {
  const previousLocalSelectionRef = useRef<{
    model?: string | null;
    engine?: string | null;
  } | null>(null);

  // Auto-restart sharing only after the local model selection changes.
  useEffect(() => {
    if (!settings) return;
    const previousSelection = previousLocalSelectionRef.current;
    const nextSelection = {
      model: currentModel,
      engine: currentEngine,
    };

    previousLocalSelectionRef.current = nextSelection;

    if (!previousSelection) return;
    if (
      previousSelection.model === nextSelection.model &&
      previousSelection.engine === nextSelection.engine
    ) {
      return;
    }
    if (!statusEnabled || !currentModel) return;
    if (previousSelection.model !== currentModel && statusModelName === currentModel) return;

    const autoRestartSharing = async () => {
      log.debug(
        `[Remote Transcription] Local model changed to ${currentModel}, restarting sharing...`,
      );

      const validatedPort = parseSharingPort(port);
      if (!validatedPort) {
        toast.error(`Enter a valid port between ${MIN_SHARING_PORT} and ${MAX_SHARING_PORT}`);
        await fetchStatus();
        return;
      }

      try {
        await invoke("stop_sharing");
        await invoke("start_sharing", {
          port: validatedPort,
          password: null,
          preservePassword: true,
          serverName: null,
        });
        await fetchStatus();
        toast.success(
          `Remote transcription now uses ${modelDisplayName ?? getModelDisplayName(currentModel) ?? currentModel}`,
        );
      } catch (error) {
        log.error("Failed to restart remote transcription with new model:", error);
        toast.error("Failed to switch remote transcription model");
        await fetchStatus();
      }
    };

    autoRestartSharing();
  }, [
    settings,
    currentModel,
    currentEngine,
    statusEnabled,
    statusModelName,
    port,
    fetchStatus,
    modelDisplayName,
  ]);
}

export function useSharingStatus() {
  const { settings, updateSettings } = useSettings();
  const [status, setStatus] = useState<SharingStatus>(INITIAL_STATUS);
  const [showPassword, setShowPassword] = useState(false);
  const [port, setPort] = useState("47842");
  const [password, setPassword] = useState("");
  const [savedPort, setSavedPort] = useState("47842");
  const [savedPassword, setSavedPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [savingPort, setSavingPort] = useState(false);
  const [savingPassword, setSavingPassword] = useState(false);
  const [savingModelControl, setSavingModelControl] = useState(false);
  const [modelDisplayName, setModelDisplayName] = useState<string | null>(null);
  const [hasShareableModel, setHasShareableModel] = useState<boolean>(true);
  const [currentSelectionShareable, setCurrentSelectionShareable] = useState<boolean>(true);
  const [activeRemoteServer, setActiveRemoteServer] = useState<string | null>(null);
  const [firewallStatus, setFirewallStatus] = useState<FirewallStatus | null>(null);

  const currentModel = settings?.current_model;
  const currentEngine = settings?.current_model_engine ?? "whisper";
  const sharedModelDisplayName =
    status.enabled && status.model_name
      ? (getModelDisplayName(status.model_name) ?? status.model_name)
      : modelDisplayName;

  const fetchStatus = useCallback(async () => {
    try {
      const result = await invoke<SharingStatus>("get_sharing_status");
      let bindingResults = result.binding_results ?? [];

      if (result.enabled && bindingResults.length === 0) {
        try {
          const localIps = await invoke<string[]>("get_local_ips");
          bindingResults = bindingResultsFromLocalIps(localIps);
        } catch (error) {
          log.error("Failed to get local IPs:", error);
        }
      }

      const normalizedResult = {
        ...result,
        binding_results: bindingResults,
      };
      setStatus(normalizedResult);
      // Only update port/password from server status when sharing is enabled
      // When disabled, we rely on persisted settings to preserve values
      if (normalizedResult.enabled) {
        if (normalizedResult.port) {
          const portStr = normalizedResult.port.toString();
          setPort(portStr);
          setSavedPort(portStr);
        }
      }
    } catch (error) {
      log.error("Failed to get sharing status:", error);
    }
  }, []);

  const fetchActiveRemoteServer = useCallback(async () => {
    try {
      const activeId = await invoke<string | null>("get_active_remote_server");
      setActiveRemoteServer(activeId);
    } catch (error) {
      log.error("Failed to get active remote server:", error);
    }
  }, []);

  const fetchFirewallStatus = useCallback(async () => {
    try {
      const result = await invoke<FirewallStatus>("get_firewall_status");
      setFirewallStatus(result);
    } catch (error) {
      log.error("Failed to get firewall status:", error);
      setFirewallStatus(null);
    }
  }, []);

  const fetchModelInfo = useCallback(async () => {
    try {
      const response = await invoke<ModelStatusResponse>("get_model_status");
      const models = response.models || [];
      const shareableModels = models.filter(isShareableModel);
      setHasShareableModel(shareableModels.length > 0);

      const selectedModel = currentModel ? models.find((m) => m.name === currentModel) : null;
      const selectionShareable = selectedModel
        ? isShareableModel(selectedModel)
        : isShareableEngine(currentEngine) && shareableModels.length > 0;
      setCurrentSelectionShareable(selectionShareable);

      if (currentModel && selectionShareable) {
        const selected = shareableModels.find((m) => m.name === currentModel);
        setModelDisplayName(
          selected?.display_name || getModelDisplayName(currentModel) || currentModel,
        );
      } else if (shareableModels.length > 0) {
        setModelDisplayName(shareableModels[0].display_name);
      } else {
        setModelDisplayName(null);
      }
    } catch (error) {
      log.error("Failed to get model status:", error);
      setHasShareableModel(false);
      setCurrentSelectionShareable(false);
    }
  }, [currentModel, currentEngine]);

  useEffect(() => {
    // Initial IPC fetches; deferred a microtask so no setState runs
    // synchronously inside the effect.
    queueMicrotask(() => {
      void fetchStatus();
      void fetchActiveRemoteServer();
      void fetchFirewallStatus();
    });
  }, [fetchStatus, fetchActiveRemoteServer, fetchFirewallStatus]);

  useEffect(() => {
    queueMicrotask(() => {
      void fetchModelInfo();
    });
  }, [currentModel, fetchModelInfo]);

  useTauriEvent("sharing-status-changed", () => {
    log.debug("[NetworkSharingCard] Received sharing-status-changed event, refreshing status...");
    void fetchStatus();
    void fetchActiveRemoteServer(); // Also refresh remote server state in case it changed
  });

  // Hydrate the port fields from persisted settings — adjusted during render
  // when the saved port changes.
  const savedPortSetting = settings?.sharing_port;
  const [lastSavedPortSetting, setLastSavedPortSetting] = useState(savedPortSetting);
  if (savedPortSetting !== lastSavedPortSetting) {
    setLastSavedPortSetting(savedPortSetting);
    if (savedPortSetting) {
      const portStr = savedPortSetting.toString();
      setPort(portStr);
      setSavedPort(portStr);
    }
  }

  useSharingAutoRestart({
    settings,
    currentModel,
    currentEngine,
    statusEnabled: status.enabled,
    statusModelName: status.model_name,
    port,
    fetchStatus,
    modelDisplayName,
  });

  return {
    settings,
    updateSettings,
    status,
    setStatus,
    showPassword,
    setShowPassword,
    port,
    setPort,
    password,
    setPassword,
    savedPort,
    setSavedPort,
    savedPassword,
    setSavedPassword,
    loading,
    setLoading,
    savingPort,
    setSavingPort,
    savingPassword,
    setSavingPassword,
    savingModelControl,
    setSavingModelControl,
    modelDisplayName,
    hasShareableModel,
    currentSelectionShareable,
    activeRemoteServer,
    firewallStatus,
    currentModel,
    currentEngine,
    sharedModelDisplayName,
    fetchStatus,
    fetchFirewallStatus,
  };
}
