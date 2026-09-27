import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { loadApiKeysToCache, removeApiKey, saveApiKey } from "./keyring";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
}));

describe("keyring provider isolation", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("clears custom cache using custom provider key", async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

    await removeApiKey("custom");

    expect(invoke).toHaveBeenNthCalledWith(1, "keyring_delete", { key: "ai_api_key_custom" });
    expect(invoke).toHaveBeenNthCalledWith(2, "clear_ai_api_key_cache", { provider: "custom" });
    expect(emit).toHaveBeenCalledWith("api-key-removed", { provider: "custom" });
  });

  it("validates before saving, caching, and emitting API key changes without resetting AI settings", async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

    await saveApiKey("anthropic", "anthropic-key");

    expect(invoke).toHaveBeenNthCalledWith(1, "validate_ai_api_key", {
      args: { provider: "anthropic", apiKey: "anthropic-key" },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "keyring_set", {
      key: "ai_api_key_anthropic",
      value: "anthropic-key",
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "cache_ai_api_key", {
      args: { provider: "anthropic", apiKey: "anthropic-key" },
    });
    expect(invoke).not.toHaveBeenCalledWith("update_ai_settings", expect.anything());
    expect(emit).toHaveBeenCalledWith("api-key-saved", { provider: "anthropic" });
  });

  it("persists nothing when validation fails", async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === "validate_ai_api_key") {
        return Promise.reject(new Error("invalid key"));
      }
      return Promise.resolve(undefined);
    });

    await expect(saveApiKey("gemini", "bad-key")).rejects.toThrow("invalid key");

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("validate_ai_api_key", {
      args: { provider: "gemini", apiKey: "bad-key" },
    });
    expect(emit).not.toHaveBeenCalled();
  });

  it("loads custom and openai keys into separate backend cache providers from the backend provider list", async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(
      (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "list_ai_providers") {
          return Promise.resolve([{ id: "openai" }, { id: "custom" }]);
        }
        if (cmd === "keyring_get") {
          const key = (args as { key: string }).key;
          if (key === "ai_api_key_openai") return Promise.resolve("openai-key");
          if (key === "ai_api_key_custom") return Promise.resolve("custom-key");
          return Promise.resolve(null);
        }
        return Promise.resolve(undefined);
      },
    );

    await loadApiKeysToCache();

    expect(invoke).toHaveBeenCalledWith("cache_ai_api_key", {
      args: { provider: "openai", apiKey: "openai-key" },
    });
    expect(invoke).toHaveBeenCalledWith("cache_ai_api_key", {
      args: { provider: "custom", apiKey: "custom-key" },
    });
    expect(invoke).not.toHaveBeenCalledWith("cache_ai_api_key", {
      args: { provider: "openai", apiKey: "custom-key" },
    });
  });
});
