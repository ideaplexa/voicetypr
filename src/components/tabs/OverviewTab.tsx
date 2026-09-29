import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { AudioLines, ChevronRight, Languages, Sparkles, TextCursorInput } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { SettingsCard, SettingsPage } from "@/components/settings/settings-ui";
import { ShareStatsModal } from "@/components/ShareStatsModal";
import { languages } from "@/components/languages";
import { useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { useTranscriptionHistory } from "@/hooks/useTranscriptionHistory";
import { useActiveTrigger } from "@/hooks/useActiveTrigger";
import { isCloudEngine } from "@/lib/cloudProviders";
import { getModelDisplayName } from "@/lib/model-display";
import { isMacOS } from "@/lib/platform";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import { presetDisplayLabel, type AISettings, type EnhancementOptions } from "@/types/ai";
import { cn } from "@/lib/utils";
import type { TranscriptionHistory } from "@/types";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import { useActiveRemoteLabel } from "./overview/useActiveRemoteLabel";
import { formatTimeSaved, formatWeekSavedTime, useOverviewStats } from "./overview/useOverviewStats";

interface HomeStatus {
  label: string;
  ready: boolean;
}

export function homeStatus({ model, engine, modelAvailable, canRecord, remoteSelected, remoteLabel, downloadProgress }: {
  model: string;
  engine: string;
  modelAvailable: boolean | null;
  canRecord: boolean;
  remoteSelected: boolean;
  remoteLabel: string | null;
  downloadProgress: number | null;
}): HomeStatus {
  const modelLabel = getModelDisplayName(model) ?? model;
  if (remoteSelected) return canRecord
    ? { label: `Ready · ${remoteLabel ?? "Remote Voicetypr"}`, ready: true }
    : { label: "Remote source unavailable", ready: false };
  if (!model) return { label: "No model yet", ready: false };
  if (isCloudEngine(engine)) return modelAvailable === false
    ? { label: "Needs an API key", ready: false }
    : canRecord
      ? { label: `Ready · ${modelLabel.replace(/ \(Cloud\)$/, "")} (cloud)`, ready: true }
      : { label: "Needs attention", ready: false };
  if (downloadProgress !== null && modelAvailable !== true) return { label: `Downloading ${modelLabel} · ${Math.round(downloadProgress)}%`, ready: false };
  if (modelAvailable === false) return { label: "No model yet", ready: false };
  if (!canRecord) return { label: "Needs attention", ready: false };
  return { label: `Ready · ${modelLabel} runs on this ${isMacOS ? "Mac" : "PC"}`, ready: true };
}

function ShortcutCaps({ caps }: { caps: string[] }) {
  if (caps.length === 0) return <span className="text-warn">your recording shortcut</span>;
  return <span aria-label={caps.join(" + ")} className="inline-flex flex-wrap items-center gap-1 align-middle">
    {caps.map((cap, index) => <kbd key={`${cap}-${index}`} className="inline-flex min-h-9 items-center rounded-[9px] border border-border bg-secondary px-2.5 font-sans text-[0.76em] font-semibold text-foreground">{cap}</kbd>)}
  </span>;
}

function relativeTime(date: Date): string {
  const minutes = Math.max(0, Math.floor((Date.now() - date.getTime()) / 60_000));
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 1_440) return `${Math.floor(minutes / 60)}h`;
  if (minutes < 10_080) return `${Math.floor(minutes / 1_440)}d`;
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" }).format(date);
}

function RecentRow({ item }: { item: TranscriptionHistory }) {
  return <li className="flex min-w-0 items-center gap-3 border-b border-border py-3.5 last:border-0">
    <span className="min-w-0 flex-1 truncate text-[13px] text-foreground" title={item.text}>{item.text}</span>
    <span className="max-w-24 shrink-0 truncate text-xs text-text-3">{item.writing?.context_hint?.app_name ?? "Voicetypr"}</span>
    <time dateTime={item.timestamp.toISOString()} className="w-10 shrink-0 text-right font-mono text-[11px] text-text-3">{relativeTime(item.timestamp)}</time>
  </li>;
}

export function OverviewTab({ onNavigate }: { onNavigate?: (section: ScreenId) => void; onNavigateSettingsPane?: (pane: SettingsPane) => void }) {
  const readiness = useReadiness();
  const { settings } = useSettings();
  const trigger = useActiveTrigger(settings?.hotkey);
  const remoteLabel = useActiveRemoteLabel(readiness.remoteSelected);
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [polishLabel, setPolishLabel] = useState("Off");
  const [tryOpen, setTryOpen] = useState(false);
  const [tryText, setTryText] = useState("");
  const [shareOpen, setShareOpen] = useState(false);
  const { history, totalCount, isLoading, loadError, refreshHistory } = useTranscriptionHistory({ limit: 500, includeTotalCount: true });
  const stats = useOverviewStats(history, totalCount);
  const model = settings?.current_model ?? "";
  const engine = settings?.current_model_engine ?? "whisper";
  const status = homeStatus({ model, engine, modelAvailable: readiness.selectedModelAvailable, canRecord: readiness.canRecord, remoteSelected: readiness.remoteSelected, remoteLabel, downloadProgress });
  const caps = shortcutKeyCaps(trigger.hotkey || trigger.kbdLabel, isMacOS ? "darwin" : "windows");
  const language = languages.find((entry) => entry.value === (settings?.speech_language ?? "en"))?.label ?? settings?.speech_language ?? "English";
  const engineLabel = readiness.remoteSelected ? (remoteLabel ?? "Remote") : (getModelDisplayName(model)?.replace(/ \(Cloud\)$/, "") ?? "Choose model");
  const weekTime = formatWeekSavedTime(stats.weekSavedMinutes);

  useEffect(() => {
    let active = true;
    void Promise.all([invoke<AISettings>("get_ai_settings"), invoke<EnhancementOptions>("get_enhancement_options")]).then(([ai, options]) => {
      if (active) setPolishLabel(ai.enabled ? presetDisplayLabel(options.preset).replace(" Dictation", "") : "Off");
    }).catch(() => { if (active) setPolishLabel("Off"); });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<{ model: string; progress: number }>("download-progress", ({ payload }) => {
      if (active && payload.model === model) setDownloadProgress(payload.progress);
    }).then((remove) => { if (active) unlisten = remove; else remove(); }).catch(() => {});
    return () => { active = false; unlisten?.(); };
  }, [model]);

  const chips = [
    { label: "Engine", value: engineLabel, screen: "transcription" as const, icon: AudioLines },
    { label: "Language", value: language, screen: "transcription" as const, icon: Languages },
    { label: "Polish", value: polishLabel, screen: "polish" as const, icon: Sparkles },
    { label: "Live preview", value: settings?.transcription_mode === "live_preview" ? "On" : "Off", screen: "transcription" as const, icon: TextCursorInput },
  ];

  return <SettingsPage wide container className="min-h-full gap-5 pt-0">
    <section className="rounded-[14px] border border-border bg-card p-6 sm:p-7">
      <div className="flex flex-wrap items-center justify-between gap-3">
        {status.ready ? <span role="status" className="inline-flex items-center gap-1.5 rounded-full bg-sage-bg px-2.5 py-1 text-xs font-medium text-sage"><span aria-hidden className="size-1.5 rounded-full bg-sage" />{status.label}</span> : <button type="button" onClick={() => onNavigate?.("transcription")} className="inline-flex items-center gap-1.5 rounded-full bg-warn-bg px-2.5 py-1 text-xs font-medium text-warn focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"><span aria-hidden className="size-1.5 rounded-full bg-warn" />{status.label}<ChevronRight className="size-3" /></button>}
        <Button variant="outline" size="sm" onClick={() => { setTryText(""); setTryOpen(true); }}>Try a test dictation</Button>
      </div>
      <h1 className="mt-6 flex flex-wrap items-center gap-x-2 gap-y-1 text-[clamp(1.5rem,3vw,2rem)] font-semibold leading-tight tracking-tight text-foreground">Press <ShortcutCaps caps={caps} /> and start talking</h1>
      <p className="mt-3 text-[13px] text-muted-foreground">{settings?.recording_mode === "push_to_talk" ? "Hold to talk, release to paste into any app. Press Esc twice to cancel." : "Press once to start, again to paste into any app. Press Esc twice to cancel."}</p>
      <div className="mt-5 flex flex-wrap gap-2">{chips.map(({ label, value, screen, icon: Icon }) => <button key={label} type="button" onClick={() => onNavigate?.(screen)} className="inline-flex min-h-8 max-w-full items-center gap-2 rounded-[10px] bg-muted px-3 text-xs text-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"><Icon aria-hidden className="size-3.5 shrink-0 text-muted-foreground" /><span className="text-muted-foreground">{label}</span><strong className="truncate font-medium">{value}</strong></button>)}</div>
    </section>

    <div className="grid min-h-[340px] flex-1 gap-5 @min-[720px]:grid-cols-[minmax(0,1fr)_230px]">
      <section aria-labelledby="recent-title" className="min-w-0">
        <div className="flex items-center justify-between gap-3"><h2 id="recent-title" className="text-sm font-semibold text-foreground">Recent</h2><button type="button" onClick={() => onNavigate?.("history")} className="text-xs font-medium text-sage hover:underline focus-visible:outline-2 focus-visible:outline-ring">View all history →</button></div>
        {isLoading && history.length === 0 ? <p className="py-6 text-sm text-muted-foreground">Loading dictations…</p> : loadError && history.length === 0 ? <p className="py-6 text-sm text-muted-foreground">Couldn’t load history. <button type="button" onClick={() => void refreshHistory()} className="text-sage underline">Retry</button></p> : history.length === 0 ? <p className="py-6 text-sm text-muted-foreground">Your dictations will show up here.</p> : <ul className="mt-2">{history.slice(0, 4).map((item) => <RecentRow key={item.id} item={item} />)}</ul>}
      </section>
      <SettingsCard title="This week" className="min-h-[300px] [&>div:first-child_h2]:text-[13px] [&>div:first-child_h2]:font-medium [&>div:first-child_h2]:text-muted-foreground">
        <div className="mt-4 font-sans text-[27px] font-semibold tracking-tight text-foreground">{weekTime}</div>
        <p className="text-xs text-muted-foreground">{stats.weekSavedMinutes === 0 ? "nothing yet this week" : "estimated saved vs typing at 40 wpm"}</p>
        <div role="img" aria-label="Dictations over the last seven days" className="mt-5 flex h-20 items-end gap-2">{stats.weekDays.map((day) => <div key={day.key} className="flex h-full min-w-0 flex-1 flex-col justify-end gap-1"><div title={`${day.count} dictations on ${day.label}`} className={cn("min-h-1 rounded-sm", day.count === stats.weekMax && day.count > 0 ? "bg-sage" : "bg-sage-bg")} style={{ height: `${Math.max(6, Math.round(day.count / Math.max(1, stats.weekMax) * 70))}%` }} /><span className="text-center font-mono text-[10px] text-text-3">{day.label.slice(0, 1)}</span></div>)}</div>
        <dl className="mt-5 grid grid-cols-2 gap-4"><div><dd className="font-mono text-base font-semibold text-foreground">{stats.weekWords.toLocaleString()}</dd><dt className="text-xs text-muted-foreground">words</dt></div><div><dd className="font-mono text-base font-semibold text-foreground">{stats.weekCount.toLocaleString()}</dd><dt className="text-xs text-muted-foreground">dictations</dt></div></dl>
        <Button variant="ghost" size="sm" className="mt-4 -ml-2 text-xs" onClick={() => setShareOpen(true)}>Share stats</Button>
      </SettingsCard>
    </div>

    <Dialog open={tryOpen} onOpenChange={setTryOpen}><DialogContent className="sm:max-w-lg"><DialogHeader><DialogTitle>Try a test dictation</DialogTitle><DialogDescription>Place the cursor below, press <ShortcutCaps caps={caps} />, and speak. Your words will appear here.</DialogDescription></DialogHeader><Textarea autoFocus aria-label="Test dictation" placeholder="Dictate here…" value={tryText} onChange={(event) => setTryText(event.target.value)} className="min-h-32" /><p role="status" className={cn("text-sm", tryText.trim() ? "text-sage" : "text-muted-foreground")}>{tryText.trim() ? `Worked · ${tryText.trim().split(/\s+/).length} words` : "Waiting for your dictation…"}</p></DialogContent></Dialog>
    <ShareStatsModal open={shareOpen} onOpenChange={setShareOpen} stats={{ totalTranscriptions: stats.totalTranscriptions, totalWords: stats.totalWords, timeSavedDisplay: formatTimeSaved(stats) }} />
  </SettingsPage>;
}
