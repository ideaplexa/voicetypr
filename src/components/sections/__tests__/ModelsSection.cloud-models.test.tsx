import { useState } from "react";
import type { SourceFilter } from "../models/types";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ModelsSection } from "../ModelsSection";
import type { CloudModelInfo, LocalModelInfo } from "@/types";

const updateSettings = vi.fn();
const refreshSettings = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: {
      current_model: "openai",
      current_model_engine: "openai",
      speech_language: "en",
    },
    updateSettings,
    refreshSettings,
  }),
}));

const openai: CloudModelInfo = {
  name: "openai",
  display_name: "OpenAI",
  size: 0,
  url: "",
  sha256: "",
  downloaded: true,
  speed_score: 7,
  accuracy_score: 9,
  recommended: false,
  engine: "openai",
  kind: "cloud",
  requires_setup: false,
  underlying_model: "gpt-transcribe",
  available_models: [
    { id: "gpt-transcribe", display_name: "GPT Transcribe" },
    {
      id: "gpt-4o-mini-transcribe",
      display_name: "GPT-4o mini Transcribe",
    },
  ],
};

const soniox: CloudModelInfo = {
  name: "soniox",
  display_name: "Soniox",
  size: 0,
  url: "",
  sha256: "",
  downloaded: true,
  speed_score: 8,
  accuracy_score: 9,
  recommended: true,
  engine: "soniox",
  kind: "cloud",
  requires_setup: false,
  underlying_model: "stt-async-v5",
  available_models: [{ id: "stt-async-v5", display_name: "Soniox v5" }],
};

const whisper: LocalModelInfo = {
  name: "tiny",
  display_name: "Whisper Tiny",
  engine: "whisper",
  kind: "local",
  recommended: false,
  downloaded: true,
  requires_setup: false,
  size: 0,
  url: "",
  sha256: "",
  speed_score: 9,
  accuracy_score: 5,
};

function ControlledSources({
  currentModel,
  initialFilter,
}: {
  currentModel: string;
  initialFilter?: SourceFilter;
}) {
  const [filter, setFilter] = useState<SourceFilter | undefined>(initialFilter);
  return (
    <ModelsSection
      models={[
        ["tiny", whisper],
        ["openai", openai],
        ["soniox", soniox],
      ]}
      currentModel={currentModel}
      sourceFilter={filter}
      onSourceFilterChange={setFilter}
      downloadProgress={{}}
      verifyingModels={new Set()}
      onDownload={vi.fn()}
      onDelete={vi.fn()}
      onCancelDownload={vi.fn()}
      onSelect={vi.fn()}
      refreshModels={vi.fn().mockResolvedValue(undefined)}
    />
  );
}

function renderModels(overrides: Partial<Parameters<typeof ModelsSection>[0]> = {}) {
  const props: Parameters<typeof ModelsSection>[0] = {
    models: [
      ["openai", openai],
      ["soniox", soniox],
    ],
    downloadProgress: {},
    verifyingModels: new Set(),
    currentModel: "openai",
    onDownload: vi.fn(),
    onDelete: vi.fn(),
    onCancelDownload: vi.fn(),
    onSelect: vi.fn(),
    refreshModels: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };

  render(<ModelsSection {...props} />);
  return props;
}

describe("ModelsSection cloud model labels", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listen).mockResolvedValue(vi.fn());
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === "list_remote_servers" || command === "discover_remote_servers") {
        return Promise.resolve([]);
      }
      if (command === "get_active_remote_server") return Promise.resolve(null);
      return Promise.resolve(undefined);
    });
  });

  it("opens the active Cloud source on ordinary first navigation without an explicit destination", async () => {
    const user = userEvent.setup();
    render(<ControlledSources currentModel="openai" />);
    expect(await screen.findByRole("heading", { name: "Soniox v5" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Cloud/ })).toHaveAttribute("aria-selected", "true");
    await user.click(screen.getByRole("tab", { name: /Local/ }));
    expect(screen.getByRole("tab", { name: /Local/ })).toHaveAttribute("aria-selected", "true");
  });

  it("preserves a controlled Cloud destination with a local active model without render-phase parent updates", async () => {
    const errorSpy = vi.spyOn(console, "error");
    try {
      render(<ControlledSources currentModel="tiny" initialFilter="cloud" />);
      expect(await screen.findByRole("heading", { name: "Soniox v5" })).toBeInTheDocument();
      expect(screen.getByRole("tab", { name: /Cloud/ })).toHaveAttribute("aria-selected", "true");
      expect(
        errorSpy.mock.calls.some((args) =>
          args.some((arg) => typeof arg === "string" && arg.includes("Cannot update a component")),
        ),
      ).toBe(false);
    } finally {
      errorSpy.mockRestore();
    }
  });

  it("follows later source changes once while preserving user browsing across unrelated renders", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<ControlledSources currentModel="tiny" />);
    await user.click(screen.getByRole("tab", { name: /Remote/ }));
    rerender(<ControlledSources currentModel="tiny" />);
    expect(screen.getByRole("tab", { name: /Remote/ })).toHaveAttribute("aria-selected", "true");

    rerender(<ControlledSources currentModel="openai" />);
    expect(screen.getByRole("tab", { name: /Cloud/ })).toHaveAttribute("aria-selected", "true");
    await user.click(screen.getByRole("tab", { name: /Remote/ }));
    rerender(<ControlledSources currentModel="soniox" />);
    expect(screen.getByRole("tab", { name: /Remote/ })).toHaveAttribute("aria-selected", "true");

    rerender(<ControlledSources currentModel="tiny" />);
    expect(screen.getByRole("tab", { name: /Local/ })).toHaveAttribute("aria-selected", "true");
  });

  it("shows curated labels and omits a redundant selector for one-model providers", async () => {
    renderModels();

    expect(await screen.findByRole("heading", { name: "GPT Transcribe" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Soniox v5" })).toBeInTheDocument();
    expect(screen.queryByText("gpt-transcribe")).not.toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: "OpenAI transcription model" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("combobox", { name: "Soniox transcription model" }),
    ).not.toBeInTheDocument();
  });

  it("uses source tabs for browsing without changing the active source", async () => {
    const user = userEvent.setup();
    const props = renderModels();

    expect(await screen.findByRole("tab", { name: "Cloud (2)" })).toHaveAttribute("data-active");
    expect(screen.getByRole("tab", { name: "Local (0)" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Remote (0)" })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: /all/i })).not.toBeInTheDocument();
    expect(screen.getAllByText("Spoken language")).toHaveLength(1);

    await user.click(screen.getByRole("tab", { name: "Remote (0)" }));
    expect(await screen.findByText("No remote Voicetyprs configured")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Local (0)" }));

    expect(props.onSelect).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("set_active_remote_server", {
      serverId: expect.anything(),
    });
    expect(screen.getByText("OpenAI (Cloud)")).toBeInTheDocument();
    expect(screen.getByText("Cloud")).toBeInTheDocument();
  });

  it("persists and activates a selected curated model", async () => {
    const user = userEvent.setup();
    const props = renderModels();

    await user.click(
      await screen.findByRole("combobox", {
        name: "OpenAI transcription model",
      }),
    );
    await user.click(
      await screen.findByRole("option", {
        name: "GPT-4o mini Transcribe",
      }),
    );

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_cloud_stt_model", {
        providerId: "openai",
        modelId: "gpt-4o-mini-transcribe",
      });
    });
    expect(props.refreshModels).toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("set_active_remote_server", {
      serverId: null,
    });
    expect(props.onSelect).toHaveBeenCalledWith("openai");
  });
});
