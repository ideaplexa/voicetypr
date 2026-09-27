import { render, screen, fireEvent, waitFor, within, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, it, expect, vi, beforeEach } from "vitest";
import { EnhancementsSection } from "../EnhancementsSection";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { useEnhancementsStore } from "@/state/enhancements";
import { SettingsProvider } from "@/contexts/SettingsContext";
import { hasApiKey, saveApiKey } from "@/utils/keyring";
import { defaultWritingSettings, mergeWritingSettings } from "@/types/writing";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

type MockEventHandler = (event: { payload: unknown }) => void | Promise<void>;
const eventListeners = vi.hoisted(() => new Map<string, MockEventHandler>());

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, handler: MockEventHandler) => {
    eventListeners.set(event, handler);
    return () => {
      if (eventListeners.get(event) === handler) {
        eventListeners.delete(event);
      }
    };
  }),
}));

const readinessState = vi.hoisted(() => ({
  value: null as { ai_ready: boolean } | null,
}));

vi.mock("@/contexts/ReadinessContext", () => ({
  useReadinessState: () => readinessState.value,
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
    info: vi.fn(),
  },
}));

vi.mock("@/utils/keyring", () => ({
  saveApiKey: vi.fn().mockResolvedValue(undefined),
  hasApiKey: vi.fn().mockResolvedValue(false),
  removeApiKey: vi.fn().mockResolvedValue(undefined),
  getApiKey: vi.fn().mockResolvedValue(null),
}));

const providerModels = vi.hoisted(
  (): Record<
    string,
    Array<{
      id: string;
      name: string;
      recommended: boolean;
      reasoning?: boolean;
      contextWindow?: number | null;
      sourceProvider?: string | null;
      cliDefault?: boolean;
      costInput?: number | null;
      costOutput?: number | null;
    }>
  > => ({
    gemini: [{ id: "gemini-1.5-flash", name: "Gemini 1.5 Flash", recommended: true }],
    openai: [
      {
        id: "gpt-5-mini",
        name: "GPT-5 Mini",
        recommended: true,
        reasoning: true,
        contextWindow: 400000,
        costInput: 0.25,
        costOutput: 2,
      },
      { id: "gpt-5-nano", name: "GPT-5 Nano", recommended: false },
    ],
    anthropic: [{ id: "claude-sonnet-4", name: "Claude Sonnet 4", recommended: true }],
    groq: [
      {
        id: "llama-3.3-70b-versatile",
        name: "Llama 3.3 70B Versatile",
        recommended: true,
      },
    ],
    "claude-code": [
      { id: "", name: "Default", recommended: true, cliDefault: true },
      { id: "haiku", name: "Haiku", recommended: false },
      { id: "sonnet", name: "Sonnet", recommended: false },
      { id: "opus", name: "Opus", recommended: false },
    ],
    pi: [
      {
        id: "",
        name: "Default",
        recommended: true,
        cliDefault: true,
      },
      {
        id: "openai/gpt-5-mini",
        name: "GPT-5 Mini",
        recommended: false,
        sourceProvider: "OpenAI",
      },
      {
        id: "anthropic/claude-sonnet-4",
        name: "Claude Sonnet 4",
        recommended: false,
        sourceProvider: "Anthropic",
      },
    ],
    omp: [
      {
        id: "",
        name: "Default",
        recommended: true,
        cliDefault: true,
      },
      {
        id: "google/gemini-2.5-flash",
        name: "Gemini 2.5 Flash",
        recommended: false,
        sourceProvider: "Google",
      },
    ],
    codex: [
      {
        id: "",
        name: "Default",
        recommended: true,
        cliDefault: true,
      },
    ],
    droid: [
      { id: "", name: "Default", recommended: true, cliDefault: true },
      {
        id: "claude-opus-5",
        name: "Opus 5",
        recommended: false,
        reasoning: true,
      },
      {
        id: "gpt-5.6-sol",
        name: "GPT-5.6 Sol",
        recommended: false,
        reasoning: true,
      },
    ],
  }),
);
const modelDiscovery = vi.hoisted(() => ({
  loading: {} as Record<string, boolean>,
  errors: {} as Record<string, string | null>,
  hiddenProviders: new Set<string>(),
  fetchModels: vi.fn((providerId: string, _signal?: AbortSignal, _options?: { force?: boolean }) =>
    Promise.resolve(providerModels[providerId] || []),
  ),
}));

vi.mock("@/hooks/useProviderModels", () => ({
  useAllProviderModels: () => ({
    fetchModels: (...args: [string, AbortSignal?, { force?: boolean }?]) =>
      modelDiscovery.fetchModels(...args),
    getModels: (providerId: string) =>
      modelDiscovery.hiddenProviders.has(providerId) ? [] : providerModels[providerId] || [],
    isLoading: (providerId: string) => modelDiscovery.loading[providerId] || false,
    getError: (providerId: string) => modelDiscovery.errors[providerId] || null,
    clearModels: (providerId: string) => {
      delete modelDiscovery.errors[providerId];
      delete modelDiscovery.loading[providerId];
    },
  }),
}));

const baseAISettings = {
  enabled: false,
  provider: "",
  model: "",
  hasApiKey: false,
  modelsByProvider: {},
  reasoningByProvider: {},
  fastModeByProvider: {},
  aiModelNeedsReselection: false,
};
let aiSettingsResponse: typeof baseAISettings = baseAISettings;
let aiSettingsHandler: (() => Promise<typeof aiSettingsResponse>) | undefined;

const enabledAISettings = {
  enabled: true,
  provider: "openai",
  model: "gpt-5-mini",
  hasApiKey: true,
  modelsByProvider: {
    openai: "gpt-5-mini",
  },
  reasoningByProvider: {},
  fastModeByProvider: {},
  aiModelNeedsReselection: false,
};

const providerListResponse = [
  {
    id: "openai",
    name: "OpenAI",
    status: "production",
    supportsReasoning: true,
  },
  {
    id: "gemini",
    name: "Google Gemini",
    status: "production",
    supportsReasoning: true,
  },
  {
    id: "anthropic",
    name: "Anthropic",
    status: "production",
    supportsReasoning: true,
  },
  {
    id: "custom",
    name: "Custom (OpenAI-compatible)",
    status: "production",
    supportsBaseUrl: true,
  },
  {
    id: "groq",
    name: "Groq",
    status: "experimental",
    supportsReasoning: false,
  },
  {
    id: "claude-code",
    name: "Claude Code",
    status: "production",
    supportsReasoning: false,
  },
  { id: "pi", name: "pi", status: "production", supportsReasoning: false },
  {
    id: "omp",
    name: "oh-my-pi",
    status: "production",
    supportsReasoning: false,
  },
  {
    id: "codex",
    name: "Codex",
    status: "production",
    supportsReasoning: false,
  },
  {
    id: "droid",
    name: "Droid",
    status: "production",
    supportsReasoning: false,
  },
  { id: "grok", name: "Grok", status: "production", supportsReasoning: false },
  {
    id: "opencode",
    name: "OpenCode",
    status: "production",
    supportsReasoning: false,
  },
  {
    id: "cline",
    name: "Cline",
    status: "production",
    supportsReasoning: false,
  },
];
const localAgentProviderIds = new Set([
  "claude-code",
  "pi",
  "omp",
  "codex",
  "droid",
  "grok",
  "opencode",
  "cline",
]);

const isAgentCliProviderForTest = (providerId: string) => localAgentProviderIds.has(providerId);

let rejectWritingSettingsUpdate = false;
let agentCliProbeResponse: {
  state: "ready" | "missing" | "unsafe_launcher";
  reasoningLevels?: string[];
  supportsFastMode?: boolean;
} = { state: "missing" };
let agentCliProbeHandler:
  | ((args?: Record<string, unknown>) => Promise<typeof agentCliProbeResponse>)
  | undefined;
let enhancementOptionsResponse = { preset: "PersonalDictation" };

const baseAppSettings = {
  hotkey: "CommandOrControl+Shift+Space",
  current_model: "base",
  current_model_engine: "whisper",
  speech_language: "en",
  transcription_task: "transcribe",
  final_text_language: "same_as_transcript",
  theme: "system",
};

function renderWithProviders() {
  return render(
    <SettingsProvider>
      <EnhancementsSection />
    </SettingsProvider>,
  );
}

async function openPolishTab(
  _user: ReturnType<typeof userEvent.setup>,
  name: "Provider" | "Dictionary" | "Corrections" | "Snippets" | "Modes",
) {
  // Sections are always rendered now — resolve the region for async settling.
  await screen.findByRole("region", { name });
}

async function openModes(user: ReturnType<typeof userEvent.setup>) {
  await openPolishTab(user, "Modes");
}

async function getProviderSetupPanel() {
  const launcher = await screen.findByRole("button", {
    name: "Choose provider and model",
  });
  const alreadyOpen =
    screen.queryByRole("dialog", { name: /Choose provider & model/i }) ??
    screen.queryByRole("dialog");
  if (!alreadyOpen) {
    await fireEvent.click(launcher);
  }
  return await screen.findByRole("dialog");
}

describe("EnhancementsSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventListeners.clear();
    useEnhancementsStore.getState().clearPolishError();
    readinessState.value = null;
    modelDiscovery.loading = {};
    modelDiscovery.errors = {};
    modelDiscovery.hiddenProviders.clear();
    window.localStorage.clear();
    rejectWritingSettingsUpdate = false;
    agentCliProbeHandler = undefined;
    aiSettingsHandler = undefined;
    aiSettingsResponse = baseAISettings;
    enhancementOptionsResponse = { preset: "PersonalDictation" };
    agentCliProbeResponse = {
      state: "missing",
    };
    (hasApiKey as ReturnType<typeof vi.fn>).mockResolvedValue(false);
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(
      (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "list_ai_providers") {
          return Promise.resolve(providerListResponse);
        }
        if (cmd === "get_settings") {
          return Promise.resolve(baseAppSettings);
        }
        if (cmd === "save_settings") {
          return Promise.resolve(undefined);
        }
        if (cmd === "get_enhancement_options") {
          return Promise.resolve(enhancementOptionsResponse);
        }
        if (cmd === "update_enhancement_options") {
          return Promise.resolve(undefined);
        }
        if (cmd === "get_writing_settings") {
          return Promise.resolve(defaultWritingSettings);
        }
        if (cmd === "update_writing_settings") {
          return rejectWritingSettingsUpdate
            ? Promise.reject(new Error("disk full"))
            : Promise.resolve(undefined);
        }
        if (cmd === "get_ai_settings") {
          return aiSettingsHandler ? aiSettingsHandler() : Promise.resolve(aiSettingsResponse);
        }
        if (cmd === "get_ai_settings_for_provider") {
          const provider = (args as { provider?: string })?.provider || "";
          const model =
            (aiSettingsResponse.modelsByProvider as Record<string, string>)[provider] ??
            (aiSettingsResponse.provider === provider ? aiSettingsResponse.model : "");
          return Promise.resolve({ ...aiSettingsResponse, provider, model });
        }
        if (cmd === "get_openai_config") {
          return Promise.resolve({ baseUrl: "https://api.openai.com/v1" });
        }
        if (cmd === "update_ai_settings") {
          const nextAISettings = args as typeof aiSettingsResponse;
          aiSettingsResponse = {
            ...aiSettingsResponse,
            ...nextAISettings,
            aiModelNeedsReselection: nextAISettings.model
              ? false
              : aiSettingsResponse.aiModelNeedsReselection,
          };

          return Promise.resolve(undefined);
        }
        if (cmd === "cache_ai_api_key") {
          return Promise.resolve(undefined);
        }
        if (cmd === "probe_agent_cli") {
          return agentCliProbeHandler
            ? agentCliProbeHandler(args)
            : Promise.resolve(agentCliProbeResponse);
        }
        return Promise.resolve(undefined);
      },
    );
  });

  // Tests that simulate an external CLI account change swap the shared
  // provider catalog; restore it (and the default fetch implementation)
  // so later tests keep seeing the pristine fixtures.
  let claudeCodeModelsRestore: (() => void) | undefined;
  afterEach(() => {
    claudeCodeModelsRestore?.();
    claudeCodeModelsRestore = undefined;
    modelDiscovery.fetchModels.mockImplementation((providerId: string) =>
      Promise.resolve(providerModels[providerId] || []),
    );
  });

  it("renders cloud providers in the provider setup tabs", async () => {
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    expect(within(providersPanel).getByRole("tab", { name: "Cloud API" })).toHaveAttribute(
      "data-active",
    );
    expect(within(providersPanel).getByRole("tab", { name: "Local Agents" })).toBeInTheDocument();
    expect(within(providersPanel).getByRole("heading", { name: "OpenAI" })).toBeInTheDocument();
    expect(
      within(providersPanel).getByRole("heading", { name: "Google Gemini" }),
    ).toBeInTheDocument();
    expect(within(providersPanel).getByRole("heading", { name: "Anthropic" })).toBeInTheDocument();
    expect(within(providersPanel).getByText("Experimental")).toBeInTheDocument();
    expect(
      within(providersPanel).getByLabelText("Search cloud providers and models"),
    ).toBeInTheDocument();
  });

  it("does not restart local-agent probes on rerender or ai-ready events", async () => {
    const user = userEvent.setup();
    const view = renderWithProviders();
    const providersPanel = await getProviderSetupPanel();
    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));

    await waitFor(() => {
      const probeCalls = vi
        .mocked(invoke)
        .mock.calls.filter(([command]) => command === "probe_agent_cli");
      expect(probeCalls).toHaveLength(8);
      expect(eventListeners.has("ai-ready")).toBe(true);
    });

    readinessState.value = { ai_ready: false };
    view.rerender(
      <SettingsProvider>
        <EnhancementsSection />
      </SettingsProvider>,
    );
    await eventListeners.get("ai-ready")?.({ payload: null });

    await waitFor(() => {
      const probeCalls = vi
        .mocked(invoke)
        .mock.calls.filter(([command]) => command === "probe_agent_cli");
      expect(probeCalls).toHaveLength(8);
    });
  });

  it("filters cloud providers and exposes matching models in the model select", async () => {
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(
      async (providerId: string) => providerId === "openai" || providerId === "groq",
    );
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    await user.type(
      within(providersPanel).getByLabelText("Search cloud providers and models"),
      "llama",
    );

    await waitFor(() => {
      expect(
        within(providersPanel).queryByRole("heading", { name: "OpenAI" }),
      ).not.toBeInTheDocument();
      expect(within(providersPanel).getByRole("heading", { name: "Groq" })).toBeInTheDocument();
    });
    await user.click(within(providersPanel).getByRole("combobox", { name: "Model for Groq" }));
    expect(
      await screen.findByRole("option", { name: /llama 3\.3 70b versatile/i }),
    ).toBeInTheDocument();
  });

  it("persists a model selected from the compact model dropdown", async () => {
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(
      async (providerId: string) => providerId === "openai",
    );
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    await user.click(
      within(providersPanel).getByRole("combobox", {
        name: "Model for OpenAI",
      }),
    );
    await user.click(await screen.findByRole("option", { name: /gpt-5 nano/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_ai_settings", {
        enabled: false,
        provider: "openai",
        model: "gpt-5-nano",
      });
    });
  });

  it("organizes Polish controls into task-led sections", async () => {
    const user = userEvent.setup();
    renderWithProviders();

    expect(await screen.findByRole("region", { name: "Provider" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: "Select a provider to enable Polish",
      }),
    ).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Dictionary" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Corrections" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Snippets" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Modes" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Style" })).not.toBeInTheDocument();

    await openModes(user);
    expect(await screen.findByText("Default mode")).toBeInTheDocument();
    expect(screen.getByText("Per-app modes")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("list_ai_providers");
  });

  it("keeps the default mode picker segmented control in Modes", async () => {
    const user = userEvent.setup();
    renderWithProviders();

    await openModes(user);
    expect(await screen.findByRole("tablist", { name: "Default mode" })).toBeInTheDocument();
    for (const mode of ["Polish Off", "Clean Dictation", "Writing", "Notes", "Message", "Code"]) {
      expect(screen.getByRole("tab", { name: mode })).toBeInTheDocument();
    }
    expect(screen.queryByRole("region", { name: "Style" })).not.toBeInTheDocument();
  });

  it("shows cloud and local setup tabs when Polish is unconfigured", async () => {
    renderWithProviders();
    expect(
      await screen.findByRole("button", {
        name: "Select a provider to enable Polish",
      }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: /polish/i })).not.toBeInTheDocument();

    const providersPanel = await getProviderSetupPanel();
    expect(
      within(providersPanel).getByText(
        "Use your own cloud API key or an agent already installed on this Mac.",
      ),
    ).toBeInTheDocument();
    expect(within(providersPanel).getByRole("tab", { name: "Cloud API" })).toBeInTheDocument();
    expect(within(providersPanel).getByRole("tab", { name: "Local Agents" })).toBeInTheDocument();
    expect(
      within(providersPanel).getByRole("heading", {
        name: "Choose provider & model",
      }),
    ).toBeInTheDocument();
  });

  it("keeps the selected provider and model in the launcher summary", async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: false };
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(
      async (providerId: string) => providerId === "openai",
    );
    const user = userEvent.setup();
    renderWithProviders();

    const launcher = await screen.findByRole("button", {
      name: "Choose provider and model",
    });
    expect(launcher).toHaveTextContent("Provider & model");
    // The summary text arrives after the async settings load — wait for it
    // instead of asserting once (this raced under full-suite load).
    await waitFor(() => expect(launcher).toHaveTextContent("OpenAI · GPT-5 Mini"), {
      timeout: 3000,
    });
    expect(launcher).toHaveTextContent("Active");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    await user.click(launcher);
    const providersPanel = await screen.findByRole("dialog");
    expect(within(providersPanel).getByRole("tab", { name: "Cloud API" })).toBeInTheDocument();

    await user.click(within(providersPanel).getByRole("button", { name: /close/i }));
    // Closing may already unmount the dialog; assert the user-visible result.
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(launcher).toHaveTextContent("OpenAI · GPT-5 Mini");
    expect(launcher).toHaveTextContent("Active");
    await user.click(launcher);
    const reopenedPanel = await screen.findByRole("dialog");
    expect(within(reopenedPanel).getByRole("combobox", { name: "Model for OpenAI" })).toHaveValue(
      "GPT-5 Mini",
    );
  });

  it("does not expand then collapse while configured settings load", async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: false };
    let resolveSettings: ((settings: typeof aiSettingsResponse) => void) | undefined;
    aiSettingsHandler = () =>
      new Promise((resolve) => {
        resolveSettings = resolve;
      });

    renderWithProviders();

    const launcher = await screen.findByRole("button", {
      name: "Choose provider and model",
    });
    expect(launcher).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    resolveSettings?.(aiSettingsResponse);

    await waitFor(() => {
      expect(
        screen.getByRole("button", {
          name: "Choose provider and model",
        }),
      ).toHaveTextContent("OpenAI · GPT-5 Mini");
    });
  });

  it("keeps a refreshed local agent inactive until the user selects it", async () => {
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["low", "medium"],
    };
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));
    expect(
      await within(providersPanel).findByRole("heading", {
        name: "Claude Code",
      }),
    ).toBeInTheDocument();
    expect(
      within(providersPanel).getByRole("button", {
        name: "Model for Claude Code",
      }),
    ).toHaveTextContent("Default");

    (invoke as ReturnType<typeof vi.fn>).mockClear();
    await user.click(
      within(providersPanel).getByRole("button", {
        name: "Refresh Claude Code status",
      }),
    );

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("probe_agent_cli", {
        provider: "claude-code",
        refresh: true,
      });
    });
    await waitFor(() => {
      expect(within(providersPanel).getAllByText("Installed").length).toBeGreaterThan(0);
    });
    expect(
      (invoke as ReturnType<typeof vi.fn>).mock.calls.some(
        ([command]) => command === "update_ai_settings",
      ),
    ).toBe(false);

    fireEvent.keyDown(within(providersPanel).getByRole("button", { name: "Select Claude Code" }), {
      key: "Enter",
    });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_ai_settings", {
        enabled: false,
        provider: "claude-code",
        model: "",
      });
    });
  });

  it("shows the refreshed CLI account's models in the picker after a ready Refresh", async () => {
    agentCliProbeResponse = {
      state: "ready",
    };
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();
    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));
    expect(
      await within(providersPanel).findByRole("heading", {
        name: "Claude Code",
      }),
    ).toBeInTheDocument();
    // Let the initial probes settle so the Refresh click reaches probeAgentCli.
    await waitFor(() => {
      expect(within(providersPanel).getAllByText("Installed").length).toBeGreaterThan(0);
    });

    // The CLI account changed outside the app. A ready Refresh must reload the
    // model cache, so the picker offers the new account's models instead of
    // the ones captured before the Refresh.
    const previousModels = providerModels["claude-code"];
    const refreshedAccountModels = [
      { id: "", name: "Default", recommended: true, cliDefault: true },
      { id: "sonnet-4-5", name: "Sonnet 4.5", recommended: false },
    ];
    claudeCodeModelsRestore = () => {
      providerModels["claude-code"] = previousModels;
    };
    modelDiscovery.fetchModels.mockImplementation(async (providerId: string) => {
      if (providerId === "claude-code") {
        providerModels["claude-code"] = refreshedAccountModels;
      }
      return providerModels[providerId] || [];
    });

    await user.click(
      within(providersPanel).getByRole("button", {
        name: "Refresh Claude Code status",
      }),
    );

    await waitFor(() => {
      expect(modelDiscovery.fetchModels).toHaveBeenCalledWith("claude-code", undefined, {
        force: true,
      });
    });

    await user.click(within(providersPanel).getByRole("button", { name: "Model for Claude Code" }));
    const picker = await screen.findByRole("dialog", { name: "Choose a Claude Code model" });
    expect(
      within(picker).getByRole("button", { name: /^Sonnet 4\.5.*claude-code\/sonnet-4-5/i }),
    ).toBeInTheDocument();
    expect(within(picker).queryByRole("button", { name: /^Haiku/i })).not.toBeInTheDocument();
    expect(within(picker).queryByRole("button", { name: /^Opus/i })).not.toBeInTheDocument();
  });

  it("keeps the previous model list in the picker when a Refresh fails", async () => {
    agentCliProbeResponse = {
      state: "ready",
    };
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();
    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));
    expect(
      await within(providersPanel).findByRole("heading", {
        name: "Claude Code",
      }),
    ).toBeInTheDocument();
    // Let the initial probes settle so the CLI starts out ready and the
    // picker is enabled with the cached list.
    await waitFor(() => {
      expect(within(providersPanel).getAllByText("Installed").length).toBeGreaterThan(0);
    });

    // The Refresh probe itself fails; the previously probed status and the
    // cached model list must survive it.
    agentCliProbeHandler = () => Promise.reject(new Error("probe failed"));
    await user.click(
      within(providersPanel).getByRole("button", {
        name: "Refresh Claude Code status",
      }),
    );

    await user.click(within(providersPanel).getByRole("button", { name: "Model for Claude Code" }));
    const picker = await screen.findByRole("dialog", { name: "Choose a Claude Code model" });
    expect(within(picker).getByRole("button", { name: /^Haiku/i })).toBeInTheDocument();
    expect(within(picker).getByRole("button", { name: /^Sonnet/i })).toBeInTheDocument();
    expect(within(picker).getByRole("button", { name: /^Opus/i })).toBeInTheDocument();
  });

  it("opens the API key modal from a cloud provider row", async () => {
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    await user.click(
      within(providersPanel).getByRole("button", {
        name: "Add Anthropic API key",
      }),
    );

    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeInTheDocument();
      expect(screen.getByText("Add Anthropic API Key")).toBeInTheDocument();
      expect(screen.getByLabelText("API Key")).toBeInTheDocument();
    });
  });

  it("shows installed local agents without an API key flow", async () => {
    agentCliProbeResponse = {
      state: "ready",
    };
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));

    expect(
      await within(providersPanel).findByRole("heading", {
        name: "Claude Code",
      }),
    ).toBeInTheDocument();
    for (const providerName of ["Codex", "Droid", "Grok", "OpenCode", "Cline"]) {
      expect(
        within(providersPanel).getByRole("heading", { name: providerName }),
      ).toBeInTheDocument();
    }
    expect(within(providersPanel).queryByRole("heading", { name: "Amp" })).not.toBeInTheDocument();
    expect(
      within(providersPanel).queryByRole("heading", { name: "Kilo Code" }),
    ).not.toBeInTheDocument();
    expect(within(providersPanel).getAllByText("Installed").length).toBeGreaterThan(0);
    expect(
      within(providersPanel).getAllByRole("button", { name: /refresh/i }).length,
    ).toBeGreaterThan(0);
    await user.click(within(providersPanel).getByRole("button", { name: /close/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();
  });

  it("keeps model controls stable during initial probes", async () => {
    agentCliProbeHandler = () => Promise.race<typeof agentCliProbeResponse>([]);
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();
    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));

    expect(within(providersPanel).getAllByText("Checking installation…").length).toBeGreaterThan(0);
    expect(
      within(providersPanel).getByRole("button", {
        name: "Model for Claude Code",
      }),
    ).toBeDisabled();
    expect(within(providersPanel).queryByText("Checking support…")).not.toBeInTheDocument();
  });

  it("keeps ready controls mounted while a status refresh is pending", async () => {
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["low", "medium"],
      supportsFastMode: true,
    };
    agentCliProbeHandler = (args) =>
      args?.refresh
        ? Promise.race<typeof agentCliProbeResponse>([])
        : Promise.resolve(agentCliProbeResponse);

    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();
    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));
    const modelButton = await within(providersPanel).findByRole("button", {
      name: "Model for Claude Code",
    });
    expect(modelButton).toBeEnabled();

    await user.click(
      within(providersPanel).getByRole("button", {
        name: "Refresh Claude Code status",
      }),
    );
    expect(modelButton).toBeEnabled();
    expect(within(providersPanel).getByText("Installed · Refreshing…")).toBeInTheDocument();
  });

  it("persists the selected local-agent thinking level", async () => {
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["off", "low", "medium"],
      supportsFastMode: false,
    };
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));
    await user.click(await within(providersPanel).findByRole("button", { name: "Select pi" }));
    await user.click(
      await within(providersPanel).findByRole("combobox", {
        name: "Thinking for pi",
      }),
    );
    expect(screen.queryByRole("option", { name: "High" })).not.toBeInTheDocument();
    await user.click(await screen.findByRole("option", { name: "Medium" }));

    expect(invoke).toHaveBeenCalledWith("update_agent_cli_reasoning", {
      provider: "pi",
      reasoning: "medium",
    });
  });

  it("persists native fast mode only when the CLI adapter supports it", async () => {
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["low", "medium"],
      supportsFastMode: true,
    };
    agentCliProbeHandler = (args) =>
      Promise.resolve({
        ...agentCliProbeResponse,
        supportsFastMode: args?.provider === "claude-code",
      });
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));
    // Base UI routes the switch's accessible name through a hidden input,
    // so query by role within the Local Agents tab (only Claude Code exposes
    // fast mode here; pi does not).
    const fastModeSwitch = await waitFor(() => within(providersPanel).getByRole("switch"), {
      timeout: 3000,
    });
    await user.click(fastModeSwitch);

    expect(invoke).toHaveBeenCalledWith("update_agent_cli_fast_mode", {
      provider: "claude-code",
      enabled: true,
    });
    expect(
      within(providersPanel).queryByRole("switch", {
        name: "Fast mode for pi",
      }),
    ).not.toBeInTheDocument();
  });

  it("auto-selects the recommended model and turns Polish on after guided key validation", async () => {
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    await user.click(within(providersPanel).getByRole("button", { name: "Add OpenAI API key" }));
    await user.type(await screen.findByLabelText("API Key"), "openai-key");
    await user.click(screen.getByRole("button", { name: "Save API Key" }));

    await waitFor(() => {
      expect(saveApiKey).toHaveBeenCalledWith("openai", "openai-key");
      expect(invoke).toHaveBeenCalledWith("update_ai_settings", {
        enabled: true,
        provider: "openai",
        model: "gpt-5-mini",
      });
      expect(invoke).toHaveBeenCalledWith("update_enhancement_options", {
        options: { preset: "CleanDictation" },
      });
      expect(toast.success).toHaveBeenCalledWith("Polish on");
    });
  });

  it("keeps a loaded key-based provider connected when backend AI readiness is ready", async () => {
    readinessState.value = { ai_ready: true };
    aiSettingsResponse = {
      ...enabledAISettings,
      provider: "anthropic",
      model: "claude-sonnet-4",
      modelsByProvider: { anthropic: "claude-sonnet-4" },
    };
    vi.mocked(hasApiKey).mockResolvedValue(false);

    renderWithProviders();

    await waitFor(() => {
      const polishSwitch = screen.getByRole("switch", { name: "Polish" });
      const providerSummary = screen.getByRole("button", {
        name: "Choose provider and model",
      });
      expect(polishSwitch).toBeChecked();
      expect(polishSwitch).not.toHaveAttribute("aria-disabled", "true");
      expect(providerSummary).toHaveTextContent("Anthropic · Claude Sonnet 4");
      expect(providerSummary).toHaveTextContent("Active");
    });
  });

  it("uses executable detection as local agent readiness", async () => {
    readinessState.value = { ai_ready: true };
    aiSettingsResponse = {
      ...enabledAISettings,
      provider: "claude-code",
      model: "haiku",
      modelsByProvider: { "claude-code": "haiku" },
    };
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["low", "medium"],
    };
    vi.mocked(hasApiKey).mockResolvedValue(false);

    renderWithProviders();

    await waitFor(() => {
      const polishSwitch = screen.getByRole("switch", { name: "Polish" });
      expect(polishSwitch).not.toHaveAttribute("aria-disabled", "true");
    });
  });

  it("shows the connected status line when Polish is configured but off", async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: false };
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(
      async (providerId: string) => providerId === "openai",
    );

    renderWithProviders();

    await waitFor(() => {
      const providerSummary = screen.getByRole("button", {
        name: "Choose provider and model",
      });
      expect(providerSummary).toHaveTextContent("OpenAI · GPT-5 Mini");
      expect(providerSummary).toHaveTextContent("Active");
      expect(screen.queryByText("Connect an AI to turn on Polish")).not.toBeInTheDocument();
    });
    expect(screen.getByRole("region", { name: "Corrections" })).toBeInTheDocument();

    const providersPanel = await getProviderSetupPanel();
    expect(
      within(providersPanel).getByRole("heading", {
        name: "Choose provider & model",
      }),
    ).toBeInTheDocument();
  });

  it("keeps the simple Polish surface free of paywall or locked cues", async () => {
    const { container } = renderWithProviders();

    expect(
      await screen.findByRole("button", {
        name: "Select a provider to enable Polish",
      }),
    ).toBeInTheDocument();
    expect(screen.getByText("Choose a cloud API or local agent")).toBeInTheDocument();
    expect(container.querySelector(".lucide-lock")).toBeNull();
    expect(screen.queryByText(/premium|paywall|locked/i)).not.toBeInTheDocument();
  });

  it("hides specific language selection when Polish Off is loaded", async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(
      (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "list_ai_providers") {
          return Promise.resolve(providerListResponse);
        }
        if (cmd === "get_settings") {
          return Promise.resolve({
            ...baseAppSettings,
            final_text_language: "fr",
            transcription_task: "transcribe",
          });
        }
        if (cmd === "get_enhancement_options") {
          return Promise.resolve({ preset: "PersonalDictation" });
        }
        if (cmd === "get_writing_settings") {
          return Promise.resolve(defaultWritingSettings);
        }
        if (cmd === "get_ai_settings") {
          return Promise.resolve(baseAISettings);
        }
        if (cmd === "get_ai_settings_for_provider") {
          const provider = (args as { provider?: string })?.provider || "";
          return Promise.resolve({ ...baseAISettings, provider });
        }
        if (cmd === "get_openai_config") {
          return Promise.resolve({ baseUrl: "https://api.openai.com/v1" });
        }
        return Promise.resolve(undefined);
      },
    );

    const user = userEvent.setup();
    renderWithProviders();
    await openModes(user);

    expect(await screen.findByRole("button", { name: "Same as transcript" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Specific language" })).toBeDisabled();
    expect(screen.queryByRole("combobox", { name: /language/i })).not.toBeInTheDocument();
  });

  it("uses the non-AI preset when Polish is turned off", async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: true };
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(
      async (providerId: string) => providerId === "openai",
    );
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(
      (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "list_ai_providers") {
          return Promise.resolve(providerListResponse);
        }
        if (cmd === "get_enhancement_options") {
          return Promise.resolve({ preset: "Writing" });
        }
        if (cmd === "get_ai_settings") {
          return Promise.resolve(aiSettingsResponse);
        }
        if (cmd === "update_ai_settings") {
          aiSettingsResponse = {
            ...aiSettingsResponse,
            ...(args as typeof aiSettingsResponse),
          };
          return Promise.resolve(undefined);
        }
        if (cmd === "update_enhancement_options") {
          return Promise.resolve(undefined);
        }
        if (cmd === "get_settings") {
          return Promise.resolve({
            ...baseAppSettings,
            final_text_language: "fr",
            transcription_task: "transcribe",
          });
        }
        if (cmd === "get_writing_settings") {
          return Promise.resolve({
            replacements: [],
            custom_words: [],
            snippets: [],
            context_policy: "off",
          });
        }
        if (cmd === "get_ai_settings_for_provider") {
          const provider = (args as { provider?: string })?.provider || "";
          return Promise.resolve({
            ...aiSettingsResponse,
            provider,
            hasApiKey: provider === "openai",
          });
        }
        if (cmd === "get_openai_config") {
          return Promise.resolve({ baseUrl: "https://api.openai.com/v1" });
        }
        if (cmd === "cache_ai_api_key") {
          return Promise.resolve(undefined);
        }
        return Promise.resolve(undefined);
      },
    );

    const user = userEvent.setup();
    renderWithProviders();

    const aiToggle = await screen.findByRole("switch", { name: /polish/i });
    await waitFor(() => expect(aiToggle).toBeEnabled());
    await user.click(aiToggle);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_enhancement_options", {
        options: { preset: "PersonalDictation" },
      });
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: expect.objectContaining({
          final_text_language: "same_as_transcript",
        }),
      });
      expect(toast.success).toHaveBeenCalledWith("Polish off");
    });
  });

  it("switches to Clean Dictation when Polish is turned on", async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: false };
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(
      async (providerId: string) => providerId === "openai",
    );
    const user = userEvent.setup();
    renderWithProviders();

    const aiToggle = await screen.findByRole("switch", { name: /polish/i });
    await waitFor(() => expect(aiToggle).toBeEnabled());
    await user.click(aiToggle);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_enhancement_options", {
        options: { preset: "CleanDictation" },
      });
    });
  });

  it("saves custom provider setup without enabling Polish", async () => {
    aiSettingsResponse = { ...baseAISettings, provider: "", model: "" };
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    await user.click(
      within(providersPanel).getByRole("button", {
        name: "Configure Custom (OpenAI-compatible)",
      }),
    );
    await user.type(await screen.findByLabelText("Model ID"), "local-model");
    await user.click(screen.getByRole("button", { name: "Test" }));

    await waitFor(() => {
      expect(screen.getByText("Connection successful")).toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_ai_settings", {
        enabled: false,
        provider: "custom",
        model: "local-model",
      });
    });
    expect(invoke).not.toHaveBeenCalledWith("update_enhancement_options", {
      options: { preset: "CleanDictation" },
    });
  });

  it("saves final text language changes through save_settings", async () => {
    aiSettingsResponse = enabledAISettings;
    enhancementOptionsResponse = { preset: "CleanDictation" };
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(
      async (providerId: string) => providerId === "openai",
    );
    const user = userEvent.setup();
    renderWithProviders();
    await openModes(user);

    await user.click(await screen.findByRole("button", { name: "Specific language" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: expect.objectContaining({
          final_text_language: "en",
          transcription_task: "translate_to_english",
        }),
      });
    });
  });

  it("renders the task-led Polish hierarchy", async () => {
    const user = userEvent.setup();
    renderWithProviders();

    expect(await screen.findByRole("region", { name: "Provider" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: "Select a provider to enable Polish",
      }),
    ).toBeInTheDocument();

    await openModes(user);
    expect(await screen.findByText("Default mode")).toBeInTheDocument();
    expect(screen.getByText("Per-app modes")).toBeInTheDocument();
    expect(screen.getByText(/Override the default mode when dictation starts/)).toBeInTheDocument();
  });

  it("does not render a context_policy control after the app-hint removal", async () => {
    renderWithProviders();

    expect(await screen.findByRole("region", { name: "Provider" })).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Context-aware cleanup" })).not.toBeInTheDocument();
  });

  it("renders deterministic editors in their own tabs with AI off", async () => {
    const user = userEvent.setup();
    renderWithProviders();

    await openPolishTab(user, "Dictionary");
    expect(await screen.findByText("Words & names")).toBeInTheDocument();
    await openPolishTab(user, "Corrections");
    expect(screen.getByRole("region", { name: "Corrections" })).toBeInTheDocument();
    await openPolishTab(user, "Snippets");
    expect(await screen.findByText("Saved text")).toBeInTheDocument();
  });

  it("keeps AiProviderStatus single-sourced and drops removed writing fields on merge", () => {
    const projectRoot = path.resolve(
      path.dirname(fileURLToPath(import.meta.url)),
      "..",
      "..",
      "..",
      "..",
    );
    const aiSrc = readFileSync(path.join(projectRoot, "src/types/ai.ts"), "utf8");
    const providersSrc = readFileSync(path.join(projectRoot, "src/types/providers.ts"), "utf8");
    expect(providersSrc).toMatch(/export type AiProviderStatus/);
    expect(aiSrc).not.toMatch(/AiProviderStatus/);

    const legacy = {
      replacements: [{ from: "x", to: "y", language: null, enabled: true }],
      custom_words: [],
      snippets: [],
      context_policy: "app_hint_only",
      voice_commands: [{ phrase: "insert comma", output: "comma", enabled: true }],
    } as unknown as Partial<typeof defaultWritingSettings>;
    const merged = mergeWritingSettings(legacy);
    expect(merged.replacements).toHaveLength(1);
    expect(merged.app_formatting_rules).toEqual([]);
    expect("context_policy" in merged).toBe(false);
    expect("voice_commands" in merged).toBe(false);
    expect(mergeWritingSettings({})).toEqual(defaultWritingSettings);
  });

  it("deletes the unused ProviderCard component", () => {
    const projectRoot = path.resolve(
      path.dirname(fileURLToPath(import.meta.url)),
      "..",
      "..",
      "..",
      "..",
    );
    expect(() =>
      readFileSync(path.join(projectRoot, "src/components/ProviderCard.tsx"), "utf8"),
    ).toThrow();
  });

  it("does not persist placeholder writing settings before backend settings load", async () => {
    const user = userEvent.setup();
    let resolveWritingSettings: (settings: typeof defaultWritingSettings) => void = () => {};
    const loadedWritingSettings = {
      ...defaultWritingSettings,
      replacements: [
        {
          from: "voice typer",
          to: "Voicetypr",
          language: null,
          enabled: true,
        },
      ],
    };
    const writingSettingsPromise = new Promise<typeof defaultWritingSettings>((resolve) => {
      resolveWritingSettings = resolve;
    });

    (invoke as ReturnType<typeof vi.fn>).mockImplementation(
      (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "list_ai_providers") {
          return Promise.resolve(providerListResponse);
        }
        if (cmd === "get_settings") {
          return Promise.resolve(baseAppSettings);
        }
        if (cmd === "save_settings") {
          return Promise.resolve(undefined);
        }
        if (cmd === "get_enhancement_options") {
          return Promise.resolve({ preset: "PersonalDictation" });
        }
        if (cmd === "update_enhancement_options") {
          return Promise.resolve(undefined);
        }
        if (cmd === "get_writing_settings") {
          return writingSettingsPromise;
        }
        if (cmd === "update_writing_settings") {
          return Promise.resolve(undefined);
        }
        if (cmd === "get_ai_settings") {
          return Promise.resolve(aiSettingsResponse);
        }
        if (cmd === "get_ai_settings_for_provider") {
          const provider = (args as { provider?: string })?.provider || "";
          return Promise.resolve({ ...aiSettingsResponse, provider });
        }
        if (cmd === "get_openai_config") {
          return Promise.resolve({ baseUrl: "https://api.openai.com/v1" });
        }
        if (cmd === "update_ai_settings") {
          aiSettingsResponse = {
            ...aiSettingsResponse,
            ...(args as typeof aiSettingsResponse),
          };
          return Promise.resolve(undefined);
        }
        if (cmd === "cache_ai_api_key") {
          return Promise.resolve(undefined);
        }
        return Promise.resolve(undefined);
      },
    );

    renderWithProviders();
    await openPolishTab(user, "Corrections");

    const addRuleButton = await screen.findByRole("button", {
      name: /add rule/i,
    });
    expect(addRuleButton).toBeDisabled();
    fireEvent.click(addRuleButton);
    expect(
      (invoke as ReturnType<typeof vi.fn>).mock.calls.some(
        ([cmd, args]) =>
          cmd === "update_writing_settings" &&
          (args as { settings?: typeof defaultWritingSettings })?.settings?.replacements.length ===
            0,
      ),
    ).toBe(false);

    resolveWritingSettings(loadedWritingSettings);
    await waitFor(() => expect(addRuleButton).toBeEnabled());
    await user.click(addRuleButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_writing_settings", {
        settings: expect.objectContaining({
          replacements: [
            ...loadedWritingSettings.replacements,
            expect.objectContaining({ from: "", to: "", enabled: true }),
          ],
        }),
      });
    });
  });

  it("adds an app mode override and persists writing settings", async () => {
    const user = userEvent.setup();
    renderWithProviders();
    await openModes(user);

    await user.click(await screen.findByRole("button", { name: /add override/i }));

    const appInput = await screen.findByPlaceholderText("App name, e.g. Slack");
    await user.type(appInput, "Slack");

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_writing_settings", {
        settings: expect.objectContaining({
          app_formatting_rules: [
            expect.objectContaining({
              app_name: "Slack",
              preset: "PersonalDictation",
              enabled: true,
            }),
          ],
        }),
      });
    });
  });

  it("adds a text replacement row and persists writing settings", async () => {
    const user = userEvent.setup();
    renderWithProviders();
    await openPolishTab(user, "Corrections");

    await user.click(await screen.findByRole("button", { name: /add rule/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_writing_settings", {
        settings: expect.objectContaining({
          replacements: [
            expect.objectContaining({
              from: "",
              to: "",
              enabled: true,
            }),
          ],
        }),
      });
    });
  });

  it("coalesces rapid writing settings saves so the latest edit wins on disk", async () => {
    const user = userEvent.setup();
    let resolveFirstSave: (() => void) | undefined;
    const firstSaveGate = new Promise<void>((resolve) => {
      resolveFirstSave = resolve;
    });
    let firstSaveStarted = false;

    (invoke as ReturnType<typeof vi.fn>).mockImplementation(
      (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "list_ai_providers") {
          return Promise.resolve(providerListResponse);
        }
        if (cmd === "get_settings") {
          return Promise.resolve(baseAppSettings);
        }
        if (cmd === "save_settings") {
          return Promise.resolve(undefined);
        }
        if (cmd === "get_enhancement_options") {
          return Promise.resolve({ preset: "PersonalDictation" });
        }
        if (cmd === "update_enhancement_options") {
          return Promise.resolve(undefined);
        }
        if (cmd === "get_writing_settings") {
          return Promise.resolve(defaultWritingSettings);
        }
        if (cmd === "update_writing_settings") {
          if (!firstSaveStarted) {
            firstSaveStarted = true;
            return firstSaveGate.then(() => Promise.resolve(undefined));
          }
          return Promise.resolve(undefined);
        }
        if (cmd === "get_ai_settings") {
          return Promise.resolve(aiSettingsResponse);
        }
        if (cmd === "get_ai_settings_for_provider") {
          const provider = (args as { provider?: string })?.provider || "";
          return Promise.resolve({ ...aiSettingsResponse, provider });
        }
        if (cmd === "get_openai_config") {
          return Promise.resolve({ baseUrl: "https://api.openai.com/v1" });
        }
        if (cmd === "update_ai_settings") {
          aiSettingsResponse = {
            ...aiSettingsResponse,
            ...(args as typeof aiSettingsResponse),
          };
          return Promise.resolve(undefined);
        }
        if (cmd === "cache_ai_api_key") {
          return Promise.resolve(undefined);
        }
        return Promise.resolve(undefined);
      },
    );

    renderWithProviders();
    await openPolishTab(user, "Corrections");

    const addRuleButton = await screen.findByRole("button", {
      name: /add rule/i,
    });

    await user.click(addRuleButton);
    await user.click(addRuleButton);

    resolveFirstSave?.();

    await waitFor(() => {
      const updateCalls = (invoke as ReturnType<typeof vi.fn>).mock.calls.filter(
        ([cmd]) => cmd === "update_writing_settings",
      );
      expect(updateCalls).toHaveLength(2);
      expect(updateCalls[1]?.[1]).toEqual({
        settings: expect.objectContaining({
          replacements: expect.arrayContaining([
            expect.objectContaining({ enabled: true }),
            expect.objectContaining({ enabled: true }),
          ]),
        }),
      });
    });
  });

  it("saves each queued writing settings snapshot instead of the latest ref for every save", async () => {
    const user = userEvent.setup();
    renderWithProviders();
    await openPolishTab(user, "Corrections");

    const addRuleButton = await screen.findByRole("button", {
      name: /add rule/i,
    });

    await user.click(addRuleButton);
    await user.click(addRuleButton);

    await waitFor(() => {
      const updateCalls = (invoke as ReturnType<typeof vi.fn>).mock.calls.filter(
        ([cmd]) => cmd === "update_writing_settings",
      );
      expect(updateCalls).toHaveLength(2);
      expect(
        (updateCalls[0]![1] as { settings: typeof defaultWritingSettings }).settings.replacements,
      ).toHaveLength(1);
      expect(
        (updateCalls[1]![1] as { settings: typeof defaultWritingSettings }).settings.replacements,
      ).toHaveLength(2);
    });
  });

  it("does not roll back writing settings when an older queued save fails after a newer edit", async () => {
    const user = userEvent.setup();
    let rejectFirstSave: (() => void) | undefined;
    const firstSaveGate = new Promise<void>((_, reject) => {
      rejectFirstSave = () => reject(new Error("stale save failed"));
    });
    let saveCount = 0;

    (invoke as ReturnType<typeof vi.fn>).mockImplementation(
      (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "list_ai_providers") {
          return Promise.resolve(providerListResponse);
        }
        if (cmd === "get_settings") {
          return Promise.resolve(baseAppSettings);
        }
        if (cmd === "save_settings") {
          return Promise.resolve(undefined);
        }
        if (cmd === "get_enhancement_options") {
          return Promise.resolve({ preset: "PersonalDictation" });
        }
        if (cmd === "update_enhancement_options") {
          return Promise.resolve(undefined);
        }
        if (cmd === "get_writing_settings") {
          return Promise.resolve(defaultWritingSettings);
        }
        if (cmd === "update_writing_settings") {
          saveCount += 1;
          return saveCount === 1 ? firstSaveGate : Promise.resolve(undefined);
        }
        if (cmd === "get_ai_settings") {
          return Promise.resolve(aiSettingsResponse);
        }
        if (cmd === "get_ai_settings_for_provider") {
          const provider = (args as { provider?: string })?.provider || "";
          return Promise.resolve({ ...aiSettingsResponse, provider });
        }
        if (cmd === "get_openai_config") {
          return Promise.resolve({ baseUrl: "https://api.openai.com/v1" });
        }
        if (cmd === "update_ai_settings") {
          aiSettingsResponse = {
            ...aiSettingsResponse,
            ...(args as typeof aiSettingsResponse),
          };
          return Promise.resolve(undefined);
        }
        if (cmd === "cache_ai_api_key") {
          return Promise.resolve(undefined);
        }
        return Promise.resolve(undefined);
      },
    );

    renderWithProviders();
    await openPolishTab(user, "Corrections");

    const addRuleButton = await screen.findByRole("button", {
      name: /add rule/i,
    });

    await user.click(addRuleButton);
    await waitFor(() => expect(saveCount).toBe(1));
    await user.click(addRuleButton);
    rejectFirstSave?.();

    await waitFor(() => {
      expect(saveCount).toBe(2);
      expect(screen.getByText("Rule 2")).toBeInTheDocument();
    });
    expect(toast.error).not.toHaveBeenCalledWith("stale save failed");
  });

  it("rolls back optimistic writing settings when save fails", async () => {
    const user = userEvent.setup();
    rejectWritingSettingsUpdate = true;
    renderWithProviders();
    await openPolishTab(user, "Corrections");

    await user.click(await screen.findByRole("button", { name: /add rule/i }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("disk full");
    });
    await waitFor(() => {
      expect(screen.queryByText("Rule 1")).not.toBeInTheDocument();
    });
  });

  it("restores a remembered model when saving an API key for another cloud provider", async () => {
    aiSettingsResponse = {
      ...enabledAISettings,
      enabled: false,
      modelsByProvider: {
        openai: "gpt-5-mini",
        gemini: "gemini-1.5-flash",
      },
    };
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(
      async (providerId: string) => providerId === "openai",
    );
    const user = userEvent.setup();
    renderWithProviders();

    await user.click(await screen.findByRole("button", { name: "Choose provider and model" }));

    await user.click(await screen.findByRole("button", { name: "Add Google Gemini API key" }));
    await user.type(await screen.findByLabelText("API Key"), "gemini-key");
    await user.click(screen.getByRole("button", { name: "Save API Key" }));

    await waitFor(() => {
      expect(saveApiKey).toHaveBeenCalledWith("gemini", "gemini-key");
      expect(screen.getByRole("combobox", { name: "Model for Google Gemini" })).toHaveValue(
        "Gemini 1.5 Flash",
      );
    });
  });

  it("surfaces model reselection and clears it when a replacement is chosen", async () => {
    aiSettingsResponse = {
      ...enabledAISettings,
      model: "",
      modelsByProvider: {},
      aiModelNeedsReselection: true,
    };
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(
      async (providerId: string) => providerId === "openai",
    );
    const user = userEvent.setup();
    renderWithProviders();

    const providersPanel = await getProviderSetupPanel();
    expect(
      within(providersPanel).getByText(
        "Your previous model is unavailable. Choose another model to continue.",
      ),
    ).toBeInTheDocument();
    await user.click(within(providersPanel).getByRole("combobox", { name: "Model for OpenAI" }));
    await user.click(await screen.findByRole("option", { name: /GPT-5 Mini/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_ai_settings", {
        enabled: true,
        provider: "openai",
        model: "gpt-5-mini",
      });
      expect(aiSettingsResponse.aiModelNeedsReselection).toBe(false);
    });
  });

  it("documents cloud and isolated local-agent setup in the guide", async () => {
    const user = userEvent.setup();
    renderWithProviders();

    await user.click(await screen.findByRole("button", { name: /polish guide/i }));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/cloud API or isolated local agent/i)).toBeInTheDocument();
    expect(within(dialog).getAllByText(/exact replacements/i)).not.toHaveLength(0);
  });

  it("uses a searchable model dialog and supported effort levels for Claude CLI", async () => {
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["low", "medium"],
    };
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();
    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));

    expect(await within(providersPanel).findAllByText("Installed")).not.toHaveLength(0);
    await user.click(
      within(providersPanel).getByRole("button", {
        name: "Model for Claude Code",
      }),
    );
    const modelDialog = await screen.findByRole("dialog", {
      name: "Choose a Claude Code model",
    });
    await user.type(
      within(modelDialog).getByRole("textbox", {
        name: "Search Claude Code models",
      }),
      "sonnet",
    );
    await user.click(
      within(modelDialog).getByRole("button", {
        name: /Sonnet.*claude-code\/sonnet/i,
      }),
    );

    expect(invoke).toHaveBeenCalledWith("update_ai_settings", {
      enabled: false,
      provider: "claude-code",
      model: "sonnet",
    });
    expect(
      within(providersPanel).getByRole("combobox", {
        name: "Effort for Claude Code",
      }),
    ).toHaveTextContent("Low");
  });

  it("restores the selected local agent and model after remount", async () => {
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["low", "medium"],
    };
    const user = userEvent.setup();
    const firstRender = renderWithProviders();
    const providersPanel = await getProviderSetupPanel();
    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));
    await user.click(
      within(providersPanel).getByRole("button", {
        name: "Model for Claude Code",
      }),
    );
    await user.click(
      within(await screen.findByRole("dialog")).getByRole("button", {
        name: /Sonnet.*claude-code\/sonnet/i,
      }),
    );
    await waitFor(() => {
      expect(aiSettingsResponse.provider).toBe("claude-code");
      expect(aiSettingsResponse.model).toBe("sonnet");
    });

    firstRender.unmount();
    renderWithProviders();
    const restoredLauncher = await screen.findByRole("button", {
      name: "Choose provider and model",
    });
    expect(restoredLauncher).toHaveTextContent("Claude Code");
    expect(restoredLauncher).toHaveTextContent("Sonnet");

    const restoredPanel = await getProviderSetupPanel();
    await user.click(within(restoredPanel).getByRole("tab", { name: "Local Agents" }));
    expect(
      await within(restoredPanel).findByRole("heading", {
        name: "Claude Code",
      }),
    ).toBeInTheDocument();
    expect(
      within(restoredPanel).getByRole("button", {
        name: "Model for Claude Code",
      }),
    ).toHaveTextContent("Sonnet");
    expect(
      within(restoredPanel).getByRole("combobox", {
        name: "Effort for Claude Code",
      }),
    ).toHaveTextContent("Low");
  });

  it("keeps an undiscovered saved local model label truthful", async () => {
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["low", "medium"],
    };
    aiSettingsResponse = {
      ...baseAISettings,
      provider: "claude-code",
      model: "legacy-model",
      modelsByProvider: { "claude-code": "legacy-model" },
    };
    const user = userEvent.setup();
    renderWithProviders();

    const providerSummary = await screen.findByRole("button", {
      name: "Choose provider and model",
    });
    expect(providerSummary).toHaveTextContent("Claude Code · Legacy Model · Effort low");
    const providersPanel = await getProviderSetupPanel();
    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));

    const modelButton = await within(providersPanel).findByRole("button", {
      name: "Model for Claude Code",
    });
    expect(modelButton).toHaveTextContent("Legacy Model");
    expect(modelButton).not.toHaveTextContent("CLI default");
  });

  it("persists the named pi CLI default as an empty model id", async () => {
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["off", "low", "medium"],
    };
    aiSettingsResponse = {
      ...baseAISettings,
      provider: "pi",
      model: "openai/gpt-5-mini",
      modelsByProvider: { pi: "openai/gpt-5-mini" },
    };
    const user = userEvent.setup();
    renderWithProviders();
    const providerSummary = await screen.findByRole("button", {
      name: "Choose provider and model",
    });
    expect(providerSummary).toHaveTextContent("pi · GPT-5 Mini · Thinking off");
    const providersPanel = await getProviderSetupPanel();

    const piModel = await within(providersPanel).findByRole("button", {
      name: "Model for pi",
    });
    expect(piModel).toHaveTextContent("GPT-5 Mini");
    await user.click(piModel);
    await user.click(
      await screen.findByRole("button", {
        name: /^Default.*pi\/default/i,
      }),
    );

    expect(invoke).toHaveBeenCalledWith("update_ai_settings", {
      enabled: false,
      provider: "pi",
      model: "",
    });
    expect(
      within(providersPanel).getByRole("combobox", { name: "Thinking for pi" }),
    ).toHaveTextContent("Off");
  });

  it("discovers local-agent models when its picker is opened", async () => {
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["off", "low", "medium"],
    };
    modelDiscovery.hiddenProviders.add("pi");
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));
    const piModel = await within(providersPanel).findByRole("button", {
      name: "Model for pi",
    });
    expect(modelDiscovery.fetchModels).not.toHaveBeenCalledWith("pi");

    await user.click(piModel);

    await waitFor(() => {
      expect(modelDiscovery.fetchModels).toHaveBeenCalledWith("pi");
    });
  });

  it("shows discovered Droid models beside its current default", async () => {
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["low", "medium"],
    };
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();
    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));
    await user.click(await within(providersPanel).findByRole("button", { name: "Select Droid" }));

    const droidModel = await within(providersPanel).findByRole("button", {
      name: "Model for Droid",
    });
    expect(droidModel).toHaveTextContent("Default");
    await user.click(droidModel);
    const modelDialog = await screen.findByRole("dialog", {
      name: "Choose a Droid model",
    });
    expect(
      within(modelDialog).getByRole("button", {
        name: /^Default.*droid\/default/i,
      }),
    ).toBeInTheDocument();
    expect(
      within(modelDialog).getByRole("button", {
        name: /^Opus 5.*droid\/claude-opus-5.*Reasoning/i,
      }),
    ).toBeInTheDocument();
    await user.type(
      within(modelDialog).getByRole("textbox", { name: "Search Droid models" }),
      "gpt-5.6",
    );
    expect(
      within(modelDialog).getByRole("button", {
        name: /^GPT-5.6 Sol.*droid\/gpt-5.6-sol.*Reasoning/i,
      }),
    ).toBeInTheDocument();
    expect(
      within(modelDialog).queryByRole("button", {
        name: /^Opus 5.*Reasoning/i,
      }),
    ).not.toBeInTheDocument();
    await user.click(
      within(modelDialog).getByRole("button", {
        name: /^GPT-5.6 Sol.*droid\/gpt-5.6-sol.*Reasoning/i,
      }),
    );

    expect(invoke).toHaveBeenCalledWith("update_ai_settings", {
      enabled: false,
      provider: "droid",
      model: "gpt-5.6-sol",
    });
  });

  it("probes local agents only when their setup tab is opened", async () => {
    agentCliProbeResponse = {
      state: "ready",
      reasoningLevels: ["low", "medium"],
    };
    const user = userEvent.setup();
    renderWithProviders();
    const providersPanel = await getProviderSetupPanel();

    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "probe_agent_cli")).toBe(
      false,
    );

    await user.click(within(providersPanel).getByRole("tab", { name: "Local Agents" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("probe_agent_cli", {
        provider: "claude-code",
        refresh: false,
      });
    });

    expect(
      vi
        .mocked(invoke)
        .mock.calls.some(
          ([command, args]) =>
            command === "list_provider_models" &&
            isAgentCliProviderForTest((args as { provider?: string })?.provider ?? ""),
        ),
    ).toBe(false);
  });

  describe("Polish failure banner", () => {
    const authCopy = "Polish failed — your API key was rejected. Update it below.";

    const emitEvent = (name: string, payload: unknown) => {
      act(() => {
        void eventListeners.get(name)?.({ payload });
      });
    };

    it("shows the inline banner with auth copy when an auth error fires", async () => {
      renderWithProviders();

      emitEvent("ai-enhancement-auth-error", "Please check your AI API key in settings.");

      expect(screen.getByText(authCopy)).toBeInTheDocument();
      expect(
        screen.getByRole("button", {
          name: "Dismiss Polish error",
        }),
      ).toBeInTheDocument();
    });

    it("shows the failure message for a generic polish error", async () => {
      renderWithProviders();

      emitEvent("enhancing-failed", {
        category: "service_unavailable",
        message: "AI service unavailable",
      });

      expect(screen.getByText("AI service unavailable")).toBeInTheDocument();
    });

    it("dismisses the banner", async () => {
      const user = userEvent.setup();
      renderWithProviders();

      emitEvent("ai-enhancement-auth-error", "Please check your AI API key in settings.");
      expect(screen.getByText(authCopy)).toBeInTheDocument();

      await user.click(
        screen.getByRole("button", {
          name: "Dismiss Polish error",
        }),
      );

      expect(screen.queryByText(authCopy)).not.toBeInTheDocument();
    });

    it("clears the banner when a polish run completes", async () => {
      renderWithProviders();

      emitEvent("ai-enhancement-auth-error", "Please check your AI API key in settings.");
      expect(screen.getByText(authCopy)).toBeInTheDocument();

      emitEvent("enhancing-completed", null);

      await waitFor(() => {
        expect(screen.queryByText(authCopy)).not.toBeInTheDocument();
      });
    });

    it("keeps the banner across remounts (tab switches) until dismissed", async () => {
      const view = renderWithProviders();

      emitEvent("ai-enhancement-auth-error", "Please check your AI API key in settings.");
      expect(screen.getByText(authCopy)).toBeInTheDocument();

      act(() => {
        view.unmount();
      });
      renderWithProviders();

      await waitFor(() => {
        expect(screen.getByText(authCopy)).toBeInTheDocument();
      });
    });
  });
});
