import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { AudioUploadSection } from "../AudioUploadSection";
import { useUploadStore } from "@/state/upload";

// Minimal mocks - only what's absolutely necessary
vi.mock("sonner");
vi.mock("@tauri-apps/api/core");
vi.mock("@tauri-apps/plugin-dialog");
vi.mock("@tauri-apps/api/event");
const mockSettings = {
  current_model: "base.en",
  current_model_engine: "whisper",
  hotkey: "Cmd+Shift+Space",
  speech_language: "en",
  theme: "system",
};

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: mockSettings,
    isLoading: false,
    error: null,
    refreshSettings: vi.fn(),
    updateSettings: vi.fn(),
  }),
}));

// AudioUploadSection now reads availability from context; delegate the context
// hook to the real useModelAvailability so the per-test mocked invoke still drives it.
vi.mock("@/contexts/ModelAvailabilityContext", async () => {
  const actual = await vi.importActual<typeof import("@/hooks/useModelAvailability")>(
    "@/hooks/useModelAvailability",
  );
  return { useModelAvailabilityContext: actual.useModelAvailability };
});

import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";

const readyLocalModel = {
  name: "base.en",
  display_name: "Whisper Base (English)",
  size: 157_286_400,
  url: "https://example.com/base.en",
  sha256: "sha256-base-en",
  downloaded: true,
  speed_score: 6,
  accuracy_score: 6,
  recommended: false,
  engine: "whisper",
  kind: "local" as const,
  requires_setup: false,
};

describe("AudioUploadSection - Essential User Flows", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSettings.current_model = "base.en";
    mockSettings.current_model_engine = "whisper";
    // Default mock for event listener
    vi.mocked(listen).mockResolvedValue(() => {});
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "get_model_status") {
        return { models: [readyLocalModel] };
      }
      if (cmd === "get_active_remote_server") {
        return null;
      }
      if (cmd === "list_remote_servers") {
        return [];
      }
      return null;
    });
    useUploadStore.getState().reset();
  });

  describe("Critical Path: Upload and Transcribe", () => {
    it("user can select a file and get transcription", async () => {
      const user = userEvent.setup();

      // Mock file selection and transcription
      vi.mocked(open).mockResolvedValue("/audio/meeting.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return { models: [readyLocalModel] };
        }
        if (cmd === "transcribe_audio_file") {
          return { text: "This is the meeting transcript content that was processed", words: null };
        }
        if (cmd === "check_whisper_models") {
          return ["base.en"];
        }
        return null;
      });

      render(<AudioUploadSection />);

      // User selects a file
      const selectButton = await screen.findByRole("button", { name: /select file/i });
      await user.click(selectButton);

      // File appears
      await waitFor(() => {
        expect(screen.getByText(/meeting.mp3/)).toBeInTheDocument();
      });

      // User clicks transcribe
      const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
      await user.click(transcribeButton);

      // Transcription appears
      await waitFor(
        () => {
          expect(screen.getByText(/This is the meeting transcript/)).toBeInTheDocument();
        },
        { timeout: 3000 },
      );

      expect(invoke).toHaveBeenCalledWith("transcribe_audio_file", {
        filePath: "/audio/meeting.mp3",
        modelName: "base.en",
        modelEngine: "whisper",
      });

      // Success is reflected by rendering the result text
      expect(screen.getByText(/This is the meeting transcript/)).toBeInTheDocument();
      expect(toast.success).toHaveBeenCalledWith("Transcription completed and saved to history!");
    });

    it("keeps result actions available with a long Parakeet speaker timeline", async () => {
      const user = userEvent.setup();
      mockSettings.current_model = "parakeet-tdt-0.6b-v3";
      mockSettings.current_model_engine = "parakeet";
      vi.mocked(open).mockResolvedValue("/audio/interview.wav");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return {
            models: [
              {
                ...readyLocalModel,
                name: "parakeet-tdt-0.6b-v3",
                display_name: "Parakeet V3",
                engine: "parakeet",
              },
            ],
          };
        }
        if (cmd === "get_active_remote_server") {
          return null;
        }
        if (cmd === "get_recognition_availability_snapshot") {
          return {
            parakeet_available: true,
            remote_available: false,
          };
        }
        if (cmd === "transcribe_audio_file") {
          return { text: "Speaker transcript", words: null };
        }
        if (cmd === "diarize_audio_file") {
          return Array.from({ length: 100 }, (_, index) => ({
            speaker_id: `speaker_${(index % 2) + 1}`,
            start_ms: index * 2500,
            end_ms: (index + 1) * 2500,
          }));
        }
        return null;
      });

      render(<AudioUploadSection />);

      await user.click(await screen.findByRole("button", { name: /select file/i }));
      await waitFor(() => screen.getByText(/interview.wav/));
      await user.click(await screen.findByRole("button", { name: /transcribe/i }));

      await waitFor(() => {
        expect(screen.getByText("Speaker transcript")).toBeInTheDocument();
      });

      expect(invoke).toHaveBeenCalledWith("diarize_audio_file", {
        filePath: "/audio/interview.wav",
      });
      expect(screen.getByText("Speaker timeline")).toBeInTheDocument();
      expect(screen.getByText("0:00–0:02")).toBeInTheDocument();
      expect(screen.getByLabelText("Speaker timeline segments")).toBeVisible();
      expect(screen.getAllByText("speaker_1")).toHaveLength(50);
      expect(screen.getByRole("button", { name: "Copy" })).toBeVisible();
      expect(screen.getByRole("button", { name: "Save" })).toBeVisible();
      expect(screen.getByRole("button", { name: "Transcribe Another File" })).toBeVisible();
    });

    it("user can copy transcribed text to clipboard", async () => {
      const user = userEvent.setup();

      // Mock clipboard API properly
      const mockWriteText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, "clipboard", {
        value: { writeText: mockWriteText },
        writable: true,
        configurable: true,
      });

      // Setup transcription
      vi.mocked(open).mockResolvedValue("/audio/file.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return { models: [readyLocalModel] };
        }
        if (cmd === "transcribe_audio_file") {
          return { text: "Text to copy to clipboard", words: null };
        }
        return null;
      });

      render(<AudioUploadSection />);

      // Select and transcribe
      const selectButton = await screen.findByRole("button", { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/file.mp3/));

      const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
      await user.click(transcribeButton);
      await waitFor(() => screen.getByText("Text to copy to clipboard"));

      // The copy button is an icon button - find it by the Copy icon SVG
      const copyButton = await screen.findByRole("button", { name: /copy/i });
      await user.click(copyButton);

      expect(mockWriteText).toHaveBeenCalledWith("Text to copy to clipboard");
      expect(toast.success).toHaveBeenCalledWith("Copied to clipboard");
    });
  });

  describe("Critical Errors: User Guidance", () => {
    it("shows clear error when file is too large", async () => {
      const user = userEvent.setup();

      vi.mocked(open).mockResolvedValue("/audio/huge-file.wav");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return { models: [readyLocalModel] };
        }
        if (cmd === "transcribe_audio_file") {
          throw new Error("File too large. Maximum size is 1GB");
        }
        return null;
      });

      render(<AudioUploadSection />);

      // Select file
      const selectButton = await screen.findByRole("button", { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/huge-file.wav/));

      // Try to transcribe
      const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
      await user.click(transcribeButton);

      // User sees inline error message
      await waitFor(() => {
        expect(screen.getByText(/file too large/i)).toBeInTheDocument();
      });
      expect(toast.error).toHaveBeenCalledWith("File too large. Maximum size is 1GB");
    });

    it("guides user when no model is installed", async () => {
      const user = userEvent.setup();

      vi.mocked(open).mockResolvedValue("/audio/file.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return { models: [{ ...readyLocalModel, downloaded: false }] };
        }
        if (cmd === "transcribe_audio_file") {
          throw new Error("No Whisper model found. Please download a model first.");
        }
        return null;
      });

      render(<AudioUploadSection />);

      // Select file
      const selectButton = await screen.findByRole("button", { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/file.mp3/));

      // Try to transcribe
      const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
      await user.click(transcribeButton);

      // User sees helpful error
      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith(
          "Download the selected model before transcribing audio.",
        );
      });
    });

    it("allows remote-only users to transcribe uploaded audio", async () => {
      const user = userEvent.setup();
      mockSettings.current_model = "";
      mockSettings.current_model_engine = "whisper";

      vi.mocked(open).mockResolvedValue("/audio/remote-file.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return { models: [{ ...readyLocalModel, downloaded: false }] };
        }
        if (cmd === "get_active_remote_server") {
          return "remote-1";
        }
        if (cmd === "get_recognition_availability_snapshot") {
          return {
            whisper_available: false,
            parakeet_available: false,
            cloud_selected: false,
            cloud_ready: false,
            remote_selected: true,
            remote_available: true,
          };
        }
        if (cmd === "transcribe_audio_file") {
          return { text: "Remote transcript", words: null };
        }
        return null;
      });

      render(<AudioUploadSection />);

      const selectButton = await screen.findByRole("button", { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/remote-file.mp3/));

      const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
      await user.click(transcribeButton);

      await waitFor(() => {
        expect(screen.getByText("Remote transcript")).toBeInTheDocument();
      });

      expect(toast.error).not.toHaveBeenCalledWith(
        "Select a speech model in Models before transcribing.",
      );
      expect(invoke).toHaveBeenCalledWith("transcribe_audio_file", {
        filePath: "/audio/remote-file.mp3",
        modelName: "",
        modelEngine: null,
      });
    });

    it("saves uploaded remote transcriptions with the active remote label", async () => {
      const user = userEvent.setup();
      mockSettings.current_model = "base.en";
      mockSettings.current_model_engine = "whisper";

      vi.mocked(open).mockResolvedValue("/audio/remote-history.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return { models: [readyLocalModel] };
        }
        if (cmd === "get_active_remote_server") {
          return "remote-1";
        }
        if (cmd === "list_remote_servers") {
          return [{ id: "remote-1", name: "Office Mac", host: "10.0.0.7", port: 47842 }];
        }
        if (cmd === "get_recognition_availability_snapshot") {
          return {
            whisper_available: false,
            parakeet_available: false,
            cloud_selected: false,
            cloud_ready: false,
            remote_selected: true,
            remote_available: true,
          };
        }
        if (cmd === "transcribe_audio_file") {
          return { text: "Remote history transcript", words: null };
        }
        return null;
      });

      render(<AudioUploadSection />);

      const selectButton = await screen.findByRole("button", { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/remote-history.mp3/));

      const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
      await user.click(transcribeButton);

      await waitFor(() => {
        expect(screen.getByText("Remote history transcript")).toBeInTheDocument();
      });

      expect(invoke).toHaveBeenCalledWith("save_transcription", {
        text: "Remote history transcript",
        model: "Remote: Office Mac",
      });
    });

    it("labels Soniox upload history entries as cloud sources", async () => {
      const user = userEvent.setup();
      mockSettings.current_model = "soniox";
      mockSettings.current_model_engine = "soniox";

      vi.mocked(open).mockResolvedValue("/audio/soniox-file.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return {
            models: [
              {
                name: "soniox",
                display_name: "Soniox",
                size: 0,
                url: "",
                sha256: "",
                downloaded: true,
                speed_score: 9,
                accuracy_score: 10,
                recommended: true,
                engine: "soniox",
                kind: "cloud",
                requires_setup: false,
              },
            ],
          };
        }
        if (cmd === "get_active_remote_server") {
          return null;
        }
        if (cmd === "get_recognition_availability_snapshot") {
          return {
            whisper_available: false,
            parakeet_available: false,
            cloud_selected: true,
            cloud_ready: true,
            remote_selected: false,
            remote_available: false,
          };
        }
        if (cmd === "transcribe_audio_file") {
          return { text: "Soniox transcript", words: null };
        }
        return null;
      });

      render(<AudioUploadSection />);

      const selectButton = await screen.findByRole("button", { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/soniox-file.mp3/));

      const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
      await user.click(transcribeButton);

      await waitFor(() => {
        expect(screen.getByText("Soniox transcript")).toBeInTheDocument();
      });

      expect(invoke).toHaveBeenCalledWith("save_transcription", {
        text: "Soniox transcript",
        model: "Soniox (Cloud)",
      });
    });

    it("labels Deepgram upload history entries as cloud sources", async () => {
      const user = userEvent.setup();
      mockSettings.current_model = "deepgram";
      mockSettings.current_model_engine = "deepgram";

      vi.mocked(open).mockResolvedValue("/audio/deepgram-file.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return {
            models: [
              {
                name: "deepgram",
                display_name: "Deepgram",
                size: 0,
                url: "",
                sha256: "",
                downloaded: true,
                speed_score: 8,
                accuracy_score: 9,
                recommended: false,
                engine: "deepgram",
                kind: "cloud",
                requires_setup: false,
              },
            ],
          };
        }
        if (cmd === "get_active_remote_server") {
          return null;
        }
        if (cmd === "get_recognition_availability_snapshot") {
          return {
            whisper_available: false,
            parakeet_available: false,
            cloud_selected: true,
            cloud_ready: true,
            remote_selected: false,
            remote_available: false,
          };
        }
        if (cmd === "transcribe_audio_file") {
          return { text: "Deepgram transcript", words: null };
        }
        return null;
      });

      render(<AudioUploadSection />);

      const selectButton = await screen.findByRole("button", { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/deepgram-file.mp3/));

      const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
      await user.click(transcribeButton);

      await waitFor(() => {
        expect(screen.getByText("Deepgram transcript")).toBeInTheDocument();
      });

      expect(invoke).toHaveBeenCalledWith("save_transcription", {
        text: "Deepgram transcript",
        model: "Deepgram (Cloud)",
      });
    });

    it("still blocks missing model when no remote server is active", async () => {
      const user = userEvent.setup();
      mockSettings.current_model = "";
      mockSettings.current_model_engine = "whisper";

      vi.mocked(open).mockResolvedValue("/audio/file.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return { models: [{ ...readyLocalModel, downloaded: false }] };
        }
        if (cmd === "get_active_remote_server") {
          return null;
        }
        return null;
      });

      render(<AudioUploadSection />);

      const selectButton = await screen.findByRole("button", { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/file.mp3/));

      const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
      await user.click(transcribeButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith(
          "Select a speech model in Models before transcribing.",
        );
      });
    });

    it("rejects unsupported file types immediately", async () => {
      const user = userEvent.setup();

      // Mock selecting a PDF file
      vi.mocked(open).mockResolvedValue("/document.pdf");

      render(<AudioUploadSection />);

      // Try to select unsupported file
      await user.click(screen.getByRole("button", { name: /select file/i }));

      // Should either:
      // 1. Not show the file (filtered by dialog)
      // 2. Show error message
      // The actual behavior depends on implementation

      // For now, we'll verify the file dialog was called with correct filters
      expect(open).toHaveBeenCalledWith(
        expect.objectContaining({
          multiple: false,
          filters: expect.arrayContaining([
            expect.objectContaining({
              name: "Audio/Video Files",
              extensions: expect.arrayContaining([
                "wav",
                "mp3",
                "m4a",
                "flac",
                "ogg",
                "mp4",
                "webm",
              ]),
            }),
          ]),
        }),
      );
    });
  });

  describe("UI State Management", () => {
    it("shows loading state during transcription", async () => {
      const user = userEvent.setup();

      vi.mocked(open).mockResolvedValue("/audio/file.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return { models: [readyLocalModel] };
        }
        if (cmd === "transcribe_audio_file") {
          // Simulate processing time
          await new Promise((resolve) => setTimeout(resolve, 100));
          return { text: "Transcription result", words: null };
        }
        return null;
      });

      render(<AudioUploadSection />);

      // Select file
      const selectButton = await screen.findByRole("button", { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/file.mp3/));

      // Start transcription
      const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
      await user.click(transcribeButton);

      // Loading state appears (button text shows Processing...)
      expect(screen.getAllByText(/processing/i).length).toBeGreaterThan(0);

      // Result appears
      await waitFor(() => {
        expect(screen.getByText("Transcription result")).toBeInTheDocument();
      });
    });

    it("keeps the original file when a native drop arrives during processing", async () => {
      const user = userEvent.setup();
      type NativeListener = (event: { payload: unknown }) => void;
      const listeners = new Map<string, NativeListener>();
      let resolveTranscription: ((value: { text: string; words: null }) => void) | undefined;
      const transcription = new Promise<{ text: string; words: null }>((resolve) => {
        resolveTranscription = resolve;
      });

      vi.mocked(listen).mockImplementation(async (eventName, handler) => {
        listeners.set(eventName, handler as unknown as NativeListener);
        return () => {};
      });
      vi.mocked(open).mockResolvedValue("/audio/meeting-a.mp3");
      vi.mocked(save).mockResolvedValue("/exports/meeting-a.txt");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") return { models: [readyLocalModel] };
        if (cmd === "get_active_remote_server") return null;
        if (cmd === "get_recognition_availability_snapshot") {
          return { whisper_available: true, remote_available: false };
        }
        if (cmd === "list_remote_servers") return [];
        if (cmd === "transcribe_audio_file") return transcription;
        return null;
      });

      render(<AudioUploadSection />);

      await user.click(await screen.findByRole("button", { name: /select file/i }));
      await waitFor(() => screen.getByText(/meeting-a.mp3/));
      await user.click(await screen.findByRole("button", { name: /transcribe/i }));
      await waitFor(() => {
        expect(useUploadStore.getState().status).toBe("processing");
      });

      await act(async () => {
        listeners.get("tauri://drag-hover")?.({ payload: null });
        listeners.get("tauri://drag-drop")?.({
          payload: {
            paths: ["/audio/meeting-b.mp3"],
            position: { x: 0, y: 0 },
          },
        });
      });

      expect(useUploadStore.getState().selectedFile).toEqual({
        path: "/audio/meeting-a.mp3",
        name: "meeting-a.mp3",
      });
      expect(screen.queryByText(/meeting-b.mp3/)).not.toBeInTheDocument();
      useUploadStore.getState().clearSelection();
      expect(useUploadStore.getState().selectedFile?.path).toBe("/audio/meeting-a.mp3");

      resolveTranscription!({ text: "Transcript for A", words: null });
      await waitFor(() => {
        expect(screen.getByText("Transcript for A")).toBeInTheDocument();
      });

      expect(useUploadStore.getState().selectedFile?.path).toBe("/audio/meeting-a.mp3");
      expect(useUploadStore.getState().resultText).toBe("Transcript for A");
      expect(screen.getByText("meeting-a.mp3")).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: /save/i }));
      await waitFor(() => {
        expect(save).toHaveBeenCalledWith(
          expect.objectContaining({ defaultPath: "meeting-a.txt" }),
        );
      });
      expect(invoke).toHaveBeenCalledWith("save_transcript_file", {
        path: "/exports/meeting-a.txt",
        content: "Transcript for A",
      });
    });

    it("handles empty/silent audio gracefully", async () => {
      const user = userEvent.setup();

      vi.mocked(open).mockResolvedValue("/audio/silent.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return { models: [readyLocalModel] };
        }
        if (cmd === "transcribe_audio_file") {
          return { text: "[BLANK_AUDIO]", words: null };
        }
        return null;
      });

      render(<AudioUploadSection />);

      // Select and transcribe
      const selectButton = await screen.findByRole("button", { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/silent.mp3/));

      const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
      await user.click(transcribeButton);

      // User sees appropriate inline message
      await waitFor(() => {
        expect(screen.getByText("No speech detected in the audio file")).toBeInTheDocument();
      });
    });
  });

  it("blocks upload transcription when the selected remote is unavailable", async () => {
    const user = userEvent.setup();
    mockSettings.current_model = "";
    mockSettings.current_model_engine = "whisper";

    vi.mocked(open).mockResolvedValue("/audio/offline-remote.mp3");
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "get_model_status") {
        return { models: [{ ...readyLocalModel, downloaded: false }] };
      }
      if (cmd === "get_active_remote_server") {
        return "remote-1";
      }
      if (cmd === "get_recognition_availability_snapshot") {
        return {
          whisper_available: false,
          parakeet_available: false,
          cloud_selected: false,
          cloud_ready: false,
          remote_selected: true,
          remote_available: false,
        };
      }
      return null;
    });

    render(<AudioUploadSection />);

    const selectButton = await screen.findByRole("button", { name: /select file/i });
    await user.click(selectButton);
    await waitFor(() => screen.getByText(/offline-remote.mp3/));

    const transcribeButton = await screen.findByRole("button", { name: /transcribe/i });
    await user.click(transcribeButton);

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith(
        "Selected remote unavailable. Reconnect or choose another source.",
      );
    });
    expect(invoke).not.toHaveBeenCalledWith("transcribe_audio_file", expect.anything());
  });

  describe("Cloud diarization", () => {
    it("sets diarized=true and preserves multi-line text when words are returned", async () => {
      const user = userEvent.setup();
      mockSettings.current_model = "deepgram";
      mockSettings.current_model_engine = "deepgram";

      vi.mocked(open).mockResolvedValue("/audio/meeting.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") {
          return {
            models: [
              {
                name: "deepgram",
                display_name: "Deepgram",
                size: 0,
                url: "",
                sha256: "",
                downloaded: true,
                speed_score: 8,
                accuracy_score: 9,
                recommended: false,
                engine: "deepgram",
                kind: "cloud" as const,
                requires_setup: false,
              },
            ],
          };
        }
        if (cmd === "get_active_remote_server") return null;
        if (cmd === "get_recognition_availability_snapshot") {
          return { cloud_selected: true, cloud_ready: true, remote_available: false };
        }
        if (cmd === "transcribe_audio_file") {
          return {
            text: "Speaker 0: Hello there.\n\nSpeaker 1: Hi, how are you?",
            words: [
              { text: "Hello", start_ms: 0, end_ms: 500, speaker_id: "0", confidence: 0.99 },
              { text: "there", start_ms: 500, end_ms: 900, speaker_id: "0", confidence: 0.98 },
              { text: "Hi", start_ms: 1200, end_ms: 1500, speaker_id: "1", confidence: 0.97 },
            ],
          };
        }
        return null;
      });

      render(<AudioUploadSection />);

      await user.click(await screen.findByRole("button", { name: /select file/i }));
      await waitFor(() => screen.getByText(/meeting.mp3/));
      await user.click(await screen.findByRole("button", { name: /transcribe/i }));

      await waitFor(() => {
        expect(screen.getByText(/Speaker 0: Hello there/)).toBeInTheDocument();
      });

      // diarized flag set in store
      expect(useUploadStore.getState().diarized).toBe(true);

      // multi-line text preserved
      expect(useUploadStore.getState().resultText).toBe(
        "Speaker 0: Hello there.\n\nSpeaker 1: Hi, how are you?",
      );

      // diarization hint visible
      expect(screen.getByText(/Speaker labels via Deepgram \/ Soniox/i)).toBeInTheDocument();
    });

    it("sets diarized=false and shows no hint when words is null", async () => {
      const user = userEvent.setup();

      vi.mocked(open).mockResolvedValue("/audio/plain.mp3");
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === "get_model_status") return { models: [readyLocalModel] };
        if (cmd === "get_active_remote_server") return null;
        if (cmd === "transcribe_audio_file") {
          return { text: "Plain transcript without diarization", words: null };
        }
        return null;
      });

      render(<AudioUploadSection />);

      await user.click(await screen.findByRole("button", { name: /select file/i }));
      await waitFor(() => screen.getByText(/plain.mp3/));
      await user.click(await screen.findByRole("button", { name: /transcribe/i }));

      await waitFor(() => {
        expect(screen.getByText("Plain transcript without diarization")).toBeInTheDocument();
      });

      // diarized flag NOT set
      expect(useUploadStore.getState().diarized).toBe(false);

      // no hint rendered
      expect(screen.queryByText(/Speaker labels via Deepgram \/ Soniox/i)).not.toBeInTheDocument();
    });
  });
});
