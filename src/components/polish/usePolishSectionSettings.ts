import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import type { EnhancementOptions, EnhancementPreset } from "@/types/ai";
import { fromBackendOptions, toBackendOptions } from "@/types/ai";
import type { AppSettings } from "@/types";
import type { WritingSettings } from "@/types/writing";
import { defaultWritingSettings, mergeWritingSettings } from "@/types/writing";
import { getErrorMessage } from "@/utils/error";
import { createLogger } from "@/lib/logger";

const log = createLogger("enhancements");

export interface UsePolishSectionSettingsOptions {
  settings: AppSettings | null;
  updateSettings: (updates: Partial<AppSettings>) => Promise<void>;
}

export function usePolishSectionSettings({
  settings,
  updateSettings,
}: UsePolishSectionSettingsOptions) {
  const [enhancementOptions, setEnhancementOptions] = useState<{
    preset: EnhancementPreset;
  }>({
    preset: "PersonalDictation",
  });
  const [writingSettings, setWritingSettings] = useState<WritingSettings>(defaultWritingSettings);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const writingSaveGeneration = useRef(0);
  const enhancementSaveGeneration = useRef(0);
  const writingSettingsRef = useRef(writingSettings);
  const writingSaveQueueRef = useRef<Promise<void> | null>(null);

  useEffect(() => {
    writingSettingsRef.current = writingSettings;
  }, [writingSettings]);

  const loadEnhancementOptions = async (aiEnabled: boolean, signal?: AbortSignal) => {
    try {
      const options = await invoke<EnhancementOptions>("get_enhancement_options");
      if (signal?.aborted) return;
      setEnhancementOptions(fromBackendOptions(options, aiEnabled));
    } catch (error) {
      if (signal?.aborted) return;
      log.error("Failed to load Polish options:", error);
    }
  };

  const loadWritingSettings = async (signal?: AbortSignal) => {
    try {
      const nextSettings = await invoke<Partial<WritingSettings>>("get_writing_settings");
      if (signal?.aborted) return false;
      setWritingSettings(mergeWritingSettings(nextSettings));
      return true;
    } catch (error) {
      if (signal?.aborted) return false;
      log.error("Failed to load writing settings:", error);
      return false;
    }
  };

  const persistEnhancementOptions = async (nextOptions: { preset: EnhancementPreset }) => {
    const rollbackOptions = enhancementOptions;
    const generationAtEnqueue = enhancementSaveGeneration.current + 1;
    enhancementSaveGeneration.current = generationAtEnqueue;
    setEnhancementOptions(nextOptions);
    try {
      await invoke("update_enhancement_options", {
        options: toBackendOptions(nextOptions),
      });
    } catch (error) {
      if (enhancementSaveGeneration.current === generationAtEnqueue) {
        setEnhancementOptions(rollbackOptions);
      }
      const message = getErrorMessage(error, "Failed to save Polish settings");
      toast.error(message);
    }
  };

  const enqueueWritingSettingsSave = (
    settingsToSave: WritingSettings,
    rollbackSettings: WritingSettings,
    generationAtEnqueue: number,
  ) => {
    const queue = writingSaveQueueRef.current ?? Promise.resolve();
    writingSaveQueueRef.current = queue.then(async () => {
      try {
        await invoke("update_writing_settings", { settings: settingsToSave });
      } catch (error) {
        if (writingSaveGeneration.current === generationAtEnqueue) {
          setWritingSettings(rollbackSettings);
          writingSettingsRef.current = rollbackSettings;
          const message = getErrorMessage(error, "Failed to save writing settings");
          toast.error(message);
        }
      }
    });
  };

  const handleWritingSettingsChange = (nextSettings: WritingSettings) => {
    const rollbackSettings = writingSettingsRef.current;
    const generationAtEnqueue = writingSaveGeneration.current + 1;
    writingSaveGeneration.current = generationAtEnqueue;
    setWritingSettings(nextSettings);
    writingSettingsRef.current = nextSettings;
    if (settingsLoaded) {
      enqueueWritingSettingsSave(nextSettings, rollbackSettings, generationAtEnqueue);
    }
  };

  const handleFinalTextLanguageChange = async (value: string) => {
    if (!settings) return;
    const nextTask = value === "en" ? "translate_to_english" : "transcribe";
    try {
      await updateSettings({
        final_text_language: value,
        transcription_task: nextTask,
      });
    } catch (error) {
      const message = getErrorMessage(error, "Failed to save final text language");
      toast.error(message);
    }
  };

  const persistEnhancementOptionsRef = useRef(persistEnhancementOptions);
  const loadEnhancementOptionsRef = useRef(loadEnhancementOptions);
  const loadWritingSettingsRef = useRef(loadWritingSettings);
  const handleFinalTextLanguageChangeRef = useRef(handleFinalTextLanguageChange);
  const updateSettingsRef = useRef(updateSettings);
  useEffect(() => {
    persistEnhancementOptionsRef.current = persistEnhancementOptions;
    loadEnhancementOptionsRef.current = loadEnhancementOptions;
    loadWritingSettingsRef.current = loadWritingSettings;
    handleFinalTextLanguageChangeRef.current = handleFinalTextLanguageChange;
    updateSettingsRef.current = updateSettings;
  });

  const handlePolishEnabled = useCallback(async () => {
    if (enhancementOptions.preset !== "CleanDictation") {
      await persistEnhancementOptionsRef.current({ preset: "CleanDictation" });
    }
  }, [enhancementOptions.preset]);

  const handleEnabledModelSelected = useCallback(async () => {
    await loadEnhancementOptionsRef.current(true);
  }, []);

  const handlePolishToggled = useCallback(
    async (enabled: boolean) => {
      const nextPreset: EnhancementPreset = enabled ? "CleanDictation" : "PersonalDictation";

      if (
        nextPreset === "PersonalDictation" &&
        settings?.final_text_language &&
        settings.final_text_language !== "same_as_transcript"
      ) {
        await handleFinalTextLanguageChangeRef.current("same_as_transcript");
      }

      if (nextPreset !== enhancementOptions.preset) {
        await persistEnhancementOptionsRef.current({ preset: nextPreset });
      }
    },
    [settings, enhancementOptions.preset],
  );

  const handleActiveProviderCleared = useCallback(async () => {
    setEnhancementOptions({ preset: "PersonalDictation" });
    try {
      await updateSettingsRef.current({
        final_text_language: "same_as_transcript",
        transcription_task: "transcribe",
      });
    } catch (error) {
      log.error("Failed to refresh language settings after API key removal:", error);
    }
  }, []);

  return {
    enhancementOptions,
    setEnhancementOptions,
    writingSettings,
    settingsLoaded,
    setSettingsLoaded,
    persistEnhancementOptions,
    handleWritingSettingsChange,
    handleFinalTextLanguageChange,
    handlePolishEnabled,
    handleEnabledModelSelected,
    handlePolishToggled,
    handleActiveProviderCleared,
    loadEnhancementOptionsRef,
    loadWritingSettingsRef,
  };
}
