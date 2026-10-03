import { describe, expect, it } from "vitest";
import {
  activityLevel,
  calendarWeeks,
  computeUsageOverview,
  filterDays,
  localDateKey,
  milestoneProgress,
  quantileThresholds,
  streaks,
} from "@/components/insights/stats";
import { createUsageFixture } from "@/ui-preview/usageFixture";
import type { UsageDay, UsageStats } from "@/types/usage";
const now = new Date(2026, 9, 2, 14);
const day = (date: string, words = 100, audio_ms = 60000): UsageDay => ({
  date,
  words,
  dictations: 1,
  audio_ms,
});
const stats = (days: UsageDay[]): UsageStats => ({
  first_use: days[0]?.date ?? null,
  days,
  apps: [],
  total_words: days.reduce((s, d) => s + d.words, 0),
  total_dictations: days.length,
  total_audio_ms: days.reduce((s, d) => s + d.audio_ms, 0),
  polished_dictations: 0,
});

describe("usage maths", () => {
  it("counts streaks across a month boundary with today alive", () => {
    expect(
      streaks([day("2026-09-29"), day("2026-09-30"), day("2026-10-01"), day("2026-10-02")], now),
    ).toEqual({ current: 4, best: 4 });
  });
  it("keeps yesterday alive but expires older streaks", () => {
    expect(streaks([day("2026-09-30"), day("2026-10-01")], now)).toEqual({ current: 2, best: 2 });
    expect(streaks([day("2026-09-29"), day("2026-09-30")], now)).toEqual({ current: 0, best: 2 });
  });
  it("deduplicates days, ignores future and non-dictation days", () => {
    expect(
      streaks(
        [
          day("2026-10-01"),
          day("2026-10-01"),
          day("2026-10-03"),
          { ...day("2026-10-02"), dictations: 0 },
        ],
        now,
      ),
    ).toEqual({ current: 1, best: 1 });
  });
  it("handles local calendar days across daylight saving changes", () => {
    expect(
      streaks([day("2026-03-28"), day("2026-03-29"), day("2026-03-30")], new Date(2026, 2, 30)),
    ).toEqual({ current: 3, best: 3 });
  });
  it("hides pace when audio is zero", () => {
    expect(computeUsageOverview(stats([day("2026-10-02", 200, 0)]), "all", now).pace).toBeNull();
  });
  it("computes speaking pace and floors negative saved time at zero", () => {
    const result = computeUsageOverview(stats([day("2026-10-02", 20, 120000)]), "all", now);
    expect(result.pace).toBe(10);
    expect(result.savedMinutes).toBe(0);
    expect(
      computeUsageOverview(stats([day("2026-10-02", 400, 60000)]), "all", now).savedMinutes,
    ).toBe(9);
  });
  it("filters inclusive last 7 and 30 local days and excludes tomorrow", () => {
    const days = [
      day("2026-09-02"),
      day("2026-09-03"),
      day("2026-09-25"),
      day("2026-09-26"),
      day("2026-10-02"),
      day("2026-10-03"),
    ];
    expect(filterDays(days, "week", now).map((d) => d.date)).toEqual(["2026-09-26", "2026-10-02"]);
    expect(filterDays(days, "month", now).map((d) => d.date)).toEqual([
      "2026-09-03",
      "2026-09-25",
      "2026-09-26",
      "2026-10-02",
    ]);
  });
  it("uses authoritative all-time totals even if days differ", () => {
    const data = { ...stats([day("2026-10-02")]), total_words: 9999, total_dictations: 300 };
    expect(computeUsageOverview(data, "all", now).words).toBe(9999);
    expect(computeUsageOverview(data, "week", now).words).toBe(100);
  });
  it("assigns four positive quantile levels plus zero", () => {
    const thresholds = quantileThresholds(
      [0, 10, 20, 30, 40].map((words) => day("2026-10-01", words)),
    );
    expect(thresholds).toEqual([10, 20, 30]);
    expect([0, 10, 20, 30, 40].map((n) => activityLevel(n, thresholds))).toEqual([0, 1, 2, 3, 4]);
    expect(activityLevel(0, quantileThresholds([]))).toBe(0);
    expect(
      activityLevel(10, quantileThresholds([day("2026-10-01", 10), day("2026-10-02", 10)])),
    ).toBe(1);
  });
  it("creates Monday–Sunday columns ending this week, with outlined today and future blanks", () => {
    const weeks = calendarWeeks([day("2026-10-02")], 43, now);
    expect(weeks).toHaveLength(43);
    expect(weeks[0]).toHaveLength(7);
    expect(weeks[42][0].key).toBe("2026-09-28");
    expect(weeks[42][4].today).toBe(true);
    expect(weeks[42][5].future).toBe(true);
  });
  it("tracks fixed milestone set and caps completion at one million", () => {
    const data = {
      ...stats([day("2026-10-01"), day("2026-10-02")]),
      total_words: 48210,
      total_dictations: 1284,
      polished_dictations: 1,
    };
    const result = milestoneProgress(data, now);
    expect(result.next).toBe(50000);
    expect(result.progress).toBeCloseTo(0.9642);
    expect(result.earned).toBe(5);
    expect(result.badges[4]).toMatchObject({ value: 2, target: 30 });
    expect(milestoneProgress({ ...data, total_words: 1000001 }, now)).toMatchObject({
      next: null,
      progress: 1,
    });
    expect(milestoneProgress(stats([]), now)).toMatchObject({ next: 1000, earned: 0, progress: 0 });
  });
  it("fixtures filter only apps by since and retain all-time totals and days", () => {
    const all = createUsageFixture(null, false, now);
    const week = createUsageFixture("2026-09-26", false, now);
    expect(week.days).toEqual(all.days);
    expect(week.total_words).toBe(all.total_words);
    expect(week.apps.reduce((s, app) => s + app.words, 0)).toBe(
      filterDays(all.days, "week", now).reduce((s, d) => s + d.words, 0),
    );
    expect(streaks(all.days, now)).toEqual({ current: 12, best: 21 });
    expect(localDateKey(now)).toBe("2026-10-02");
  });
});
