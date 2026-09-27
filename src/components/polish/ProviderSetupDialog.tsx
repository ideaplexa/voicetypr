import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { AISettings } from "@/types/ai";
import type { AIProviderConfig, AIProviderModel, AgentCliProbe } from "@/types/providers";
import { Search } from "lucide-react";
import { isAgentCliProvider, modelMatchesQuery } from "./agentCli";
import { AgentProviderRow } from "./AgentProviderRow";
import { CloudProviderRow } from "./CloudProviderRow";
import { buildProviderRowViewModel, type ProviderRowProps } from "./providerRowShared";
import type { ProviderTab } from "./useAiProviderSettings";

export interface ProviderSetupDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  aiSettings: AISettings;
  providers: AIProviderConfig[];
  providerApiKeys: Record<string, boolean>;
  agentCliStatus: Record<string, AgentCliProbe>;
  agentCliProbing: Record<string, boolean>;
  customModelName: string;
  guidedSetupProvider: string | null;
  setGuidedSetupProvider: (provider: string | null) => void;
  isModelsLoading: (providerId: string) => boolean;
  getModels: (providerId: string) => AIProviderModel[];
  fetchModels: (providerId: string) => void;
  getError: (providerId: string) => string | null;
  getDisplayModels: (providerId: string) => AIProviderModel[];
  showGuidedSetup: boolean;
  showAiModelReselectionNotice: boolean;
  providerTab: ProviderTab;
  setProviderTab: (tab: ProviderTab) => void;
  providerSearch: string;
  setProviderSearch: (search: string) => void;
  onSelectModel: (providerId: string, modelId: string) => Promise<void>;
  onSelectReasoning: (providerId: string, reasoning: string) => Promise<void>;
  onToggleFastMode: (providerId: string, enabled: boolean) => Promise<void>;
  onRefreshAgentCli: (provider: AIProviderConfig) => Promise<void>;
  onSetupApiKey: (providerId: string) => Promise<void>;
  onRemoveApiKey: (providerId: string) => Promise<void>;
}

function ProviderRow(props: ProviderRowProps) {
  const model = buildProviderRowViewModel(props);
  if (model.agentCli) {
    return <AgentProviderRow props={props} model={model} />;
  }
  return <CloudProviderRow props={props} model={model} />;
}

export function ProviderSetupDialog(props: ProviderSetupDialogProps) {
  const {
    open,
    onOpenChange,
    aiSettings,
    providers,
    providerApiKeys,
    agentCliStatus,
    agentCliProbing,
    customModelName,
    guidedSetupProvider,
    setGuidedSetupProvider,
    isModelsLoading,
    getModels,
    fetchModels,
    getError,
    getDisplayModels,
    showGuidedSetup,
    showAiModelReselectionNotice,
    providerTab,
    setProviderTab,
    providerSearch,
    setProviderSearch,
    onSelectModel,
    onSelectReasoning,
    onToggleFastMode,
    onRefreshAgentCli,
    onSetupApiKey,
    onRemoveApiKey,
  } = props;

  const providerQuery = providerSearch.trim().toLowerCase();
  const filteredProviders = providers.filter((provider) => {
    const matchesTab =
      providerTab === "local" ? isAgentCliProvider(provider.id) : !isAgentCliProvider(provider.id);
    if (!matchesTab) return false;
    if (providerTab === "local" || !providerQuery) return true;

    return (
      provider.name.toLowerCase().includes(providerQuery) ||
      (provider.isCustom && customModelName.toLowerCase().includes(providerQuery)) ||
      getDisplayModels(provider.id).some((model) => modelMatchesQuery(model, providerQuery))
    );
  });

  const hasLoadingProviders = providers.some((provider) => isModelsLoading(provider.id));

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex max-h-[calc(100svh-4rem)] flex-col overflow-hidden sm:max-w-3xl"
        aria-describedby="provider-setup-description"
      >
        <DialogHeader>
          <DialogTitle>Choose provider &amp; model</DialogTitle>
          <DialogDescription id="provider-setup-description">
            Use your own cloud API key or an agent already installed on this Mac.
          </DialogDescription>
        </DialogHeader>

        {showAiModelReselectionNotice && (
          <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
            Your previous model is unavailable. Choose another model to continue.
          </div>
        )}
        <Tabs
          value={providerTab}
          onValueChange={(value) => {
            setProviderTab(value as ProviderTab);
            setProviderSearch("");
          }}
        >
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <TabsList aria-label="Polish provider type">
              <TabsTrigger value="cloud">Cloud API</TabsTrigger>
              <TabsTrigger value="local">Local Agents</TabsTrigger>
            </TabsList>
            {providerTab === "cloud" && (
              <div className="relative min-w-0 flex-1 sm:max-w-72">
                <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  id="ai-provider-model-search"
                  aria-label="Search cloud providers and models"
                  value={providerSearch}
                  onChange={(event) => setProviderSearch(event.target.value)}
                  placeholder="Search cloud APIs"
                  className="pl-9"
                />
              </div>
            )}
          </div>

          <div className="max-h-[min(52svh,24rem)] min-h-0 flex-1 overflow-y-auto overscroll-contain rounded-md pr-1">
            <TabsContent value={providerTab} className="mt-0 space-y-2">
              {providerTab === "cloud" && hasLoadingProviders && (
                <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <Spinner className="h-3.5 w-3.5" />
                  Refreshing models
                </p>
              )}
              {filteredProviders.length === 0 && (
                <div className="rounded-lg border border-dashed border-border/70 px-4 py-6 text-center text-sm text-muted-foreground">
                  No providers or models match your search.
                </div>
              )}

              {filteredProviders.map((provider) => (
                <ProviderRow
                  key={provider.id}
                  provider={provider}
                  {...{
                    aiSettings,
                    providerApiKeys,
                    agentCliStatus,
                    agentCliProbing,
                    customModelName,
                    guidedSetupProvider,
                    setGuidedSetupProvider,
                    isModelsLoading,
                    getModels,
                    fetchModels,
                    getError,
                    getDisplayModels,
                    providerQuery,
                    showGuidedSetup,
                    onSelectModel,
                    onSelectReasoning,
                    onToggleFastMode,
                    onRefreshAgentCli,
                    onSetupApiKey,
                    onRemoveApiKey,
                  }}
                />
              ))}
            </TabsContent>
          </div>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
