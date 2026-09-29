import { InfoButton, PageHeader } from "@/components/settings/settings-ui";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Switch } from "@/components/settings/SettingsSwitch";
import { useState } from "react";

export function EnhancementsHeader({
  hasSelectedModel, polishEnabled, onToggleEnabled, onOpenProviderSetup,
}: {
  hasSelectedModel: boolean;
  activeProviderName: string;
  activeModelName: string;
  polishEnabled: boolean;
  onToggleEnabled: (enabled: boolean) => void;
  onOpenProviderSetup: () => void;
}) {
  const [guideOpen, setGuideOpen] = useState(false);
  return <>
    <PageHeader title="Polish" description="AI cleans up fillers, false starts and punctuation. Your words stay yours."
      action={<>
        <InfoButton label="Polish guide" onClick={() => setGuideOpen(true)} />
        <div className="flex items-center gap-3 rounded-[10px] border border-border bg-card px-3 py-2 text-[13px] font-medium text-muted-foreground">
          Polish is {polishEnabled ? "on" : "off"}
          <Switch id="polish-enabled" aria-label="Polish" checked={polishEnabled} onCheckedChange={hasSelectedModel ? onToggleEnabled : onOpenProviderSetup} />
        </div>
      </>} />
    <Dialog open={guideOpen} onOpenChange={setGuideOpen}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader><DialogTitle>Polish guide</DialogTitle><DialogDescription>Choose an AI provider, default style and optional app styles. Dictionary has words, corrections and snippets.</DialogDescription></DialogHeader>
        <div className="space-y-3 text-sm text-muted-foreground">
          <p><strong className="text-foreground">Provider</strong> chooses the cloud API or isolated local agent used by Polish.</p>
          <p><strong className="text-foreground">Dictionary</strong> protects words and names and can improve recognition.</p>
          <p><strong className="text-foreground">Corrections</strong> applies exact replacements with or without Polish.</p>
          <p><strong className="text-foreground">Snippets</strong> expands “insert” triggers into saved text.</p>
          <p><strong className="text-foreground">Modes</strong> sets the default writing mode and optional per-app overrides.</p>
        </div>
      </DialogContent>
    </Dialog>
  </>;
}
