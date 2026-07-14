import { useCallback } from "react";
import { toast } from "sonner";
import { ModelsSection } from "../sections/ModelsSection";
import { useSettings } from "@/contexts/SettingsContext";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { AppSettings } from "@/types";
import { createLogger } from "@/lib/logger";

const log = createLogger("models-tab");

export function ModelsTab() {
  const { settings, updateSettings } = useSettings();

  // Use the model management context
  const {
    downloadProgress,
    downloadPhases,
    verifyingModels,
    downloadErrors,
    isLoading,
    downloadModel,
    cancelDownload,
    deleteModel,
    repairModel,
    loadModels,
    sortedModels
  } = useModelManagementContext();


  // Save settings
  const saveSettings = useCallback(
    async (updates: Partial<AppSettings>) => {
      try {
        await updateSettings(updates);
      } catch (error) {
        log.error("Failed to save settings:", error);
      }
    },
    [updateSettings]
  );

  // Handle deleting a model with settings update
  const handleDeleteModel = useCallback(
    async (modelName: string) => {
      // delete_model failures are surfaced to the user by the hook; swallow the
      // rethrow here so ModelCard's fire-and-forget onDelete(name) does not
      // become an unhandled rejection. Selection stays unchanged on failure.
      let deleted = false;
      try {
        deleted = await deleteModel(modelName);
      } catch (error) {
        log.error("Model deletion failed:", error);
        return;
      }

      // If deleted model was the current one, clear selection in settings
      if (deleted && settings?.current_model === modelName) {
        await saveSettings({ current_model: "", current_model_engine: 'whisper' });
      }
    },
    [deleteModel, settings, saveSettings]
  );


  return (
    <ModelsSection
      models={sortedModels}
      downloadProgress={downloadProgress}
      downloadPhases={downloadPhases}
      verifyingModels={verifyingModels}
      downloadErrors={downloadErrors}
      isLoading={isLoading}
      currentModel={settings?.current_model}
      onDownload={downloadModel}
      onDelete={handleDeleteModel}
      onCancelDownload={cancelDownload}
      onRepair={repairModel}
      onSelect={async (modelName) => {
        if (!settings) return;
        const selectedModel = sortedModels.find(([name]) => name === modelName)?.[1];
        const engine = selectedModel?.engine ?? 'whisper';
        const previousSpeechLanguage = settings.speech_language;
        const supportedLanguages = selectedModel?.supported_languages;
        const requiresSpeechLanguageReset =
          (engine === 'whisper' && /\.en$/i.test(modelName) && previousSpeechLanguage !== 'en') ||
          (engine === 'parakeet' &&
            supportedLanguages !== undefined &&
            supportedLanguages !== null &&
            !supportedLanguages.includes(previousSpeechLanguage));

        await saveSettings({
          current_model: modelName,
          current_model_engine: engine,
          ...(requiresSpeechLanguageReset ? { speech_language: 'en' } : {}),
        });

        if (requiresSpeechLanguageReset) {
          toast.info('Spoken language reset to English for the new model.');
        }
      }}
      refreshModels={async () => {
        await loadModels();
      }}
    />
  );
}
