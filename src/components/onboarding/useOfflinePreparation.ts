import { languages } from "@/components/languages";
import { useSettings } from "@/contexts/SettingsContext";
import type { OnboardingDesktopProps } from "@/components/onboarding/onboardingTypes";
import { isLocalModel, type ModelInfo } from "@/types";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

export function supportsSpokenLanguage(model: ModelInfo, language: string): boolean {
  if (model.supported_languages?.length) {
    return language === "auto"
      ? model.supported_languages.length > 1
      : model.supported_languages.includes(language);
  }
  if (model.engine === "whisper") return !/\.en$/i.test(model.name) || language === "en";
  // Without explicit capabilities, do not recommend an unknown local engine.
  return language === "en";
}

export function suggestedSpeechLanguage(saved: string | undefined, locales: readonly string[]): string {
  if (saved && saved !== "en") return saved;
  for (const locale of locales) {
    const code = locale.toLowerCase().split(/[-_]/)[0];
    if (languages.some((language) => language.value === code)) return code;
  }
  return "en";
}

const LANGUAGE_CHOSEN = "onboarding-language-chosen";

function languageModelPatch(management: OnboardingDesktopProps["modelManagement"], current: string | undefined, language: string) {
  const selected = current ? management.models[current] : undefined;
  if (selected && (!isLocalModel(selected) || supportsSpokenLanguage(selected, language))) return {};
  const compatible = management.modelOrder.filter((name) => {
    const model = management.models[name];
    return model && isLocalModel(model) && supportsSpokenLanguage(model, language);
  });
  const name = compatible.find((candidate) => management.models[candidate].recommended) ?? compatible[0];
  return name ? { current_model: name, current_model_engine: management.models[name].engine } : {};
}

export function useOfflinePreparation(management: OnboardingDesktopProps["modelManagement"]) {
  const { settings, updateSettings } = useSettings();
  const initialized = useRef(false);
  const [savingLanguage, setSavingLanguage] = useState(false);
  const [target, setTarget] = useState<string | null>(null);
  const [warningModel, setWarningModel] = useState<string | null>(null);
  const language = settings?.speech_language ?? "en";
  useEffect(() => {
    if (!settings || initialized.current || management.isLoading) return;
    initialized.current = true;
    // Remember explicit English as well: it is indistinguishable from the backend default.
    if (localStorage.getItem(LANGUAGE_CHOSEN)) return;
    const suggestion = suggestedSpeechLanguage(settings.speech_language, navigator.languages);
    if (suggestion === language) return;
    void updateSettings({ speech_language: suggestion, ...languageModelPatch(management, settings.current_model, suggestion) }).catch(() => {
      toast.error("Could not save your language. Try again.");
    });
  }, [settings, language, updateSettings, management]);

  const compatible = management.modelOrder.filter((name) => {
    const model = management.models[name];
    return model && isLocalModel(model) && supportsSpokenLanguage(model, language);
  });
  const selected = settings?.current_model;
  // The best match for the language, shown as "Recommended"; an already
  // selected compatible model still wins for what Continue prepares.
  const bestMatch = compatible.find((name) => management.models[name].recommended);
  const recommended = (selected && compatible.includes(selected) ? selected : undefined) ?? bestMatch ?? compatible[0];
  const name = target ?? recommended;
  const model = name ? management.models[name] : undefined;
  const ready = Boolean(model?.downloaded && !model.requires_setup && name &&
    !management.verifyingModels.has(name) && settings?.current_model === name &&
    settings?.current_model_engine === model.engine);

  const prepare = async (modelName: string) => {
    const info = management.models[modelName];
    if (!info) return;
    if (!supportsSpokenLanguage(info, language)) {
      if (info.downloaded) toast.error("Choose a model that supports your language.");
      else setWarningModel(modelName);
      return;
    }
    setTarget(modelName);
    // Selection is explicit; practice remains gated by verified availability.
    await updateSettings({ current_model: modelName, current_model_engine: info.engine });
    if (!info.downloaded && management.downloadProgress[modelName] === undefined &&
      !management.verifyingModels.has(modelName)) await management.downloadModel(modelName);
  };

  return {
    language, savingLanguage, recommended, bestMatch, name, model, ready, warningModel,
    changeLanguage: async (value: string) => {
      setSavingLanguage(true);
      try {
        await updateSettings({ speech_language: value, ...languageModelPatch(management, settings?.current_model, value) });
        localStorage.setItem(LANGUAGE_CHOSEN, "true");
        setTarget(null);
      } catch {
        toast.error("Could not save your language. Try again.");
      } finally { setSavingLanguage(false); }
    },
    prepare,
    download: (modelName: string) => {
      const info = management.models[modelName];
      if (info && !supportsSpokenLanguage(info, language)) setWarningModel(modelName);
      else void prepare(modelName).catch(() => toast.error("Could not prepare offline dictation. Try again."));
    },
    dismissWarning: () => setWarningModel(null),
    confirmDownload: () => {
      if (warningModel) void management.downloadModel(warningModel);
      setWarningModel(null);
    },
  };
}
