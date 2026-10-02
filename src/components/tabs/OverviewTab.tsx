import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { AudioLines, ChevronRight, Languages, Sparkles, TextCursorInput } from "lucide-react";
import { Button } from "@/components/settings/SettingsButton";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { SettingsCard, SettingsPage } from "@/components/settings/settings-ui";
import { KeyCaps } from "@/components/KeyCaps";
import { toShareCardStats } from "@/components/shareCardRenderer";
import { useUsageStats } from "@/components/insights/useUsageStats";
import { ShareStatsModal } from "@/components/ShareStatsModal";
import { languages } from "@/components/languages";
import { useReadiness } from "@/contexts/ReadinessContext";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { useSettings } from "@/contexts/SettingsContext";
import { useTranscriptionHistory } from "@/hooks/useTranscriptionHistory";
import { useActiveTrigger } from "@/hooks/useActiveTrigger";
import { getModelDisplayName } from "@/lib/model-display";
import { homeStatus } from "@/lib/home-readiness";
import { isMacOS } from "@/lib/platform";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import { presetDisplayLabel, type AISettings, type EnhancementOptions } from "@/types/ai";
import { cn } from "@/lib/utils";
import type { TranscriptionHistory } from "@/types";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import type { SourceFilter } from "@/components/sections/models/types";
import { useTestDictation } from "./overview/useTestDictation";
import { useActiveRemoteLabel } from "./overview/useActiveRemoteLabel";
import { formatWeekSavedTime, useOverviewStats } from "./overview/useOverviewStats";


function relativeTime(date: Date): string {
  const minutes = Math.max(0, Math.floor((Date.now() - date.getTime()) / 60_000));
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 1_440) return `${Math.floor(minutes / 60)}h`;
  if (minutes < 10_080) return `${Math.floor(minutes / 1_440)}d`;
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" }).format(date);
}

function RecentRow({ item }: { item: TranscriptionHistory }) {
  return <li className="flex min-w-0 items-center gap-[14px] rounded-[10px] border-b border-border px-3 py-[11px] last:border-0">
    <span className="min-w-0 flex-1 line-clamp-2 text-[13px] leading-[normal] text-foreground" title={item.text}>{item.text}</span>
    <span className="max-w-24 shrink-0 truncate text-xs text-muted-foreground">{item.writing?.context_hint?.app_name ?? "Voicetypr"}</span>
    <time dateTime={item.timestamp.toISOString()} className="w-10 shrink-0 text-right font-mono text-[11px] text-muted-foreground">{relativeTime(item.timestamp)}</time>
  </li>;
}

export function OverviewTab({ onNavigate, onNavigateSettingsPane, onSourceFilterChange }: { onNavigate?: (section: ScreenId) => void; onNavigateSettingsPane?: (pane: SettingsPane) => void; onSourceFilterChange?: (source: SourceFilter) => void }) {
  const readiness = useReadiness();
  const { settings } = useSettings();
  const trigger = useActiveTrigger(settings);
  const remoteLabel = useActiveRemoteLabel(readiness.remoteSelected);
  const { downloadProgress } = useModelManagementContext();
  const [polishLabel, setPolishLabel] = useState("Off");
  const [tryOpen, setTryOpen] = useState(false);
  const [tryText, setTryText] = useState("");
  const { feedback: tryFeedback, contentChanged, reset: resetTryFeedback } = useTestDictation(tryOpen);
  const [shareOpen, setShareOpen] = useState(false);
  const usage = useUsageStats("all", shareOpen);
  const { history, totalCount, isLoading, loadError, refreshHistory } = useTranscriptionHistory({ limit: 500, includeTotalCount: true });
  const stats = useOverviewStats(history, totalCount);
  const model = settings?.current_model ?? "";
  const engine = settings?.current_model_engine ?? "whisper";
  const status = homeStatus({ model, engine, modelAvailable: readiness.selectedModelAvailable, canRecord: readiness.canRecord, remoteSelected: readiness.remoteSelected, remoteAvailable: readiness.remoteAvailable, remoteLabel, downloadProgress: downloadProgress[model] ?? null, licenseValid: readiness.licenseValid, licenseStatus: readiness.licenseStatus, hasMicrophonePermission: readiness.hasMicrophonePermission });
  const caps = trigger.hotkey
    ? shortcutKeyCaps(trigger.hotkey, isMacOS ? "darwin" : "windows")
    : trigger.kbdLabel && trigger.kbdLabel !== "Not set" ? [trigger.kbdLabel] : [];
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

  const chips = [
    { label: "Engine", value: engineLabel, screen: "transcription" as const, icon: AudioLines },
    { label: "Language", value: language, screen: "transcription" as const, icon: Languages },
    { label: "Polish", value: polishLabel, screen: "polish" as const, icon: Sparkles },
    { label: "Live preview", value: settings?.transcription_mode === "live_preview" ? "On" : "Off", screen: "transcription" as const, icon: TextCursorInput },
  ];

  return <SettingsPage wide container className="min-h-full gap-[22px]">
    <section data-pencil-name="Hero" className="flex flex-col gap-[14px] rounded-[16px] border border-border bg-card px-7 py-[26px]">
      <div className="flex flex-wrap items-center justify-between gap-3">
        {status.ready ? <span role="status" className="inline-flex items-center gap-1.5 rounded-full bg-sage-bg px-[9px] py-[3px] text-[11.5px] leading-[normal] font-medium text-sage"><span aria-hidden className="size-1.5 rounded-full bg-sage" />{status.label}</span> : <button type="button" onClick={() => { if (status.pane) onNavigateSettingsPane?.(status.pane); else if (status.screen) { if (status.source) onSourceFilterChange?.(status.source); onNavigate?.(status.screen); } }} className="inline-flex items-center gap-1.5 rounded-full bg-warn-bg px-2.5 py-1 text-xs font-medium text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"><span aria-hidden className="size-1.5 rounded-full bg-warn" />{status.label}<ChevronRight className="size-3 text-warn" /></button>}
        <Button variant="outline" size="sm" onClick={() => { setTryText(""); resetTryFeedback(); setTryOpen(true); }}>Try a test dictation</Button>
      </div>
      <h1 className="flex flex-wrap items-center gap-x-[10px] gap-y-1 text-[30px] font-semibold leading-[normal] tracking-[-0.6px] text-foreground">Press <KeyCaps caps={caps} size="lg" /> and start talking</h1>
      <p className="text-[14px] leading-[normal] text-muted-foreground">{trigger.mode === "push_to_talk" ? "Hold to talk, release to paste into any app. Press Esc twice to cancel." : "Press once to start, again to paste into any app. Press Esc twice to cancel."}</p>
      <div className="pt-2 flex flex-wrap gap-2">{chips.map(({ label, value, screen, icon: Icon }) => <button key={label} type="button" onClick={() => onNavigate?.(screen)} className="inline-flex max-w-full items-center gap-[7px] rounded-[9px] bg-muted px-[11px] py-[7px] text-xs leading-[normal] text-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"><Icon aria-hidden className="size-3.5 shrink-0 text-muted-foreground" /><span className="text-muted-foreground">{label}</span><strong className="truncate font-medium">{value}</strong></button>)}</div>
    </section>

    <div className="grid flex-1 gap-[22px] @min-[620px]:grid-cols-[minmax(0,1fr)_230px]">
      <section aria-labelledby="recent-title" data-pencil-name="Recent" className="min-w-0">
        <div className="flex items-center justify-between gap-3"><h2 id="recent-title" className="text-[15px] leading-[normal] font-semibold text-foreground">Recent</h2><button type="button" onClick={() => onNavigate?.("history")} className="text-[12.5px] leading-[normal] font-medium text-sage hover:underline focus-visible:outline-2 focus-visible:outline-ring">View all history →</button></div>
        {isLoading && history.length === 0 ? <p className="py-6 text-sm text-muted-foreground">Loading dictations…</p> : loadError && history.length === 0 ? <p className="py-6 text-sm text-muted-foreground">Couldn’t load history. <button type="button" onClick={() => void refreshHistory()} className="text-sage underline">Retry</button></p> : history.length === 0 ? <p className="py-6 text-sm text-muted-foreground">Your dictations will show up here.</p> : <ul className="mt-2">{history.slice(0, 4).map((item) => <RecentRow key={item.id} item={item} />)}</ul>}
      </section>
      <SettingsCard title="Last 7 days" className="rounded-[16px] p-5 [&>div:first-child_h2]:text-[13px] [&>div:first-child_h2]:font-medium [&>div:first-child_h2]:text-muted-foreground">
        <div className="mt-4 font-sans text-[28px] font-semibold tracking-tight text-foreground">{weekTime}</div>
        <p className="text-xs text-muted-foreground">{stats.weekCount === 0 ? "nothing yet in the last 7 days" : stats.weekSavedMinutes === 0 ? "less than a minute estimated saved" : "estimated saved vs typing at 40 wpm"}</p>
        <div role="img" aria-label={`Dictations over the last seven days: ${stats.weekDays.map((day) => `${day.count} dictations on ${day.label}`).join(", ")}`} className="mt-4 flex h-[70px] items-end gap-1.5">{stats.weekDays.map((day) => <div key={day.key} className="flex h-full min-w-0 flex-1 flex-col justify-end gap-1"><div title={`${day.count} dictations on ${day.label}`} className={cn("min-h-1 rounded-sm", day.count === stats.weekMax && day.count > 0 ? "bg-sage" : "bg-sage-bg")} style={{ height: `${Math.max(6, Math.round(day.count / Math.max(1, stats.weekMax) * 70))}%` }} /><span className="text-center font-mono text-[10px] text-muted-foreground">{day.label.slice(0, 1)}</span></div>)}</div>
        <dl className="mt-5 grid grid-cols-2 gap-4"><div><dd className="font-mono text-base font-semibold text-foreground">{stats.weekWords.toLocaleString()}</dd><dt className="text-xs text-muted-foreground">words</dt></div><div><dd className="font-mono text-base font-semibold text-foreground">{stats.weekCount.toLocaleString()}</dd><dt className="text-xs text-muted-foreground">dictations</dt></div></dl>
        <div className="mt-4 flex flex-wrap items-center gap-1"><button type="button" className="text-xs text-sage hover:underline focus-visible:outline-2 focus-visible:outline-ring" onClick={() => onNavigate?.("insights")}>See insights →</button><Button variant="ghost" size="sm" className="text-xs" onClick={() => setShareOpen(true)}>Share stats</Button></div>
        {shareOpen && !usage.stats && <p role="status" className="text-xs text-muted-foreground">{usage.error ? "Couldn’t load stats." : "Loading stats…"}{usage.error && <button className="ml-1 text-sage underline" onClick={() => void usage.refresh()}>Retry</button>}</p>}
      </SettingsCard>
    </div>

    <Dialog open={tryOpen} onOpenChange={(open) => { resetTryFeedback(); setTryOpen(open); }}><DialogContent className="sm:max-w-lg"><DialogHeader><DialogTitle>Try a test dictation</DialogTitle><DialogDescription>Place the cursor below, press <KeyCaps caps={caps} />, and speak. Your words will appear here.</DialogDescription></DialogHeader><Textarea autoFocus aria-label="Test dictation" placeholder="Dictate here…" value={tryText} onChange={(event) => { if (event.target.value !== tryText) contentChanged(); setTryText(event.target.value); }} className="min-h-32" /><p role="status" className="text-sm text-muted-foreground">{tryFeedback ?? (tryText.trim() ? `${tryText.trim().split(/\s+/).length} words` : "Waiting for your dictation…")}</p></DialogContent></Dialog>
    {usage.stats && <ShareStatsModal open={shareOpen} onOpenChange={setShareOpen} stats={toShareCardStats(usage.stats)} />}
  </SettingsPage>;
}
