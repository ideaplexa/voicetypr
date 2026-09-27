import { ProviderSetupDialog } from "@/components/polish/ProviderSetupDialog";
import { useSettings } from "@/contexts/SettingsContext";
import { EnhancementsHeader } from "@/components/polish/EnhancementsHeader";
import { EnhancementSettingsPanel } from "@/components/polish/EnhancementSettingsPanel";
import { EnhancementsProviderModals } from "@/components/polish/EnhancementsProviderModals";
import { useAiProviderSettings, type ProviderTab } from "@/components/polish/useAiProviderSettings";
import { usePolishSectionSettings } from "@/components/polish/usePolishSectionSettings";
import { usePolishErrorEvents } from "@/components/polish/usePolishErrorEvents";
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
import { ScrollArea } from "@/components/ui/scroll-area";
export function EnhancementsSection() {
  const readiness = useReadinessState();
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
  usePolishErrorEvents();

  const isUsingCustomProvider = aiSettings.provider === "custom";
  const hasSelectedModel = Boolean(
    aiSettings.provider &&
    (aiSettings.model || isAgentCliProvider(aiSettings.provider)) &&
    (isUsingCustomProvider || providerApiKeys[aiSettings.provider]),
  );
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
      <EnhancementsHeader
        hasSelectedModel={hasSelectedModel}
        activeProviderName={activeProviderName}
        activeModelName={activeModelName}
        polishEnabled={aiSettings.enabled}
        onToggleEnabled={handleToggleEnabled}
        onOpenProviderSetup={openProviderSetup}
      />

      <ScrollArea className="flex-1 min-h-0 min-w-0 overflow-x-hidden">
        <div className="min-w-0 max-w-full overflow-x-hidden pt-5 pb-4 pl-2 pr-4">
          <EnhancementSettingsPanel
            preset={enhancementOptions.preset}
            finalTextLanguage={effectiveFinalTextLanguage}
            writingSettings={writingSettings}
            aiFormattingEnabled={aiSettings.enabled}
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
        </div>
      </ScrollArea>

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
