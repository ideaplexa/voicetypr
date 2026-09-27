import { useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AIProviderModel } from "@/types/providers";
import { createLogger } from "@/lib/logger";

const log = createLogger("providers");

type ProviderModelWire = AIProviderModel & {
  context_window?: number | null;
  source_provider?: string | null;
  cli_default?: boolean;
};

const normalizeProviderModels = (models: ProviderModelWire[]): AIProviderModel[] =>
  models.map((model) => {
    const hasSnakeCaseMetadata =
      Object.prototype.hasOwnProperty.call(model, "context_window") ||
      Object.prototype.hasOwnProperty.call(model, "source_provider") ||
      Object.prototype.hasOwnProperty.call(model, "cli_default");
    if (!hasSnakeCaseMetadata) {
      return model;
    }

    const {
      context_window: contextWindowSnake,
      source_provider: sourceProviderSnake,
      cli_default: cliDefaultSnake,
      ...rest
    } = model;
    return {
      ...rest,
      contextWindow: model.contextWindow ?? contextWindowSnake ?? null,
      sourceProvider: model.sourceProvider ?? sourceProviderSnake ?? null,
      cliDefault: model.cliDefault ?? cliDefaultSnake ?? false,
    };
  });

interface UseProviderModelsReturn {
  models: AIProviderModel[];
  loading: boolean;
  error: string | null;
  fetchModels: () => Promise<AIProviderModel[]>;
  clearModels: () => void;
}

/**
 * Hook for fetching and managing models for a specific provider.
 * Models are fetched on demand (when fetchModels is called).
 * Results are cached in component state.
 */
export function useProviderModels(providerId: string): UseProviderModelsReturn {
  const [models, setModels] = useState<AIProviderModel[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlightRef = useRef<Promise<AIProviderModel[]> | null>(null);

  const fetchModels = useCallback(async () => {
    // Don't fetch for custom provider (user defines model in config)
    if (providerId === "custom") {
      return [];
    }

    // Don't refetch if already loading
    if (inFlightRef.current) {
      return inFlightRef.current;
    }

    setLoading(true);
    setError(null);

    const request = (async () => {
      try {
        const fetchedModels = normalizeProviderModels(
          await invoke<ProviderModelWire[]>("list_provider_models", {
            provider: providerId,
          }),
        );
        setModels(fetchedModels);
        return fetchedModels;
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : String(err);
        setError(errorMessage);
        log.error(`Failed to fetch models for ${providerId}:`, err);
        return [];
      } finally {
        inFlightRef.current = null;
        setLoading(false);
      }
    })();

    inFlightRef.current = request;
    return request;
  }, [providerId]);

  const clearModels = useCallback(() => {
    setModels([]);
    setError(null);
  }, []);

  return {
    models,
    loading,
    error,
    fetchModels,
    clearModels,
  };
}

/**
 * Hook for managing models across all providers.
 * Provides a centralized way to fetch and cache models.
 */
export function useAllProviderModels() {
  const [modelsMap, setModelsMap] = useState<Record<string, AIProviderModel[]>>({});
  const [loadingMap, setLoadingMap] = useState<Record<string, boolean>>({});
  const [errorMap, setErrorMap] = useState<Record<string, string | null>>({});
  const inFlightMapRef = useRef<Record<string, Promise<AIProviderModel[]> | undefined>>({});
  const requestGenerationMapRef = useRef<Record<string, number | undefined>>({});

  const fetchModels = useCallback(
    async (
      providerId: string,
      signal?: AbortSignal,
      options?: { force?: boolean },
    ): Promise<AIProviderModel[]> => {
      if (signal?.aborted) return [];
      // Don't fetch for custom provider
      if (providerId === "custom") {
        return [];
      }

      // Ordinary fetches share one request per provider. A forced refresh is
      // used after an external CLI/account change and must supersede any older
      // discovery already in flight.
      const inFlightRequest = inFlightMapRef.current[providerId];
      if (inFlightRequest && !options?.force) {
        return inFlightRequest;
      }

      const generation = (requestGenerationMapRef.current[providerId] ?? 0) + 1;
      requestGenerationMapRef.current[providerId] = generation;
      setLoadingMap((prev) => ({ ...prev, [providerId]: true }));
      setErrorMap((prev) => ({ ...prev, [providerId]: null }));

      const release = () => {
        if (requestGenerationMapRef.current[providerId] !== generation) return;
        delete inFlightMapRef.current[providerId];
        setLoadingMap((prev) => ({ ...prev, [providerId]: false }));
      };
      const request = Promise.resolve()
        .then(() =>
          invoke<ProviderModelWire[]>("list_provider_models", {
            provider: providerId,
          }),
        )
        .then((wireModels) => {
          const fetchedModels = normalizeProviderModels(wireModels);
          if (signal?.aborted || requestGenerationMapRef.current[providerId] !== generation) {
            return [];
          }
          setModelsMap((prev) => ({ ...prev, [providerId]: fetchedModels }));
          return fetchedModels;
        })
        .catch((err: unknown) => {
          if (signal?.aborted || requestGenerationMapRef.current[providerId] !== generation) {
            return [];
          }
          const errorMessage = err instanceof Error ? err.message : String(err);
          setErrorMap((prev) => ({ ...prev, [providerId]: errorMessage }));
          log.error(`Failed to fetch models for ${providerId}:`, err);
          return [];
        })
        .finally(() => {
          signal?.removeEventListener("abort", release);
          release();
        });

      inFlightMapRef.current[providerId] = request;
      signal?.addEventListener("abort", release, { once: true });
      return request;
    },
    [],
  );

  const getModels = useCallback(
    (providerId: string): AIProviderModel[] => {
      return modelsMap[providerId] || [];
    },
    [modelsMap],
  );

  const isLoading = useCallback(
    (providerId: string): boolean => {
      return loadingMap[providerId] || false;
    },
    [loadingMap],
  );

  const getError = useCallback(
    (providerId: string): string | null => {
      return errorMap[providerId] || null;
    },
    [errorMap],
  );

  const clearModels = useCallback((providerId: string) => {
    setModelsMap((prev) => {
      const next = { ...prev };
      delete next[providerId];
      return next;
    });
    setErrorMap((prev) => {
      const next = { ...prev };
      delete next[providerId];
      return next;
    });
  }, []);

  return {
    fetchModels,
    getModels,
    isLoading,
    getError,
    clearModels,
  };
}
