import { Badge } from "@/components/ui/badge";
import { FieldSet } from "@/components/ui/field";
import type { AISettings } from "@/types/ai";
import { AlertTriangle, ChevronRight, X } from "lucide-react";
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

export function ProviderSetupCard({
  aiSettings,
  setProviderSetupOpen,
  setProviderTab,
  setProviderSearch,
  hasSelectedModel,
  showGuidedSetup,
  activeProviderName,
  activeModelName,
  activeReasoningName,
}: ProviderSetupCardProps) {
  const polishError = useEnhancementsStore((s) => s.polishError);
  const clearPolishError = useEnhancementsStore((s) => s.clearPolishError);

  const openProviderSetup = () => {
    setProviderTab(isAgentCliProvider(aiSettings.provider) ? "local" : "cloud");
    setProviderSearch("");
    setProviderSetupOpen(true);
  };

  return (
    <FieldSet className="overflow-hidden rounded-xl border border-border/60 bg-card [&_button:not(:disabled)]:cursor-pointer">
      {polishError && (
        <div className="border-b border-border/60 p-4">
          <div className="flex items-start gap-2 rounded-lg border border-amber-500/20 bg-amber-500/10 p-3">
            <AlertTriangle className="mt-0.5 h-4 w-4 flex-shrink-0 text-amber-500" />
            <p className="flex-1 text-sm font-medium text-amber-700 dark:text-amber-400">
              {polishError.kind === "auth"
                ? "Polish failed — your API key was rejected. Update it below."
                : polishError.message}
            </p>
            <button
              type="button"
              onClick={clearPolishError}
              aria-label="Dismiss Polish error"
              className="-m-1 shrink-0 rounded-md p-1 text-amber-600 transition-colors hover:bg-amber-500/10 hover:text-amber-700 focus-visible:ring-2 focus-visible:ring-ring dark:text-amber-400 dark:hover:text-amber-300"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>
      )}
      <button
        type="button"
        className="flex w-full items-center justify-between gap-4 px-4 py-3 text-left outline-none transition-colors hover:bg-muted/30 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
        aria-label="Choose provider and model"
        onClick={openProviderSetup}
      >
        <span className="min-w-0">
          <span className="block text-sm font-semibold">
            {showGuidedSetup ? "Connect an AI to turn on Polish" : "Provider & model"}
          </span>
          <span className="mt-0.5 block truncate text-xs text-muted-foreground">
            {hasSelectedModel
              ? `${activeProviderName}${activeModelName ? ` · ${activeModelName}` : ""}${activeReasoningName ? ` · ${activeReasoningName}` : ""}`
              : "Choose a cloud API or local agent"}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-2">
          {hasSelectedModel && (
            <Badge variant="secondary" className="text-sage">
              Active
            </Badge>
          )}
          <ChevronRight className="size-4 text-muted-foreground" />
        </span>
      </button>
    </FieldSet>
  );
}
