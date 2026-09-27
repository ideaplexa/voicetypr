import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { ModelInfo, isCloudModel } from "../types";
import { useEventCoordinator } from "./useEventCoordinator";
import { getModelDisplayName } from "@/lib/model-display";
import { createLogger } from "@/lib/logger";

const log = createLogger("models");

interface UseModelManagementOptions {
  windowId?: "main" | "pill" | "onboarding";
  showToasts?: boolean;
}

const CLOUD_SPEED_FALLBACK = 8;
const CLOUD_ACCURACY_FALLBACK = 9;

const isDownloadCancellation = (error: unknown) =>
  String(error).toLowerCase().includes("cancelled") ||
  String(error).toLowerCase().includes("canceled");

const getErrorText = (error: unknown) => (error instanceof Error ? error.message : String(error));

const createDownloadRequestId = () => {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }

  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
};

const getSpeedScore = (model: ModelInfo) =>
  model.speed_score ?? (isCloudModel(model) ? CLOUD_SPEED_FALLBACK : 0);

const getAccuracyScore = (model: ModelInfo) =>
  model.accuracy_score ?? (isCloudModel(model) ? CLOUD_ACCURACY_FALLBACK : 0);

const getSizeValue = (model: ModelInfo) =>
  isCloudModel(model) ? Number.POSITIVE_INFINITY : (model.size ?? Number.POSITIVE_INFINITY);

// Helper function to calculate balanced performance score
function calculateBalancedScore(model: ModelInfo): number {
  const speed = getSpeedScore(model);
  const accuracy = getAccuracyScore(model);
  // Weighted average: 40% speed, 60% accuracy
  return ((speed * 0.4 + accuracy * 0.6) / 10) * 100;
}

// Helper function to sort models by various criteria
export function sortModels(
  models: [string, ModelInfo][],
  sortBy: "balanced" | "speed" | "accuracy" | "size" = "balanced",
): [string, ModelInfo][] {
  return [...models].sort(([, a], [, b]) => {
    // Sort by the specified criteria
    switch (sortBy) {
      case "speed":
        return getSpeedScore(b) - getSpeedScore(a);
      case "accuracy":
        return getAccuracyScore(a) - getAccuracyScore(b);
      case "size":
        if (isCloudModel(a) && isCloudModel(b)) return 0;
        if (isCloudModel(a)) return 1;
        if (isCloudModel(b)) return -1;
        return getSizeValue(a) - getSizeValue(b);
      case "balanced":
      default:
        return calculateBalancedScore(b) - calculateBalancedScore(a);
    }
  });
}

export function useModelManagement(options: UseModelManagementOptions = {}) {
  const { windowId = "main", showToasts = true } = options;
  const { registerEvent } = useEventCoordinator(windowId);

  // Track active downloads to prevent duplicates
  const activeDownloads = useRef(new Set<string>());
  const cancelledDownloads = useRef(new Set<string>());
  const activeDownloadRequests = useRef(new Map<string, string>());
  const cancelledDownloadRequests = useRef(new Set<string>());
  const modelsRef = useRef<Record<string, ModelInfo>>({});

  const [models, setModels] = useState<Record<string, ModelInfo>>({});
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});
  const [downloadPhases, setDownloadPhases] = useState<Record<string, string>>({});
  const [verifyingModels, setVerifyingModels] = useState<Set<string>>(new Set());
  const [downloadErrors, setDownloadErrors] = useState<Record<string, string>>({});

  // Removed selectedModel state - now using settings.current_model directly
  const [isLoading, setIsLoading] = useState(true);

  const clearDownloadState = useCallback((modelName: string) => {
    activeDownloads.current.delete(modelName);
    activeDownloadRequests.current.delete(modelName);
    setDownloadProgress((prev) => {
      const newProgress = { ...prev };
      delete newProgress[modelName];
      return newProgress;
    });
    setDownloadPhases((prev) => {
      const newPhases = { ...prev };
      delete newPhases[modelName];
      return newPhases;
    });
    setVerifyingModels((prev) => {
      const newSet = new Set(prev);
      newSet.delete(modelName);
      return newSet;
    });
  }, []);

  const clearDownloadError = useCallback((modelName: string) => {
    setDownloadErrors((prev) => {
      if (!(modelName in prev)) {
        return prev;
      }
      const newErrors = { ...prev };
      delete newErrors[modelName];
      return newErrors;
    });
  }, []);

  // Load models from backend
  const loadModels = useCallback(async () => {
    try {
      setIsLoading(true);
      log.debug("[useModelManagement] Calling get_model_status...");
      const response = await invoke<{ models: ModelInfo[] }>("get_model_status");
      log.debug("model status loaded", { count: response.models.length });

      if (!response || !Array.isArray(response.models)) {
        throw new Error("Invalid response format from get_model_status");
      }

      const modelStatus = Object.fromEntries(response.models.map((model) => [model.name, model]));
      setModels(modelStatus);
      modelsRef.current = modelStatus;

      return response.models;
    } catch (error) {
      log.error("[useModelManagement.loadModels] Failed to load models:", error);
      if (showToasts) {
        toast.error("Failed to load models");
      }
      return [];
    } finally {
      setIsLoading(false);
    }
  }, [showToasts]);

  // Download model
  const downloadModel = useCallback(
    async (modelName: string) => {
      const target = models[modelName];
      if (target && isCloudModel(target)) {
        if (showToasts) {
          toast.info(`${target.display_name} is a cloud model and does not require downloading.`);
        }
        return;
      }

      // Check if already downloading
      if (activeDownloads.current.has(modelName)) {
        if (showToasts) {
          toast.info(`${getModelDisplayName(modelName, models)} is already downloading`);
        }
        return;
      }

      try {
        const requestId = createDownloadRequestId();

        // Mark as downloading
        activeDownloads.current.add(modelName);
        activeDownloadRequests.current.set(modelName, requestId);

        // Set initial progress to show download started
        setDownloadProgress((prev) => ({
          ...prev,
          [modelName]: 0,
        }));
        setDownloadPhases((prev) => {
          const newPhases = { ...prev };
          delete newPhases[modelName];
          return newPhases;
        });
        clearDownloadError(modelName);

        // Don't await - let it run async so progress events can update UI
        invoke("download_model", { modelName, requestId }).catch((error) => {
          if (cancelledDownloadRequests.current.has(requestId) || isDownloadCancellation(error)) {
            const isCurrentRequest = activeDownloadRequests.current.get(modelName) === requestId;
            cancelledDownloadRequests.current.add(requestId);
            if (isCurrentRequest) {
              clearDownloadState(modelName);
              clearDownloadError(modelName);
            }
            return;
          }

          if (activeDownloadRequests.current.get(modelName) !== requestId) {
            return;
          }

          const errorText = getErrorText(error);
          log.error("[useModelManagement.downloadModel] Failed to download model:", error);
          setDownloadErrors((prev) => ({
            ...prev,
            [modelName]: errorText,
          }));
          if (showToasts) {
            toast.error(
              `Failed to download ${getModelDisplayName(modelName, models)}: ${errorText}`,
            );
          }
          if (activeDownloadRequests.current.get(modelName) === requestId) {
            clearDownloadState(modelName);
          }
        });
      } catch (error) {
        log.error("[useModelManagement.downloadModel] Failed to start download:", error);
        const errorText = getErrorText(error);
        if (showToasts) {
          toast.error(
            `Failed to start ${getModelDisplayName(modelName, models)} download: ${errorText}`,
          );
        }
        setDownloadErrors((prev) => ({
          ...prev,
          [modelName]: errorText,
        }));
        clearDownloadState(modelName);
      }
    },
    [clearDownloadError, clearDownloadState, models, showToasts],
  );

  // Cancel download
  const cancelDownload = useCallback(
    async (modelName: string) => {
      const requestId = activeDownloadRequests.current.get(modelName);

      try {
        cancelledDownloads.current.add(modelName);
        if (requestId) {
          cancelledDownloadRequests.current.add(requestId);
        }

        await invoke("cancel_download", { modelName });

        if (!requestId || activeDownloadRequests.current.get(modelName) === requestId) {
          setDownloadPhases((prev) => ({
            ...prev,
            [modelName]: "cancelling",
          }));
        }
      } catch (error) {
        cancelledDownloads.current.delete(modelName);
        if (requestId) {
          cancelledDownloadRequests.current.delete(requestId);
        }
        log.error("Failed to cancel download:", error);
        if (showToasts) {
          const errorText = getErrorText(error);
          toast.error(
            `Failed to cancel ${getModelDisplayName(modelName, modelsRef.current)} download: ${errorText}`,
          );
        }
      }
    },
    [showToasts],
  );

  // Delete model
  const deleteModel = useCallback(
    async (modelName: string) => {
      const target = models[modelName];
      if (target && isCloudModel(target)) {
        if (showToasts) {
          toast.info(`${target.display_name} is a cloud model and cannot be deleted locally.`);
        }
        return false;
      }

      try {
        const confirmed = await ask(
          `Are you sure you want to delete the ${getModelDisplayName(modelName, models)} model?`,
          {
            title: "Delete Model",
            kind: "warning",
          },
        );

        if (!confirmed) {
          return false;
        }

        await invoke("delete_model", { modelName });

        // Refresh model status
        await loadModels();

        // Model selection clearing is handled by the component via settings
        return true;
      } catch (error) {
        const errorText = getErrorText(error);
        log.error("Failed to delete model:", error);
        if (showToasts) {
          toast.error(`Failed to delete ${getModelDisplayName(modelName, models)}: ${errorText}`);
        }
        throw error;
      }
    },
    [loadModels, models, showToasts],
  );

  const repairModel = useCallback(
    async (modelName: string) => {
      const confirmed = await ask(
        `Repair ${getModelDisplayName(modelName, modelsRef.current)} by deleting its local cache and downloading it again?`,
        {
          title: "Repair Model",
          kind: "warning",
        },
      );

      if (!confirmed) {
        return;
      }

      try {
        await invoke("delete_model", { modelName });
        setModels((prev) => {
          const existing = prev[modelName];
          if (!existing) {
            return prev;
          }

          return {
            ...prev,
            [modelName]: {
              ...existing,
              downloaded: false,
              requires_setup: true,
            },
          };
        });
        await downloadModel(modelName);
      } catch (error) {
        log.error("Failed to repair model:", error);
        if (showToasts) {
          toast.error(
            `Failed to repair ${getModelDisplayName(modelName, modelsRef.current)}: ${error}`,
          );
        }
      }
    },
    [downloadModel, showToasts],
  );

  // Setup event listeners BEFORE any other effects
  useEffect(() => {
    let isMounted = true;
    const unlisteners: Array<() => void> = [];

    const register = async <T>(
      eventName: string,
      handler: (payload: T) => void | Promise<void>,
    ) => {
      const unlisten = await registerEvent<T>(eventName, handler);
      if (typeof unlisten !== "function") return;
      if (!isMounted) {
        unlisten();
        return;
      }
      unlisteners.push(unlisten);
    };

    const setupListeners = async () => {
      // Progress updates
      await register<{
        model: string;
        engine?: string;
        downloaded: number;
        total: number;
        progress: number;
        requestId?: string;
        phase?: string | null;
      }>("download-progress", (payload) => {
        const { model, progress, engine, requestId, phase } = payload;
        if (requestId && cancelledDownloadRequests.current.has(requestId)) {
          return;
        }

        const activeRequestId = activeDownloadRequests.current.get(model);
        if (requestId && activeRequestId && activeRequestId !== requestId) {
          return;
        }

        log.debug(
          `[useModelManagement] Download progress for ${model} (${engine ?? "whisper"}): ${progress.toFixed(1)}%`,
        );

        // Keep updating progress until we receive the verifying event
        setDownloadProgress((prev) => ({
          ...prev,
          [model]: Math.min(progress, 100), // Ensure progress doesn't exceed 100%
        }));
        if (phase) {
          setDownloadPhases((prev) => ({
            ...prev,
            [model]: phase,
          }));
        }
      });

      // Model verifying (after download, before verification)
      await register<{
        model: string;
        engine?: string;
        requestId?: string;
      }>("model-verifying", (event) => {
        const modelName = event.model;
        const requestId = event.requestId;
        if (requestId && cancelledDownloadRequests.current.has(requestId)) {
          return;
        }

        const activeRequestId = activeDownloadRequests.current.get(modelName);
        if (requestId && activeRequestId && activeRequestId !== requestId) {
          return;
        }

        // First ensure the progress shows 100% before transitioning to verification
        setDownloadProgress((prev) => ({
          ...prev,
          [modelName]: 100,
        }));
        setDownloadPhases((prev) => {
          const newPhases = { ...prev };
          delete newPhases[modelName];
          return newPhases;
        });

        // Add to verifying set
        setVerifyingModels((prev) => new Set(prev).add(modelName));

        // Remove from download progress after a brief delay to ensure UI updates
        setTimeout(() => {
          const activeRequestId = activeDownloadRequests.current.get(modelName);
          if ((requestId && activeRequestId !== requestId) || (!requestId && activeRequestId)) {
            return;
          }

          setDownloadProgress((prev) => {
            const newProgress = { ...prev };
            delete newProgress[modelName];
            return newProgress;
          });
        }, 500);
      });

      // Download complete
      await register<{
        model: string;
        engine?: string;
        requestId?: string;
      }>("model-downloaded", async (event) => {
        const modelName = event.model;
        const requestId = event.requestId;
        const isCancelledCompletion = requestId
          ? cancelledDownloadRequests.current.has(requestId)
          : cancelledDownloads.current.has(modelName);

        if (isCancelledCompletion) {
          if (requestId) {
            cancelledDownloadRequests.current.delete(requestId);
          }
          cancelledDownloads.current.delete(modelName);
          if (!requestId || activeDownloadRequests.current.get(modelName) === requestId) {
            activeDownloads.current.delete(modelName);
            activeDownloadRequests.current.delete(modelName);
          }
          clearDownloadState(modelName);
          clearDownloadError(modelName);
          return;
        }

        const activeRequestId = activeDownloadRequests.current.get(modelName);
        if (requestId && activeRequestId && activeRequestId !== requestId) {
          return;
        }

        // Remove from active downloads
        clearDownloadState(modelName);
        setModels((prev) => {
          const existing = prev[modelName];
          if (!existing) {
            return prev;
          }

          return {
            ...prev,
            [modelName]: {
              ...existing,
              downloaded: true,
              requires_setup: false,
            },
          };
        });

        clearDownloadError(modelName);
        // Refresh model list
        await loadModels();

        if (showToasts) {
          toast.success(
            `${getModelDisplayName(modelName, modelsRef.current)} downloaded successfully`,
          );
        }
      });

      // Download error
      await register<{
        model: string;
        engine?: string;
        requestId?: string;
        error: string;
      }>("download-error", (payload) => {
        const { model: modelName, requestId } = payload;
        const errorText = payload.error || "Download failed";

        if (requestId && cancelledDownloadRequests.current.has(requestId)) {
          if (activeDownloadRequests.current.get(modelName) === requestId) {
            clearDownloadState(modelName);
            clearDownloadError(modelName);
          }
          cancelledDownloadRequests.current.delete(requestId);
          cancelledDownloads.current.delete(modelName);
          return;
        }

        // Only handle if this is still the active request. If it was already cleared (e.g. the
        // download_model invoke rejection handled this same failure) or superseded by a newer
        // request, skip — otherwise one backend failure produces duplicate error UX.
        const activeRequestId = activeDownloadRequests.current.get(modelName);
        if (requestId && activeRequestId !== requestId) {
          return;
        }

        setDownloadErrors((prev) => ({
          ...prev,
          [modelName]: errorText,
        }));
        clearDownloadState(modelName);

        if (!isDownloadCancellation(errorText) && showToasts) {
          toast.error("Download Failed", {
            description: `Failed to download ${getModelDisplayName(modelName, modelsRef.current)}. Please try again.`,
            duration: 5000,
          });
        }
      });

      // Download cancelled
      await register<{
        model: string;
        engine?: string;
        requestId?: string;
      }>("download-cancelled", (payload) => {
        const modelName = typeof payload === "string" ? payload : payload.model;
        const requestId = typeof payload === "string" ? undefined : payload.requestId;
        const isCurrentRequest =
          !requestId || activeDownloadRequests.current.get(modelName) === requestId;
        // Remove from active downloads
        if (isCurrentRequest) {
          activeDownloads.current.delete(modelName);
          activeDownloadRequests.current.delete(modelName);
        }
        cancelledDownloads.current.add(modelName);
        if (requestId) {
          cancelledDownloadRequests.current.add(requestId);
        }

        if (isCurrentRequest) {
          clearDownloadState(modelName);
          clearDownloadError(modelName);
        }

        if (showToasts) {
          toast.info(`Download cancelled for ${getModelDisplayName(modelName, modelsRef.current)}`);
        }
      });
    };

    void setupListeners();

    // Cleanup
    return () => {
      isMounted = false;
      unlisteners.forEach((unlisten) => {
        if (typeof unlisten === "function") {
          unlisten();
        }
      });
    };
  }, [registerEvent, loadModels, showToasts, clearDownloadState, clearDownloadError]);

  // Load models on mount
  useEffect(() => {
    void (async () => {
      await Promise.resolve();
      await loadModels();
    })();
  }, [loadModels]);

  // Derive model order from sorted models
  const sortedModelsArray = sortModels(Object.entries(models), "accuracy");
  const modelOrder = sortedModelsArray.map(([name]) => name);

  return {
    // State
    models,
    modelOrder,
    downloadProgress,
    verifyingModels,
    downloadPhases,
    downloadErrors,
    isLoading,

    // Actions
    loadModels,
    downloadModel,
    cancelDownload,
    deleteModel,
    repairModel,

    // Utils
    sortedModels: sortedModelsArray,
  };
}
