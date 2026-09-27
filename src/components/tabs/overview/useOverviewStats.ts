import { useMemo } from "react";
import type { TranscriptionHistory } from "@/types";

export interface OverviewWeekDay {
  key: number;
  label: string;
  count: number;
}

export interface OverviewStats {
  todayCount: number;
  weekCount: number;
  totalWords: number;
  avgLength: number;
  timeSavedHours: number;
  timeSavedRemMinutes: number;
  timeSavedMinutes: number;
  totalTranscriptions: number;
  currentStreak: number;
  longestStreak: number;
  weekDays: OverviewWeekDay[];
  weekMax: number;
}

export function formatTimeSaved(stats: OverviewStats): string {
  return stats.timeSavedHours > 0
    ? `${stats.timeSavedHours}h ${stats.timeSavedRemMinutes}m`
    : `${stats.timeSavedMinutes}m`;
}

export function computeOverviewStats(
  history: TranscriptionHistory[],
  totalCount: number,
): OverviewStats {
  const now = new Date();
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);

  const startOfWeek = new Date(now);
  startOfWeek.setDate(startOfWeek.getDate() - 7);
  const todayCount = history.filter((item) => new Date(item.timestamp) >= startOfToday).length;
  const weekCount = history.filter((item) => new Date(item.timestamp) >= startOfWeek).length;

  const totalWords = history.reduce(
    (acc, item) => acc + item.text.split(/\s+/).filter(Boolean).length,
    0,
  );
  const avgLength = history.length > 0 ? Math.round(totalWords / history.length) : 0;

  const avgTypingSpeed = 40;
  const timeSavedMinutes = Math.round(totalWords / avgTypingSpeed);
  const timeSavedHours = Math.floor(timeSavedMinutes / 60);

  // Per-day counts for the last 7 days (weekly rhythm sparkline).
  const weekDays = Array.from({ length: 7 }, (_, index) => {
    const dayStart = new Date(startOfToday);
    dayStart.setDate(dayStart.getDate() - (6 - index));
    const dayEnd = new Date(dayStart);
    dayEnd.setDate(dayEnd.getDate() + 1);
    const count = history.filter((item) => {
      const t = new Date(item.timestamp);
      return t >= dayStart && t < dayEnd;
    }).length;
    return {
      key: dayStart.getTime(),
      label: dayStart.toLocaleDateString(undefined, { weekday: "short" }).slice(0, 3),
      count,
    };
  });
  const weekMax = Math.max(0, ...weekDays.map((day) => day.count));

  let currentStreak = 0;
  let longestStreak = 0;

  if (history.length > 0) {
    const activeDays = new Set<number>();
    history.forEach((item) => {
      const date = new Date(item.timestamp);
      date.setHours(0, 0, 0, 0);
      activeDays.add(date.getTime());
    });

    const sortedDays = Array.from(activeDays).sort((a, b) => b - a);

    if (sortedDays.length > 0) {
      const today = new Date();
      today.setHours(0, 0, 0, 0);
      const yesterday = new Date(today);
      yesterday.setDate(yesterday.getDate() - 1);

      const mostRecentDay = sortedDays[0];
      if (mostRecentDay === today.getTime() || mostRecentDay === yesterday.getTime()) {
        currentStreak = 1;
        for (let index = 1; index < sortedDays.length; index += 1) {
          const expectedDate = new Date(sortedDays[index - 1]);
          expectedDate.setDate(expectedDate.getDate() - 1);
          if (sortedDays[index] === expectedDate.getTime()) {
            currentStreak += 1;
          } else {
            break;
          }
        }
      }

      let tempStreak = 1;
      longestStreak = 1;
      for (let index = 1; index < sortedDays.length; index += 1) {
        const expectedDate = new Date(sortedDays[index - 1]);
        expectedDate.setDate(expectedDate.getDate() - 1);
        if (sortedDays[index] === expectedDate.getTime()) {
          tempStreak += 1;
          longestStreak = Math.max(longestStreak, tempStreak);
        } else {
          tempStreak = 1;
        }
      }
    }
  }

  return {
    todayCount,
    weekCount,
    totalWords,
    avgLength,
    timeSavedHours,
    timeSavedRemMinutes: timeSavedMinutes % 60,
    timeSavedMinutes,
    totalTranscriptions: totalCount,
    currentStreak,
    longestStreak,
    weekDays,
    weekMax,
  };
}

export function useOverviewStats(
  history: TranscriptionHistory[],
  totalCount: number,
): OverviewStats {
  return useMemo(() => computeOverviewStats(history, totalCount), [history, totalCount]);
}
