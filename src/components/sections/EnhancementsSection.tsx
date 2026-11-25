import { toast } from "sonner";
import { ProviderSetupDialog } from "@/components/polish/ProviderSetupDialog";
import { useSettings } from "@/contexts/SettingsContext";
import { EnhancementsHeader, type PolishOutcome } from "@/components/polish/EnhancementsHeader";
import { EnhancementSettingsPanel } from "@/components/polish/EnhancementSettingsPanel";
import { EnhancementsProviderModals } from "@/components/polish/EnhancementsProviderModals";
import { useAiProviderSettings, type ProviderTab } from "@/components/polish/useAiProviderSettings";
import { usePolishSectionSettings } from "@/components/polish/usePolishSectionSettings";
import { usePolishSettingsLoad } from "@/components/polish/usePolishSettingsLoad";
import {
  AGENT_CLI_DEFAULT_LABEL,
  formatAgentCliReasoning,
  defaultAgentCliReasoning,
  isAgentCliProvider,
  shortModelName,
} from "@/components/polish/agentCli";
import { useCallback, useEffect, useState } from "react";
import { useReadinessState } from "@/contexts/ReadinessContext";
import { KeepWordsSetting } from "@/components/polish/KeepWordsSetting";
import { PolishContextSetting } from "@/components/polish/PolishContextSetting";
import { BuiltInPolishSetting, builtinPolishView } from "@/components/polish/BuiltInPolishSetting";
import { OwnProviderSetting } from "@/components/polish/OwnProviderSetting";
import { usePolishStatus } from "@/components/polish/usePolishStatus";
import { useLicense } from "@/contexts/LicenseContext";
import { SettingsPage } from "@/components/settings/settings-ui";
import { SectionIntro } from "@/components/discovery/SectionIntro";
import { SpokenPunctuationField } from "@/components/polish/SpokenPunctuationField";
import { VoiceCommandsField } from "@/components/polish/VoiceCommandsField";
export function EnhancementsSection() {
  const readiness = useReadinessState();
  const { status: licenseStatus } = useLicense();
  const { settings, updateSettings } = useSettings();
  const [providerSearch, setProviderSearch] = useState("");
  const [providerSetupOpen, setProviderSetupOpen] = useState(false);
  const [providerTab, setProviderTab] = useState<ProviderTab>("cloud");

  const {
    enhancementOptions,
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
  } = usePolishSectionSettings({ settings, updateSettings });

  const {
    aiSettings,
    aiModelNeedsReselection,
    providers,
    providerApiKeys,
    agentCliStatus,
    agentCliProbing,
    showApiKeyModal,
    setShowApiKeyModal,
    showOpenAIConfig,
    setShowOpenAIConfig,
    openAIDefaultBaseUrl,
    setOpenAIDefaultBaseUrl,
    customModelName,
    setCustomModelName,
    selectedProvider,
    guidedSetupProvider,
    setGuidedSetupProvider,
    isLoading,
    setIsLoading,
    setProviderApiKeys,
    setAISettings,
    loadAISettings,
    probeAgentCli,
    handleRefreshAgentCli,
    handleToggleEnabled,
    handleSetupApiKey,
    handleApiKeySubmit,
    handleRemoveApiKey,
    handleSelectModel,
    handleSelectReasoning,
    handleToggleFastMode,
    getDisplayModels,
    isModelsLoading,
    getModels,
    fetchModels,
    getError,
  } = useAiProviderSettings({
    readinessAiReady: readiness?.ai_ready,
    settingsLoaded,
    onPolishEnabled: handlePolishEnabled,
    onModelSelected: () => setProviderSetupOpen(true),
    onEnabledModelSelected: handleEnabledModelSelected,
    onPolishToggled: handlePolishToggled,
    onActiveProviderCleared: handleActiveProviderCleared,
  });

  usePolishSettingsLoad({
    settingsLoaded,
    setSettingsLoaded,
    loadAISettings,
    loadEnhancementOptionsRef,
    loadWritingSettingsRef,
  });

  const isUsingCustomProvider = aiSettings.provider === "custom";
  const hasSelectedModel = Boolean(
    aiSettings.provider &&
    (aiSettings.model || isAgentCliProvider(aiSettings.provider)) &&
    (isUsingCustomProvider || providerApiKeys[aiSettings.provider]),
  );
  const builtinChosen = settings?.builtin_polish_enabled === true;
  const { status: polishStatus } = usePolishStatus(`${licenseStatus?.status}:${hasSelectedModel}:${aiSettings.enabled}:${builtinChosen}`);
  const builtinPolishActive = builtinPolishView(polishStatus?.builtin ?? null, builtinChosen).active;
  // The gate dictation uses, as Home and Capabilities read it.
  const polishOutcome: PolishOutcome | null = !polishStatus ? null
    : polishStatus.will_run ? "on"
    : !polishStatus.own_provider_ready && polishStatus.builtin_enabled ? "waiting"
    : "off";
  const showAiModelReselectionNotice =
    aiModelNeedsReselection && aiSettings.enabled && !aiSettings.model;
  const effectiveFinalTextLanguage =
    enhancementOptions.preset === "PersonalDictation"
      ? "same_as_transcript"
      : (settings?.final_text_language ?? "same_as_transcript");
  const selectedDisplayModel = getDisplayModels(aiSettings.provider).find(
    (model) => model.id === aiSettings.model,
  );
  const activeModelName = isUsingCustomProvider
    ? customModelName
    : !aiSettings.model && isAgentCliProvider(aiSettings.provider)
      ? AGENT_CLI_DEFAULT_LABEL
      : shortModelName(aiSettings.model, selectedDisplayModel?.name);
  const activeProviderName =
    providers.find((p) => p.id === aiSettings.provider)?.name || aiSettings.provider;
  const activeReasoningLevels = agentCliStatus[aiSettings.provider]?.reasoningLevels ?? [];
  const activeReasoningName =
    isAgentCliProvider(aiSettings.provider) && activeReasoningLevels.length > 0
      ? formatAgentCliReasoning(
          aiSettings.provider,
          aiSettings.reasoningByProvider[aiSettings.provider] ??
            defaultAgentCliReasoning(aiSettings.provider),
        )
      : "";

  const openProviderSetup = useCallback(() => {
    setProviderTab(isAgentCliProvider(aiSettings.provider) ? "local" : "cloud");
    setProviderSearch("");
    setProviderSetupOpen(true);
  }, [aiSettings.provider]);

  const showGuidedSetup = settingsLoaded && !hasSelectedModel;
  useEffect(() => {
    if (!providerSetupOpen || providerTab !== "local") return;

    for (const candidate of providers) {
      if (!isAgentCliProvider(candidate.id)) continue;
      if (!agentCliStatus[candidate.id]) {
        void probeAgentCli(candidate.id, false);
      }
    }
  }, [agentCliStatus, providerSetupOpen, probeAgentCli, providerTab, providers]);

  return (
    <div className="h-full min-h-0 min-w-0 flex flex-col overflow-x-hidden">
      <SettingsPage className="gap-[18px]">
      <EnhancementsHeader outcome={polishOutcome} />
      <SectionIntro
        art="polish"
        line={!settingsLoaded || hasSelectedModel || builtinChosen ? "Pick a default style, and give any app its own." : "Polish needs an AI provider below: a key, or a tool you already have like Claude Code."}
      />
      <VoiceCommandsField
        enabled={writingSettings.editing_commands}
        disabled={!settingsLoaded}
        onChange={(editing_commands) => handleWritingSettingsChange({ editing_commands })}
      />
      <SpokenPunctuationField
        enabled={writingSettings.spoken_punctuation}
        disabled={!settingsLoaded}
        onChange={(spoken_punctuation) => handleWritingSettingsChange({ spoken_punctuation })}
      />

      <BuiltInPolishSetting chosen={settings?.builtin_polish_enabled === true}
        availability={polishStatus?.builtin ?? null}
        onCheckedChange={(checked) => void Promise.resolve(updateSettings({ builtin_polish_enabled: checked }, { publishAfterSave: true }))
          .catch(() => { toast.error("Couldn’t save the Polish switch. Try again."); })} />
      <OwnProviderSetting enabled={aiSettings.enabled} hasSelectedModel={hasSelectedModel}
        providerName={activeProviderName}
        onCheckedChange={handleToggleEnabled} onOpenProviderSetup={openProviderSetup} />
      <KeepWordsSetting checked={settings?.polish_keep_words ?? false}
        onCheckedChange={(checked) => void updateSettings({ polish_keep_words: checked })} />
      <PolishContextSetting checked={settings?.polish_context_enabled ?? true}
        onCheckedChange={(checked) => void updateSettings({ polish_context_enabled: checked })} />

          <EnhancementSettingsPanel
            preset={enhancementOptions.preset}
            finalTextLanguage={effectiveFinalTextLanguage}
            writingSettings={writingSettings}
            aiFormattingEnabled={aiSettings.enabled || builtinPolishActive}
            translationAvailable={aiSettings.enabled && hasSelectedModel}
            writingSettingsDisabled={!settingsLoaded}
            onPresetChange={(preset) => void persistEnhancementOptions({ preset })}
            onFinalTextLanguageChange={handleFinalTextLanguageChange}
            onWritingSettingsChange={handleWritingSettingsChange}
            providerSetup={{
              aiSettings,
              setProviderSetupOpen,
              setProviderTab,
              setProviderSearch,
              hasSelectedModel,
              showGuidedSetup,
              activeProviderName,
              activeModelName,
              activeReasoningName,
            }}
          />
      </SettingsPage>

      <ProviderSetupDialog
        open={providerSetupOpen}
        onOpenChange={setProviderSetupOpen}
        aiSettings={aiSettings}
        providers={providers}
        providerApiKeys={providerApiKeys}
        agentCliStatus={agentCliStatus}
        agentCliProbing={agentCliProbing}
        customModelName={customModelName}
        guidedSetupProvider={guidedSetupProvider}
        setGuidedSetupProvider={setGuidedSetupProvider}
        isModelsLoading={isModelsLoading}
        getModels={getModels}
        fetchModels={(providerId) => void fetchModels(providerId)}
        getError={getError}
        getDisplayModels={getDisplayModels}
        showGuidedSetup={showGuidedSetup}
        showAiModelReselectionNotice={showAiModelReselectionNotice}
        providerTab={providerTab}
        setProviderTab={setProviderTab}
        providerSearch={providerSearch}
        setProviderSearch={setProviderSearch}
        onSelectModel={handleSelectModel}
        onSelectReasoning={handleSelectReasoning}
        onToggleFastMode={handleToggleFastMode}
        onRefreshAgentCli={handleRefreshAgentCli}
        onSetupApiKey={handleSetupApiKey}
        onRemoveApiKey={handleRemoveApiKey}
      />

      <EnhancementsProviderModals
        showApiKeyModal={showApiKeyModal}
        setShowApiKeyModal={setShowApiKeyModal}
        setGuidedSetupProvider={setGuidedSetupProvider}
        handleApiKeySubmit={handleApiKeySubmit}
        selectedProvider={selectedProvider}
        isLoading={isLoading}
        setIsLoading={setIsLoading}
        showOpenAIConfig={showOpenAIConfig}
        setShowOpenAIConfig={setShowOpenAIConfig}
        openAIDefaultBaseUrl={openAIDefaultBaseUrl}
        setOpenAIDefaultBaseUrl={setOpenAIDefaultBaseUrl}
        customModelName={customModelName}
        setCustomModelName={setCustomModelName}
        aiSettings={aiSettings}
        setAISettings={setAISettings}
        setProviderApiKeys={setProviderApiKeys}
      />
    </div>
  );
}
