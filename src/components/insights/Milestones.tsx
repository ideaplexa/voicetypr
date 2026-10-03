import { AudioLines, CalendarCheck, Flame, Sparkles, Type } from "lucide-react";
import type { UsageStats } from "@/types/usage";
import { milestoneProgress } from "@/components/insights/stats";
import { cn } from "@/lib/utils";

export function Milestones({ stats }: { stats: UsageStats }) {
  const { next, progress, earned, badges } = milestoneProgress(stats);
  const icons = [Type, Flame, AudioLines, Sparkles, CalendarCheck];
  return (
    <section
      data-pencil-name="Milestones"
      className="min-w-0 rounded-[14px] ring-1 ring-inset ring-border bg-card px-5 py-4"
    >
      <div data-pencil-name="Head" className="mb-3 flex items-center justify-between">
        <h2 className="text-sm font-semibold">Milestones</h2>
        <span className="text-xs text-text-3">{earned} of 10</span>
      </div>
      <div
        data-pencil-name="Next"
        className="mb-3 rounded-[10px] bg-sage-bg px-3 py-[10px] text-sage"
      >
        <div className="mb-1.5 flex justify-between gap-2">
          <span className="text-[12.5px] font-semibold">
            {next ? `Next: ${next.toLocaleString()} words` : "1,000,000 words reached"}
          </span>
          {next && (
            <span className="font-mono text-[11px]">
              {(next - stats.total_words).toLocaleString()} to go
            </span>
          )}
        </div>
        <div
          role="progressbar"
          aria-label="Next word milestone"
          aria-valuenow={Math.round(progress * 100)}
          aria-valuemin={0}
          aria-valuemax={100}
          data-pencil-name="Track"
          className="h-[5px] overflow-hidden rounded bg-card"
        >
          <div
            data-pencil-name="Bar"
            className="h-full rounded bg-sage"
            style={{ width: `${progress * 100}%` }}
          />
        </div>
      </div>
      <div data-pencil-name="Badges" className="flex justify-between gap-1">
        {badges.map((badge, i) => {
          const unlocked = badge.value >= badge.target;
          const Icon = icons[i];
          return (
            <div
              data-pencil-name={badge.label}
              key={badge.label}
              className="flex w-[62px] min-w-0 flex-col items-center gap-1.5 text-center"
            >
              <span
                data-pencil-name="Disc"
                className={cn(
                  "flex size-[38px] items-center justify-center rounded-full border",
                  unlocked
                    ? "border-sage/40 bg-sage-bg text-sage"
                    : "border-border bg-muted text-text-3",
                )}
              >
                <Icon className="size-[18px]" />
              </span>
              <span
                data-pencil-name="Label"
                className={cn(
                  "text-[10.5px] leading-[13px]",
                  unlocked ? "text-muted-foreground" : "text-text-3",
                )}
              >
                {badge.label}
              </span>
              {!unlocked && (
                <span data-pencil-name="Progress" className="font-mono text-[10.5px] text-text-3">
                  {badge.label === "30-day streak" ? "Best " : ""}{Math.min(badge.value, badge.target).toLocaleString()}/
                  {badge.target.toLocaleString()}
                </span>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}
