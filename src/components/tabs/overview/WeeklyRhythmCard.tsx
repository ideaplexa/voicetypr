import { cn } from "@/lib/utils";
import { formatTimeSaved, type OverviewStats } from "./useOverviewStats";

function AllTimeValue({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
      <dd className="mt-1 truncate text-lg font-semibold tabular-nums text-foreground">{value}</dd>
    </div>
  );
}

export function WeeklyRhythmCard({
  stats,
  isLoading,
  loadError,
  historyLength,
  onRetry,
}: {
  stats: OverviewStats;
  isLoading: boolean;
  loadError: string | null;
  historyLength: number;
  onRetry: () => void;
}) {
  return (
    <section aria-labelledby="weekly-rhythm-title" className="rounded-xl bg-card p-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 id="weekly-rhythm-title" className="text-[15px] font-semibold text-foreground">
            Last 7 days
          </h2>
          <p className="mt-1 text-[13px] text-muted-foreground">Completed transcripts by day.</p>
        </div>
        {stats.weekMax > 0 ? (
          <span className="rounded-full bg-sage-bg px-2.5 py-1 text-xs font-medium text-sage">
            Busiest {stats.weekDays.find((day) => day.count === stats.weekMax)?.label} ·{" "}
            {stats.weekMax}
          </span>
        ) : null}
      </div>
      {isLoading && historyLength === 0 ? (
        <div aria-hidden className="mt-5 flex h-40 items-end gap-2">
          {[40, 70, 55].map((height) => (
            <div
              key={height}
              className="min-w-0 flex-1 bg-muted animate-pulse h-full rounded-md"
              style={{ height: `${height}%` }}
            />
          ))}
        </div>
      ) : loadError && historyLength === 0 ? (
        <p className="mt-5 text-[13px] text-muted-foreground">
          Couldn&apos;t load history{" "}
          <button type="button" onClick={onRetry} className="text-[11px] text-sage hover:underline">
            Retry
          </button>
        </p>
      ) : stats.weekCount === 0 ? (
        <p className="mt-5 text-[13px] text-muted-foreground">
          No transcripts this week — your daily activity will chart here.
        </p>
      ) : (
        <div className="mt-5 flex h-40 items-end gap-2">
          {stats.weekDays.map((day) => {
            const isPeak = day.count > 0 && day.count === stats.weekMax;
            const chartMax = Math.max(1, stats.weekMax);
            const heightPct = Math.max(6, Math.round((day.count / chartMax) * 100));
            return (
              <div key={day.key} className="flex h-full min-w-0 flex-1 flex-col justify-end gap-2">
                <div
                  className={cn(
                    "w-full rounded-md transition-[height]",
                    isPeak ? "bg-sage" : "bg-sage/20",
                  )}
                  style={{ height: `${heightPct}%` }}
                  title={`${day.count} on ${day.label}`}
                />
                <span className="text-center text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                  {day.label}
                </span>
              </div>
            );
          })}
        </div>
      )}
      <p className="mt-4 text-[13px] tabular-nums text-muted-foreground">
        {stats.todayCount.toLocaleString()} today · {stats.weekCount.toLocaleString()} last 7 days
        {stats.avgLength > 0 ? ` · ${stats.avgLength}/transcript` : ""}
      </p>
      <dl className="mt-5 grid grid-cols-2 gap-4 border-t border-border/70 pt-4 sm:grid-cols-4">
        <AllTimeValue label="Words spoken" value={stats.totalWords.toLocaleString()} />
        <AllTimeValue label="Transcripts" value={stats.totalTranscriptions.toLocaleString()} />
        <AllTimeValue label="Time saved" value={formatTimeSaved(stats)} />
        <AllTimeValue label="Day streak" value={stats.currentStreak.toLocaleString()} />
      </dl>
    </section>
  );
}
