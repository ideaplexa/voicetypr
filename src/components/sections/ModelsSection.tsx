import { useTauriEvent } from "@/hooks/useTauriEvent";
import { ChoiceCard, SettingsCard, SettingsPage } from "@/components/settings/settings-ui";
import { SonioxStorageCard } from "@/components/SonioxStorageCard";
import { useSettings } from "@/contexts/SettingsContext";
import { isMacOS } from "@/lib/platform";
import { isCloudModel, isLocalModel } from "@/types";
import { Cloud, Laptop, Network } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { CloudApiKeyModal } from "./models/CloudProvidersBlock";
import { CloudModelCard } from "./models/CloudModelCard";
import { LocalModelsList } from "./models/LocalModelsList";
import { ModelsLanguageRow } from "./models/ModelsLanguageRow";
import { ModelsSourcesHeader } from "./models/ModelsSourcesHeader";
import { RemoteServersBlock } from "./models/RemoteServersBlock";
import { TranscriptionControls } from "./models/TranscriptionControls";
import { TranscriptionPerformanceCard } from "./recording/TranscriptionPerformanceCard";
import type { ModelsSectionProps, SourceFilter } from "./models/types";
import { useCloudProviders } from "./models/useCloudProviders";
import { useRemoteServers } from "./models/useRemoteServers";
import { useSpokenLanguage } from "./models/useSpokenLanguage";

export function ModelsSection({
  sourceFilter: controlledSourceFilter,
  onSourceFilterChange,
  models,
  downloadProgress,
  downloadPhases = {},
  verifyingModels,
  currentModel,
  downloadErrors = {},
  isLoading = false,
  onDownload,
  onDelete,
  onCancelDownload,
  onRepair,
  onSelect,
  refreshModels,
}: ModelsSectionProps) {
  const { refreshSettings } = useSettings();
  const selectedModel = models.find(([name]) => name === currentModel)?.[1];
  const language = useSpokenLanguage(selectedModel?.supported_languages ?? undefined);
  const remotes = useRemoteServers();
  const cloud = useCloudProviders({
    onSelect,
    refreshModels,
    clearActiveRemote: remotes.clearActiveRemote,
  });
  const trackedSource: SourceFilter = remotes.activeRemoteServer
    ? "remote"
    : selectedModel && isCloudModel(selectedModel)
      ? "cloud"
      : "local";
  const [localSourceFilter, setLocalSourceFilter] = useState<SourceFilter>(trackedSource);
  const sourceFilter = controlledSourceFilter ?? localSourceFilter;
  const setSourceFilter = onSourceFilterChange ?? setLocalSourceFilter;
  const lastTrackedSource = useRef(trackedSource);
  useEffect(() => {
    if (trackedSource !== lastTrackedSource.current) {
      lastTrackedSource.current = trackedSource;
      setSourceFilter(trackedSource);
    }
  }, [trackedSource, setSourceFilter]);
  useTauriEvent<{ model: string; engine: string }>("model-changed", () => {
    void remotes.fetchActiveRemoteServer();
    void remotes.fetchRemoteServers();
    void refreshSettings();
  });

  const local = models
    .filter(([, model]) => isLocalModel(model))
    .sort(([a], [b]) => Number(b === currentModel) - Number(a === currentModel));
  const providers = models
    .filter(([, model]) => isCloudModel(model))
    .sort(([a], [b]) => Number(b === currentModel) - Number(a === currentModel));
  const sourceCards = [
    {
      value: "local" as const,
      label: isMacOS ? "On this Mac" : "On this PC",
      description: "Private and offline. No account, no cost per minute.",
      tag: "Recommended",
      icon: Laptop,
    },
    {
      value: "cloud" as const,
      label: "Cloud",
      description: "Soniox, Deepgram and more with your own API key.",
      tag: providers.some(([, model]) => model.downloaded && !model.requires_setup)
        ? "Key added"
        : "Needs a key",
      icon: Cloud,
    },
    {
      value: "remote" as const,
      label: "Another computer",
      description: "Use a stronger Voicetypr on your network.",
      tag: "Local network",
      icon: Network,
    },
  ];
  return (
    <>
      <SettingsPage>
        <ModelsSourcesHeader />
        <div className="grid gap-3 sm:grid-cols-3" aria-label="Transcription sources">
          {sourceCards.map((source) => (
            <ChoiceCard
              key={source.value}
              {...source}
              selected={sourceFilter === source.value}
              onSelect={() => setSourceFilter(source.value)}
            />
          ))}
        </div>
        {sourceFilter === "local" ? (
          <SettingsCard
            title="Model"
            action={
              <span className="text-xs text-muted-foreground">
                {isMacOS
                  ? "Fastest on Apple Silicon: Parakeet"
                  : "Choose an offline model for this PC"}
              </span>
            }
            className="!p-0 [&>div:first-child]:px-4 [&>div:first-child]:pt-4 [&>div:last-child]:mt-3"
          >
            <div role="radiogroup" aria-label="Local models">
              <LocalModelsList
                models={local}
                downloadProgress={downloadProgress}
                downloadPhases={downloadPhases}
                verifyingModels={verifyingModels}
                downloadErrors={downloadErrors}
                onDownload={onDownload}
                onDelete={onDelete}
                onCancelDownload={onCancelDownload}
                onRepair={onRepair}
                onSelect={onSelect}
                currentModel={currentModel}
                activeRemoteServer={remotes.activeRemoteServer}
                clearActiveRemote={remotes.clearActiveRemote}
              />
            </div>
            {local.length === 0 && (
              <p className="px-4 pb-4 text-sm text-muted-foreground">
                {isLoading ? "Loading models…" : "No local models available."}
              </p>
            )}
          </SettingsCard>
        ) : null}
        {sourceFilter === "cloud" ? (
          <SettingsCard
            title="Cloud providers"
            description="Your own API key connects each provider. Personal Library words and corrections may be sent as context; snippets are not sent."
            className="!p-0 [&>div:first-child]:px-4 [&>div:first-child]:pt-4 [&>div:last-child]:mt-3"
          >
            <div className="divide-y divide-border" role="radiogroup" aria-label="Cloud providers">
              {providers.map(([name, model]) => (
                <CloudModelCard
                  key={name}
                  name={name}
                  model={model}
                  currentModel={currentModel}
                  activeRemoteServer={remotes.activeRemoteServer}
                  onSelect={onSelect}
                  clearActiveRemote={remotes.clearActiveRemote}
                  openCloudModal={cloud.openCloudModal}
                  onDisconnect={cloud.handleCloudDisconnect}
                  onModelChange={cloud.handleCloudModelChange}
                />
              ))}
            </div>
            {providers.length === 0 && (
              <p className="px-4 pb-4 text-sm text-muted-foreground">
                No cloud providers available.
              </p>
            )}
          </SettingsCard>
        ) : null}
        <RemoteServersBlock remotes={remotes} visible={sourceFilter === "remote"} />
        <div className="grid gap-3 sm:grid-cols-2">
          <SettingsCard title="Spoken language">
            <ModelsLanguageRow
              languageValue={language.languageValue}
              currentEngine={language.currentEngine}
              isEnglishOnlyModel={language.isEnglishOnlyModel}
              supportedLanguages={language.supportedLanguages}
              hasDownloading={Object.keys(downloadProgress).length > 0}
              hasVerifying={verifyingModels.size > 0}
              onLanguageChange={language.handleLanguageChange}
            />
          </SettingsCard>
          <TranscriptionControls
            engine={language.currentEngine}
            modelName={language.currentModelName}
          />
        </div>
        {language.currentEngine === "whisper" ? (
          <TranscriptionControls
            engine={language.currentEngine}
            modelName={language.currentModelName}
            speedOnly
          />
        ) : null}
        {sourceFilter === "cloud" &&
        providers.some(
          ([name, model]) => name === "soniox" && model.downloaded && !model.requires_setup,
        ) ? (
          <SonioxStorageCard />
        ) : null}
        <TranscriptionPerformanceCard />
      </SettingsPage>
      <CloudApiKeyModal cloud={cloud} />
    </>
  );
}
