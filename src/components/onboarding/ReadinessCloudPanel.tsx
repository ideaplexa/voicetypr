import { ApiKeyModal } from "@/components/ApiKeyModal";
import { SettingsPaneCard } from "@/components/settings/settings-ui";
import { Button } from "@/components/settings/SettingsButton";
import { CLOUD_PROVIDERS, getCloudProviderByModel } from "@/lib/cloudProviders";
import type { ModelInfo } from "@/types";

export interface ReadinessCloudPanelProps {
  cloudModelNames: string[];
  models: Record<string, ModelInfo>;
  currentModel: string | undefined;
  isLoading: boolean;
  cloudModelSetup: string | null;
  isSavingCloudKey: boolean;
  isModelReady: (name: string) => boolean;
  onSelectCloud: (modelName: string) => void;
  onSetCloudModelSetup: (modelName: string | null) => void;
  onCloudKeySubmit: (apiKey: string) => void;
}

/** Every cloud provider the app supports, one row each, with its key as the control. */
export function ReadinessCloudPanel({
  cloudModelNames,
  currentModel,
  cloudModelSetup,
  isSavingCloudKey,
  isModelReady,
  onSelectCloud,
  onSetCloudModelSetup,
  onCloudKeySubmit,
}: ReadinessCloudPanelProps) {
  const activeCloudProvider = cloudModelSetup ? getCloudProviderByModel(cloudModelSetup) : undefined;
  const providers = Object.values(CLOUD_PROVIDERS).filter(
    (provider) => cloudModelNames.length === 0 || cloudModelNames.includes(provider.modelName),
  );
  return (
    <div className="flex flex-col gap-4">
      <SettingsPaneCard className="!px-[18px] !py-1">
        {providers.map((provider) => {
          const name = provider.modelName;
          const keyAdded = isModelReady(name);
          const inUse = keyAdded && currentModel === name;
          return (
            <div key={name} className="flex items-center gap-3 border-b border-border py-3.5 last:border-0">
              <div className="flex min-w-0 flex-1 flex-col items-start gap-0.5">
                <span className="text-sm font-semibold">{provider.displayName}</span>
                <span className="text-[13px] text-muted-foreground">{provider.description}</span>
              </div>
              {inUse ? (
                <span className="text-[13px] font-medium text-sage">In use</span>
              ) : keyAdded ? (
                <>
                  <span className="text-[13px] text-muted-foreground">Key added</span>
                  <Button size="sm" variant="outline" aria-label={`Use ${provider.displayName}`} onClick={() => void onSelectCloud(name)}>
                    Use
                  </Button>
                </>
              ) : (
                <Button size="sm" variant="outline" aria-label={`Add key for ${provider.displayName}`} onClick={() => onSetCloudModelSetup(name)}>
                  Add key
                </Button>
              )}
            </div>
          );
        })}
      </SettingsPaneCard>
      {activeCloudProvider ? (
        <ApiKeyModal
          inline
          isOpen
          onClose={() => {
            if (!isSavingCloudKey) onSetCloudModelSetup(null);
          }}
          onSubmit={(apiKey) => void onCloudKeySubmit(apiKey)}
          providerName={activeCloudProvider.providerName}
          isLoading={isSavingCloudKey}
          description={`Enter your ${activeCloudProvider.providerName} API key. It is stored securely in the system keychain.`}
          docsUrl={activeCloudProvider.docsUrl}
        />
      ) : null}
    </div>
  );
}
