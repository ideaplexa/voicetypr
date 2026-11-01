import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ModelsSection } from "../ModelsSection";
import type { ActiveStreamCapabilities, AppSettings, ModelInfo } from "@/types";
import { toast } from "sonner";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>(),
  listen: vi.fn<(_event: string, _handler: unknown) => Promise<() => void>>(() =>
    Promise.resolve(vi.fn()),
  ),
  updateSettings: vi.fn(() => Promise.resolve()),
  refreshSettings: vi.fn(() => Promise.resolve()),
}));

let mockSettings: AppSettings;
let mockCapabilities: ActiveStreamCapabilities;
let activateLivePreviewError: Error | null;
let polishOn = false;
let activeRemoteServer: string | null = null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => mocks.invoke(cmd, args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: unknown) => mocks.listen(event, handler),
}));

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: mockSettings,
    updateSettings: mocks.updateSettings,
    refreshSettings: mocks.refreshSettings,
  }),
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

vi.mock("@/components/ApiKeyModal", () => ({
  ApiKeyModal: () => null,
}));

vi.mock("@/components/LanguageSelection", () => ({
  LanguageSelection: ({ supportedLanguages, onValueChange }: {
    supportedLanguages?: readonly string[];
    onValueChange: (value: string) => void;
  }) => (
    <button data-testid="language-selection" onClick={() => onValueChange("ja")}>
      {supportedLanguages?.join(",")}
    </button>
  ),
}));

vi.mock("@/components/ModelCard", () => ({
  ModelCard: ({ name }: { name: string }) => <div>{name}</div>,
}));

vi.mock("@/components/RemoteServerCard", () => ({
  RemoteServerCard: () => <div data-testid="remote-server-card" />,
}));

vi.mock("../AddServerModal", () => ({
  AddServerModal: () => null,
}));

const parakeetModel: ModelInfo = {
  name: "parakeet-tdt-0.6b-v3",
  display_name: "Parakeet TDT 0.6B v3",
  engine: "parakeet",
  kind: "local",
  recommended: true,
  downloaded: true,
  requires_setup: false,
  speed_score: 9,
  accuracy_score: 9,
  size: 1200,
  url: "",
  sha256: "",
  supported_languages: ["en", "ja", "vi"],
};

const baseProps = {
  models: [["parakeet-tdt-0.6b-v3", parakeetModel]] as [string, ModelInfo][],
  downloadProgress: {},
  verifyingModels: new Set<string>(),
  currentModel: "parakeet-tdt-0.6b-v3",
  onDownload: vi.fn(),
  onDelete: vi.fn(),
  onCancelDownload: vi.fn(),
  onSelect: vi.fn(),
  refreshModels: vi.fn(() => Promise.resolve()),
};

function makeCapabilities(supportsStreaming: boolean): ActiveStreamCapabilities {
  return {
    active_engine: "parakeet",
    eou_model_downloaded: false,
    eou_model_path: null,
    eou_chunk_ms: 320,
    transcription_mode: mockSettings.transcription_mode ?? "regular",
    capabilities: {
      supports_streaming: supportsStreaming,
      supports_committed_prefix: supportsStreaming,
      supports_tentative_tail: supportsStreaming,
      supports_endpointing: supportsStreaming,
      final_only: !supportsStreaming,
    },
  };
}

function renderSection() {
  return render(<ModelsSection {...baseProps} />);
}

describe("ModelsSection per-model options", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSettings = {
      hotkey: "CommandOrControl+Shift+Space",
      current_model: "parakeet-tdt-0.6b-v3",
      current_model_engine: "parakeet",
      speech_language: "en",
      theme: "system",
      transcription_mode: "regular",
    };
    mockCapabilities = makeCapabilities(false);
    activateLivePreviewError = null;
    polishOn = false;
    activeRemoteServer = null;
    mocks.invoke.mockImplementation(async (cmd) => {
      switch (cmd) {
        case "get_active_stream_capabilities":
          return mockCapabilities;
        case "activate_live_preview":
          if (activateLivePreviewError) {
            throw activateLivePreviewError;
          }
          return {};
        case "get_ai_settings":
          return { enabled: polishOn };
        case "list_remote_servers":
          return [];
        case "get_active_remote_server":
          return activeRemoteServer;
        case "discover_remote_servers":
          return [];
        default:
          return null;
      }
    });
  });

  it("lets remote users choose a language without the inactive local model's English restriction", async () => {
    activeRemoteServer = "remote-1";
    mockSettings = { ...mockSettings, current_model: "small.en", current_model_engine: "whisper", speech_language: "ja" };
    renderSection();
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Another computer"));
    await userEvent.click(await screen.findByTestId("language-selection"));
    expect(mocks.updateSettings).toHaveBeenCalledWith({ speech_language: "ja" });
    expect(mocks.updateSettings).not.toHaveBeenCalledWith({ speech_language: "en" });
    expect(screen.queryByText("English only")).not.toBeInTheDocument();
  });

  it.each(["live_preview", "regular"] as const)("only enables Deepgram paragraphs with streaming off (%s)", async (mode) => {
    const deepgram: ModelInfo = { ...parakeetModel, name: "deepgram", engine: "deepgram", kind: "cloud" };
    mockSettings = { ...mockSettings, current_model: "deepgram", current_model_engine: "deepgram", transcription_mode: mode };
    mockCapabilities = makeCapabilities(true);
    render(<ModelsSection {...baseProps} models={[["deepgram", deepgram]]} currentModel="deepgram" />);
    const paragraphs = await screen.findByRole("switch", { name: "Paragraphs at long pauses" });
    if (mode === "live_preview") {
      expect(paragraphs).toHaveAttribute("aria-disabled", "true");
      expect(screen.getByText("Deepgram paragraphs need Real-time words off.")).toBeInTheDocument();
      await userEvent.click(paragraphs);
      expect(mocks.updateSettings).not.toHaveBeenCalled();
    } else {
      expect(paragraphs).not.toHaveAttribute("aria-disabled", "true");
      await userEvent.click(paragraphs);
      expect(mocks.updateSettings).toHaveBeenCalledWith({ transcript_paragraphs: false });
    }
  });

  it.each(["downloading", "verifying"] as const)("keeps %s status in the model list, outside the model options", async (phase) => {
    const pendingModel: ModelInfo = { ...parakeetModel, name: "pending-model", display_name: "Pending model", downloaded: false };
    render(<ModelsSection {...baseProps}
      models={[...baseProps.models, [pendingModel.name, pendingModel]]}
      downloadProgress={phase === "downloading" ? { [pendingModel.name]: 42 } : {}}
      verifyingModels={new Set(phase === "verifying" ? [pendingModel.name] : [])} />);
    await screen.findByTestId("model-options");
    const options = screen.getByTestId("model-options");
    expect(screen.getByText(phase === "downloading" ? "Downloading 42%" : "Verifying")).toBeInTheDocument();
    expect(within(options).queryByText(/Downloading|Verifying/)).not.toBeInTheDocument();
    expect(within(options).getByTestId("language-selection")).toBeInTheDocument();
  });

  it("removes the old bottom Spoken language and Live preview cards", async () => {
    renderSection();
    await screen.findByTestId("model-options");
    expect(screen.queryByRole("heading", { name: "Spoken language" })).not.toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Live preview" })).not.toBeInTheDocument();
  });

  it("hides Real-time words when the model does not stream", async () => {
    renderSection();
    await screen.findByTestId("model-options");
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("get_active_stream_capabilities", undefined));
    expect(screen.queryByRole("switch", { name: "Real-time words" })).not.toBeInTheDocument();
  });

  it("shows Real-time words under the selected model when it streams", async () => {
    mockCapabilities = makeCapabilities(true);
    renderSection();

    expect(await screen.findByRole("switch", { name: "Real-time words" })).toBeEnabled();
    expect(screen.getByText("en,ja,vi")).toBeInTheDocument();
    expect(screen.getByText("Transcribe while you talk — final text arrives faster")).toBeInTheDocument();
  });

  it("shows paragraph and filler options by engine and language", async () => {
    renderSection();
    expect(await screen.findByRole("switch", { name: "Paragraphs at long pauses" })).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Remove filler words" })).toBeInTheDocument();
  });

  it("hides filler removal for a non-English spoken language", async () => {
    mockSettings = { ...mockSettings, speech_language: "ja" };
    renderSection();
    await screen.findByRole("switch", { name: "Paragraphs at long pauses" });
    expect(screen.queryByRole("switch", { name: "Remove filler words" })).not.toBeInTheDocument();
  });

  it("hides paragraphs for the unified Parakeet model and shows the English only tag", async () => {
    const unified: ModelInfo = { ...parakeetModel, name: "parakeet-unified-640ms", supported_languages: ["en"] };
    mockSettings = { ...mockSettings, current_model: unified.name };
    render(<ModelsSection {...baseProps} models={[[unified.name, unified]]} currentModel={unified.name} />);
    await screen.findByTestId("model-options");
    expect(screen.getByText("English only")).toBeInTheDocument();
    expect(screen.queryByTestId("language-selection")).not.toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Paragraphs at long pauses" })).not.toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Remove filler words" })).toBeInTheDocument();
  });

  it("saves the paragraph and filler switches", async () => {
    renderSection();
    await userEvent.click(await screen.findByRole("switch", { name: "Remove filler words" }));
    expect(mocks.updateSettings).toHaveBeenCalledWith({ transcript_remove_fillers: true });
    await userEvent.click(screen.getByRole("switch", { name: "Paragraphs at long pauses" }));
    expect(mocks.updateSettings).toHaveBeenCalledWith({ transcript_paragraphs: false });
  });

  it("mentions Polish only when Polish is on", async () => {
    renderSection();
    await screen.findByTestId("model-options");
    expect(screen.queryByText("Polish already handles this.")).not.toBeInTheDocument();
  });

  it("shows the Polish note when Polish is on", async () => {
    polishOn = true;
    renderSection();
    expect(await screen.findByText("Polish already handles this.")).toBeInTheDocument();
  });

  it("activates live preview through the rollback-safe backend command", async () => {
    mockCapabilities = makeCapabilities(true);
    renderSection();

    await userEvent.click(await screen.findByRole("switch", { name: "Real-time words" }));

    await waitFor(() => {
      expect(mocks.invoke).toHaveBeenCalledWith("activate_live_preview", undefined);
    });
    expect(mocks.updateSettings).not.toHaveBeenCalledWith({ transcription_mode: "live_preview" });
    expect(mocks.refreshSettings).toHaveBeenCalled();
    expect(toast.success).toHaveBeenCalledWith("Real-time words on");
  });

  it("refreshes settings and shows an error when activation fails", async () => {
    mockCapabilities = makeCapabilities(true);
    activateLivePreviewError = new Error("warmup failed");
    renderSection();

    await userEvent.click(await screen.findByRole("switch", { name: "Real-time words" }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("warmup failed");
    });
    expect(mocks.refreshSettings).toHaveBeenCalled();
    expect(mocks.updateSettings).not.toHaveBeenCalledWith({ transcription_mode: "live_preview" });
  });

  it("persists regular mode directly", async () => {
    mockSettings = { ...mockSettings, transcription_mode: "live_preview" };
    mockCapabilities = makeCapabilities(true);
    renderSection();

    await userEvent.click(await screen.findByRole("switch", { name: "Real-time words" }));

    await waitFor(() => {
      expect(mocks.updateSettings).toHaveBeenCalledWith({ transcription_mode: "regular" });
    });
    expect(mocks.refreshSettings).toHaveBeenCalled();
  });
});
