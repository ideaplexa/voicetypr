import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import type { SpeechModelEngine } from "@/types";
import { useCallback, useEffect, useMemo } from "react";
import { toast } from "sonner";

const log = createLogger("models");

export function useSpokenLanguage(supportedLanguages?: readonly string[]) {
  const { settings, updateSettings } = useSettings();
  const currentEngine = (settings?.current_model_engine ?? "whisper") as SpeechModelEngine;
  const currentModelName = settings?.current_model ?? "";
  const languageValue = settings?.speech_language ?? "en";

  const isEnglishOnlyModel = useMemo(() => {
    if (!settings) return false;
    if (currentEngine === "whisper") {
      return /\.en$/i.test(currentModelName);
    }
    if (currentEngine === "parakeet") {
      if (supportedLanguages) {
        return supportedLanguages.length === 1 && supportedLanguages[0] === "en";
      }
      return currentModelName.includes("-v2") || currentModelName === "parakeet-unified-640ms";
    }
    return false;
  }, [currentEngine, currentModelName, settings, supportedLanguages]);

  const handleLanguageChange = useCallback(
    async (value: string) => {
      try {
        await updateSettings({ speech_language: value });
      } catch (error) {
        log.error("Failed to update spoken language:", error);
        toast.error("Failed to update spoken language");
      }
    },
    [updateSettings],
  );

  useEffect(() => {
    if (!settings) return;
    if (isEnglishOnlyModel && settings.speech_language !== "en") {
      updateSettings({ speech_language: "en" }).catch((error) => {
        log.error("Failed to enforce English fallback:", error);
      });
    }
  }, [isEnglishOnlyModel, settings, updateSettings]);

  return {
    settings,
    updateSettings,
    currentEngine,
    currentModelName,
    languageValue,
    supportedLanguages,
    isEnglishOnlyModel,
    handleLanguageChange,
  };
}
