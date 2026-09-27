import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { WeeklyRhythmCard } from "./WeeklyRhythmCard";
import type { OverviewStats } from "./useOverviewStats";

const statsWithUnplottedWeeklyTranscript: OverviewStats = {
  todayCount: 0,
  weekCount: 1,
  totalWords: 0,
  avgLength: 0,
  timeSavedHours: 0,
  timeSavedRemMinutes: 0,
  timeSavedMinutes: 0,
  totalTranscriptions: 1,
  currentStreak: 0,
  longestStreak: 0,
  weekDays: [
    { key: 1, label: "Mon", count: 0 },
    { key: 2, label: "Tue", count: 0 },
    { key: 3, label: "Wed", count: 0 },
    { key: 4, label: "Thu", count: 0 },
    { key: 5, label: "Fri", count: 0 },
    { key: 6, label: "Sat", count: 0 },
    { key: 7, label: "Sun", count: 0 },
  ],
  weekMax: 0,
};

describe("WeeklyRhythmCard", () => {
  it("keeps chart bars finite when the weekly count has no plotted bucket", () => {
    render(
      <WeeklyRhythmCard
        stats={statsWithUnplottedWeeklyTranscript}
        isLoading={false}
        loadError={null}
        historyLength={1}
        onRetry={() => undefined}
      />,
    );

    expect(screen.queryByText(/Busiest/)).not.toBeInTheDocument();
    expect(screen.getAllByTitle(/0 on/)).toHaveLength(7);
    expect(screen.getAllByTitle(/0 on/).every((bar) => bar.style.height === "6%")).toBe(true);
  });
});
