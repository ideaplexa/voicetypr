import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import trayIcon from "../../../src-tauri/icons/tray-light.png";
import { ArrowLeft, ArrowRight, AudioLines, ChartNoAxesColumnIncreasing, ChevronRight, Download, FileAudio, Laptop, Monitor, Network, RotateCcw, Share2, Upload, Wifi } from "lucide-react";
import { appColour } from "@/pill/app-icon";
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

/** The setup tour's full-size pictures (10b, 10d to 10h); the caller sizes the picture. */
export type TourArtKind = "tour_suggestion" | "tour_file" | "tour_share" | "tour_styles" | "tour_history" | "tour_tray";

export type DiscoveryArtKind = "hold" | "dictionary" | "suggestion" | "polish" | "voice_commands" | "realtime" | "pill" | "retry" | "tray" | "media" | "file" | "share" | "remote" | "history" | "insights" | "model" | "cloud" | "terminal" | TourArtKind;

const tourCard = "rounded-[14px] bg-card shadow-[0_0_0_0.5px_var(--border)]";

function TourComputer({ icon: Icon, name, line }: { icon: LucideIcon; name: string; line: string }) {
  return (
    <span className={cn(tourCard, "flex flex-col items-center gap-2 px-5 py-4")}>
      <Icon className="size-[34px] text-foreground" strokeWidth={1.5} />
      <span className="text-[14.5px] font-semibold text-foreground">{name}</span>
      <span className="text-[13px] text-muted-foreground">{line}</span>
    </span>
  );
}

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
      content = <Panel className="flex-row items-center justify-between gap-2">
        <span className="font-medium">Pause media during recording</span>
        <span className="flex h-3.5 w-6 shrink-0 items-center justify-end rounded-full bg-sage px-0.5"><i className="size-2.5 rounded-full bg-white" /></span>
      </Panel>;
      break;
    case "file":
      content = <><Tile icon={Upload} /><Chip>Transcribe a file…</Chip></>;
      break;
    case "share":
      content = <Tile icon={Share2} />;
      break;
    case "remote":
      content = <Tile icon={Network} />;
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
    case "tour_suggestion":
      content = <span className={cn(tourCard, "flex w-[360px] flex-col items-start gap-3.5 p-5")}>
        <span className="flex items-center gap-2.5 text-[17px]">
          <span className="text-text-3">Brian</span>
          <ArrowRight className="size-4 text-text-3" />
          <span className="font-semibold text-foreground">Bryan</span>
        </span>
        <span className="flex gap-2 text-[15px] font-semibold">
          <span className="flex h-11 items-center rounded-[10px] bg-sage px-[22px] text-sage-foreground">Add to Dictionary</span>
          <span className="flex h-11 items-center rounded-[10px] bg-card px-[22px] text-foreground shadow-[0_0_0_0.5px_var(--border)]">Dismiss</span>
        </span>
      </span>;
      break;
    case "tour_file":
      content = <span className="flex items-center gap-[18px]">
        <span className={cn(tourCard, "flex items-center gap-2.5 px-[18px] py-3.5")}>
          <FileAudio className="size-[22px] text-sage" />
          <span className="font-mono text-[15px] font-medium text-foreground">call.m4a</span>
        </span>
        <ArrowRight className="size-[18px] text-text-3" />
        <span className={cn(tourCard, "flex w-[170px] flex-col gap-2 px-4 py-3.5")}>
          {[130, 110, 140, 80].map((width) => <i key={width} className="block h-[7px] rounded bg-border" style={{ width }} />)}
        </span>
      </span>;
      break;
    case "tour_share":
      content = <span className="flex items-center gap-[22px]">
        <TourComputer icon={Laptop} name={mac("MacBook Air", "Laptop")} line={mac("Uses the Mac mini", "Uses the desktop PC")} />
        <span className="flex flex-col items-center gap-1.5">
          <Wifi className="size-5 text-sage" />
          <ArrowLeft className="size-4 text-text-3" />
        </span>
        <TourComputer icon={Monitor} name={mac("Studio Mac mini", "Desktop PC")} line="Sharing on" />
      </span>;
      break;
    case "tour_styles":
      content = <span className={cn(tourCard, "flex w-[380px] flex-col overflow-hidden")}>
        {[["Messages", "Messages and email"], ["ChatGPT", "Prompts for AI"], [mac("Ghostty", "Windows Terminal"), "Code and tech notes"]].map(([app, style]) => (
          <span key={app} className="flex h-[52px] items-center gap-3 border-b border-border px-4 last:border-b-0">
            <span className="flex size-6 items-center justify-center text-[12px] font-semibold text-white" style={{ borderRadius: isMacOS ? 6 : 5, background: appColour(app) }}>{app[0]}</span>
            <span className="min-w-0 flex-1 text-left text-[15px] font-medium text-foreground">{app}</span>
            <span className="rounded-full bg-sage-bg px-[9px] py-[3px] text-[12.5px] font-semibold text-sage">{style}</span>
          </span>
        ))}
      </span>;
      break;
    case "tour_history":
      content = <span className={cn(tourCard, "flex w-[400px] flex-col overflow-hidden text-left text-[14.5px] text-foreground")}>
        {[["14:32", "Let’s move the sync to Friday at 3…"], ["14:05", "Bryan, the deck is ready for review…"]].map(([time, text]) => (
          <span key={time} className="flex h-[46px] items-center gap-3 border-b border-border px-4">
            <span className="font-mono text-[13px] text-text-3">{time}</span>
            {text}
          </span>
        ))}
        <span className="flex h-[54px] items-center gap-3 bg-warn-bg px-4">
          <AudioLines className="size-[17px] text-warn" />
          <span className="min-w-0 flex-1">Couldn’t transcribe · audio kept</span>
          <span className="flex h-11 items-center gap-2 rounded-[10px] bg-card px-[22px] text-[15px] font-semibold shadow-[0_0_0_0.5px_var(--border)]"><RotateCcw className="size-4" />Retry</span>
        </span>
      </span>;
      break;
    case "tour_tray":
      content = <span className="flex w-[340px] flex-col items-end gap-1.5">
        <span className="flex items-center gap-3.5 rounded-lg bg-card px-2.5 py-[5px] text-[12.5px] text-text-3">
          <Wifi className="size-3.5" />
          <span className="rounded-[5px] bg-sage-bg p-[3px]"><img src={trayIcon} alt="" className="size-4" /></span>
          14:32
        </span>
        <span className="flex w-[250px] flex-col rounded-[10px] bg-card p-[5px] text-left text-[13px] text-foreground shadow-[0_0_0_0.5px_var(--border),0_6px_18px_rgba(0,0,0,.12)]">
          {[mac("Start Dictation", "Start dictation"), mac("Retry Last Dictation", "Retry last dictation"), "Polish: On", "Engine: Parakeet V3", "Open Voicetypr"].map((item, index) => (
            <span key={item} className={cn("flex items-center justify-between rounded-md px-2.5 py-[5px]", index === 0 && "bg-sage text-sage-foreground")}>
              {item}
              {index === 2 || index === 3 ? <ChevronRight className="size-3 text-text-3" /> : null}
            </span>
          ))}
        </span>
      </span>;
      break;
    case "terminal":
      content = <span className="w-[min(160px,90%)] rounded-[7px] bg-[#121316] px-2 py-1.5 text-left font-mono text-[9.5px] leading-[1.35] text-white/90"><span className="text-[#8FD1A8]">{isMacOS ? "$" : "PS>"}</span> {isMacOS ? "" : "cmd /c "}voicetypr transcribe --file call.m4a --json</span>;
      break;
  }
  return <div aria-hidden className={cn("relative flex h-24 w-[150px] shrink-0 flex-col items-center justify-center gap-2.5 overflow-hidden rounded-[14px] bg-sage-bg", className)}>{content}</div>;
}
