import { ApiKeyModal } from "@/components/ApiKeyModal";
import { LanguageSelection } from "@/components/LanguageSelection";
import { ModelCard } from "@/components/ModelCard";
import {
  RemoteServerCard,
  SavedConnection,
} from "@/components/RemoteServerCard";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  SettingRow,
  SettingsCard,
  SettingsHeader,
  SettingsPage,
} from "@/components/settings/settings-ui";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useSettings } from "@/contexts/SettingsContext";
import { getErrorMessage } from "@/utils/error";
import { getCloudProviderByModel } from "@/lib/cloudProviders";
import { cn } from "@/lib/utils";
import { getModelDisplayName, humanizeModelId } from "@/lib/model-display";
import {
  ActiveStreamCapabilities,
  ModelInfo,
  SpeechModelEngine,
  isCloudModel,
  isLocalModel,
} from "@/types";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Bot,
  CheckCircle,
  Cloud,
  Download,
  Globe,
  HardDrive,
  HelpCircle,
  Plus,
  Server,
  Star,
  Zap,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { AddServerModal } from "./AddServerModal";
import { createLogger } from "@/lib/logger";

const log = createLogger("models");
const EOU_MODEL_ID = "parakeet-eou-live-preview";

interface ModelsSectionProps {
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

type CloudModalMode = "connect" | "update";

interface DiscoveredRemoteServer {
  name: string;
  host: string;
  port: number;
  model: string;
  auth_required: boolean;
  machine_id: string;
}

interface CloudModalState {
  providerId: string;
  mode: CloudModalMode;
}

export function ModelsSection({
  models,
  downloadProgress,
  downloadPhases = {},
  verifyingModels,
  currentModel,
  downloadErrors = {},
  isLoading = false,
  onDownload,
  onDelete,
  onCancelDownload,
  onRepair,
  onSelect,
  refreshModels,
}: ModelsSectionProps) {
  const { settings, updateSettings, refreshSettings } = useSettings();
  const [cloudModal, setCloudModal] = useState<CloudModalState | null>(null);
  const [cloudModalLoading, setCloudModalLoading] = useState(false);
  const [remoteServers, setRemoteServers] = useState<SavedConnection[]>([]);
  const [activeRemoteServer, setActiveRemoteServer] = useState<string | null>(
    null
  );
  const [addServerModalOpen, setAddServerModalOpen] = useState(false);
  const [editingServer, setEditingServer] = useState<SavedConnection | null>(null);
  const [isRefreshingServers, setIsRefreshingServers] = useState(false);
  const [discoveredServers, setDiscoveredServers] = useState<DiscoveredRemoteServer[]>([]);
  const [selectedDiscoveredServer, setSelectedDiscoveredServer] = useState<DiscoveredRemoteServer | null>(null);
  const [isDiscoveringServers, setIsDiscoveringServers] = useState(false);
  const [sourceFilter, setSourceFilter] = useState<"all" | "local" | "cloud" | "remote">("all");
  const [streamCapabilities, setStreamCapabilities] = useState<ActiveStreamCapabilities | null>(null);
  const [isActivatingLivePreview, setIsActivatingLivePreview] = useState(false);
  const [livePreviewProgress, setLivePreviewProgress] = useState<number | null>(null);

  const { availableToUse, availableToSetup } = useMemo(() => {
    const useList: [string, ModelInfo][] = [];
    const setupList: [string, ModelInfo][] = [];

    models.forEach(([name, model]) => {
      const isReady = !!model.downloaded && !model.requires_setup;
      if (isReady) {
        useList.push([name, model]);
      } else {
        setupList.push([name, model]);
      }
    });

    // Locals first within each list
    const sortFn = ([, a]: [string, ModelInfo], [, b]: [string, ModelInfo]) => {
      if (isLocalModel(a) && isCloudModel(b)) return -1;
      if (isCloudModel(a) && isLocalModel(b)) return 1;
      return 0;
    };
    useList.sort(sortFn);
    setupList.sort(sortFn);

    return { availableToUse: useList, availableToSetup: setupList };
  }, [models]);

  const readyLocalModels = availableToUse.filter(([, model]) => isLocalModel(model));
  const readyCloudModels = availableToUse.filter(([, model]) => isCloudModel(model));
  const setupLocalModels = availableToSetup.filter(([, model]) => isLocalModel(model));
  const setupCloudModels = availableToSetup.filter(([, model]) => isCloudModel(model));

  const localCount = readyLocalModels.length + setupLocalModels.length;
  const cloudCount = readyCloudModels.length + setupCloudModels.length;
  const remoteCount = remoteServers.length;
  const allCount = localCount + cloudCount + remoteCount;
  const showLocal = sourceFilter === "all" || sourceFilter === "local";
  const showCloud = sourceFilter === "all" || sourceFilter === "cloud";
  const showRemote = sourceFilter === "all" || sourceFilter === "remote";

  // No header summary line — section titles include counts

  const currentEngine = (settings?.current_model_engine ?? "whisper") as SpeechModelEngine;
  const currentModelName = settings?.current_model ?? "";
  const languageValue = settings?.speech_language ?? "en";
  const transcriptionMode = settings?.transcription_mode ?? "regular";
  const whisperSpeedMode = settings?.whisper_speed_mode ?? false;
  const showSpeedModeRecommendation = currentEngine === "whisper" && whisperSpeedMode;
  const supportsLivePreview = streamCapabilities?.capabilities.supports_streaming === true;
  // No engine needs a download to enable live preview anymore: Whisper and Parakeet
  // decode-ahead (plans 032/051) reuse the loaded transcription model, and the cloud
  // engines are just a socket. Kept as a flag (with the download-progress UI below)
  // for when Parakeet's native EOU returns after the upstream fix (plans/042 + 051).
  const livePreviewNeedsDownload = false;

  const isEnglishOnlyModel = useMemo(() => {
    if (!settings) return false;
    if (currentEngine === "whisper") {
      return /\.en$/i.test(currentModelName);
    }
    if (currentEngine === "parakeet") {
      return currentModelName.includes("-v2");
    }
    return false;
  }, [currentEngine, currentModelName, settings]);

  const handleLanguageChange = useCallback(
    async (value: string) => {
      try {
        await updateSettings({ speech_language: value });
      } catch (error) {
        log.error("Failed to update spoken language:", error);
        toast.error("Failed to update spoken language");
      }
    },
    [updateSettings],
  );

  const handleWhisperSpeedModeChange = useCallback(
    async (checked: boolean) => {
      try {
        await updateSettings({ whisper_speed_mode: checked });
      } catch (error) {
        log.error("Failed to update Whisper speed mode:", error);
        toast.error("Failed to update speed mode");
      }
    },
    [updateSettings],
  );

  const refreshStreamCapabilities = useCallback(async () => {
    try {
      const capabilities = await invoke<ActiveStreamCapabilities>("get_active_stream_capabilities");
      setStreamCapabilities(capabilities);
    } catch (error) {
      log.warn("Failed to load stream capabilities:", error);
      setStreamCapabilities(null);
    }
  }, []);

  useEffect(() => {
    void refreshStreamCapabilities();
  }, [refreshStreamCapabilities, currentEngine, currentModelName]);

  useEffect(() => {
    const unlisten = listen<{
      model: string;
      progress: number;
      phase?: string | null;
    }>("download-progress", (event) => {
      if (event.payload.model === EOU_MODEL_ID) {
        setLivePreviewProgress(Math.min(100, Math.max(0, event.payload.progress)));
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleTranscriptionModeChange = useCallback(
    async (value: string) => {
      if (!value || value === transcriptionMode) return;
      if (value === "regular") {
        try {
          await updateSettings({ transcription_mode: "regular" });
          await refreshSettings();
          await refreshStreamCapabilities();
        } catch (error) {
          log.error("Failed to switch transcription mode:", error);
          toast.error("Failed to switch transcription mode");
        }
        return;
      }

      setIsActivatingLivePreview(true);
      setLivePreviewProgress(
        livePreviewNeedsDownload && !streamCapabilities?.eou_model_downloaded ? 0 : 100,
      );
      try {
        await invoke("activate_live_preview");
        await refreshSettings();
        await refreshStreamCapabilities();
        toast.success("Live preview enabled");
      } catch (error) {
        await refreshSettings();
        await refreshStreamCapabilities();
        const message = getErrorMessage(error, "Live preview could not be enabled");
        toast.error(message);
      } finally {
        setIsActivatingLivePreview(false);
        setLivePreviewProgress(null);
      }
    },
    [
      livePreviewNeedsDownload,
      refreshSettings,
      refreshStreamCapabilities,
      streamCapabilities?.eou_model_downloaded,
      transcriptionMode,
      updateSettings,
    ],
  );

  // Remote servers management
  // Quick list fetch (no status checks) - for immediate display
  const fetchRemoteServers = useCallback(async () => {
    try {
      const servers = await invoke<SavedConnection[]>("list_remote_servers");
      setRemoteServers(servers);
    } catch (error) {
      log.error("Failed to fetch remote servers:", error);
    }
  }, []);

  // Full refresh with status checks - check each server in parallel for immediate UI updates
  const refreshRemoteServers = useCallback(async () => {
    setIsRefreshingServers(true);
    try {
      // First get the list of servers
      const servers = await invoke<SavedConnection[]>("list_remote_servers");
      setRemoteServers(servers);

      // Check each server in parallel - update UI as each responds
      const checkPromises = servers.map(async (server) => {
        try {
          const updated = await invoke<SavedConnection>("check_remote_server_status", {
            serverId: server.id,
          });
          // Update this specific server in state immediately
          setRemoteServers((prev) => {
            const index = prev.findIndex((s) => s.id === updated.id);
            if (index >= 0) {
              const newList = [...prev];
              newList[index] = updated;
              return newList;
            }
            return prev;
          });
          return updated;
        } catch (error) {
          log.error(`Failed to check server ${server.id}:`, error);
          return server; // Keep existing data on error
        }
      });

      // Wait for all checks to complete
      await Promise.all(checkPromises);
    } catch (error) {
      log.error("Failed to refresh remote servers:", error);
    } finally {
      setIsRefreshingServers(false);
    }
  }, []);

  const discoverRemoteServers = useCallback(async (notifyEmpty = false) => {
    setIsDiscoveringServers(true);
    try {
      const discovered = await invoke<DiscoveredRemoteServer[]>("discover_remote_servers", {
        timeoutMs: 1200,
      });
      setDiscoveredServers(discovered);
      if (notifyEmpty && discovered.length === 0) {
        toast.info("No remote Voicetypr devices found. You can still add one manually.");
      }
    } catch (error) {
      log.error("Failed to discover remote Voicetypr devices:", error);
      if (notifyEmpty) {
        toast.error("Failed to scan for remote Voicetypr devices");
      }
    } finally {
      await refreshRemoteServers();
      setIsDiscoveringServers(false);
    }
  }, [refreshRemoteServers]);

  const fetchActiveRemoteServer = useCallback(async () => {
    try {
      const activeId = await invoke<string | null>("get_active_remote_server");
      setActiveRemoteServer(activeId);
    } catch (error) {
      log.error("Failed to fetch active remote server:", error);
    }
  }, []);

  useEffect(() => {
    // On mount: fetch list quickly, then refresh status in background
    fetchRemoteServers();
    fetchActiveRemoteServer();
    // Trigger status refresh after initial list load
    discoverRemoteServers();
  }, [fetchRemoteServers, fetchActiveRemoteServer, discoverRemoteServers]);

  // Note: Status updates are handled via the refreshRemoteServers function
  // which calls check_remote_server_status for each server in parallel
  // and updates the UI immediately as each server responds

  // Refresh active remote server when window gains focus (handles tray menu changes)
  useEffect(() => {
    const handleFocus = () => {
      fetchActiveRemoteServer();
      // Refresh server status when user returns to the app
      refreshRemoteServers();
    };

    window.addEventListener("focus", handleFocus);

    // Also listen for Tauri window focus events
    const unlisten = listen("tauri://focus", handleFocus);

    return () => {
      window.removeEventListener("focus", handleFocus);
      unlisten.then((fn) => fn());
    };
  }, [fetchActiveRemoteServer, refreshRemoteServers]);

  // Listen for model-changed events (from tray menu selection or UI)
  useEffect(() => {
    const unlistenModelChanged = listen<{ model: string; engine: string }>(
      "model-changed",
      (event) => {
        log.debug("[ModelsSection] model-changed event received:", event.payload);
        // Refresh all model-related state
        fetchActiveRemoteServer();
        fetchRemoteServers();
        refreshSettings();
      }
    );

    return () => {
      unlistenModelChanged.then((fn) => fn());
    };
  }, [fetchActiveRemoteServer, fetchRemoteServers, refreshSettings]);

  const handleSelectRemoteServer = useCallback(
    async (serverId: string) => {
      if (serverId === activeRemoteServer) return;

      try {
        await invoke("set_active_remote_server", { serverId });
        setActiveRemoteServer(serverId);
        toast.success("Remote Voicetypr selected");
      } catch (error) {
        const message = getErrorMessage(error, "Failed to select remote Voicetypr");
        log.error("Failed to set active remote server:", error);
        toast.error(message);
      }
    },
    [activeRemoteServer]
  );

  const handleDeselectRemoteServer = useCallback(async () => {
    try {
      await invoke("set_active_remote_server", { serverId: null });
      setActiveRemoteServer(null);
      toast.success("Remote Voicetypr deselected");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to stop routing to remote Voicetypr");
      log.error("Failed to clear active remote server:", error);
      toast.error(message);
    }
  }, []);

  const handleRemoveRemoteServer = useCallback(
    async (serverId: string) => {
      try {
        if (activeRemoteServer === serverId) {
          await invoke("set_active_remote_server", { serverId: null });
          setActiveRemoteServer(null);
        }

        await invoke("remove_remote_server", { serverId });
        setRemoteServers((prev) => prev.filter((s) => s.id !== serverId));
        toast.success("Remote Voicetypr removed");
      } catch (error) {
        log.error("Failed to remove remote server:", error);
        toast.error("Failed to remove remote Voicetypr");
      }
    },
    [activeRemoteServer]
  );

  const handleServerAdded = useCallback(
    (server: SavedConnection) => {
      setRemoteServers((prev) => {
        // Check if this is an update (server already exists)
        const existingIndex = prev.findIndex((s) => s.id === server.id);
        if (existingIndex >= 0) {
          // Update existing server
          const updated = [...prev];
          updated[existingIndex] = server;
          return updated;
        }
        // Add new server
        return [...prev, server];
      });
      setEditingServer(null);
      // Trigger status refresh for all servers
      refreshRemoteServers();
    },
    [refreshRemoteServers]
  );

  const handleAddDiscoveredServer = useCallback(
    async (server: DiscoveredRemoteServer) => {
      if (server.auth_required) {
        toast.info("This remote Voicetypr requires a password. Enter it to finish adding the server.");
        setSelectedDiscoveredServer(server);
        setEditingServer(null);
        setAddServerModalOpen(true);
        return;
      }

      try {
        const added = await invoke<SavedConnection>("add_remote_server", {
          host: server.host,
          port: server.port,
          password: null,
          name: server.name,
        });
        handleServerAdded(added);
        setDiscoveredServers((prev) =>
          prev.filter((candidate) => !(candidate.host === server.host && candidate.port === server.port)),
        );
        toast.success(`${server.name} added`);
      } catch (error) {
        log.error("Failed to add discovered remote Voicetypr:", error);
        toast.error(error instanceof Error ? error.message : "Failed to add remote Voicetypr");
      }
    },
    [handleServerAdded],
  );

  const handleEditServer = useCallback((server: SavedConnection) => {
    setSelectedDiscoveredServer(null);
    setEditingServer(server);
    setAddServerModalOpen(true);
  }, []);

  useEffect(() => {
    if (!settings) return;
    if (isEnglishOnlyModel && settings.speech_language !== "en") {
      updateSettings({ speech_language: "en" }).catch((error) => {
        log.error("Failed to enforce English fallback:", error);
      });
    }
  }, [isEnglishOnlyModel, settings, updateSettings]);

  const hasDownloading = useMemo(
    () => Object.keys(downloadProgress).length > 0,
    [downloadProgress],
  );
  const hasVerifying = verifyingModels.size > 0;

  const openCloudModal = useCallback(
    (providerId: string, mode: CloudModalMode) => {
      setCloudModal({ providerId, mode });
    },
    [],
  );

  const closeCloudModal = useCallback(() => {
    if (cloudModalLoading) return;
    setCloudModal(null);
  }, [cloudModalLoading]);

  const clearActiveRemote = async () => {
    try {
      await invoke("set_active_remote_server", { serverId: null });
      setActiveRemoteServer(null);
    } catch (error) {
      log.error("Failed to clear active remote:", error);
    }
  };


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
          `${provider.providerName} key ${
            cloudModal.mode === "update" ? "updated" : "saved"
          }`,
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
          await invoke('update_tray_menu');
        } catch (e) {
          log.warn('[ModelsSection] Failed to refresh tray menu after disconnect:', e);
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        toast.error(
          `Failed to disconnect ${provider.providerName}: ${message}`,
        );
      }
    },
    [refreshModels, settings?.current_model, updateSettings],
  );

  const activeProvider = cloudModal
    ? getCloudProviderByModel(cloudModal.providerId)
    : undefined;
  const isModalOpen = !!cloudModal && !!activeProvider;

  // Subtle inline key explaining the model-card score badges.
  const modelScoreLegend = (
    <div className="mb-3 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[11px] text-muted-foreground">
      <span className="font-medium uppercase tracking-wide">Badges</span>
      <span className="flex items-center gap-1.5">
        <Zap className="size-3.5 text-emerald-600" />
        Speed
      </span>
      <span className="flex items-center gap-1.5">
        <CheckCircle className="size-3.5 text-blue-600" />
        Accuracy
      </span>
      <span className="flex items-center gap-1.5">
        <HardDrive className="size-3.5" />
        Size
      </span>
      <span className="flex items-center gap-1.5">
        <Star className="size-3.5 fill-amber-500 text-amber-500" />
        Recommended
      </span>
    </div>
  );

  const renderCloudCard = useCallback(
    ([name, model]: [string, ModelInfo]) => {
      if (!isCloudModel(model)) return null;

      const provider =
        getCloudProviderByModel(name) ?? getCloudProviderByModel(model.engine);
      const requiresSetup = model.requires_setup;
      const isActive = currentModel === name && !activeRemoteServer;

      return (
        <Card
          key={name}
          className={cn(
            "group rounded-xl border border-border bg-card p-4 transition-colors",
            requiresSetup ? "" : "cursor-pointer",
            isActive
              ? "border-sage/50 bg-sage-bg/40"
              : "hover:border-sage/40 hover:bg-muted/30",
          )}
          onClick={async () => {
            if (requiresSetup) {
              openCloudModal(name, "connect");
              return;
            }
            await clearActiveRemote();
            void onSelect(name);
          }}
        >
          <div className="flex items-center justify-between gap-4">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <h3 className={cn("truncate text-sm font-semibold tracking-tight", isActive && "text-sage")}>
                  {provider?.displayName || provider?.providerName || getModelDisplayName(name, { [name]: model }) || name}
                </h3>
                {isCloudModel(model) && model.underlying_model && (
                  <Badge variant="outline" className="gap-1 font-mono text-muted-foreground">
                    {model.underlying_model}
                  </Badge>
                )}
                {isActive && (
                  <Badge className="gap-1 bg-sage text-sage-foreground">
                    <CheckCircle className="size-3" />
                    Active
                  </Badge>
                )}
              </div>
              {provider?.description && (
                <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
                  {provider.description}
                </p>
              )}
              <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-xs text-muted-foreground">
                <span className="inline-flex items-center gap-1.5">
                  <Zap className="size-3.5 text-sage" />
                  Speed <span className="font-medium text-foreground">{model.speed_score ?? "—"}</span>
                </span>
                <span className="inline-flex items-center gap-1.5">
                  <CheckCircle className="size-3.5 text-sage" />
                  Accuracy <span className="font-medium text-foreground">{model.accuracy_score ?? "—"}</span>
                </span>
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
              {requiresSetup ? (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={(event) => {
                    event.stopPropagation();
                    openCloudModal(name, "connect");
                  }}
                >
                  {provider?.setupCta ?? "Add API Key"}
                </Button>
              ) : (
                <Button
                  size="sm"
                  variant="ghost"
                  className="text-muted-foreground hover:text-destructive"
                  onClick={(event) => {
                    event.stopPropagation();
                    handleCloudDisconnect(name);
                  }}
                >
                  Remove API Key
                </Button>
              )}
            </div>
          </div>
        </Card>
      );
    },
    [currentModel, activeRemoteServer, handleCloudDisconnect, onSelect, openCloudModal, clearActiveRemote],
  );

  return (
    <>
      <SettingsPage>
      <SettingsHeader
        title={
          <span className="inline-flex items-center gap-2">
            Transcription
            <Dialog>
              <DialogTrigger asChild>
                <Button type="button" variant="ghost" size="icon-sm" aria-label="Transcription sources guide" className="size-7 rounded-full text-muted-foreground">
                  <HelpCircle className="h-4 w-4" />
                </Button>
              </DialogTrigger>
              <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                  <DialogTitle>Transcription sources guide</DialogTitle>
                  <DialogDescription>
                    Pick where speech recognition runs before recording or uploading files.
                  </DialogDescription>
                </DialogHeader>
                <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                  <p><strong className="text-foreground">Local</strong> models run on this machine and keep raw audio local.</p>
                  <p><strong className="text-foreground">Cloud</strong> sources use a connected provider when you choose one.</p>
                  <p><strong className="text-foreground">Remote Voicetypr</strong> uses another device on your network when that server is online.</p>
                </div>
              </DialogContent>
            </Dialog>
          </span>
        }
        description="Choose where your speech is transcribed — local models, cloud providers, and remote Voicetypr servers."
      />

      <SettingsCard icon={Globe} title="Spoken language">
        <SettingRow
          title="Spoken language"
          description="The language you speak. English-only models lock this to English."
          control={
            <div className="flex items-center gap-2">
              {(hasDownloading || hasVerifying) && (
                <Badge variant="outline" className="gap-1.5 bg-primary/10 text-primary">
                  {hasDownloading ? (
                    <Download className="size-3.5" />
                  ) : (
                    <Spinner className="size-3.5" />
                  )}
                  {hasDownloading ? "Downloading..." : "Verifying..."}
                </Badge>
              )}
              <LanguageSelection
                value={languageValue}
                engine={currentEngine}
                englishOnly={isEnglishOnlyModel}
                onValueChange={(value) => {
                  void handleLanguageChange(value);
                }}
              />
            </div>
          }
        />
      </SettingsCard>

      {currentEngine === "whisper" && (
        <SettingsCard icon={Zap} title="Whisper performance">
          <SettingRow
            title="Speed mode"
            description="Faster transcription (flash attention); pairs best with Large v3 Turbo."
            control={
              <Switch
                id="whisper-speed-mode"
                checked={whisperSpeedMode}
                onCheckedChange={(checked) => {
                  void handleWhisperSpeedModeChange(checked);
                }}
                aria-label="Speed mode"
              />
            }
          />
        </SettingsCard>
      )}

      {supportsLivePreview && (
        <SettingsCard
          icon={Zap}
          title="Transcription mode"
          description="Live preview is English-only for now; your final text still uses your selected model."
        >
          <SettingRow
            title="Mode"
            description="Regular waits for the final transcript. Live preview shows text locally as you speak (final text still uses your selected model)."
            control={
              <div className="flex flex-col items-end gap-2">
                <ToggleGroup
                  type="single"
                  variant="outline"
                  size="sm"
                  spacing={0}
                  value={transcriptionMode}
                  onValueChange={(value) => {
                    void handleTranscriptionModeChange(value);
                  }}
                  aria-label="Transcription mode"
                  disabled={isActivatingLivePreview}
                  className="[&_[data-state=on]]:!bg-sage-bg [&_[data-state=on]]:!text-sage [&_[data-state=on]]:!border-sage/50 [&_[data-state=on]]:font-medium"
                >
                  <ToggleGroupItem value="regular">Regular</ToggleGroupItem>
                  <ToggleGroupItem value="live_preview">Live preview (English)</ToggleGroupItem>
                </ToggleGroup>
                {isActivatingLivePreview && (
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Spinner className="size-3.5" />
                    <span>
                      {livePreviewNeedsDownload
                        ? livePreviewProgress !== null && livePreviewProgress < 100
                          ? `Downloading EOU model ${Math.round(livePreviewProgress)}%`
                          : "Verifying EOU model"
                        : "Enabling live preview…"}
                    </span>
                  </div>
                )}
              </div>
            }
          />
        </SettingsCard>
      )}

      <div className="flex flex-wrap items-center justify-between gap-3">
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          spacing={0}
          value={sourceFilter}
          onValueChange={(value) => {
            if (value) setSourceFilter(value as typeof sourceFilter);
          }}
          aria-label="Filter transcription sources"
          className="[&_[data-state=on]]:!bg-sage-bg [&_[data-state=on]]:!text-sage [&_[data-state=on]]:!border-sage/50 [&_[data-state=on]]:font-medium"
        >
          <ToggleGroupItem value="all">All ({allCount})</ToggleGroupItem>
          <ToggleGroupItem value="local">Local ({localCount})</ToggleGroupItem>
          <ToggleGroupItem value="cloud">Cloud ({cloudCount})</ToggleGroupItem>
          <ToggleGroupItem value="remote">Remote ({remoteCount})</ToggleGroupItem>
        </ToggleGroup>
      </div>

      {showLocal && readyLocalModels.length > 0 && (
        <SettingsCard
          icon={HardDrive}
          title={`Local models (${readyLocalModels.length})`}
          description="Offline transcription models stored on this machine."
        >
          <div className="mt-4">
            {modelScoreLegend}
          </div>
          <div className="grid gap-3">
            {readyLocalModels.map(([name, model]) => (
                    <ModelCard
                      key={name}
                      name={name}
                      model={model}
                      downloadProgress={downloadProgress[name]}
                      downloadPhase={downloadPhases[name]}
                      isVerifying={verifyingModels.has(name)}
                      downloadError={downloadErrors[name]}
                      onDownload={onDownload}
                      onDelete={onDelete}
                      onCancelDownload={onCancelDownload}
                      onRepair={onRepair}
                      onSelect={async (modelName) => {
                        await clearActiveRemote();
                        void onSelect(modelName);
                      }}
                      speedModeRecommended={showSpeedModeRecommendation && name === "large-v3-turbo"}
                      showSelectButton={model.downloaded}
                      isSelected={!activeRemoteServer && currentModel === name}
                    />
                  ))}
          </div>
        </SettingsCard>
      )}

      {showCloud && readyCloudModels.length > 0 && (
        <SettingsCard
          icon={Cloud}
          title={`Cloud transcription (${readyCloudModels.length})`}
          description="Connected providers that can transcribe without a local model. When selected, Voicetypr may send Personal Library words, names, and corrections as transcription context to improve recognition; snippets are not sent."
        >
          <div className="mt-4 grid gap-3">
            {readyCloudModels.map(([name, model]) => renderCloudCard([name, model]))}
          </div>
        </SettingsCard>
      )}

      {((showLocal && setupLocalModels.length > 0) ||
        (showCloud && setupCloudModels.length > 0)) && (
        <SettingsCard
          icon={Download}
          title="Set up sources"
          description="Download local models or connect cloud providers before selecting them."
        >
          <div className="mt-4 space-y-3">
            {showLocal && setupLocalModels.length > 0 && (
              <div className="grid gap-3">
                {setupLocalModels.map(([name, model]) => (
                      <ModelCard
                        key={name}
                        name={name}
                        model={model}
                        downloadProgress={downloadProgress[name]}
                        isVerifying={verifyingModels.has(name)}
                        downloadError={downloadErrors[name]}
                        downloadPhase={downloadPhases[name]}
                        onDownload={onDownload}
                        onDelete={onDelete}
                        onCancelDownload={onCancelDownload}
                        onRepair={onRepair}
                        onSelect={async (modelName) => {
                          await clearActiveRemote();
                          void onSelect(modelName);
                        }}
                        speedModeRecommended={showSpeedModeRecommendation && name === "large-v3-turbo"}
                        showSelectButton={model.downloaded}
                        isSelected={!activeRemoteServer && currentModel === name}
                      />
                    ))}
                  </div>
                )}
                {showCloud && setupCloudModels.length > 0 && (
                  <div className="grid gap-3">
                    {setupCloudModels.map(([name, model]) => renderCloudCard([name, model]))}
                  </div>
                )}
          </div>
        </SettingsCard>
      )}

      {showRemote && (
      <SettingsCard
        icon={Server}
        title={`Remote Voicetypr (${remoteServers.length})`}
        description="Use another Voicetypr device on your network without copying audio to the cloud."
        action={
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={() => void discoverRemoteServers(true)}
              disabled={isDiscoveringServers}
            >
              {isDiscoveringServers ? <Spinner className="size-4" /> : null}
              Scan LAN
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                setSelectedDiscoveredServer(null);
                setAddServerModalOpen(true);
              }}
            >
              <Plus className="size-4" />
              Add manually
            </Button>
          </div>
        }
      >
        <div className="mt-4 space-y-3">
          {discoveredServers.length > 0 && (
            <div className="grid gap-3">
                  {discoveredServers.map((server) => {
                    const alreadySaved = remoteServers.some(
                      (saved) => saved.host === server.host && saved.port === server.port,
                    );

                    if (alreadySaved) return null;

                    return (
                      <Card
                        key={`${server.machine_id}:${server.host}:${server.port}`}
                        className="rounded-xl border border-border bg-card p-4 transition-colors hover:border-sage/40 hover:bg-muted/30"
                      >
                        <div className="flex items-center justify-between gap-4">
                          <div className="min-w-0 flex-1">
                            <div className="flex items-center gap-2">
                              <Server className="size-4 shrink-0 text-sage" />
                              <h3 className="truncate text-sm font-semibold tracking-tight">{server.name}</h3>
                              <Badge variant={server.auth_required ? "outline" : "secondary"}>
                                {server.auth_required ? "Password required" : "Found on LAN"}
                              </Badge>
                            </div>
                            <p className="mt-2.5 flex flex-wrap items-center gap-x-3 text-xs text-muted-foreground">
                              <span>{server.host}:{server.port}</span>
                              <span>·</span>
                              <span className="truncate">{getModelDisplayName(server.model) ?? humanizeModelId(server.model)}</span>
                            </p>
                          </div>
                          <Button size="sm" className="shrink-0" onClick={() => void handleAddDiscoveredServer(server)}>
                            {server.auth_required ? "Add with password" : "Add"}
                          </Button>
                        </div>
                      </Card>
                    );
                  })}
                </div>
              )}
              {remoteServers.length > 0 ? (
                <div className="grid gap-3">
                  {remoteServers.map((server) => (
                    <RemoteServerCard
                      key={server.id}
                      server={server}
                      isActive={activeRemoteServer === server.id}
                      onSelect={handleSelectRemoteServer}
                      onDeselect={handleDeselectRemoteServer}
                      onRemove={handleRemoveRemoteServer}
                      onEdit={handleEditServer}
                      isRefreshing={isRefreshingServers}
                      onServerUpdated={refreshRemoteServers}
                    />
                  ))}
                </div>
              ) : (
                <Empty className="border border-border/60 bg-card/70 py-8">
                  <EmptyHeader>
                    <EmptyMedia variant="icon">
                      <Server className="size-5" />
                    </EmptyMedia>
                    <EmptyTitle>No remote Voicetyprs configured</EmptyTitle>
                    <EmptyDescription>
                      Connect another Voicetypr device to use its local model from this machine.
                    </EmptyDescription>
                  </EmptyHeader>
                </Empty>
              )}
        </div>
      </SettingsCard>
      )}

      {isLoading &&
        availableToUse.length === 0 &&
        availableToSetup.length === 0 && (
          <Empty className="border border-border/60 bg-card/70 py-12">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Spinner className="size-5" />
              </EmptyMedia>
              <EmptyTitle>Loading models</EmptyTitle>
              <EmptyDescription>
                Checking available transcription sources.
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}

      {!isLoading &&
        availableToUse.length === 0 &&
        availableToSetup.length === 0 &&
        remoteServers.length === 0 && (
          <Empty className="border border-border/60 bg-card/70 py-12">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Bot className="size-5" />
              </EmptyMedia>
              <EmptyTitle>No models available</EmptyTitle>
              <EmptyDescription>
                Models will appear here when they become available.
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
      </SettingsPage>

      {activeProvider && (
        <ApiKeyModal
          isOpen={isModalOpen}
          onClose={closeCloudModal}
          onSubmit={handleCloudKeySubmit}
          providerName={activeProvider.providerName}
          isLoading={cloudModalLoading}
          title={
            cloudModal?.mode === "update"
              ? `Update ${activeProvider.providerName} API Key`
              : `Add ${activeProvider.providerName} API Key`
          }
          description={
            cloudModal?.mode === "update"
              ? `Update your ${activeProvider.providerName} API key to keep cloud transcription running smoothly.`
              : `Enter your ${activeProvider.providerName} API key to enable cloud transcription. Your key is stored securely in the system keychain.`
          }
          submitLabel={
            cloudModal?.mode === "update" ? "Update API Key" : "Save API Key"
          }
          docsUrl={activeProvider.docsUrl}
        />
      )}

      <AddServerModal
        open={addServerModalOpen}
        onOpenChange={(open) => {
          setAddServerModalOpen(open);
          if (!open) {
            setEditingServer(null);
            setSelectedDiscoveredServer(null);
          }
        }}
        onServerAdded={handleServerAdded}
        editServer={editingServer}
        initialServer={
          selectedDiscoveredServer
            ? {
                host: selectedDiscoveredServer.host,
                port: selectedDiscoveredServer.port,
                name: selectedDiscoveredServer.name,
                authRequired: selectedDiscoveredServer.auth_required,
              }
            : null
        }
      />
    </>
  );
}
