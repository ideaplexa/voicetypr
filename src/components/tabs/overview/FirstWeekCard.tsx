import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import { Button } from "@/components/settings/SettingsButton";
import { DiscoveryArt } from "@/components/discovery/DiscoveryArt";
import { Badge } from "@/components/ui/badge";
import { useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { useRealtimeWords } from "@/components/sections/models/useRealtimeWords";
import { useRecordingHotkey } from "@/components/sections/recording/useRecordingHotkey";
import { isShareableEngine } from "@/components/sections/network-sharing/sharingUtils";
import type { SharingStatus } from "@/components/sections/network-sharing/types";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import { advanceFirstWeek, firstWeekCard, FIRST_WEEK_LENGTH, localDate, retireFirstWeekCard, type FirstWeekCardId } from "@/lib/first-week";
import { createLogger } from "@/lib/logger";
import { loadEffectivePrimaryShortcut } from "@/lib/primary-shortcut";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { isMacOS } from "@/lib/platform";
import { useWritingSettings } from "@/state/writingSettings";
import type { AppSettings, FirstWeekProgress, SpeechModelEngine } from "@/types";

const log = createLogger("first-week");
const computer = isMacOS ? "Mac" : "PC";

interface CardCopy {
  title: string;
  line: string;
  needs?: string;
  action: string;
}

function firstWeekCopy(id: FirstWeekCardId, accessibilityMissing: boolean, polishReady: boolean | null): CardCopy {
  switch (id) {
    case "hold":
      return { title: "Hold to talk", line: "Hold your shortcut while you talk, then let go to paste.", action: "Turn on" };
    case "dictionary":
      return { title: "Teach it your words", line: "Add names and terms so Voicetypr gets them right.", action: "Open Dictionary" };
    case "polish":
      return { title: "Polish", line: "Clean up fillers, false starts and punctuation, with a style per app.", needs: polishReady === false ? "Needs a provider" : undefined, action: "Set up Polish" };
    case "voice_commands":
      return {
        title: "Say “new line”",
        line: "Say only the command, like “new line” or “undo that”, then stop.",
        needs: accessibilityMissing ? "Needs Accessibility" : undefined,
        action: "Turn on",
      };
    case "realtime":
      return { title: "Watch the words appear", line: "Turn on Real-time words to see text while you talk.", action: "Turn on" };
    case "file":
      return { title: "Transcribe a file", line: "Drop in a recording or a video and get the text.", action: "Choose a file" };
    case "share":
      return { title: `Share this ${computer}`, line: "Let your other computers on a trusted network use this one as their engine.", needs: "Needs a second computer", action: "Set up sharing" };
  }
}

export function FirstWeekCard({ sourceReady, polishOn, polishReady, dictionaryEmpty, holdToTalk, shortcut, onNavigate, onNavigateSettingsPane }: {
  sourceReady: boolean;
  polishOn: boolean | null;
  polishReady: boolean | null;
  dictionaryEmpty: boolean | null;
  holdToTalk: boolean | null;
  shortcut: string[];
  onNavigate?: (screen: ScreenId) => void;
  onNavigateSettingsPane?: (pane: SettingsPane) => void;
}) {
  const { settings, refreshSettings } = useSettings();
  const readiness = useReadiness();
  const writing = useWritingSettings((state) => state.settings);
  const writingLoaded = useWritingSettings((state) => state.loaded);
  const updateWriting = useWritingSettings((state) => state.update);
  const flushWriting = useWritingSettings((state) => state.flush);
  const engine: SpeechModelEngine = settings?.current_model_engine ?? "whisper";
  const realtime = useRealtimeWords(engine, settings?.current_model ?? "");
  const { changeRecordingMode } = useRecordingHotkey();
  const [sharingOn, setSharingOn] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [today, setToday] = useState(() => ({ date: localDate(new Date()) }));
  const saving = useRef(false);
  const progress = settings?.first_week;

  const saveProgress = useCallback(async (next: FirstWeekProgress) => {
    await invoke("set_first_week_progress", { progress: next });
    await refreshSettings();
  }, [refreshSettings]);

  // The main window hides instead of unmounting, so check the date again whenever it comes back.
  useEffect(() => {
    const recheck = () => { if (document.visibilityState === "visible") setToday({ date: localDate(new Date()) }); };
    window.addEventListener("focus", recheck);
    document.addEventListener("visibilitychange", recheck);
    return () => {
      window.removeEventListener("focus", recheck);
      document.removeEventListener("visibilitychange", recheck);
    };
  }, []);

  useEffect(() => {
    if (!progress || saving.current) return;
    const next = advanceFirstWeek(progress, today.date);
    if (!next) return;
    saving.current = true;
    void saveProgress(next)
      .catch(() => log.warn("Couldn't save first-week progress"))
      .finally(() => { saving.current = false; });
  }, [progress, today, saveProgress]);

  const refreshSharing = useCallback(() => invoke<SharingStatus>("get_sharing_status")
    .then((status) => setSharingOn(status.enabled))
    .catch(() => setSharingOn(null)), []);
  useEffect(() => { void refreshSharing(); }, [refreshSharing]);
  useTauriEvent("sharing-status-changed", () => refreshSharing());

  const id = firstWeekCard(progress, {
    licenseValid: readiness.licenseStatus === null ? null : readiness.licenseValid,
    holdToTalk,
    dictionaryEmpty,
    polishOn,
    voiceCommandsOn: writingLoaded ? writing.editing_commands : null,
    sourceReady,
    streams: realtime.streams,
    realtimeOn: realtime.realtimeOn,
    canShareThisComputer: readiness.remoteSelected ? false : isShareableEngine(engine) && readiness.selectedModelAvailable === true,
    sharingOn,
  });
  if (!id || !progress) return null;
  const copy = firstWeekCopy(id, isMacOS && readiness.hasAccessibilityPermission === false, polishReady);

  const retire = () => saveProgress(retireFirstWeekCard(progress, id)).catch(() => log.warn("Couldn't save first-week progress"));
  // Turning a feature on only retires the card once the change actually stuck; the helpers report their own errors.
  const turnOn = async (): Promise<boolean> => {
    switch (id) {
      case "hold":
        await changeRecordingMode("push_to_talk");
        return (await loadEffectivePrimaryShortcut()).mode === "hold";
      case "voice_commands":
        updateWriting({ editing_commands: true });
        await flushWriting();
        return useWritingSettings.getState().settings.editing_commands;
      case "realtime":
        await realtime.setRealtime(true);
        return (await invoke<AppSettings>("get_settings")).transcription_mode === "live_preview";
      default:
        return true;
    }
  };
  const act = async () => {
    setBusy(true);
    try {
      const done = await turnOn().catch(() => false);
      if (id === "share") onNavigateSettingsPane?.("network");
      else onNavigate?.(({ hold: "recording", dictionary: "dictionary", polish: "polish", voice_commands: "polish", realtime: "transcription", file: "audio" } as const)[id]);
      if (done) await retire();
    } finally {
      setBusy(false);
    }
  };

  return (
    <section aria-labelledby="first-week-title" data-first-week-card={id} className="relative grid items-center gap-5 rounded-[16px] border border-border bg-card p-5 pr-12 sm:grid-cols-[150px_1fr]">
      <DiscoveryArt kind={id} shortcut={shortcut} />
      <div className="flex min-w-0 flex-col items-start gap-1.5">
        <p className="font-mono text-[11.5px] leading-[normal] text-sage">Day {Math.min(progress.day, FIRST_WEEK_LENGTH)} of {FIRST_WEEK_LENGTH} · Your first week</p>
        <h2 id="first-week-title" className="text-[17px] leading-[normal] font-semibold text-foreground">{copy.title}</h2>
        <p className="text-[13.5px] leading-[20px] text-muted-foreground">{copy.line}</p>
        {copy.needs ? <Badge variant="secondary" className="bg-muted font-medium text-muted-foreground">{copy.needs}</Badge> : null}
        <Button size="sm" className="mt-1.5" disabled={busy} onClick={() => void act()}>{copy.action}</Button>
      </div>
      <Button variant="ghost" size="icon-sm" className="absolute top-3 right-3 text-muted-foreground" aria-label={`Hide “${copy.title}”`} onClick={() => void retire()}>
        <X className="size-4" />
      </Button>
    </section>
  );
}
