import type { AISettings } from "@/types/ai";
import { AlertTriangle, Bot, X } from "lucide-react";
import { useEnhancementsStore } from "@/state/enhancements";
import { isAgentCliProvider } from "./agentCli";

export interface ProviderSetupCardProps {
  aiSettings: AISettings;
  setProviderSetupOpen: (open: boolean) => void;
  setProviderTab: (tab: "cloud" | "local") => void;
  setProviderSearch: (search: string) => void;
  hasSelectedModel: boolean;
  showGuidedSetup: boolean;
  activeProviderName: string;
  activeModelName: string;
  activeReasoningName: string;
}

export function ProviderSetupCard({ aiSettings, setProviderSetupOpen, setProviderTab, setProviderSearch,
  hasSelectedModel, activeProviderName, activeModelName, activeReasoningName }: ProviderSetupCardProps) {
  const polishError = useEnhancementsStore((state) => state.polishError);
  const clearPolishError = useEnhancementsStore((state) => state.clearPolishError);
  const openProviderSetup = () => {
    setProviderTab(isAgentCliProvider(aiSettings.provider) ? "local" : "cloud");
    setProviderSearch("");
    setProviderSetupOpen(true);
  };
  return <div className="space-y-3">
    {polishError ? <div className="flex gap-2 rounded-[10px] bg-warn-bg p-3 text-xs text-muted-foreground">
      <AlertTriangle className="size-4 shrink-0 text-warn" />
      <span>{polishError.kind === "auth" ? "Polish failed — your API key was rejected. Update it below." : polishError.message}</span>
      <button type="button" aria-label="Dismiss Polish error" onClick={clearPolishError}><X className="size-4" /></button>
    </div> : null}
    <button type="button" aria-label="Choose provider and model" onClick={openProviderSetup} className="flex w-full items-center gap-3 rounded-[10px] bg-muted p-3 text-left hover:bg-muted/80 focus-visible:ring-2 focus-visible:ring-ring">
      <Bot className="size-4 shrink-0 text-sage" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-[13px] font-medium text-muted-foreground">{hasSelectedModel ? `${activeProviderName}${activeModelName ? ` · ${activeModelName}` : ""}${activeReasoningName ? ` · ${activeReasoningName}` : ""}` : "Connect an AI to turn on Polish"}</p>
        <p className="text-xs text-muted-foreground">{hasSelectedModel ? `${isAgentCliProvider(aiSettings.provider) ? "Local agent" : aiSettings.provider === "custom" ? "Custom connection" : "API key connected"} · Active` : "Choose a cloud API or local agent"}</p>
      </div>
      <span className="rounded-[10px] border border-border bg-card px-3 py-1.5 text-sm font-medium text-foreground">{hasSelectedModel ? "Change" : "Choose"}</span>
    </button>
  </div>;
}
