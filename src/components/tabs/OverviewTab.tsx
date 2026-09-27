import { Button } from "@/components/ui/button";
import { ShareStatsModal } from "@/components/ShareStatsModal";
import { languages } from "@/components/languages";
import { useCanAutoInsert, useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { useTranscriptionHistory } from "@/hooks/useTranscriptionHistory";
import { useActiveTrigger } from "@/hooks/useActiveTrigger";
import { getModelDisplayName } from "@/lib/model-display";
import { Share2 } from "lucide-react";
import { useState } from "react";
import type { ScreenId } from "@/components/navigation";
import { CurrentSetupCard } from "./overview/CurrentSetupCard";
import { WeeklyRhythmCard } from "./overview/WeeklyRhythmCard";
import { useActiveRemoteLabel } from "./overview/useActiveRemoteLabel";
import { formatTimeSaved, useOverviewStats } from "./overview/useOverviewStats";

export function OverviewTab({ onNavigate }: { onNavigate?: (section: ScreenId) => void }) {
  const readiness = useReadiness();
  const canRecord = readiness.canRecord;
  const canAutoInsert = useCanAutoInsert();
  const { settings } = useSettings();
  const { label: triggerLabel } = useActiveTrigger(settings?.hotkey);
  const [shareModalOpen, setShareModalOpen] = useState(false);
  const activeRemoteLabel = useActiveRemoteLabel(readiness.remoteSelected);
  const selectedSourceLabel = readiness.remoteSelected
    ? (activeRemoteLabel ?? "Remote Voicetypr")
    : (getModelDisplayName(settings?.current_model) ?? "No source selected");

  const { history, totalCount, isLoading, loadError, refreshHistory } = useTranscriptionHistory({
    limit: 500,
    includeTotalCount: true,
  });

  const stats = useOverviewStats(history, totalCount);

  const spokenLanguage =
    languages.find((language) => language.value === (settings?.speech_language ?? "en"))?.label ??
    settings?.speech_language ??
    "English";

  return (
    <div className="h-full min-h-0 overflow-auto">
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-5 pb-4 pl-2 pr-4">
        <header className="flex items-start justify-between gap-4">
          <div>
            <h1 className="text-[1.75rem] font-semibold tracking-[-0.025em] text-foreground">
              Overview
            </h1>
            <p className="mt-1 text-sm text-muted-foreground">
              Your active dictation setup and usage at a glance.
            </p>
          </div>
          <Button
            type="button"
            variant="outline"
            className="shrink-0"
            onClick={() => setShareModalOpen(true)}
          >
            <Share2 />
            Share stats
          </Button>
        </header>

        <CurrentSetupCard
          canRecord={canRecord}
          onNavigate={onNavigate}
          selectedSourceLabel={selectedSourceLabel}
          triggerLabel={triggerLabel}
          spokenLanguage={spokenLanguage}
          canAutoInsert={canAutoInsert}
        />

        <WeeklyRhythmCard
          stats={stats}
          isLoading={isLoading}
          loadError={loadError}
          historyLength={history.length}
          onRetry={() => {
            void refreshHistory();
          }}
        />

        <ShareStatsModal
          open={shareModalOpen}
          onOpenChange={setShareModalOpen}
          stats={{
            totalTranscriptions: stats.totalTranscriptions,
            totalWords: stats.totalWords,
            timeSavedDisplay: formatTimeSaved(stats),
          }}
        />
      </div>
    </div>
  );
}
