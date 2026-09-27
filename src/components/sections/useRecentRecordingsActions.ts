import { TranscriptionHistory } from "@/types";
import { useSettings } from "@/contexts/SettingsContext";
import { invoke } from "@tauri-apps/api/core";
import { ask, save } from "@tauri-apps/plugin-dialog";
import { useState, useCallback } from "react";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";
import { buildMarkdownHistory, buildPlainHistory } from "./recentRecordingsHelpers";
import { resolveCurrentTranscriptionSource } from "./resolveCurrentTranscriptionSource";

const log = createLogger("recordings");

export function useRecentRecordingsActions({
  history,
  onHistoryUpdate,
}: {
  history: TranscriptionHistory[];
  onHistoryUpdate?: () => void;
}) {
  const [reTranscribingIds, setReTranscribingIds] = useState<Set<string>>(new Set());
  const [reTranscribingModels, setReTranscribingModels] = useState<Map<string, string>>(new Map());
  const { settings } = useSettings();

  const handleShowInFolder = useCallback(async (item: TranscriptionHistory) => {
    if (!item.recording_file) return;

    try {
      const fullPath = await invoke<string>("get_recording_path", {
        filename: item.recording_file,
      });
      await invoke("show_in_folder", { path: fullPath });
    } catch (error) {
      log.error("Failed to show recording in folder:", error);
      toast.error("Failed to open file location");
    }
  }, []);

  const handleReTranscribe = async (item: TranscriptionHistory) => {
    if (!item.recording_file) {
      toast.error("Re-transcription needs a saved audio file", {
        description: "Enable Save recordings for future takes you may want to re-transcribe.",
      });
      return;
    }

    const currentSource = await resolveCurrentTranscriptionSource(
      settings?.current_model,
      settings?.current_model_engine,
    );
    if (!currentSource) {
      toast.error("Choose a ready transcription source in Models before re-transcribing.");
      return;
    }

    setReTranscribingIds((prev) => new Set(prev).add(item.id));
    setReTranscribingModels((prev) => new Map(prev).set(item.id, currentSource.displayName));

    const cleanup = () => {
      setReTranscribingIds((prev) => {
        const next = new Set(prev);
        next.delete(item.id);
        return next;
      });
      setReTranscribingModels((prev) => {
        const next = new Map(prev);
        next.delete(item.id);
        return next;
      });
    };

    const pendingModelName = currentSource.historyModelName;
    let retryTimestamp: string | null = null;

    try {
      const recordingsDir = await invoke<string>("get_recordings_directory");
      const separator = recordingsDir.includes("\\") ? "\\" : "/";
      const fullPath = `${recordingsDir}${separator}${item.recording_file}`;
      retryTimestamp = await invoke<string>("save_retranscription", {
        text: "In progress...",
        model: pendingModelName,
        recordingFile: item.recording_file,
        sourceRecordingId: item.id,
        status: "in_progress",
      });

      let result: string;
      let modelName: string;

      if (currentSource.type === "remote") {
        if (!currentSource.serverId) {
          throw new Error("No active remote Voicetypr source selected");
        }

        result = await invoke<string>("transcribe_remote", {
          serverId: currentSource.serverId,
          audioPath: fullPath,
        });
        modelName = currentSource.historyModelName;
      } else {
        if (!currentSource.modelName) {
          throw new Error("No local or cloud transcription model selected");
        }

        result = (
          await invoke<{
            text: string;
            words: Array<{
              text: string;
              start_ms?: number;
              end_ms?: number;
              speaker_id?: string;
              confidence?: number;
            }> | null;
          }>("transcribe_audio_file", {
            filePath: fullPath,
            modelName: currentSource.modelName,
            modelEngine: currentSource.modelEngine ?? null,
          })
        ).text;
        modelName =
          currentSource.type === "cloud" ? currentSource.displayName : currentSource.modelName;
      }

      await invoke("update_transcription", {
        timestamp: retryTimestamp,
        text: result,
        model: modelName,
        status: "completed",
      });

      cleanup();

      if (onHistoryUpdate) {
        onHistoryUpdate();
      }
    } catch (error) {
      log.error("Re-transcription failed:", error);
      const failureMessage = `Re-transcription failed: ${String(error)}`;
      try {
        if (!retryTimestamp) {
          throw error;
        }
        await invoke("update_transcription", {
          timestamp: retryTimestamp,
          text: failureMessage,
          model: pendingModelName,
          status: "failed",
        });
      } catch (updateError) {
        log.error("Failed to persist retranscription error state:", updateError);
      }
      toast.error("Re-transcription failed", {
        description: String(error),
      });
      cleanup();
      if (onHistoryUpdate) {
        onHistoryUpdate();
      }
    }
  };

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();

    try {
      const confirmed = await ask("Are you sure you want to delete this transcription?", {
        title: "Delete Transcription",
        kind: "warning",
      });

      if (!confirmed) return;

      await invoke("delete_transcription_entry", { timestamp: id });

      toast.success("Transcription deleted");

      if (onHistoryUpdate) {
        onHistoryUpdate();
      }
    } catch (error) {
      log.error("Failed to delete transcription:", error);
      toast.error("Failed to delete transcription");
    }
  };

  const handleClearAll = async () => {
    if (history.length === 0) return;

    try {
      const confirmed = await ask(
        `Are you sure you want to delete all ${history.length} transcriptions? This action cannot be undone.`,
        {
          title: "Clear All Transcriptions",
          kind: "warning",
        },
      );

      if (!confirmed) return;

      await invoke("clear_all_transcriptions");

      toast.success("All transcriptions cleared");

      if (onHistoryUpdate) {
        onHistoryUpdate();
      }
    } catch (error) {
      log.error("Failed to clear all transcriptions:", error);
      toast.error("Failed to clear all transcriptions");
    }
  };

  const handleExportText = async (format: "txt" | "md") => {
    if (history.length === 0) return;
    try {
      const path = await save({
        defaultPath: `voicetypr-history.${format}`,
        filters: [{ name: format === "md" ? "Markdown" : "Text", extensions: [format] }],
      });
      if (!path) return;
      const content = format === "md" ? buildMarkdownHistory(history) : buildPlainHistory(history);
      await invoke("save_transcript_file", { path, content });
      toast.success(`Exported ${history.length} transcript${history.length === 1 ? "" : "s"}`, {
        description: format === "md" ? "Saved as Markdown" : "Saved as plain text",
      });
    } catch (error) {
      log.error("Failed to export transcripts:", error);
      toast.error("Failed to export transcripts");
    }
  };

  const handleExport = async () => {
    if (history.length === 0) return;

    try {
      const confirmed = await ask(
        `Export ${history.length} transcription${history.length !== 1 ? "s" : ""} to JSON?\n\nThe file will be saved to your Downloads folder.`,
        {
          title: "Export Transcriptions",
          kind: "info",
        },
      );

      if (!confirmed) return;

      await invoke<string>("export_transcriptions");

      toast.success(`Exported ${history.length} transcriptions`, {
        description: `Saved to Downloads folder`,
      });
    } catch (error) {
      log.error("Failed to export transcriptions:", error);
      toast.error("Failed to export transcriptions");
    }
  };

  return {
    reTranscribingIds,
    reTranscribingModels,
    handleShowInFolder,
    handleReTranscribe,
    handleDelete,
    handleClearAll,
    handleExport,
    handleExportText,
  };
}
