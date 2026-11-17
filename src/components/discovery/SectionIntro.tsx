import type { ReactNode } from "react";
import { DiscoveryArt, PillCharacter, type DiscoveryArtKind } from "@/components/discovery/DiscoveryArt";
import { cn } from "@/lib/utils";

/** A section's small illustrated header: picture, one line, one action (MOI-154). */
export function SectionIntro({ art, line, action, className }: { art: DiscoveryArtKind; line: ReactNode; action?: ReactNode; className?: string }) {
  return (
    <section className={cn("flex flex-wrap items-center gap-5 rounded-[14px] border border-border bg-card p-3 pr-5", className)}>
      <DiscoveryArt kind={art} className="h-[84px] w-[132px]" />
      <div className="flex min-w-0 flex-1 flex-col items-start gap-2.5">
        <p className="text-[13.5px] leading-[20px] text-foreground">{line}</p>
        {action}
      </div>
    </section>
  );
}

/** An empty section: the pill asleep, one sentence and what to do next. */
export function SectionEmpty({ title, children, className }: { title: ReactNode; children?: ReactNode; className?: string }) {
  return (
    <section className={cn("flex flex-col items-center gap-3 rounded-[14px] border border-dashed border-border px-6 py-9 text-center", className)}>
      <PillCharacter size="lg" asleep className="mb-1" />
      <p className="max-w-md text-[13.5px] leading-[20px] text-foreground">{title}</p>
      {children}
    </section>
  );
}

/** Use the warning exactly as the truthfulness review wrote it. */
export const TRUSTED_NETWORK_WARNING = "Use only on a trusted network. Audio, transcripts and the sharing password travel unencrypted. Without a password, anyone who can reach this computer can send audio for transcription.";
