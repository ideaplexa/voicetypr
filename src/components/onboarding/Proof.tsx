import { cn } from "@/lib/utils";
import { ArrowRight, Sparkles } from "lucide-react";

/** The example on Welcome and on Buy when there is no try of their own. */
export const EXAMPLE = {
  raw: "um so let's move the the sync to thursday no wait friday at three",
  dropped: [0, 1, 4, 8, 9, 10],
  clean: "Let’s move the sync to Friday at 3.",
};

const bare = (word: string) => word.toLowerCase().replace(/[^\p{L}\p{N}']/gu, "").replace(/’/g, "'");

/** Words of the raw text that the cleaned text left out, by a longest common subsequence. */
export function droppedWords(raw: string, clean: string) {
  const a = raw.split(/\s+/).filter(Boolean);
  const b = clean.split(/\s+/).filter(Boolean).map(bare);
  const keep: number[][] = Array.from({ length: a.length + 1 }, () => Array.from({ length: b.length + 1 }, () => 0));
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      keep[i][j] = bare(a[i]) === b[j] ? keep[i + 1][j + 1] + 1 : Math.max(keep[i + 1][j], keep[i][j + 1]);
    }
  }
  const dropped: number[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length) {
    if (j < b.length && bare(a[i]) === b[j]) { i++; j++; }
    else if (j < b.length && keep[i][j + 1] > keep[i + 1][j]) j++;
    else dropped.push(i++);
  }
  return dropped;
}

/** Raw words with the ones Polish dropped faded, as the frames draw them. */
export function RawWords({ raw, dropped, size = 16 }: { raw: string; dropped: number[]; size?: 15 | 16 }) {
  const words = raw.split(/\s+/).filter(Boolean);
  return (
    <p className={cn("flex flex-wrap gap-x-[5px] gap-y-1 leading-[1.2]", size === 15 ? "text-[15px]" : "text-base")}>
      {words.map((word, index) => (
        <span key={index} className={dropped.includes(index) ? "text-text-3 opacity-70" : "text-muted-foreground"}>
          {word}
        </span>
      ))}
    </p>
  );
}

/** "You said" beside "Voicetypr cleaned it": the before and after cards on 01, 09 and 11. */
export function ProofPair({
  raw,
  clean,
  dropped,
  rawLabel = "You said",
  cleanLabel = "Voicetypr cleaned it",
  size = "md",
  arrow = false,
}: {
  raw: string;
  clean: string;
  dropped?: number[];
  rawLabel?: string;
  cleanLabel?: string;
  /** sm is Welcome's pair, md the result, lg Buy. */
  size?: "sm" | "md" | "lg";
  arrow?: boolean;
}) {
  const small = size === "sm";
  const large = size === "lg";
  const label = small ? "text-[13px]" : large ? "text-sm" : "text-[13.5px]";
  return (
    <div className={cn("flex self-stretch", small ? "items-center gap-[18px]" : "items-start gap-4")}>
      <div className={cn("flex min-w-0 flex-1 flex-col items-start bg-secondary", large ? "rounded-2xl" : "rounded-[14px]", small ? "gap-2.5 px-5 py-[18px]" : "gap-3 px-[22px] py-5")}>
        <span className={cn("leading-[1.2] font-semibold text-muted-foreground", label)}>{rawLabel}</span>
        <RawWords raw={raw} dropped={dropped ?? droppedWords(raw, clean)} size={small ? 15 : 16} />
      </div>
      {arrow ? <ArrowRight aria-hidden="true" className="size-5 shrink-0 text-text-3" /> : null}
      <div className={cn("flex min-w-0 flex-1 flex-col items-start border-[1.5px] border-sage bg-card", large ? "rounded-2xl" : "rounded-[14px]", small ? "gap-2.5 px-5 py-[18px]" : "gap-3 px-[22px] py-5")}>
        <span className={cn("flex items-center gap-1.5 leading-[1.2] font-semibold text-sage", label)}>
          {small ? null : <Sparkles aria-hidden="true" className={large ? "size-[15px]" : "size-3.5"} />}
          {cleanLabel}
        </span>
        <p className={cn("self-stretch font-medium whitespace-pre-wrap text-foreground", small ? "text-base leading-[1.4]" : large ? "text-lg leading-[1.45]" : "text-[17px] leading-[1.45]")}>{clean}</p>
      </div>
    </div>
  );
}
