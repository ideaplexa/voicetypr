import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import { HelpCircle } from "lucide-react";
import type { ReactNode } from "react";

export function EnhancementsHeader({
  hasSelectedModel,
  activeProviderName,
  activeModelName,
  polishEnabled,
  onToggleEnabled,
  onOpenProviderSetup,
}: {
  hasSelectedModel: boolean;
  activeProviderName: string;
  activeModelName: string;
  polishEnabled: boolean;
  onToggleEnabled: (enabled: boolean) => void;
  onOpenProviderSetup: () => void;
}) {
  const polishHeaderActions: ReactNode = hasSelectedModel ? (
    <div className="flex items-center gap-3 rounded-lg border border-border/60 bg-card px-3 py-1.5">
      <button
        type="button"
        className="min-w-0 max-w-56 flex-1 rounded-md px-1.5 py-0.5 text-right outline-none transition-colors hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring"
        aria-label="Change provider and model"
        title="Change provider & model"
        onClick={onOpenProviderSetup}
      >
        <p className="truncate text-xs font-medium text-foreground">{activeProviderName}</p>
        <p className="truncate text-[11px] text-muted-foreground">
          {activeModelName || "Connected"}
        </p>
      </button>
      <Switch
        id="polish-enabled"
        aria-label="Polish"
        className="shrink-0"
        checked={polishEnabled}
        onCheckedChange={onToggleEnabled}
      />
    </div>
  ) : (
    <Button type="button" variant="outline" onClick={onOpenProviderSetup}>
      Select a provider to enable Polish
    </Button>
  );

  return (
    <div className="shrink-0 py-5 pl-2 pr-4">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h1 className="text-2xl font-semibold tracking-tight">Polish</h1>
            <Dialog>
              <DialogTrigger
                render={
                  <Button
                    type="button"
                    variant="secondary"
                    size="icon"
                    aria-label="Polish guide"
                    className="rounded-full"
                  />
                }
              >
                <HelpCircle className="h-4.5 w-4.5" />
              </DialogTrigger>
              <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                  <DialogTitle>Polish guide</DialogTitle>
                  <DialogDescription>
                    Configure the provider, dictionary, corrections, snippets, and writing modes.
                  </DialogDescription>
                </DialogHeader>
                <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                  <p>
                    <strong className="text-foreground">Provider</strong> chooses the cloud API or
                    isolated local agent used by Polish.
                  </p>
                  <p>
                    <strong className="text-foreground">Dictionary</strong> protects words and names
                    and can improve recognition.
                  </p>
                  <p>
                    <strong className="text-foreground">Corrections</strong> applies exact
                    replacements with or without Polish.
                  </p>
                  <p>
                    <strong className="text-foreground">Snippets</strong> expands “insert” triggers
                    into saved text.
                  </p>
                  <p>
                    <strong className="text-foreground">Modes</strong> sets the default writing mode
                    and optional per-app overrides.
                  </p>
                </div>
              </DialogContent>
            </Dialog>
          </div>
          <p className="mt-0.5 text-sm leading-relaxed text-muted-foreground">
            AI cleanup when enabled; dictionary, corrections, snippets, and app identity remain
            available independently.
          </p>
        </div>
        <div className="ml-auto shrink-0">{polishHeaderActions}</div>
      </div>
    </div>
  );
}
