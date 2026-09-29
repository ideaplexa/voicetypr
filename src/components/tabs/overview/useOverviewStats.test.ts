import { describe, expect, it } from "vitest";
import { computeOverviewStats, formatWeekSavedTime } from "./useOverviewStats";

describe("formatWeekSavedTime", () => {
  it.each([
    [0, "—"],
    [4, "4 min"],
    [59, "59 min"],
    [60, "1 h 0 m"],
    [134, "2 h 14 m"],
  ])("formats %i saved minutes as %s", (minutes, formatted) => {
    expect(formatWeekSavedTime(minutes)).toBe(formatted);
  });
});

describe("computeOverviewStats", () => {
  it("uses the same local-midnight seven-day window for totals and bars", () => {
    const now = new Date();
    const today = new Date(now); today.setHours(0, 0, 0, 0);
    const firstDay = new Date(today); firstDay.setDate(firstDay.getDate() - 6);
    const before = new Date(firstDay.getTime() - 1);
    const stats = computeOverviewStats([
      { id: "before", text: "old", timestamp: before, model: "parakeet" },
      { id: "first", text: "included", timestamp: firstDay, model: "parakeet" },
      { id: "now", text: "included", timestamp: now, model: "parakeet" },
    ], 3);
    expect(stats.weekCount).toBe(2);
    expect(stats.weekWords).toBe(2);
    expect(stats.weekDays.map((day) => day.count)).toEqual([1, 0, 0, 0, 0, 0, 1]);
  });
  it("keeps the weekly maximum at zero for an empty history", () => {
    const stats = computeOverviewStats([], 0);

    expect(stats.weekCount).toBe(0);
    expect(stats.weekMax).toBe(0);
    expect(stats.weekDays.every((day) => day.count === 0)).toBe(true);
  });

  it("estimates saved time after subtracting recorded speaking time", () => {
    const stats = computeOverviewStats([{
      id: "recent",
      text: Array.from({ length: 120 }, () => "word").join(" "),
      timestamp: new Date(),
      model: "parakeet",
      writing: { audio_duration_ms: 60_000 },
    }], 1);
    expect(stats.weekWords).toBe(120);
    expect(stats.weekSavedMinutes).toBe(2);
    expect(stats.timeSavedMinutes).toBe(2);
  });
});
