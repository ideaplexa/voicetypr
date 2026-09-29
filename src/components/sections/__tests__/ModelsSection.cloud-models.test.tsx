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

  const view = render(<ModelsSection {...props} />);
  return { ...props, rerender: view.rerender };
}

describe("Transcription source cards", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listen).mockResolvedValue(vi.fn());
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === "list_remote_servers" || command === "discover_remote_servers")
        return Promise.resolve([]);
      if (command === "get_active_remote_server") return Promise.resolve(null);
      return Promise.resolve(undefined);
    });
  });

  it("starts on the active Cloud family and switches visible lists without selecting an engine", async () => {
    const user = userEvent.setup();
    const props = renderModels();
    expect(screen.getByRole("button", { name: "Cloud" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("Soniox v5 · Key added")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Another computer" }));
    expect(screen.getByRole("button", { name: "Another computer" })).toHaveAttribute("aria-pressed", "true");
    expect(await screen.findByText("No remote Voicetyprs configured")).toBeInTheDocument();
    expect(screen.queryByText("Soniox v5 · Key added")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "On this Mac" }));
    expect(screen.getByRole("button", { name: "On this Mac" })).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "Cloud" }));
    expect(screen.getByRole("radio", { name: "Use OpenAI" })).toHaveAttribute("aria-checked", "true");
    expect(props.onSelect).not.toHaveBeenCalled();
  });

  it("keeps the active source visible and marks only its family while browsing", async () => {
    const user = userEvent.setup();
    renderModels();
    expect(screen.getByText("In use: GPT Transcribe · Cloud")).toBeInTheDocument();
    const cloudCard = screen.getByRole("button", { name: "Cloud" });
    expect(cloudCard).toHaveClass("ring-sage");
    expect(cloudCard).toHaveTextContent("In use");
    await user.click(screen.getByRole("button", { name: "On this Mac" }));
    expect(screen.getByText("In use: GPT Transcribe · Cloud")).toBeInTheDocument();
    const localCard = screen.getByRole("button", { name: "On this Mac" });
    expect(localCard).toHaveAttribute("aria-pressed", "true");
    expect(localCard).not.toHaveClass("ring-sage");
    expect(localCard).not.toHaveTextContent("In use");
    expect(cloudCard).toHaveClass("ring-sage");
  });

  it("moves through provider radios with arrows and keeps actions in the tab order", async () => {
    const user = userEvent.setup();
    const props = renderModels();
    const openaiRadio = screen.getByRole("radio", { name: "Use OpenAI" });
    const sonioxRadio = screen.getByRole("radio", { name: "Use Soniox" });
    expect(openaiRadio).toHaveAttribute("tabindex", "0");
    expect(sonioxRadio).toHaveAttribute("tabindex", "-1");
    openaiRadio.focus();
    await user.keyboard("{ArrowDown}");
    expect(sonioxRadio).toHaveFocus();
    await waitFor(() => expect(props.onSelect).toHaveBeenCalledWith("soniox"));
    expect(screen.getAllByRole("button", { name: "Replace key" })).toHaveLength(2);
  });

  it("moves through downloaded local models while keeping Download independently focusable", async () => {
    const user = userEvent.setup();
    const other = { ...whisper, name: "base", display_name: "Whisper Base" };
    const unavailable = { ...whisper, name: "small", display_name: "Whisper Small", downloaded: false };
    const props = renderModels({
      models: [["tiny", whisper], ["base", other], ["small", unavailable]],
      currentModel: "tiny",
      sourceFilter: "local",
    });
    const tiny = screen.getByRole("radio", { name: "Use Whisper Tiny" });
    const base = screen.getByRole("radio", { name: "Use Whisper Base" });
    expect(tiny).toHaveAttribute("tabindex", "0");
    expect(base).toHaveAttribute("tabindex", "-1");
    tiny.focus();
    await user.keyboard("{ArrowDown}");
    expect(base).toHaveFocus();
    await waitFor(() => expect(props.onSelect).toHaveBeenCalledWith("base"));
    expect(screen.getByRole("button", { name: "Download" })).not.toHaveAttribute("tabindex", "-1");
  });

  it("gives every Transcription switch and select an accessible name", () => {
    renderModels();
    for (const control of [...screen.queryAllByRole("switch"), ...screen.queryAllByRole("combobox")]) {
      expect(control, control.outerHTML).toHaveAccessibleName();
    }
  });

  it("preserves a controlled Cloud destination with a local active model without render-phase parent updates", () => {
    const errorSpy = vi.spyOn(console, "error");
    try {
      render(<ControlledSources currentModel="tiny" initialFilter="cloud" />);
      expect(screen.getByRole("button", { name: "Cloud" })).toHaveAttribute("aria-pressed", "true");
      expect(screen.getByText("Soniox v5 · Key added")).toBeInTheDocument();
      expect(errorSpy.mock.calls.flat().some((arg) => typeof arg === "string" && arg.includes("Cannot update a component"))).toBe(false);
    } finally {
      errorSpy.mockRestore();
    }
  });

  it("keeps an inactive Cloud group controlled through selection, external change, and failed selection", async () => {
    const user = userEvent.setup();
    const props = renderModels({ currentModel: "tiny", sourceFilter: "cloud" });
    const openaiRadio = screen.getByRole("radio", { name: "Use OpenAI" });
    const sonioxRadio = screen.getByRole("radio", { name: "Use Soniox" });
    expect(openaiRadio).toHaveAttribute("aria-checked", "false");
    await user.click(openaiRadio);
    await waitFor(() => expect(props.onSelect).toHaveBeenCalledWith("openai"));
    props.rerender(<ModelsSection {...props} currentModel="openai" />);
    expect(openaiRadio).toHaveAttribute("aria-checked", "true");
    props.rerender(<ModelsSection {...props} currentModel="soniox" />);
    expect(openaiRadio).toHaveAttribute("aria-checked", "false");
    expect(sonioxRadio).toHaveAttribute("aria-checked", "true");
    await user.click(openaiRadio);
    await waitFor(() => expect(props.onSelect).toHaveBeenCalledTimes(2));
    // A failed parent update leaves currentModel on Soniox and restores its checked state.
    props.rerender(<ModelsSection {...props} currentModel="soniox" />);
    expect(openaiRadio).toHaveAttribute("aria-checked", "false");
    expect(sonioxRadio).toHaveAttribute("aria-checked", "true");
  });

  it("keeps an inactive Local group controlled after a failed selection", async () => {
    const user = userEvent.setup();
    const props = renderModels({ models: [["tiny", whisper]], currentModel: "openai", sourceFilter: "local" });
    const radio = screen.getByRole("radio", { name: "Use Whisper Tiny" });
    expect(radio).toHaveAttribute("aria-checked", "false");
    await user.click(radio);
    await waitFor(() => expect(props.onSelect).toHaveBeenCalledWith("tiny"));
    props.rerender(<ModelsSection {...props} currentModel="openai" />);
    expect(radio).toHaveAttribute("aria-checked", "false");
    props.rerender(<ModelsSection {...props} currentModel="tiny" />);
    expect(radio).toHaveAttribute("aria-checked", "true");
  });

  it("follows a later engine-family change, but preserves browsing within the same family", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<ControlledSources currentModel="tiny" />);
    await user.click(screen.getByRole("button", { name: "Another computer" }));
    expect(screen.getByRole("button", { name: "Another computer" })).toHaveAttribute("aria-pressed", "true");
    rerender(<ControlledSources currentModel="openai" />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Cloud" })).toHaveAttribute("aria-pressed", "true"),
    );
    await user.click(screen.getByRole("button", { name: "Another computer" }));
    rerender(<ControlledSources currentModel="soniox" />);
    expect(screen.getByRole("button", { name: "Another computer" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("No remote Voicetyprs configured")).toBeInTheDocument();
    rerender(<ControlledSources currentModel="tiny" />);
    await waitFor(() => expect(screen.getByRole("button", { name: "On this Mac" })).toHaveAttribute("aria-pressed", "true"));
  });

  it("shows curated model labels and a picker only when a provider has choices", () => {
    renderModels();
    expect(screen.getByText("GPT Transcribe · Key added")).toBeInTheDocument();
    expect(screen.getByText("Soniox v5 · Key added")).toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: "OpenAI transcription model" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("combobox", { name: "Soniox transcription model" }),
    ).not.toBeInTheDocument();
  });

  it("selects a downloaded local model through its radio", async () => {
    const user = userEvent.setup();
    const props = renderModels({
      models: [["tiny", whisper]],
      currentModel: "openai",
      sourceFilter: "local",
    });
    await user.click(screen.getByRole("radio", { name: "Use Whisper Tiny" }));
    await waitFor(() => expect(props.onSelect).toHaveBeenCalledWith("tiny"));
    expect(invoke).toHaveBeenCalledWith("set_active_remote_server", { serverId: null });
  });

  it("starts and cancels a model download", async () => {
    const user = userEvent.setup();
    const unavailable = {
      ...whisper,
      name: "small",
      display_name: "Whisper Small",
      downloaded: false,
    };
    const props = renderModels({
      models: [["small", unavailable]],
      currentModel: "openai",
      sourceFilter: "local",
    });
    await user.click(screen.getByRole("button", { name: "Download" }));
    expect(props.onDownload).toHaveBeenCalledWith("small");
    props.rerender(<ModelsSection {...props} downloadProgress={{ small: 42 }} />);
    expect(screen.getByText(/42%/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel Whisper Small download" }));
    expect(props.onCancelDownload).toHaveBeenCalledWith("small");
    props.rerender(<ModelsSection {...props} downloadProgress={{}} />);
  });

  it("persists and activates a selected curated model", async () => {
    const user = userEvent.setup();
    const props = renderModels();
    await user.click(screen.getByRole("combobox", { name: "OpenAI transcription model" }));
    await user.click(await screen.findByRole("option", { name: "GPT-4o mini Transcribe" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_cloud_stt_model", {
        providerId: "openai",
        modelId: "gpt-4o-mini-transcribe",
      }),
    );
    expect(props.refreshModels).toHaveBeenCalled();
    expect(props.onSelect).toHaveBeenCalledWith("openai");
  });
});
