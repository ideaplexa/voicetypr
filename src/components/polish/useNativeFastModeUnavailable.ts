import type { Dispatch, SetStateAction } from "react";
import { toast } from "sonner";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import type { AISettings } from "@/types/ai";

interface FastModeUnavailablePayload {
  provider?: string;
  model?: string;
}

/** Keep the mounted formatting screen in sync with the backend's persisted fast=off. */
export function useNativeFastModeUnavailable(
  setAISettings: Dispatch<SetStateAction<AISettings>>,
) {
  useTauriEvent<FastModeUnavailablePayload>("ai-fast-mode-unavailable", (payload) => {
    const providerId = payload?.provider;
    if (!providerId) return;
    setAISettings((prev) => ({
      ...prev,
      fastModeByProvider: { ...prev.fastModeByProvider, [providerId]: false },
    }));
  });
}

/** Main-window notice subscription, independent of which screen is mounted. */
export function useNativeFastModeUnavailableNotice() {
  useTauriEvent<FastModeUnavailablePayload>("ai-fast-mode-unavailable", (payload) => {
    const providerId = payload?.provider;
    if (!providerId) return;
    const providerNames: Record<string, string> = {
      openai: "OpenAI",
      anthropic: "Anthropic",
      gemini: "Google Gemini",
    };
    toast.info(
      `Fast mode isn't available for ${providerNames[providerId] ?? providerId} on your account — used standard speed and turned Fast off.`,
    );
  });
}
