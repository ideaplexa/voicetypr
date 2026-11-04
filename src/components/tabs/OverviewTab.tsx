import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { AudioLines, ChevronRight, Languages, Sparkles, TextCursorInput } from "lucide-react";
import { toast } from "sonner";
import { SettingsPage } from "@/components/settings/settings-ui";
import { KeyCaps } from "@/components/KeyCaps";
import { languages } from "@/components/languages";
import { useReadiness } from "@/contexts/ReadinessContext";
import { useDownloadProgress } from "@/contexts/ModelManagementContext";
import { useSettings } from "@/contexts/SettingsContext";
import { useActiveTrigger } from "@/hooks/useActiveTrigger";
import { useKeptRecordings } from "@/hooks/useKeptRecordings";
import { useUpdateStatus } from "@/hooks/useUpdateStatus";
import { getModelDisplayName } from "@/lib/model-display";
import { homeStatus } from "@/lib/home-readiness";
import { homeNeeds, homeTips, type NeedRow, type TipRow } from "@/lib/home-needs";
import { isMacOS } from "@/lib/platform";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import { presetDisplayLabel, type AISettings, type EnhancementOptions } from "@/types/ai";
import { useWritingSettings } from "@/state/writingSettings";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import type { SourceFilter } from "@/components/sections/models/types";
import { useActiveRemoteLabel } from "./overview/useActiveRemoteLabel";
import { MoreFromVoicetypr, NeedsYou } from "./overview/NeedsAndTips";

export function OverviewTab({ onNavigate, onNavigateSettingsPane, onSourceFilterChange }: { onNavigate?: (section: ScreenId) => void; onNavigateSettingsPane?: (pane: SettingsPane) => void; onSourceFilterChange?: (source: SourceFilter) => void }) {
  const readiness = useReadiness();
  const { settings } = useSettings();
  const trigger = useActiveTrigger(settings);
  const remoteLabel = useActiveRemoteLabel(readiness.remoteSelected);
  const [polish, setPolish] = useState<{ label: string; on: boolean | null }>({ label: "Off", on: null });
  const { kept, refresh: refreshKept } = useKeptRecordings();
  const update = useUpdateStatus();
  const writing = useWritingSettings((state) => state.settings);
  const writingLoaded = useWritingSettings((state) => state.loaded);
  const loadWriting = useWritingSettings((state) => state.load);
  const model = settings?.current_model ?? "";
  const modelProgress = useDownloadProgress((progress) => progress[model]);
  const engine = settings?.current_model_engine ?? "whisper";
  const status = homeStatus({ model, engine, modelAvailable: readiness.selectedModelAvailable, canRecord: readiness.canRecord, remoteSelected: readiness.remoteSelected, remoteAvailable: readiness.remoteAvailable, remoteLabel, downloadProgress: modelProgress ?? null, licenseValid: readiness.licenseValid, licenseStatus: readiness.licenseStatus, hasMicrophonePermission: readiness.hasMicrophonePermission });
  const caps = trigger.hotkey
    ? shortcutKeyCaps(trigger.hotkey, isMacOS ? "darwin" : "windows")
    : trigger.kbdLabel && trigger.kbdLabel !== "Not set" ? [trigger.kbdLabel] : [];
  const language = languages.find((entry) => entry.value === (settings?.speech_language ?? "en"))?.label ?? settings?.speech_language ?? "English";
  const engineLabel = readiness.remoteSelected ? (remoteLabel ?? "Remote") : (getModelDisplayName(model)?.replace(/ \(Cloud\)$/, "") ?? "Choose model");

  useEffect(() => {
    let active = true;
    void Promise.all([invoke<AISettings>("get_ai_settings"), invoke<EnhancementOptions>("get_enhancement_options")]).then(([ai, options]) => {
      if (active) setPolish({ on: ai.enabled, label: ai.enabled ? presetDisplayLabel(options.preset).replace(" Dictation", "") : "Off" });
    }).catch(() => { if (active) setPolish({ label: "Off", on: null }); });
    return () => { active = false; };
  }, []);
  useEffect(() => { void loadWriting(); }, [loadWriting]);

  const needs = homeNeeds({ kept, updateVersion: update.install === "restart_pending" ? update.restartVersion : (update.available?.version ?? null), updateInstall: update.install, status, accessibilityMissing: isMacOS && readiness.hasAccessibilityPermission === false });
  const tips = homeTips({ polishOn: polish.on, dictionaryEmpty: writingLoaded ? writing.custom_words.length + writing.replacements.length + writing.snippets.length === 0 : null });

  const goToStatus = () => { if (status.pane) onNavigateSettingsPane?.(status.pane); else if (status.screen) { if (status.source) onSourceFilterChange?.(status.source); onNavigate?.(status.screen); } };
  const act = (row: NeedRow) => {
    const action = row.action;
    if (action.kind === "install") void update.promptInstall();
    else if (action.kind === "restart") void update.restartNow();
    else if (action.kind === "pane") onNavigateSettingsPane?.(action.pane);
    else if (action.kind === "screen") { if (action.source) onSourceFilterChange?.(action.source); onNavigate?.(action.screen); }
    else if (action.kind === "discard") void invoke("discard_kept_dictation", { id: action.id }).catch(() => { toast.error("Couldn't discard that recording."); }).finally(() => void refreshKept());
    else void invoke(action.anyway ? "transcribe_anyway" : "retry_kept_dictation", action.anyway ? { id: action.id } : { id: action.id, engine: action.engine }).catch(() => { toast.error("Couldn't retry that recording. It may have expired, or the engine may no longer be ready."); }).finally(() => void refreshKept());
  };
  const openTip = (tip: TipRow) => onNavigate?.(tip.screen);

  const chips = [
    { label: "Engine", value: engineLabel, screen: "transcription" as const, icon: AudioLines },
    { label: "Language", value: language, screen: "transcription" as const, icon: Languages },
    { label: "Polish", value: polish.label, screen: "polish" as const, icon: Sparkles },
    { label: "Live preview", value: settings?.transcription_mode === "live_preview" ? "On" : "Off", screen: "transcription" as const, icon: TextCursorInput },
  ];

  return <SettingsPage wide container className="min-h-full gap-[22px]">
    <section data-pencil-name="Hero" className="flex flex-col gap-[14px] rounded-[16px] border border-border bg-card px-7 py-[26px]">
      <div className="flex flex-wrap items-center justify-between gap-3">
        {status.ready ? <span role="status" className="inline-flex items-center gap-1.5 rounded-full bg-sage-bg px-[9px] py-[3px] text-[11.5px] leading-[normal] font-medium text-sage" style={{ color: "var(--ready-text)" }}><span aria-hidden className="size-1.5 rounded-full bg-sage" />{status.label}</span> : status.pending ? <button type="button" onClick={goToStatus} className="inline-flex items-center gap-1.5 rounded-full bg-warn-bg px-2.5 py-1 text-xs font-medium text-warn focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"><span aria-hidden className="size-1.5 rounded-full bg-warn" />{status.label}<ChevronRight className="size-3 text-warn" /></button> : <span role="status" className="inline-flex items-center gap-1.5 rounded-full bg-warn-bg px-2.5 py-1 text-xs font-medium text-warn"><span aria-hidden className="size-1.5 rounded-full bg-warn" />{status.label}</span>}
      </div>
      <h1 className="flex flex-wrap items-center gap-x-[10px] gap-y-1 text-[30px] font-semibold leading-[normal] tracking-[-0.6px] text-foreground">Press <KeyCaps caps={caps.length ? [caps.join(isMacOS ? " " : " + ")] : []} size="lg" /> and start talking</h1>
      <p className="text-[14px] leading-[normal] text-muted-foreground">{trigger.mode === "push_to_talk" ? "Hold to talk, release to paste into any app. Press Esc twice to cancel." : "Press once to start, again to paste into any app. Press Esc twice to cancel."}</p>
      <div className="pt-2 flex flex-wrap gap-2">{chips.map(({ label, value, screen, icon: Icon }) => <button key={label} type="button" onClick={() => onNavigate?.(screen)} className="inline-flex max-w-full items-center gap-[7px] rounded-[9px] bg-muted px-[11px] py-[7px] text-xs leading-[normal] text-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"><Icon aria-hidden className="size-3.5 shrink-0 text-muted-foreground" /><span className="text-muted-foreground">{label}</span><strong className="truncate font-medium">{value}</strong></button>)}</div>
    </section>
    <NeedsYou rows={needs} onAct={act} />
    <MoreFromVoicetypr tips={tips} onOpen={openTip} />
  </SettingsPage>;
}
