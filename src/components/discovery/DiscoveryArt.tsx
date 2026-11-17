import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { isMacOS } from "@/lib/platform";

/** The pill character: the recording pill's dark capsule with five bars for a face. */
export function PillCharacter({ size = "md", asleep = false, className }: { size?: "sm" | "md" | "lg"; asleep?: boolean; className?: string }) {
  const box = size === "sm" ? "h-[21px] w-[44px] gap-[2px]" : size === "lg" ? "h-10 w-[84px] gap-1" : "h-[30px] w-[62px] gap-[3px]";
  const bar = size === "sm" ? "w-[2.4px]" : size === "lg" ? "w-1" : "w-[3px]";
  const heights = asleep ? [3, 3, 3, 3, 3] : size === "sm" ? [3, 6, 9, 6, 3] : size === "lg" ? [5, 10, 16, 10, 5] : [4, 8, 12, 8, 4];
  return (
    <span aria-hidden className={cn("relative inline-flex shrink-0 items-center justify-center rounded-full bg-[#121316] shadow-[inset_0_0_0_1px_rgba(255,255,255,.16),0_10px_22px_-10px_rgba(0,0,0,.45)]", box, className)}>
      {heights.map((height, index) => (
        <i key={index} className={cn("block rounded-[3px] bg-white", bar, index === 0 || index === 4 || asleep ? "opacity-55" : "opacity-90")} style={{ height }} />
      ))}
      {asleep ? <span className="absolute -top-4 -right-3 font-mono text-[12px] font-semibold text-sage">z<sup className="text-[9px]">z</sup></span> : null}
    </span>
  );
}

function Chip({ children, className }: { children: ReactNode; className?: string }) {
  return <span className={cn("absolute whitespace-nowrap rounded-[7px] bg-card px-[7px] py-[5px] text-[11px] leading-none text-foreground shadow-[0_0_0_1px_var(--border)]", className)}>{children}</span>;
}

function Computer({ className }: { className?: string }) {
  return (
    <span className={cn("absolute h-7 w-[38px] rounded-[5px] bg-card shadow-[0_0_0_1.5px_var(--border)]", className)}>
      <span className="absolute right-3 -bottom-[7px] left-3 h-1 rounded-sm bg-border" />
    </span>
  );
}

export type DiscoveryArtKind = "hold" | "dictionary" | "polish" | "voice_commands" | "realtime" | "file" | "share" | "history" | "insights" | "sources" | "terminal" | "pill";

const centerTop = "left-1/2 top-[14%] -translate-x-1/2";

/** Small built-from-parts pictures around the pill character (MOI-154). Decorative only. */
export function DiscoveryArt({ kind, shortcut, className }: { kind: DiscoveryArtKind; shortcut?: string[]; className?: string }) {
  const keys = shortcut && shortcut.length > 0 ? shortcut : [isMacOS ? "⌥" : "Ctrl", "Space"];
  const pillBottom = <PillCharacter size="md" className="absolute bottom-[14%] left-1/2 -translate-x-1/2" />;
  let content: ReactNode;
  switch (kind) {
    case "hold":
      content = <>
        <span className={cn("absolute flex gap-1", centerTop)}>{keys.map((key, index) => <kbd key={`${key}-${index}`} className="rounded-[6px] bg-card px-2 py-1 font-sans text-[11px] font-semibold text-foreground shadow-[0_0_0_1px_var(--border)]">{key}</kbd>)}</span>
        {pillBottom}
      </>;
      break;
    case "dictionary":
      content = <><Chip className={centerTop}><s className="text-destructive decoration-[1.5px]">Brian</s> → Bryan</Chip>{pillBottom}</>;
      break;
    case "polish":
      content = <><Chip className="top-[14%] left-[8%] text-muted-foreground"><s className="text-destructive decoration-[1.5px]">um, like</s></Chip><PillCharacter size="sm" className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2" /><Chip className="right-[8%] bottom-[14%]">Clean.</Chip></>;
      break;
    case "voice_commands":
      content = <><Chip className={centerTop}>“new line” ↵</Chip>{pillBottom}</>;
      break;
    case "realtime":
      content = <><Chip className={centerTop}>Let’s move the<span className="ml-0.5 inline-block h-[0.95em] w-[1.5px] bg-sage align-[-0.12em]" /></Chip>{pillBottom}</>;
      break;
    case "file":
      content = <>
        <span className="absolute top-[18%] left-[18%] h-11 w-[34px] rounded-[5px] bg-card shadow-[0_0_0_1px_var(--border)]"><span className="absolute top-3.5 right-2 left-2 h-4 rounded-sm bg-[repeating-linear-gradient(90deg,var(--sage)_0_2px,transparent_2px_5px)] opacity-80" /></span>
        <PillCharacter className="absolute right-[14%] bottom-[18%]" />
      </>;
      break;
    case "share":
      content = <>
        <Computer className="top-[30%] left-[7%]" />
        <Computer className="top-[30%] right-[7%]" />
        <span className="absolute top-[44%] right-[30%] left-[30%] border-t-2 border-dashed border-sage" />
        <PillCharacter size="sm" className="absolute top-[37%] left-1/2 -translate-x-1/2" />
      </>;
      break;
    case "history":
      content = <>
        <span className="absolute top-[14%] right-[12%] left-[12%] flex flex-col gap-[5px]">{["w-full", "w-4/5", "w-3/5"].map((width) => <i key={width} className={cn("block h-1.5 rounded-[3px] bg-card shadow-[0_0_0_1px_var(--border)]", width)} />)}</span>
        {pillBottom}
      </>;
      break;
    case "insights":
      content = <span className="absolute inset-x-[22%] bottom-[16%] flex h-[60%] items-end gap-1.5">{[30, 55, 40, 80, 65].map((height) => <i key={height} className="block flex-1 rounded-t-[3px] bg-sage" style={{ height: `${height}%` }} />)}</span>;
      break;
    case "sources":
      content = <>
        <Chip className="top-[14%] left-[6%]">this {isMacOS ? "Mac" : "PC"}</Chip>
        <Chip className="top-[14%] right-[8%]">cloud</Chip>
        <Chip className="bottom-[12%] left-[8%]">network</Chip>
        <PillCharacter size="sm" className="absolute right-[12%] bottom-[16%]" />
      </>;
      break;
    case "terminal":
      content = <>
        <span className="absolute top-[14%] right-[8%] left-[8%] rounded-[6px] bg-[#121316] px-[7px] py-[5px] font-mono text-[10.5px] text-[#8FD1A8]">$ voicetypr<span className="ml-0.5 inline-block h-[0.95em] w-[1.5px] bg-[#8FD1A8] align-[-0.12em]" /></span>
        {pillBottom}
      </>;
      break;
    case "pill":
      content = <PillCharacter size="lg" />;
      break;
  }
  return <div aria-hidden className={cn("relative flex h-24 w-[150px] shrink-0 items-center justify-center overflow-hidden rounded-[14px] bg-sage-bg", className)}>{content}</div>;
}
