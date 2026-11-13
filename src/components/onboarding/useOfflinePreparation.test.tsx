import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { suggestedSpeechLanguage, supportsSpokenLanguage, useOfflinePreparation } from "@/components/onboarding/useOfflinePreparation";
import type { OnboardingDesktopProps } from "@/components/onboarding/onboardingTypes";
import type { AppSettings, ModelInfo } from "@/types";

const mocks = vi.hoisted(() => ({ settings: { speech_language: "en", current_model: "base.en", current_model_engine: "whisper" }, save: vi.fn() }));
vi.mock("@/contexts/SettingsContext", () => ({ useSettings: () => ({ settings: mocks.settings, updateSettings: mocks.save }) }));
const english: ModelInfo = { name: "base.en", display_name: "English", engine: "whisper", kind: "local", recommended: true,
  downloaded: false, requires_setup: false, size: 100, url: "", sha256: "", speed_score: 5, accuracy_score: 5 };
const multilingual: ModelInfo = { ...english, name: "base", display_name: "Multilingual", recommended: false };
function management() {
  return { models: { "base.en": english, base: multilingual }, modelOrder: ["base.en", "base"], isLoading: false,
    verifyingModels: new Set<string>(), downloadProgress: {}, downloadModel: vi.fn().mockResolvedValue(undefined) } as unknown as OnboardingDesktopProps["modelManagement"];
}
beforeEach(() => {
  localStorage.clear();
  Object.assign(mocks.settings, { speech_language: "en", current_model: "base.en", current_model_engine: "whisper" });
  mocks.save.mockReset();
  mocks.save.mockImplementation(async (updates: Partial<AppSettings>) => { Object.assign(mocks.settings, updates); });
  vi.spyOn(navigator, "languages", "get").mockReturnValue(["en-US"]);
});

it("suggests a supported system language, preserves a saved language, and falls back to English", () => {
  expect(suggestedSpeechLanguage("en", ["bn-BD"])).toBe("bn");
  expect(suggestedSpeechLanguage("fr", ["bn-BD"])).toBe("fr");
  expect(suggestedSpeechLanguage("en", ["xx-XX"])).toBe("en");
  expect(supportsSpokenLanguage(english, "bn")).toBe(false);
  expect(supportsSpokenLanguage(multilingual, "bn")).toBe(true);
  expect(supportsSpokenLanguage({ ...multilingual, supported_languages: ["en", "de"] }, "bn")).toBe(false);
});

it("persists a locale suggestion with a compatible model before recommending it", async () => {
  vi.spyOn(navigator, "languages", "get").mockReturnValue(["bn-BD"]);
  const { result, rerender } = renderHook(() => useOfflinePreparation(management()));
  await act(async () => {});
  expect(mocks.save).toHaveBeenCalledWith({ speech_language: "bn", current_model: "base", current_model_engine: "whisper" });
  rerender();
  expect(result.current.language).toBe("bn");
  expect(result.current.recommended).toBe("base");
});

it("keeps an explicit English choice on relaunch instead of suggesting the locale again", async () => {
  vi.spyOn(navigator, "languages", "get").mockReturnValue(["bn-BD"]);
  const first = renderHook(() => useOfflinePreparation(management()));
  await act(async () => {});
  await act(async () => first.result.current.changeLanguage("en"));
  first.unmount();
  mocks.save.mockClear();
  const next = renderHook(() => useOfflinePreparation(management()));
  await act(async () => {});
  expect(next.result.current.language).toBe("en");
  expect(mocks.save).not.toHaveBeenCalled();
});

it("warns before downloading an incompatible model and never selects it", async () => {
  mocks.settings.speech_language = "bn";
  mocks.settings.current_model = "base";
  const models = management();
  const { result } = renderHook(() => useOfflinePreparation(models));
  await act(async () => result.current.download("base.en"));
  expect(result.current.warningModel).toBe("base.en");
  expect(models.downloadModel).not.toHaveBeenCalled();
  act(() => result.current.confirmDownload());
  expect(models.downloadModel).toHaveBeenCalledWith("base.en");
  expect(mocks.settings.current_model).toBe("base");
  expect(mocks.settings.speech_language).toBe("bn");
});
