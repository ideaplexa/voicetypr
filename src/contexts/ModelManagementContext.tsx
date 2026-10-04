import { createContext, useContext, useMemo, ReactNode } from "react";
import { useModelManagement } from "@/hooks/useModelManagement";
import { useDownloadProgressStore, type DownloadProgressStore } from "@/lib/download-progress-store";

type ModelManagement = ReturnType<typeof useModelManagement>;
/** Everything except live download progress, which is read via useDownloadProgress. */
type ModelManagementContextType = Omit<ModelManagement, "downloadProgress">;

const ModelManagementContext = createContext<ModelManagementContextType | null>(null);

export function ModelManagementProvider({ children }: { children: ReactNode }) {
  const m = useModelManagement({
    windowId: "main",
    showToasts: true,
    trackProgress: false,
  });

  const value = useMemo<ModelManagementContextType>(
    () => ({
      models: m.models,
      modelOrder: m.modelOrder,
      progressStore: m.progressStore,
      verifyingModels: m.verifyingModels,
      downloadPhases: m.downloadPhases,
      downloadErrors: m.downloadErrors,
      isLoading: m.isLoading,
      loadModels: m.loadModels,
      downloadModel: m.downloadModel,
      cancelDownload: m.cancelDownload,
      deleteModel: m.deleteModel,
      repairModel: m.repairModel,
      sortedModels: m.sortedModels,
    }),
    [
      m.models,
      m.modelOrder,
      m.progressStore,
      m.verifyingModels,
      m.downloadPhases,
      m.downloadErrors,
      m.isLoading,
      m.loadModels,
      m.downloadModel,
      m.cancelDownload,
      m.deleteModel,
      m.repairModel,
      m.sortedModels,
    ],
  );

  return <ModelManagementContext.Provider value={value}>{children}</ModelManagementContext.Provider>;
}

export function useModelManagementContext() {
  const context = useContext(ModelManagementContext);
  if (!context) {
    throw new Error("useModelManagementContext must be used within ModelManagementProvider");
  }
  return context;
}

/** Live per-model download progress (0-100). Re-renders only on progress ticks. */
export function useDownloadProgress(): Record<string, number>;
export function useDownloadProgress<T>(select: (progress: Record<string, number>) => T): T;
export function useDownloadProgress<T>(select?: (progress: Record<string, number>) => T) {
  const { progressStore } = useModelManagementContext();
  return useDownloadProgressStore(progressStore as DownloadProgressStore, select as (p: Record<string, number>) => T);
}
