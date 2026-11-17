import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { ChartNoAxesColumnIncreasing, ChevronRight, CirclePause, Download, FileMusic, Laptop, Monitor } from "lucide-react";
import { PillPreview } from "@/components/discovery/PillPreview";
import { cn } from "@/lib/utils";
import { isMacOS } from "@/lib/platform";

const mac = (macLabel: string, windowsLabel: string) => (isMacOS ? macLabel : windowsLabel);

function Chip({ children, className }: { children: ReactNode; className?: string }) {
  return <span className={cn("inline-flex items-center gap-1 whitespace-nowrap rounded-[7px] bg-card px-[7px] py-[5px] text-[11px] leading-none text-foreground shadow-[0_0_0_1px_var(--border)]", className)}>{children}</span>;
}

/** A small slice of the app's own card look, for pictures of a real row or menu. */
function Panel({ children, className }: { children: ReactNode; className?: string }) {
  return <span className={cn("flex w-[min(150px,88%)] flex-col gap-1 rounded-[8px] bg-card px-2 py-1.5 text-left text-[10px] leading-[1.25] text-foreground shadow-[0_0_0_1px_var(--border)]", className)}>{children}</span>;
}

function MiniButton({ children, ghost = false }: { children: ReactNode; ghost?: boolean }) {
  return <span className={cn("inline-flex h-[18px] shrink-0 items-center gap-0.5 rounded-[5px] px-1.5 text-[9.5px] font-medium", ghost ? "text-foreground" : "bg-foreground text-background")}>{children}</span>;
}

function Tile({ icon: Icon }: { icon: LucideIcon }) {
  return <span className="flex size-11 items-center justify-center rounded-[11px] bg-card text-sage shadow-[0_0_0_1px_var(--border)]"><Icon className="size-[22px]" strokeWidth={1.75} /></span>;
}

export type DiscoveryArtKind = "hold" | "dictionary" | "suggestion" | "polish" | "voice_commands" | "realtime" | "pill" | "retry" | "tray" | "media" | "file" | "share" | "history" | "insights" | "model" | "cloud" | "terminal";

/** Small pictures of each feature as the app really shows it: the real pill (PillPreview) only where the feature is dictating, otherwise a small slice of the feature's own row, menu or control, or its own icon. Decorative only. */
export function DiscoveryArt({ kind, shortcut, className }: { kind: DiscoveryArtKind; shortcut?: string[]; className?: string }) {
  const keys = shortcut && shortcut.length > 0 ? shortcut : [isMacOS ? "⌥" : "Ctrl", "Space"];
  let content: ReactNode;
  switch (kind) {
    case "hold":
      content = <>
        <span className="flex gap-1">{keys.map((key, index) => <kbd key={key + "-" + index} className="rounded-[6px] bg-card px-2 py-1 font-sans text-[11px] font-semibold text-foreground shadow-[0_0_0_1px_var(--border)]">{key}</kbd>)}</span>
        <PillPreview />
      </>;
      break;
    case "dictionary":
      content = <Panel className="gap-0.5 py-1">
        <span className="grid grid-cols-2 px-1 text-[8.5px] font-semibold tracking-[0.4px] text-muted-foreground uppercase"><span>Word</span><span>Sounds like</span></span>
        <span className="grid grid-cols-2 border-t border-border px-1 pt-1"><span className="font-medium">Bryan</span><span className="text-muted-foreground">Brian</span></span>
      </Panel>;
      break;
    case "suggestion":
      content = <Panel>
        <span><b className="font-medium">Bryan</b> <span className="text-muted-foreground">heard as “Brian”</span></span>
        <span className="flex gap-1"><MiniButton>Add to Dictionary</MiniButton><MiniButton ghost>Dismiss</MiniButton></span>
      </Panel>;
      break;
    case "polish":
      content = <span className="flex w-[min(150px,90%)] flex-col items-stretch gap-1.5 text-[10.5px] leading-[1.3]">
        <span className="rounded-[7px] bg-card/60 px-2 py-1 text-muted-foreground shadow-[0_0_0_1px_var(--border)]"><s className="decoration-[1.5px]">um,</s> so we <s className="decoration-[1.5px]">like,</s> move the <s className="decoration-[1.5px]">the</s> call</span>
        <span className="rounded-[7px] bg-card px-2 py-1 text-foreground shadow-[0_0_0_1px_var(--border)]">So we move the call.</span>
      </span>;
      break;
    case "voice_commands":
      content = <><Chip>“new line” ↵</Chip><PillPreview /></>;
      break;
    case "realtime":
      content = <PillPreview state="live" scale={0.42} />;
      break;
    case "pill":
      content = <PillPreview scale={0.56} />;
      break;
    case "retry":
      content = <Panel>
        <span className="font-medium">A recording was kept</span>
        <span className="flex items-center justify-between gap-1"><span className="text-muted-foreground">Audio kept for 9 more minutes</span><MiniButton>Retry</MiniButton></span>
      </Panel>;
      break;
    case "tray":
      content = <Panel className="gap-0.5 py-1">
        {[mac("Start Dictation", "Start dictation"), "Polish: Off", "Engine: Parakeet v3", "Open Voicetypr"].map((item, index) => (
          <span key={item} className={cn("flex items-center justify-between rounded-[4px] px-1 py-px", index === 0 && "bg-sage-bg")}>{item}{index === 1 || index === 2 ? <ChevronRight className="size-2.5 text-muted-foreground" /> : null}</span>
        ))}
      </Panel>;
      break;
    case "media":
      content = <Tile icon={CirclePause} />;
      break;
    case "file":
      content = <><Tile icon={FileMusic} /><Chip>Transcribe a file…</Chip></>;
      break;
    case "share":
      content = <span className="flex items-center gap-1.5"><Tile icon={Laptop} /><span className="w-5 border-t-2 border-dashed border-sage" /><Tile icon={Monitor} /></span>;
      break;
    case "history":
      content = <Panel className="gap-0.5 py-1">
        <span className="flex items-center justify-between rounded-[4px] bg-muted px-1 py-px">Export<ChevronRight className="size-2.5 text-muted-foreground" /></span>
        {["JSON (.json)", "Plain text (.txt)", "Markdown (.md)"].map((item) => <span key={item} className="px-1 py-px pl-3 text-muted-foreground">{item}</span>)}
      </Panel>;
      break;
    case "insights":
      content = <Tile icon={ChartNoAxesColumnIncreasing} />;
      break;
    case "model":
      content = <Panel>
        <span className="flex items-center gap-1.5"><i className="size-2.5 shrink-0 rounded-full border border-border" /><span className="font-medium">Base (English)</span></span>
        <span className="flex items-center justify-between pl-4"><span className="text-muted-foreground">141 MB</span><MiniButton ghost><Download className="size-2.5" />Download</MiniButton></span>
      </Panel>;
      break;
    case "cloud":
      content = <Panel className="flex-row items-center gap-1.5">
        <i className="size-2.5 shrink-0 rounded-full border border-border" />
        <span className="min-w-0 flex-1"><span className="block font-medium">Soniox</span><span className="block text-muted-foreground">Needs a key</span></span>
        <MiniButton ghost>Add key</MiniButton>
      </Panel>;
      break;
    case "terminal":
      content = <span className="w-[min(160px,90%)] rounded-[7px] bg-[#121316] px-2 py-1.5 text-left font-mono text-[9.5px] leading-[1.35] text-white/90"><span className="text-[#8FD1A8]">$</span> voicetypr transcribe --file call.m4a --json</span>;
      break;
  }
  return <div aria-hidden className={cn("relative flex h-24 w-[150px] shrink-0 flex-col items-center justify-center gap-2.5 overflow-hidden rounded-[14px] bg-sage-bg", className)}>{content}</div>;
}
