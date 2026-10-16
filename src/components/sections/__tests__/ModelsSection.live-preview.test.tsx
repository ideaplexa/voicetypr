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
  LanguageSelection: ({ supportedLanguages }: { supportedLanguages?: readonly string[] }) => (
    <div data-testid="language-selection">{supportedLanguages?.join(",")}</div>
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

describe("ModelsSection live preview mode", () => {
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
    mocks.invoke.mockImplementation(async (cmd) => {
      switch (cmd) {
        case "get_active_stream_capabilities":
          return mockCapabilities;
        case "activate_live_preview":
          if (activateLivePreviewError) {
            throw activateLivePreviewError;
          }
          return {};
        case "list_remote_servers":
          return [];
        case "get_active_remote_server":
          return null;
        case "discover_remote_servers":
          return [];
        default:
          return null;
      }
    });
  });

  it.each(["downloading", "verifying"] as const)(
    "keeps %s status in the model list, outside Spoken language",
    async (phase) => {
      const pendingModel: ModelInfo = {
        ...parakeetModel,
        name: "pending-model",
        display_name: "Pending model",
        downloaded: false,
      };
      render(
        <ModelsSection
          {...baseProps}
          models={[...baseProps.models, [pendingModel.name, pendingModel]]}
          downloadProgress={phase === "downloading" ? { [pendingModel.name]: 42 } : {}}
          verifyingModels={new Set(phase === "verifying" ? [pendingModel.name] : [])}
        />,
      );
      await screen.findByRole("switch", { name: "Live preview" });
      const language = screen.getByRole("heading", { name: "Spoken language" }).closest("section")!;
      expect(
        screen.getByText(phase === "downloading" ? "Downloading 42%" : "Verifying"),
      ).toBeInTheDocument();
      expect(within(language).queryByText(/Downloading|Verifying/)).not.toBeInTheDocument();
      expect(within(language).getByTestId("language-selection")).toBeInTheDocument();
    },
  );

  it("explains a missing CrispASR runtime instead of downloading the weights again", async () => {
    const model: ModelInfo = {
      ...parakeetModel,
      name: "confucius4-r2t2-q4_k",
      display_name: "R2T2 Q4_K",
      engine: "crispasr",
      requires_setup: true,
    };
    mockSettings.current_model = model.name;
    mockSettings.current_model_engine = "crispasr";
    render(
      <ModelsSection {...baseProps} currentModel={model.name} models={[[model.name, model]]} />,
    );
    expect(await screen.findByText("Runtime missing · reinstall app")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Download" })).not.toBeInTheDocument();
  });

  it("shows a disabled switch and reason when streaming is unavailable", async () => {
    renderSection();
    expect(await screen.findByRole("switch", { name: "Live preview" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    expect(screen.getByText("This engine cannot show words while you speak.")).toBeInTheDocument();
  });

  it("shows the live preview control when capabilities support streaming", async () => {
    mockCapabilities = makeCapabilities(true);
    renderSection();

    expect(await screen.findByRole("switch", { name: "Live preview" })).toBeEnabled();
    expect(screen.getByText("en,ja,vi")).toBeInTheDocument();
    expect(screen.queryByText(/English-only for now/i)).not.toBeInTheDocument();
  });

  it("activates live preview through the rollback-safe backend command", async () => {
    mockCapabilities = makeCapabilities(true);
    renderSection();

    await userEvent.click(await screen.findByRole("switch", { name: "Live preview" }));

    await waitFor(() => {
      expect(mocks.invoke).toHaveBeenCalledWith("activate_live_preview", undefined);
    });
    expect(mocks.updateSettings).not.toHaveBeenCalledWith({ transcription_mode: "live_preview" });
    expect(mocks.refreshSettings).toHaveBeenCalled();
    expect(toast.success).toHaveBeenCalledWith("Live preview enabled");
  });

  it("refreshes settings and shows an error when activation fails", async () => {
    mockCapabilities = makeCapabilities(true);
    activateLivePreviewError = new Error("warmup failed");
    renderSection();

    await userEvent.click(await screen.findByRole("switch", { name: "Live preview" }));

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

    await userEvent.click(await screen.findByRole("switch", { name: "Live preview" }));

    await waitFor(() => {
      expect(mocks.updateSettings).toHaveBeenCalledWith({ transcription_mode: "regular" });
    });
    expect(mocks.refreshSettings).toHaveBeenCalled();
  });
});
