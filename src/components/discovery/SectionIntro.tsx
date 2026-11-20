import type { ReactNode } from "react";
import { DiscoveryArt, type DiscoveryArtKind } from "@/components/discovery/DiscoveryArt";
import { cn } from "@/lib/utils";

/** A section's small illustrated header: a picture of the feature and one line the page title doesn't already say. */
export function SectionIntro({ art, line, className }: { art: DiscoveryArtKind; line: ReactNode; className?: string }) {
  return (
    <section className={cn("flex flex-wrap items-center gap-5 rounded-[14px] border border-border bg-card p-3 pr-5", className)}>
      <DiscoveryArt kind={art} className="h-[84px] w-[132px]" />
      <p className="min-w-0 flex-1 text-[13.5px] leading-[20px] text-foreground">{line}</p>
    </section>
  );
}

/** Use the warning exactly as the truthfulness review wrote it. */
export const TRUSTED_NETWORK_WARNING = "Use only on a trusted network. Audio, transcripts and the sharing password travel unencrypted. Without a password, anyone who can reach this computer can send audio for transcription.";
