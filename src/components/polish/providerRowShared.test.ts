import { describe, expect, it, vi } from "vitest";
import {
  buildProviderRowViewModel,
  type ProviderRowProps,
} from "@/components/polish/providerRowShared";

function props(providerId: string): ProviderRowProps {
  return {
    provider: {
      id: providerId,
      name: providerId,
      status: "production",
      color: "",
      apiKeyUrl: "",
      installHint: "",
      isCustom: false,
    },
    aiSettings: {
      enabled: true,
      provider: providerId,
      model: "",
      hasApiKey: false,
      modelsByProvider: {},
      reasoningByProvider: {},
      fastModeByProvider: {},
    },
    providerApiKeys: {},
    agentCliStatus: {},
    agentCliProbing: {},
    customModelName: "",
    guidedSetupProvider: null,
    setGuidedSetupProvider: vi.fn(),
    isModelsLoading: () => false,
    getModels: () => [],
    fetchModels: vi.fn(),
    getError: () => null,
    getDisplayModels: () => [],
    providerQuery: "",
    showGuidedSetup: false,
    onSelectModel: vi.fn(),
    onSelectReasoning: vi.fn(),
    onToggleFastMode: vi.fn(),
    onRefreshAgentCli: vi.fn(),
    onSetupApiKey: vi.fn(),
    onRemoveApiKey: vi.fn(),
  };
}

describe("Polish provider defaults", () => {
  it("shows Codex fast mode on and medium reasoning when settings are absent", () => {
    const view = buildProviderRowViewModel(props("codex"));
    expect(view.fastModeEnabled).toBe(true);
    expect(view.reasoning).toBe("medium");
  });

  it("preserves explicit Codex false and low settings", () => {
    const input = props("codex");
    input.aiSettings.fastModeByProvider.codex = false;
    input.aiSettings.reasoningByProvider.codex = "low";
    const view = buildProviderRowViewModel(input);
    expect(view.fastModeEnabled).toBe(false);
    expect(view.reasoning).toBe("low");
  });

  it.each(["claude-code", "pi", "omp", "droid", "grok", "opencode", "cline"])(
    "keeps %s defaults unchanged",
    (provider) => {
      const view = buildProviderRowViewModel(props(provider));
      expect(view.fastModeEnabled).toBe(false);
      expect(view.reasoning).toBe(provider === "pi" || provider === "omp" ? "off" : "low");
    },
  );
});
