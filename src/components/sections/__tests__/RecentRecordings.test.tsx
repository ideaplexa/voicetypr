import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RecentRecordings } from "../RecentRecordings";
import type { TranscriptionHistory } from "@/types";

const invokeMock = vi.fn();

const mockSettings: {
  current_model: string;
  current_model_engine: "whisper" | "parakeet" | "soniox";
} = {
  current_model: "small.en",
  current_model_engine: "whisper",
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  ask: vi.fn(async () => true),
}));

vi.mock("@/contexts/ReadinessContext", () => ({
  useCanRecord: () => true,
  useReadiness: () => ({
    canRecord: true,
    licenseStatus: "licensed",
    hasModels: true,
    selectedModelAvailable: true,
    remoteSelected: false,
    hasMicrophonePermission: true,
  }),
  useCanAutoInsert: () => true,
}));

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: mockSettings,
  }),
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

const historyItem: TranscriptionHistory = {
  id: "2024-01-01T00:00:00Z",
  text: "Original transcript",
  timestamp: new Date("2024-01-01T00:00:00Z"),
  model: "base.en",
  recording_file: "sample.wav",
};

const createDeferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });

  return { promise, resolve };
};

describe("RecentRecordings re-transcription", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSettings.current_model = "small.en";
    mockSettings.current_model_engine = "whisper";
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "check_recording_exists":
          return true;
        case "get_model_status":
          return {
            models: [{ name: "small.en", downloaded: true, engine: "whisper" }],
          };
        case "list_remote_servers":
          return [];
        case "get_active_remote_server":
          return null;
        default:
          return null;
      }
    });
  });

  it("uses the active remote server when re-transcribing", async () => {
    const user = userEvent.setup();

    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "check_recording_exists":
          return true;
        case "get_active_remote_server":
          return "online-server";
        case "list_remote_servers":
          return [
            {
              id: "online-server",
              name: "Office PC",
              host: "10.0.0.4",
              port: 47842,
              model: "large-v3",
            },
          ];
        case "get_recordings_directory":
          return "/recordings";
        case "save_retranscription":
          return "retry-remote";
        case "transcribe_remote":
          return "Remote retry text";
        default:
          return null;
      }
    });

    render(<RecentRecordings history={[historyItem]} onHistoryUpdate={vi.fn()} />);

    const retranscribeButton = await screen.findByTitle("Re-transcribe with current source");
    await user.click(retranscribeButton);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("save_retranscription", {
        text: "In progress...",
        model: "Remote: Office PC",
        recordingFile: "sample.wav",
        sourceRecordingId: "2024-01-01T00:00:00Z",
        status: "in_progress",
      });
      expect(invokeMock).toHaveBeenCalledWith("transcribe_remote", {
        serverId: "online-server",
        audioPath: "/recordings/sample.wav",
      });
      expect(invokeMock).toHaveBeenCalledWith("update_transcription", {
        timestamp: "retry-remote",
        text: "Remote retry text",
        model: "Remote: Office PC",
        status: "completed",
      });
    });
  });

  it("creates a durable in-progress entry before re-transcribing", async () => {
    const user = userEvent.setup();
    const onHistoryUpdate = vi.fn();
    const transcribeDeferred = createDeferred<{ text: string; words: null }>();

    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "check_recording_exists":
          return true;
        case "get_model_status":
          return {
            models: [{ name: "small.en", downloaded: true, engine: "whisper" }],
          };
        case "list_remote_servers":
          return [];
        case "get_recordings_directory":
          return "/recordings";
        case "save_retranscription":
          return "retry-1";
        case "transcribe_audio_file":
          return transcribeDeferred.promise;
        default:
          return null;
      }
    });

    render(<RecentRecordings history={[historyItem]} onHistoryUpdate={onHistoryUpdate} />);

    const retranscribeButton = await screen.findByTitle("Re-transcribe with current source");
    await user.click(retranscribeButton);

    await waitFor(() => {
      expect(screen.getByTitle("Re-transcribe with current source")).toBeDisabled();
      expect(screen.getByText("Re-transcribing with Small (English)...")).toBeInTheDocument();
      expect(invokeMock).toHaveBeenCalledWith("save_retranscription", {
        text: "In progress...",
        model: "Small (English)",
        recordingFile: "sample.wav",
        sourceRecordingId: "2024-01-01T00:00:00Z",
        status: "in_progress",
      });
    });

    transcribeDeferred.resolve({ text: "Re-transcribed text", words: null });

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_transcription", {
        timestamp: "retry-1",
        text: "Re-transcribed text",
        model: "small.en",
        status: "completed",
      });
    });
    await waitFor(() => {
      expect(onHistoryUpdate).toHaveBeenCalled();
    });
  });

  it("marks the pending retry as failed when re-transcription errors", async () => {
    const user = userEvent.setup();
    const onHistoryUpdate = vi.fn();

    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "check_recording_exists":
          return true;
        case "get_model_status":
          return {
            models: [{ name: "small.en", downloaded: true, engine: "whisper" }],
          };
        case "list_remote_servers":
          return [];
        case "get_recordings_directory":
          return "/recordings";
        case "save_retranscription":
          return "retry-2";
        case "transcribe_audio_file":
          throw new Error("remote offline");
        default:
          return null;
      }
    });

    render(<RecentRecordings history={[historyItem]} onHistoryUpdate={onHistoryUpdate} />);

    const retranscribeButton = await screen.findByTitle("Re-transcribe with current source");
    await user.click(retranscribeButton);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_transcription", {
        timestamp: "retry-2",
        text: "Re-transcription failed: Error: remote offline",
        model: "Small (English)",
        status: "failed",
      });
    });

    expect(onHistoryUpdate).toHaveBeenCalled();
  });

  it("keeps a loaded persisted in-progress row blocked until backend reconciliation", async () => {
    render(
      <RecentRecordings
        history={[{ ...historyItem, status: "in_progress", text: "Still retrying" }]}
        onHistoryUpdate={vi.fn()}
      />,
    );

    const retranscribeButton = await screen.findByTitle("Re-transcribe with current source");

    expect(retranscribeButton).toBeDisabled();
    expect(
      screen.getByText("Re-transcription in progress with Base (English)..."),
    ).toBeInTheDocument();
  });

  it("keeps reconciled failed rows retryable after reload", async () => {
    render(
      <RecentRecordings
        history={[{ ...historyItem, status: "failed", text: "Recovered after restart" }]}
        onHistoryUpdate={vi.fn()}
      />,
    );

    const retranscribeButton = await screen.findByTitle("Re-transcribe with current source");

    expect(retranscribeButton).toBeEnabled();
  });
  it("shows neutral failed copy when the recording is unavailable for retry", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "check_recording_exists":
          return false;
        case "get_model_status":
          return {
            models: [{ name: "small.en", downloaded: true, engine: "whisper" }],
          };
        case "list_remote_servers":
          return [];
        default:
          return null;
      }
    });

    render(
      <RecentRecordings
        history={[{ ...historyItem, status: "failed", text: "Recovered after restart" }]}
        onHistoryUpdate={vi.fn()}
      />,
    );

    expect(
      await screen.findByText("Transcription failed - recording unavailable for retry"),
    ).toBeInTheDocument();
    expect(screen.queryByTitle("Re-transcribe with current source")).not.toBeInTheDocument();
  });
});

it("uses Soniox when it is the current cloud transcription source", async () => {
  const user = userEvent.setup();
  mockSettings.current_model = "soniox";
  mockSettings.current_model_engine = "soniox";

  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "check_recording_exists":
        return true;
      case "get_active_remote_server":
        return null;
      case "get_recordings_directory":
        return "/recordings";
      case "save_retranscription":
        return "retry-soniox";
      case "transcribe_audio_file":
        return { text: "Cloud retry text", words: null };
      default:
        return null;
    }
  });

  render(<RecentRecordings history={[historyItem]} onHistoryUpdate={vi.fn()} />);

  const retranscribeButton = await screen.findByTitle("Re-transcribe with current source");
  await user.click(retranscribeButton);

  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("save_retranscription", {
      text: "In progress...",
      model: "Soniox (Cloud)",
      recordingFile: "sample.wav",
      sourceRecordingId: "2024-01-01T00:00:00Z",
      status: "in_progress",
    });
    expect(invokeMock).toHaveBeenCalledWith("transcribe_audio_file", {
      filePath: "/recordings/sample.wav",
      modelName: "soniox",
      modelEngine: "soniox",
    });
    expect(invokeMock).toHaveBeenCalledWith("update_transcription", {
      timestamp: "retry-soniox",
      text: "Cloud retry text",
      model: "Soniox (Cloud)",
      status: "completed",
    });
  });
});

// ---------------------------------------------------------------------------
// Before/after original text toggle
// ---------------------------------------------------------------------------

describe("before polish detail", () => {
  const defaultInvoke = async (cmd: string) => {
    if (cmd === "check_recording_exists") return false;
    if (cmd === "get_active_remote_server") return null;
    return null;
  };

  beforeEach(() => {
    vi.clearAllMocks();
    invokeMock.mockImplementation(defaultInvoke);
  });

  it("shows the original when ai_applied and original_text differs from text", async () => {
    const item: TranscriptionHistory = {
      id: "toggle-1",
      text: "AI formatted text",
      timestamp: new Date("2024-01-01T00:00:00Z"),
      model: "base.en",
      writing: {
        ai_applied: true,
        original_text: "raw transcript before AI",
      },
    };

    render(<RecentRecordings history={[item]} onHistoryUpdate={vi.fn()} />);

    expect(screen.getByRole("region", { name: "Dictation detail" })).toHaveTextContent(
      "AI formatted text",
    );
    expect(screen.getByRole("region", { name: "Before polish" })).toHaveTextContent(
      "raw transcript before AI",
    );
  });

  it("changes the displayed original when selection changes", async () => {
    const user = userEvent.setup();
    const item: TranscriptionHistory = {
      id: "toggle-2",
      text: "AI formatted text",
      timestamp: new Date("2024-01-01T00:00:00Z"),
      model: "base.en",
      writing: {
        ai_applied: true,
        original_text: "raw transcript before AI",
      },
    };

    const other = { ...item, id: "other", text: "Second dictation", writing: undefined };
    render(<RecentRecordings history={[item, other]} onHistoryUpdate={vi.fn()} />);
    expect(screen.getByRole("region", { name: "Before polish" })).toHaveTextContent(
      "raw transcript before AI",
    );
    await user.click(screen.getByRole("button", { name: /Second dictation/ }));
    expect(screen.getByRole("region", { name: "Dictation detail" })).toHaveTextContent(
      "Second dictation",
    );
    expect(screen.queryByRole("region", { name: "Before polish" })).not.toBeInTheDocument();
  });

  it("copy actions copy polished and original text separately", async () => {
    const user = userEvent.setup();
    const writeTextMock = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: writeTextMock },
    });

    const item: TranscriptionHistory = {
      id: "toggle-3",
      text: "AI formatted text",
      timestamp: new Date("2024-01-01T00:00:00Z"),
      model: "base.en",
      writing: {
        ai_applied: true,
        original_text: "raw transcript before AI",
      },
    };

    render(<RecentRecordings history={[item]} onHistoryUpdate={vi.fn()} />);

    // Default: copy copies the polished text
    await user.click(await screen.findByTitle("Copy"));
    expect(writeTextMock).toHaveBeenLastCalledWith("AI formatted text");

    // The original block has its own copy action.
    await user.click(screen.getByTitle("Copy original transcript"));
    expect(writeTextMock).toHaveBeenLastCalledWith("raw transcript before AI");

    // Detail copy still copies the polished text.
    await user.click(screen.getByTitle("Copy"));
    expect(writeTextMock).toHaveBeenLastCalledWith("AI formatted text");
  });

  it("does not show toggle when original_text is absent", async () => {
    const item: TranscriptionHistory = {
      id: "toggle-4",
      text: "Formatted text",
      timestamp: new Date("2024-01-01T00:00:00Z"),
      model: "base.en",
      writing: { ai_applied: true },
    };

    render(<RecentRecordings history={[item]} onHistoryUpdate={vi.fn()} />);

    expect(screen.getByRole("region", { name: "Dictation detail" })).toHaveTextContent(
      "Formatted text",
    );
    expect(screen.queryByRole("region", { name: "Before polish" })).not.toBeInTheDocument();
  });

  it("does not show toggle when original_text equals text (AI made no change)", async () => {
    const item: TranscriptionHistory = {
      id: "toggle-5",
      text: "Same text",
      timestamp: new Date("2024-01-01T00:00:00Z"),
      model: "base.en",
      writing: {
        ai_applied: true,
        original_text: "Same text",
      },
    };

    render(<RecentRecordings history={[item]} onHistoryUpdate={vi.fn()} />);

    expect(screen.getByRole("region", { name: "Dictation detail" })).toHaveTextContent("Same text");
    expect(screen.queryByRole("region", { name: "Before polish" })).not.toBeInTheDocument();
  });

  it("shows a stored original even when the legacy ai_applied flag is absent", async () => {
    const item: TranscriptionHistory = {
      id: "toggle-6",
      text: "Plain text",
      timestamp: new Date("2024-01-01T00:00:00Z"),
      model: "base.en",
      writing: {
        original_text: "raw text",
      },
    };

    render(<RecentRecordings history={[item]} onHistoryUpdate={vi.fn()} />);

    expect(screen.getByRole("region", { name: "Dictation detail" })).toHaveTextContent(
      "Plain text",
    );
    expect(screen.getByRole("region", { name: "Before polish" })).toHaveTextContent("raw text");
  });
});

describe("history load states", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "check_recording_exists") return false;
      if (cmd === "get_active_remote_server") return null;
      return undefined;
    });
  });

  it("shows skeleton rows while loading with empty history", () => {
    render(<RecentRecordings history={[]} isLoading onHistoryUpdate={vi.fn()} />);

    const skeleton = document.querySelector("[aria-hidden]");
    expect(skeleton).toBeInTheDocument();
    expect(screen.queryByText("No recordings yet")).not.toBeInTheDocument();
    expect(screen.queryByText("Couldn't load your history.")).not.toBeInTheDocument();
  });

  it("shows an error banner with retry when the initial load failed", async () => {
    const user = userEvent.setup();
    const onHistoryUpdate = vi.fn();
    render(
      <RecentRecordings
        history={[]}
        loadError="Couldn't load history"
        onHistoryUpdate={onHistoryUpdate}
      />,
    );

    expect(await screen.findByText("Couldn't load your history.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /retry/i })).toBeInTheDocument();
    expect(screen.queryByText("No recordings yet")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /retry/i }));
    expect(onHistoryUpdate).toHaveBeenCalledTimes(1);
  });

  it("renders the list without the error banner when history has data despite a prior error", () => {
    render(
      <RecentRecordings
        history={[historyItem]}
        loadError="Couldn't load history"
        onHistoryUpdate={vi.fn()}
      />,
    );

    expect(screen.getByRole("region", { name: "Dictation detail" })).toHaveTextContent(
      "Original transcript",
    );
    expect(screen.queryByText("Couldn't load your history.")).not.toBeInTheDocument();
  });
});

describe("application context badge", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_application_icon") {
        return "data:image/png;base64,aWNvbg==";
      }
      if (cmd === "get_active_remote_server") return null;
      return null;
    });
  });

  it("shows the target app with its icon and hides the internal Other category", async () => {
    const item: TranscriptionHistory = {
      id: "app-context-1",
      text: "Ghostty recording",
      timestamp: new Date("2024-01-01T00:00:00Z"),
      model: "base.en",
      writing: {
        context_hint: {
          app_name: "Ghostty",
          process_path: "/Applications/Ghostty.app",
          category: "other",
        },
      },
    };

    render(<RecentRecordings history={[item]} onHistoryUpdate={vi.fn()} />);

    const badge = await screen.findByLabelText("Application: Ghostty");
    await waitFor(() => {
      expect(badge.querySelector("img")).toHaveAttribute("src", "data:image/png;base64,aWNvbg==");
    });
    expect(badge.parentElement).toHaveTextContent("Ghostty");
    expect(screen.queryByText("other", { exact: true })).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("get_application_icon", {
      processPath: "/Applications/Ghostty.app",
    });
  });
});

describe("History split view", () => {
  const first: TranscriptionHistory = {
    id: "first",
    text: "First final dictation",
    timestamp: new Date("2026-09-29T10:42:00Z"),
    model: "parakeet-v3",
    writing: {
      source: "desktop_recording",
      context_hint: { app_name: "Slack" },
      original_text: "first raw dictation",
      mode: "Message",
    },
  };
  const second: TranscriptionHistory = {
    id: "second",
    text: "Second final dictation",
    timestamp: new Date("2026-09-29T10:18:00Z"),
    model: "base.en",
    writing: { source: "desktop_recording", context_hint: { app_name: "Notes" } },
  };

  beforeEach(() => {
    vi.clearAllMocks();
    invokeMock.mockResolvedValue(null);
  });

  it("selects dictations by click and arrow keys", async () => {
    const user = userEvent.setup();
    render(<RecentRecordings history={[first, second]} />);
    const firstButton = screen.getByRole("button", { name: /First final dictation/ });
    const secondButton = screen.getByRole("button", { name: /Second final dictation/ });
    expect(firstButton).toHaveAttribute("aria-current", "true");
    await user.click(secondButton);
    expect(secondButton).toHaveAttribute("aria-current", "true");
    await user.keyboard("{ArrowUp}");
    expect(firstButton).toHaveFocus();
    expect(firstButton).toHaveAttribute("aria-current", "true");
    await user.keyboard("{ArrowDown}");
    expect(secondButton).toHaveAttribute("aria-current", "true");
    await user.keyboard("{Enter}");
    expect(screen.getByRole("region", { name: "Dictation detail" })).toHaveTextContent(
      "Second final dictation",
    );
    await user.keyboard("{Space}");
    expect(secondButton).toHaveAttribute("aria-current", "true");
  });

  it("filters the list and clears a filtered-out selection", async () => {
    const user = userEvent.setup();
    render(<RecentRecordings history={[first, second]} />);
    await user.type(screen.getByRole("textbox", { name: "Search dictations" }), "Second");
    expect(screen.queryByRole("button", { name: /First final dictation/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Dictation detail" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Second final dictation/ }));
    expect(screen.getByRole("region", { name: "Dictation detail" })).toHaveTextContent(
      "Second final dictation",
    );
    await user.click(screen.getByRole("button", { name: "Clear search" }));
    expect(screen.getByRole("button", { name: /First final dictation/ })).toBeInTheDocument();
  });

  it("deletes the selected item and moves to the next item", async () => {
    const user = userEvent.setup();
    const onHistoryUpdate = vi.fn();
    const view = render(
      <RecentRecordings history={[first, second]} onHistoryUpdate={onHistoryUpdate} />,
    );
    await user.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_transcription_entry", { timestamp: "first" }),
    );
    view.rerender(<RecentRecordings history={[second]} onHistoryUpdate={onHistoryUpdate} />);
    expect(screen.getByRole("button", { name: /Second final dictation/ })).toHaveAttribute(
      "aria-current",
      "true",
    );
    expect(screen.getByRole("region", { name: "Dictation detail" })).toHaveTextContent(
      "Second final dictation",
    );
  });

  it("copies final text while showing the stored original separately", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    render(<RecentRecordings history={[first, second]} />);
    expect(screen.getByRole("region", { name: "Before polish" })).toHaveTextContent(
      "first raw dictation",
    );
    await user.click(screen.getByRole("button", { name: /^Copy$/ }));
    expect(writeText).toHaveBeenCalledWith("First final dictation");
    await user.click(screen.getByRole("button", { name: /Second final dictation/ }));
    expect(screen.queryByRole("region", { name: "Before polish" })).not.toBeInTheDocument();
  });

  it("shows both empty states with the upload entry point", async () => {
    const user = userEvent.setup();
    const upload = vi.fn();
    const view = render(
      <RecentRecordings history={[]} onTranscribeFile={upload} hotkey="Cmd+Shift+Space" />,
    );
    expect(screen.getByText(/Your dictations will show up here/)).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Transcribe a file…" })).toHaveLength(2);
    await user.click(screen.getAllByRole("button", { name: "Transcribe a file…" })[1]);
    expect(upload).toHaveBeenCalledTimes(1);
    view.rerender(<RecentRecordings history={[first]} onTranscribeFile={upload} />);
    await user.type(screen.getByRole("textbox", { name: "Search dictations" }), "missing phrase");
    expect(screen.getByText("No dictations match")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Transcribe a file…" })).toHaveLength(2);
  });

  it("selects a newly completed file upload from the existing history flow", () => {
    const view = render(<RecentRecordings history={[first]} />);
    const upload: TranscriptionHistory = {
      id: "uploaded",
      text: "Transcript from the file",
      timestamp: new Date("2026-09-29T11:00:00Z"),
      model: "base.en",
      writing: { source: "audio_file" },
    };
    view.rerender(<RecentRecordings history={[upload, first]} />);
    expect(screen.getByRole("button", { name: /Transcript from the file/ })).toHaveAttribute(
      "aria-current",
      "true",
    );
    expect(screen.getByRole("region", { name: "Dictation detail" })).toHaveTextContent(
      "Transcript from the file",
    );
  });
});
