import type { UsageDay, UsageStats } from "@/types/usage";

export type UsagePeriod = "week" | "month" | "all";
export const periodLabels = { week: "Week", month: "Month", all: "All time" };
export function localDateKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}
export function parseLocalDay(key: string): Date {
  const [year, month, day] = key.split("-").map(Number);
  return new Date(year, month - 1, day);
}
export function shiftDay(date: Date, count: number): Date {
  const result = new Date(date);
  result.setDate(result.getDate() + count);
  return result;
}
export function filterDays(days: UsageDay[], period: UsagePeriod, now = new Date()): UsageDay[] {
  const end = localDateKey(now);
  const start = period === "all" ? "" : localDateKey(shiftDay(now, period === "week" ? -6 : -29));
  return days.filter((day) => day.date >= start && day.date <= end);
}
export function streaks(days: UsageDay[], now = new Date()) {
  const active = new Set(
    days
      .filter((day) => day.dictations > 0 && day.date <= localDateKey(now))
      .map((day) => day.date),
  );
  const sorted = [...active].sort();
  let best = 0;
  let run = 0;
  let previous = "";
  for (const key of sorted) {
    run = previous && localDateKey(shiftDay(parseLocalDay(previous), 1)) === key ? run + 1 : 1;
    best = Math.max(best, run);
    previous = key;
  }
  let cursor = now;
  if (!active.has(localDateKey(cursor))) cursor = shiftDay(cursor, -1);
  let current = 0;
  while (active.has(localDateKey(cursor))) {
    current++;
    cursor = shiftDay(cursor, -1);
  }
  return { current, best };
}
export function computeUsageOverview(stats: UsageStats, period: UsagePeriod, now = new Date()) {
  const days = filterDays(stats.days, period, now);
  const words =
    period === "all" ? stats.total_words : days.reduce((sum, day) => sum + day.words, 0);
  const dictations =
    period === "all" ? stats.total_dictations : days.reduce((sum, day) => sum + day.dictations, 0);
  const audioMs =
    period === "all" ? stats.total_audio_ms : days.reduce((sum, day) => sum + day.audio_ms, 0);
  const savedMinutes = Math.max(0, Math.round(words / 40 - audioMs / 60_000));
  const pace = audioMs > 0 ? words / (audioMs / 60_000) : null;
  return {
    words,
    dictations,
    audioMs,
    savedMinutes,
    pace,
    average: dictations ? Math.round(words / dictations) : 0,
    ...streaks(stats.days, now),
  };
}
export function savedTime(minutes: number): string {
  return `${Math.floor(minutes / 60)} h ${minutes % 60} m`;
}
/** Empty plus four positive quantile buckets. Equal counts always share a level. */
export function quantileThresholds(days: UsageDay[]): number[] {
  const values = days
    .filter((day) => day.words > 0)
    .map((day) => day.words)
    .sort((a, b) => a - b);
  return [0.25, 0.5, 0.75].map((q) => values[Math.max(0, Math.ceil(values.length * q) - 1)] ?? 0);
}
export function activityLevel(words: number, thresholds: number[]): number {
  return words <= 0 ? 0 : 1 + thresholds.filter((threshold) => words > threshold).length;
}
export function calendarWeeks(days: UsageDay[], count: number, now = new Date()) {
  const monday = shiftDay(now, -((now.getDay() + 6) % 7));
  const start = shiftDay(monday, -(count - 1) * 7);
  const byDate = new Map(days.map((day) => [day.date, day]));
  const thresholds = quantileThresholds(
    days.filter((day) => day.date >= localDateKey(start) && day.date <= localDateKey(now)),
  );
  return Array.from({ length: count }, (_, week) =>
    Array.from({ length: 7 }, (_, row) => {
      const date = shiftDay(start, week * 7 + row);
      const key = localDateKey(date);
      const words = byDate.get(key)?.words ?? 0;
      return {
        date,
        key,
        words,
        active: (byDate.get(key)?.dictations ?? 0) > 0,
        level: activityLevel(words, thresholds),
        future: key > localDateKey(now),
        today: key === localDateKey(now),
      };
    }),
  );
}
export const wordThresholds = [1000, 10000, 50000, 100000, 250000, 500000, 1000000];
export function milestoneProgress(stats: UsageStats, now = new Date()) {
  const { best } = streaks(stats.days, now);
  const next = wordThresholds.find((threshold) => threshold > stats.total_words) ?? null;
  const reachedWord =
    [...wordThresholds].reverse().find((threshold) => threshold <= stats.total_words) ?? 1000;
  const badges = [
    {
      label: `${reachedWord >= 1000000 ? "1M" : `${reachedWord / 1000}K`} words`,
      value: stats.total_words,
      target: reachedWord,
    },
    { label: "7-day streak", value: best, target: 7 },
    { label: "100 dictations", value: stats.total_dictations, target: 100 },
    { label: "First Polish", value: stats.polished_dictations, target: 1 },
    { label: "30-day streak", value: best, target: 30 },
  ];
  const earned =
    [1000, 10000, 50000, 100000].filter((n) => stats.total_words >= n).length +
    [7, 30, 100].filter((n) => best >= n).length +
    [100, 1000].filter((n) => stats.total_dictations >= n).length +
    Number(stats.polished_dictations >= 1);
  return { next, progress: next ? Math.min(1, stats.total_words / next) : 1, earned, badges };
}
