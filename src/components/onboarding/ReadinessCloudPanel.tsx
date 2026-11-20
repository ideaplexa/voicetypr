import { ApiKeyModal } from "@/components/ApiKeyModal";
import { CLOUD_PROVIDERS, getCloudProviderByModel } from "@/lib/cloudProviders";
import type { ModelInfo } from "@/types";
import { cn } from "@/lib/utils";
import { Cloud, KeyRound } from "lucide-react";

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

/** The frames' line for each provider: whether words appear while you speak. */
const ABOUT: Record<string, string> = {
  Soniox: "Supports real-time words",
  Deepgram: "Supports real-time words",
};

const ORDER = ["Soniox", "Deepgram", "OpenAI", "Groq", "Cohere"];

const rowButton = "flex items-center gap-1.5 rounded-lg border border-border px-3 py-[7px] text-[13.5px] font-medium text-foreground hover:bg-accent";

/** 04b Choose a cloud provider: every provider the app supports, its key as the control. */
export function ReadinessCloudPanel({
  currentModel,
  cloudModelSetup,
  isSavingCloudKey,
  isModelReady,
  onSelectCloud,
  onSetCloudModelSetup,
  onCloudKeySubmit,
}: ReadinessCloudPanelProps) {
  const activeCloudProvider = cloudModelSetup ? getCloudProviderByModel(cloudModelSetup) : undefined;
  const all = Object.values(CLOUD_PROVIDERS);
  const providers = [
    ...ORDER.flatMap((name) => all.filter((provider) => provider.displayName === name)),
    ...all.filter((provider) => !ORDER.includes(provider.displayName)),
  ];
  return (
    <div className="flex flex-col gap-4 self-stretch">
      <div className="flex flex-col overflow-hidden rounded-[14px] border border-border bg-card">
        {providers.map((provider) => {
          const name = provider.modelName;
          const keyAdded = isModelReady(name);
          const inUse = keyAdded && currentModel === name;
          return (
            <div key={name} className={cn("flex h-[60px] items-center gap-3.5 border-b border-border px-[18px] last:border-b-0", inUse && "bg-sage-bg")}>
              <span className="flex size-[34px] shrink-0 items-center justify-center rounded-[9px] bg-secondary text-foreground">
                <Cloud aria-hidden="true" className="size-[17px]" />
              </span>
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="text-[15.5px] leading-[1.2] font-semibold text-foreground">{provider.displayName}</span>
                <span className="text-sm leading-[1.2] text-muted-foreground">{ABOUT[provider.displayName] ?? "Text after you stop"}</span>
              </span>
              {inUse ? (
                <span className="text-[13.5px] font-medium text-sage">In use</span>
              ) : keyAdded ? (
                <button type="button" className={rowButton} aria-label={`Use ${provider.displayName}`} onClick={() => void onSelectCloud(name)}>
                  Use
                </button>
              ) : (
                <button type="button" className={rowButton} aria-label={`Add API key for ${provider.displayName}`} onClick={() => onSetCloudModelSetup(name)}>
                  <KeyRound aria-hidden="true" className="size-3.5" />
                  Add API key
                </button>
              )}
            </div>
          );
        })}
      </div>
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
