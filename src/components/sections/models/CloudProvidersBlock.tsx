import { ApiKeyModal } from "@/components/ApiKeyModal";
import { SonioxStorageCard } from "@/components/SonioxStorageCard";
import { SettingsCard } from "@/components/settings/settings-ui";
import { Cloud } from "lucide-react";
import { CloudModelCard } from "./CloudModelCard";
import type { CloudProvidersApi } from "./useCloudProviders";
import type { ModelEntry } from "./types";

export interface CloudCardBindings {
  currentModel?: string;
  activeRemoteServer: string | null;
  onSelect: (modelName: string) => Promise<void> | void;
  clearActiveRemote: () => Promise<void>;
  cloud: CloudProvidersApi;
}

interface CloudModelGridProps extends CloudCardBindings {
  models: ModelEntry[];
}

function CloudModelGrid({
  models,
  currentModel,
  activeRemoteServer,
  onSelect,
  clearActiveRemote,
  cloud,
}: CloudModelGridProps) {
  return (
    <div className="grid gap-3">
      {models.map(([name, model]) => (
        <CloudModelCard
          key={name}
          name={name}
          model={model}
          currentModel={currentModel}
          activeRemoteServer={activeRemoteServer}
          onSelect={onSelect}
          clearActiveRemote={clearActiveRemote}
          openCloudModal={cloud.openCloudModal}
          onDisconnect={cloud.handleCloudDisconnect}
          onModelChange={cloud.handleCloudModelChange}
        />
      ))}
    </div>
  );
}

interface CloudProvidersBlockProps extends CloudCardBindings {
  readyCloudModels: ModelEntry[];
}

export function CloudProvidersBlock({ readyCloudModels, ...bindings }: CloudProvidersBlockProps) {
  if (readyCloudModels.length === 0) return null;

  return (
    <SettingsCard
      icon={Cloud}
      title={`Cloud transcription (${readyCloudModels.length})`}
      description="Connected providers that can transcribe without a local model. When selected, Voicetypr may send Personal Library words, names, and corrections as transcription context to improve recognition; snippets are not sent."
    >
      <div className="mt-4 grid gap-3">
        {readyCloudModels.map(([name, model]) => (
          <CloudModelCard
            key={name}
            name={name}
            model={model}
            currentModel={bindings.currentModel}
            activeRemoteServer={bindings.activeRemoteServer}
            onSelect={bindings.onSelect}
            clearActiveRemote={bindings.clearActiveRemote}
            openCloudModal={bindings.cloud.openCloudModal}
            onDisconnect={bindings.cloud.handleCloudDisconnect}
            onModelChange={bindings.cloud.handleCloudModelChange}
          />
        ))}
      </div>
      {readyCloudModels.some(([name]) => name === "soniox") && <SonioxStorageCard />}
    </SettingsCard>
  );
}

interface CloudSetupGridProps extends CloudCardBindings {
  setupCloudModels: ModelEntry[];
}

export function CloudSetupGrid({ setupCloudModels, ...bindings }: CloudSetupGridProps) {
  if (setupCloudModels.length === 0) return null;
  return <CloudModelGrid models={setupCloudModels} {...bindings} />;
}

export function CloudApiKeyModal({ cloud }: { cloud: CloudProvidersApi }) {
  if (!cloud.activeProvider) return null;

  return (
    <ApiKeyModal
      isOpen={cloud.isModalOpen}
      onClose={cloud.closeCloudModal}
      onSubmit={cloud.handleCloudKeySubmit}
      providerName={cloud.activeProvider.providerName}
      isLoading={cloud.cloudModalLoading}
      title={
        cloud.cloudModal?.mode === "update"
          ? `Update ${cloud.activeProvider.providerName} API Key`
          : `Add ${cloud.activeProvider.providerName} API Key`
      }
      description={
        cloud.cloudModal?.mode === "update"
          ? `Update your ${cloud.activeProvider.providerName} API key to keep cloud transcription running smoothly.`
          : `Enter your ${cloud.activeProvider.providerName} API key to enable cloud transcription. Your key is stored securely in the system keychain.`
      }
      submitLabel={cloud.cloudModal?.mode === "update" ? "Update API Key" : "Save API Key"}
      docsUrl={cloud.activeProvider.docsUrl}
    />
  );
}
