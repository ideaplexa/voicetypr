import { useState } from "react";
import { Share2 } from "lucide-react";
import { PageHeader, Segmented, SettingsPage } from "@/components/settings/settings-ui";
import { Button } from "@/components/settings/SettingsButton";
import { ShareStatsModal } from "@/components/ShareStatsModal";
import { KeyCaps } from "@/components/KeyCaps";
import { ActivityCalendar } from "@/components/insights/ActivityCalendar";
import { InsightsHero } from "@/components/insights/InsightsHero";
import { Milestones } from "@/components/insights/Milestones";
import { WhereYouTalk } from "@/components/insights/WhereYouTalk";
import {
  computeUsageOverview,
  parseLocalDay,
  periodLabels,
  type UsagePeriod,
} from "@/components/insights/stats";
import { useUsageStats } from "@/components/insights/useUsageStats";
import { toShareCardStats } from "@/components/shareCardRenderer";
import { useSettings } from "@/contexts/SettingsContext";
import { useActiveTrigger } from "@/hooks/useActiveTrigger";
import { isMacOS } from "@/lib/platform";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import "@/components/insights/insights.css";

export function InsightsTab() {
  const [period, setPeriod] = useState<UsagePeriod>("all");
  const [shareOpen, setShareOpen] = useState(false);
  const { stats, loading, error, refresh } = useUsageStats(period);
  const { settings } = useSettings();
  const trigger = useActiveTrigger(settings);
  const caps = trigger.hotkey
    ? shortcutKeyCaps(trigger.hotkey, isMacOS ? "darwin" : "windows")
    : trigger.kbdLabel !== "Not set"
      ? [trigger.kbdLabel]
      : [];
  const overview = stats ? computeUsageOverview(stats, period) : null;
  const firstUse = stats?.first_use
    ? parseLocalDay(stats.first_use).toLocaleDateString("en-US", { month: "long", day: "numeric" })
    : null;
  return (
    <SettingsPage wide container className="insights gap-4">
      <PageHeader
        title="Insights"
        description={
          firstUse ? `Since ${firstUse} · counted on this ${isMacOS ? "Mac" : "PC"}` : undefined
        }
        className="items-end"
        action={
          <>
            <Segmented
              label="Insights period"
              value={period}
              onValueChange={(value) => {
                if (value === "week" || value === "month" || value === "all") setPeriod(value);
              }}
              options={Object.entries(periodLabels).map(([value, label]) => ({ value, label }))}
            />
            <Button
              onClick={() => setShareOpen(true)}
              disabled={!stats?.total_dictations || loading || error}
            >
              <Share2 className="size-3.5" />
              Share
            </Button>
          </>
        }
      />
      {error ? (
        <p role="alert" className="py-6 text-sm text-muted-foreground">
          Couldn’t load your stats.{" "}
          <button className="text-sage underline" onClick={() => void refresh()}>
            Retry
          </button>
        </p>
      ) : !stats ? (
        <p role="status" className="py-6 text-sm text-muted-foreground">
          Loading insights…
        </p>
      ) : stats.total_dictations === 0 ? (
        <section className="flex flex-col items-center gap-4 rounded-[14px] border border-border bg-card py-20 text-center">
          <h2 className="text-lg font-semibold">Your stats appear after your first dictation</h2>
          <p className="flex flex-wrap items-center justify-center gap-2 text-sm text-muted-foreground">
            Press <KeyCaps caps={caps} /> and start talking.
          </p>
        </section>
      ) : (
        overview && (
          <>
            <InsightsHero overview={overview} />
            <ActivityCalendar days={stats.days} />
            <div
              data-pencil-name="Row"
              className="grid gap-4 @min-[620px]:grid-cols-2 [&>section]:min-h-[226px]"
            >
              <WhereYouTalk apps={stats.apps} loading={loading} />
              <Milestones stats={stats} />
            </div>
          </>
        )
      )}
      {stats && (
        <ShareStatsModal
          open={shareOpen}
          onOpenChange={setShareOpen}
          stats={toShareCardStats(stats, period)}
        />
      )}
    </SettingsPage>
  );
}
