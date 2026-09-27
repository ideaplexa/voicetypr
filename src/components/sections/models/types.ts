import type { ModelInfo } from "@/types";

export type SourceFilter = "local" | "cloud" | "remote";

export interface SourceFilterProps {
  sourceFilter?: SourceFilter;
  onSourceFilterChange?: (filter: SourceFilter) => void;
}

export interface ModelsSectionProps extends SourceFilterProps {
  models: [string, ModelInfo][];
  downloadProgress: Record<string, number>;
  downloadPhases?: Record<string, string>;
  verifyingModels: Set<string>;
  currentModel?: string;
  downloadErrors?: Record<string, string>;
  isLoading?: boolean;
  onDownload: (modelName: string) => Promise<void> | void;
  onDelete: (modelName: string) => Promise<void> | void;
  onCancelDownload: (modelName: string) => Promise<void> | void;
  onRepair?: (modelName: string) => Promise<void> | void;
  onSelect: (modelName: string) => Promise<void> | void;
  refreshModels: () => Promise<void>;
}

export type CloudModalMode = "connect" | "update";

export interface CloudModalState {
  providerId: string;
  mode: CloudModalMode;
}

export interface DiscoveredRemoteServer {
  name: string;
  host: string;
  port: number;
  model: string;
  auth_required: boolean;
  machine_id: string;
}

export type ModelEntry = [string, ModelInfo];

export interface LocalModelActions {
  downloadProgress: Record<string, number>;
  downloadPhases: Record<string, string>;
  verifyingModels: Set<string>;
  downloadErrors: Record<string, string>;
  onDownload: (modelName: string) => Promise<void> | void;
  onDelete: (modelName: string) => Promise<void> | void;
  onCancelDownload: (modelName: string) => Promise<void> | void;
  onRepair?: (modelName: string) => Promise<void> | void;
  onSelect: (modelName: string) => Promise<void> | void;
  currentModel?: string;
  activeRemoteServer: string | null;
  clearActiveRemote: () => Promise<void>;
  speedModeRecommended?: boolean;
}
