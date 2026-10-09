import type { Dispatch, SetStateAction } from "react";
import { toast } from "sonner";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import type { AISettings } from "@/types/ai";
import type { AIProviderConfig } from "@/types/providers";

interface FastModeUnavailablePayload {
  provider?: string;
  model?: string;
}

/**
 * The backend refuses a fast-mode request the account can't use, retries at
 * standard speed, persists fast=off, then emits `ai-fast-mode-unavailable`.
 * Mirror that persisted state locally and tell the user once.
 */
export function useNativeFastModeUnavailable(
  providers: AIProviderConfig[],
  setAISettings: Dispatch<SetStateAction<AISettings>>,
) {
  useTauriEvent<FastModeUnavailablePayload>("ai-fast-mode-unavailable", (payload) => {
    const providerId = payload?.provider;
    if (!providerId) return;
    setAISettings((prev) => ({
      ...prev,
      fastModeByProvider: { ...prev.fastModeByProvider, [providerId]: false },
    }));
    const providerName =
      providers.find((provider) => provider.id === providerId)?.name ?? providerId;
    toast.info(
      `Fast mode isn't available for ${providerName} on your account — used standard speed and turned Fast off.`,
    );
  });
}
