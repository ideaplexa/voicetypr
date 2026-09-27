import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export type UploadStatus = "idle" | "processing" | "done" | "error";
export type UploadResult =
  | { outcome: "success"; text: string }
  | { outcome: "blank" }
  | { outcome: "error"; message: string };

export type SelectedFile = { path: string; name: string };
export type SpeakerSegment = { speaker_id: string; start_ms: number; end_ms: number };
export type TranscriptionWord = {
  text: string;
  start_ms?: number;
  end_ms?: number;
  speaker_id?: string;
  confidence?: number;
};

type UploadTranscriptionResult = {
  text: string;
  words: TranscriptionWord[] | null;
  metadata?: Record<string, unknown> | null;
};

type UploadState = {
  selectedFile: SelectedFile | null;
  status: UploadStatus;
  resultText: string | null;
  error: string | null;
  speakerSegments: SpeakerSegment[];
  diarizationError: string | null;
  diarized: boolean;
  select: (path: string) => void;
  clearSelection: () => void;
  start: (
    modelName: string,
    modelEngine: string | null,
    historyModelName?: string,
  ) => Promise<UploadResult | null>;
  reset: () => void;
};

export const useUploadStore = create<UploadState>((set, get) => ({
  selectedFile: null,
  status: "idle",
  resultText: null,
  error: null,
  speakerSegments: [],
  diarizationError: null,
  diarized: false,

  select: (path: string) => {
    if (get().status === "processing") return;

    const name = path.split(/[\\/]/).pop() || "audio file";
    set({
      selectedFile: { path, name },
      resultText: null,
      error: null,
      speakerSegments: [],
      diarizationError: null,
      diarized: false,
    });
  },

  clearSelection: () => {
    if (get().status === "processing") return;

    set({ selectedFile: null, speakerSegments: [], diarizationError: null, diarized: false });
  },

  start: async (modelName: string, modelEngine: string | null, historyModelName?: string) => {
    const { selectedFile, status } = get();
    if (!selectedFile) return null;
    if (status === "processing") return null;
    set({
      status: "processing",
      error: null,
      resultText: null,
      speakerSegments: [],
      diarizationError: null,
      diarized: false,
    });
    try {
      const res = await invoke<UploadTranscriptionResult>("transcribe_audio_file", {
        filePath: selectedFile.path,
        modelName,
        modelEngine,
      });
      if (!res.text || res.text.trim() === "" || res.text === "[BLANK_AUDIO]") {
        set({ status: "error", error: "No speech detected in the audio file" });
        return { outcome: "blank" };
      }

      let speakerSegments: SpeakerSegment[] = [];
      let diarizationError: string | null = null;
      if (modelEngine === "parakeet") {
        try {
          speakerSegments = await invoke<SpeakerSegment[]>("diarize_audio_file", {
            filePath: selectedFile.path,
          });
        } catch (error) {
          diarizationError = String(error);
        }
      }

      await invoke("save_transcription", {
        text: res.text,
        model: historyModelName || modelName,
        metadata: res.metadata ?? undefined,
      });
      set({
        status: "done",
        resultText: res.text,
        speakerSegments,
        diarizationError,
        diarized: res.words != null,
      });
      return { outcome: "success", text: res.text };
    } catch (e: any) {
      const message = String(e?.message || e);
      set({ status: "error", error: message });
      return { outcome: "error", message };
    }
  },

  reset: () => {
    if (get().status === "processing") return;

    set({
      selectedFile: null,
      status: "idle",
      resultText: null,
      error: null,
      speakerSegments: [],
      diarizationError: null,
      diarized: false,
    });
  },
}));
