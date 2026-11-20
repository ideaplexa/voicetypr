import { InfoButton, PageHeader } from "@/components/settings/settings-ui";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { useState } from "react";

/** What the next dictation gets: polished, not yet (built-in Polish waits for the server), or not polished. */
export type PolishOutcome = "on" | "waiting" | "off";

const OUTCOME: Record<PolishOutcome, { label: string; chip: string; dot: string }> = {
  on: { label: "Polish is on", chip: "bg-sage-bg text-sage", dot: "bg-sage" },
  waiting: { label: "Polish isn’t available yet", chip: "bg-warn-bg text-warn", dot: "bg-warn" },
  off: { label: "Polish is off", chip: "bg-muted text-muted-foreground", dot: "bg-muted-foreground" },
};

/** The page title with a status only: the switches that decide it are rows on the page. */
export function EnhancementsHeader({ outcome }: {
  /** null while built-in Polish is still being asked about. */
  outcome: PolishOutcome | null;
}) {
  const [guideOpen, setGuideOpen] = useState(false);
  return <>
    <PageHeader title="Polish" description="AI cleans up fillers, false starts and punctuation. Your words stay yours."
      action={<>
        <InfoButton label="Polish guide" onClick={() => setGuideOpen(true)} />
        <span role="status" className={cn("inline-flex items-center gap-1.5 rounded-full px-[9px] py-[3px] text-[11.5px] leading-[normal] font-medium", outcome ? OUTCOME[outcome].chip : "invisible")}
          style={outcome === "on" ? { color: "var(--ready-text)" } : undefined}>
          {outcome && <><span aria-hidden className={cn("size-1.5 rounded-full", OUTCOME[outcome].dot)} />{OUTCOME[outcome].label}</>}
        </span>
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
