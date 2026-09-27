import { invoke } from "@tauri-apps/api/core";

import type { AiProvider } from "@/types/providers";
import { emit } from "@tauri-apps/api/event";
import { createLogger } from "@/lib/logger";

const log = createLogger("keyring");

/**
 * Save a value to the OS keyring
 * @param key The key to store the value under
 * @param value The value to store
 */
export const keyringSet = async (key: string, value: string): Promise<void> => {
  await invoke("keyring_set", { key, value });
};

/**
 * Get a value from the OS keyring
 * @param key The key to retrieve
 * @returns The value if found, null otherwise
 */
export const keyringGet = async (key: string): Promise<string | null> => {
  return await invoke<string | null>("keyring_get", { key });
};

/**
 * Delete a value from the OS keyring
 * @param key The key to delete
 */
export const keyringDelete = async (key: string): Promise<void> => {
  await invoke("keyring_delete", { key });
};

/**
 * Check if a key exists in the OS keyring
 * @param key The key to check
 * @returns true if the key exists, false otherwise
 */
export const keyringHas = async (key: string): Promise<boolean> => {
  return await invoke<boolean>("keyring_has", { key });
};

interface SaveApiKeyOptions {
  baseUrl?: string;
  model?: string;
  noAuth?: boolean;
}

// API Key specific helpers

export const saveApiKey = async (
  provider: string,
  apiKey: string,
  options?: SaveApiKeyOptions,
): Promise<void> => {
  const key = `ai_api_key_${provider}`;

  await invoke("validate_ai_api_key", { args: { provider, apiKey, ...options } });
  await keyringSet(key, apiKey);
  await invoke("cache_ai_api_key", { args: { provider, apiKey } });
  log.debug(`[Keyring] API key saved and validated for ${provider}`);

  // Emit event to notify that API key was saved
  await emit("api-key-saved", { provider });
};

export const getApiKey = async (provider: string): Promise<string | null> => {
  const key = `ai_api_key_${provider}`;
  return await keyringGet(key);
};

export const hasApiKey = async (provider: string): Promise<boolean> => {
  const key = `ai_api_key_${provider}`;
  return await keyringHas(key);
};

export const removeApiKey = async (provider: string): Promise<void> => {
  const key = `ai_api_key_${provider}`;
  await keyringDelete(key);

  // Clear backend cache for the same provider key
  await invoke("clear_ai_api_key_cache", { provider });

  log.debug(`[Keyring] API key removed for ${provider}`);

  // Emit event to notify that API key was removed
  await emit("api-key-removed", { provider });
};

// Load all API keys to backend cache (for app startup)
export const loadApiKeysToCache = async (): Promise<void> => {
  let providers: AiProvider[];

  try {
    providers = await invoke<AiProvider[]>("list_ai_providers");
  } catch (error) {
    log.error("Failed to list AI providers for key cache warmup:", error);
    return;
  }

  await Promise.all(
    providers.map(async (provider) => {
      try {
        const apiKey = await getApiKey(provider.id);
        if (apiKey) {
          await invoke("cache_ai_api_key", { args: { provider: provider.id, apiKey } });
          log.debug(`[Keyring] Loaded ${provider.id} API key from keyring to cache`);
        }
      } catch (error) {
        log.error(`Failed to load API key for ${provider.id}:`, error);
      }
    }),
  );
};

// STT (Speech-to-Text) cloud provider keys
export const saveSttApiKey = async (provider: string, apiKey: string): Promise<void> => {
  // Validate first; only persist on success
  await invoke("validate_stt_key", { provider, apiKey });
  await keyringSet(`stt_api_key_${provider}`, apiKey);
  await emit("stt-key-saved", { provider });
};

export const hasSttApiKey = async (provider: string): Promise<boolean> => {
  return keyringHas(`stt_api_key_${provider}`);
};

export const removeSttApiKey = async (provider: string): Promise<void> => {
  await keyringDelete(`stt_api_key_${provider}`);
  try {
    await invoke("clear_stt_key_cache", { provider });
  } catch {
    // best-effort
  }
  await emit("stt-key-removed", { provider });
};
