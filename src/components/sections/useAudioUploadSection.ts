import { useState, useEffect } from "react";
import { toast } from "sonner";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useSettings } from "@/contexts/SettingsContext";
import { useModelAvailabilityContext } from "@/contexts/ModelAvailabilityContext";
import { listen } from "@tauri-apps/api/event";
import { getModelDisplayName } from "@/lib/model-display";
import { isCloudEngine } from "@/lib/cloudProviders";
import { useUploadStore } from "@/state/upload";
import { createLogger } from "@/lib/logger";

const log = createLogger("audio-upload");

export function useAudioUploadSection() {
  const [copied, setCopied] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [activeRemoteServer, setActiveRemoteServer] = useState<string | null>(null);
  const { settings } = useSettings();
  const { selectedModelAvailable, remoteAvailable } = useModelAvailabilityContext();
  const {
    selectedFile,
    status,
    resultText,
    error: storeError,
    speakerSegments,
    diarizationError,
    diarized,
    select,
    clearSelection,
    start,
    reset,
  } = useUploadStore();
  const isProcessing = status === "processing";
  const effectiveFileName = selectedFile?.name || null;
  const hasEffectiveSelection = !!selectedFile;

  const resolveHistoryModelName = async (remoteServerIdOverride?: string | null) => {
    const effectiveRemoteId = remoteServerIdOverride ?? activeRemoteServer;
    if (!effectiveRemoteId) {
      return isCloudEngine(settings?.current_model_engine ?? "whisper")
        ? getModelDisplayName(settings?.current_model) || ""
        : getModelDisplayName(settings?.current_model) || "";
    }

    try {
      const servers = await invoke<
        Array<{
          id: string;
          name?: string;
          host: string;
          port: number;
        }>
      >("list_remote_servers");
      if (!Array.isArray(servers)) {
        return `Remote: ${effectiveRemoteId}`;
      }

      const activeServer = servers.find((server) => server.id === effectiveRemoteId);

      if (!activeServer) {
        return `Remote: ${effectiveRemoteId}`;
      }

      const displayName = activeServer.name || `${activeServer.host}:${activeServer.port}`;
      return `Remote: ${displayName}`;
    } catch (error) {
      log.error("Failed to resolve active remote server name:", error);
      return `Remote: ${effectiveRemoteId}`;
    }
  };

  useEffect(() => {
    const loadActiveRemoteServer = async () => {
      try {
        const activeId = await invoke<string | null>("get_active_remote_server");
        setActiveRemoteServer(activeId);
      } catch (error) {
        log.error("Failed to get active remote server:", error);
        setActiveRemoteServer(null);
      }
    };

    loadActiveRemoteServer();

    const unlistenModelChanged = listen("model-changed", loadActiveRemoteServer);
    const unlistenSharingChanged = listen("sharing-status-changed", loadActiveRemoteServer);

    return () => {
      unlistenModelChanged.then((fn) => fn());
      unlistenSharingChanged.then((fn) => fn());
    };
  }, []);

  const activeSourceLabel = activeRemoteServer
    ? "Remote Voicetypr"
    : isCloudEngine(settings?.current_model_engine ?? "whisper")
      ? getModelDisplayName(settings?.current_model) || "No source selected"
      : getModelDisplayName(settings?.current_model) || "No source selected";

  const handleFileSelect = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: "Audio/Video Files",
            extensions: ["wav", "mp3", "m4a", "flac", "ogg", "mp4", "webm"],
          },
        ],
      });

      if (selected && typeof selected === "string") {
        select(selected);
      }
    } catch (error) {
      log.error("Failed to select file:", error);
      toast.error("Failed to select file");
    }
  };

  const handleTranscribe = async () => {
    if (!selectedFile) {
      toast.error("Please select an audio file first");
      return;
    }

    const [latestActiveRemoteServer, latestAvailability] = await Promise.all([
      invoke<string | null>("get_active_remote_server").catch(() => activeRemoteServer),
      invoke<{ remote_available?: boolean } | null>("get_recognition_availability_snapshot").catch(
        () => null,
      ),
    ]);
    const effectiveRemoteSelected = !!latestActiveRemoteServer;
    const effectiveRemoteAvailable = latestAvailability?.remote_available ?? remoteAvailable;

    if (effectiveRemoteSelected && !effectiveRemoteAvailable) {
      toast.error("Selected remote unavailable. Reconnect or choose another source.");
      return;
    }

    if (!settings?.current_model && !effectiveRemoteAvailable) {
      toast.error("Select a speech model in Models before transcribing.");
      return;
    }

    if (!effectiveRemoteSelected && selectedModelAvailable === false) {
      const engine = settings?.current_model_engine || "whisper";
      toast.error(
        isCloudEngine(engine)
          ? "Connect your cloud provider before transcribing audio."
          : "Download the selected model before transcribing audio.",
      );
      return;
    }

    if (isProcessing) {
      toast.info("A transcription is already in progress");
      return;
    }

    const result = await start(
      settings?.current_model || "",
      effectiveRemoteSelected ? null : settings?.current_model_engine || "whisper",
      await resolveHistoryModelName(latestActiveRemoteServer),
    );

    try {
      if (result?.outcome === "error") {
        toast.error(result.message);
      } else if (result?.outcome === "success") {
        toast.success("Transcription completed and saved to history!");
      }
    } catch (error) {
      log.error("[Upload] Transcription handling failed:", error);
      toast.error(`Transcription failed: ${error}`);
    } finally {
      // keep store state; UI renders from it
    }
  };

  const handleCopy = async () => {
    if (resultText) {
      try {
        await navigator.clipboard.writeText(resultText);
        setCopied(true);
        toast.success("Copied to clipboard");
        setTimeout(() => setCopied(false), 2000);
      } catch (error) {
        log.error("Failed to copy:", error);
        toast.error("Failed to copy to clipboard");
      }
    }
  };

  const handleSaveAs = async () => {
    if (!resultText) return;
    const base = (selectedFile?.name || "transcript").replace(/\.[^/.]+$/, "");
    try {
      const path = await save({
        defaultPath: `${base}.txt`,
        filters: [
          { name: "Text", extensions: ["txt"] },
          { name: "Markdown", extensions: ["md"] },
        ],
      });
      if (!path) return; // cancelled
      const content = path.toLowerCase().endsWith(".md")
        ? `# ${base}\n\n${resultText}`
        : resultText;
      await invoke("save_transcript_file", { path, content });
      toast.success("Transcript saved");
    } catch (e) {
      toast.error(`Save failed: ${e}`);
    }
  };

  const handleReset = () => {
    reset();
    setCopied(false);
  };

  useEffect(() => {
    const handleFileDrop = (filePath: string) => {
      if (useUploadStore.getState().status === "processing") return;

      const supportedExtensions = ["wav", "mp3", "m4a", "flac", "ogg", "mp4", "webm"];
      const fileExtension = filePath.split(".").pop()?.toLowerCase();

      if (!fileExtension || !supportedExtensions.includes(fileExtension)) {
        toast.error("Unsupported file format. Please drop an audio or video file.");
        return;
      }

      select(filePath);
    };

    const unlisten = listen("tauri://drag-drop", (event) => {
      setIsDragging(false);
      if (useUploadStore.getState().status === "processing") return;

      const payload = event.payload as { paths: string[]; position: { x: number; y: number } };
      if (payload.paths && payload.paths.length > 0) {
        handleFileDrop(payload.paths[0]);
      }
    });

    const unlistenHover = listen("tauri://drag-hover", () => {
      if (useUploadStore.getState().status !== "processing") setIsDragging(true);
    });

    const unlistenLeave = listen("tauri://drag-leave", () => {
      setIsDragging(false);
    });

    return () => {
      unlisten.then((fn) => fn());
      unlistenHover.then((fn) => fn());
      unlistenLeave.then((fn) => fn());
    };
  }, [select]);

  return {
    copied,
    setCopied,
    isDragging: isDragging && !isProcessing,
    selectedFile,
    status,
    resultText,
    storeError,
    speakerSegments,
    diarizationError,
    diarized,
    clearSelection,
    isProcessing,
    effectiveFileName,
    hasEffectiveSelection,
    activeSourceLabel,
    handleFileSelect,
    handleTranscribe,
    handleCopy,
    handleSaveAs,
    handleReset,
  };
}
