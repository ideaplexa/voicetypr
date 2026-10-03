import type { UsageDay, UsageStats } from "@/types/usage";
import { localDateKey, shiftDay } from "@/components/insights/stats";

/** Believable seven-month, content-free history. The final 12 days are active. */
export function createUsageFixture(
  since: string | null = null,
  empty = false,
  now = new Date(),
): UsageStats {
  if (empty)
    return {
      first_use: null,
      total_words: 0,
      total_dictations: 0,
      total_audio_ms: 0,
      polished_dictations: 0,
      days: [],
      apps: [],
    };
  const start = new Date(now.getFullYear(), now.getMonth() - 7, now.getDate());
  const days: UsageDay[] = [];
  for (
    let date = start, i = 0;
    localDateKey(date) <= localDateKey(now);
    date = shiftDay(date, 1), i++
  ) {
    const finalDays = localDateKey(date) >= localDateKey(shiftDay(now, -11));
    // A historical 21-day run, then gaps, and a current 12-day streak.
    if (!finalDays && !(i >= 35 && i <= 55) && (i % 7 === 0 || i % 7 === 6 || i % 5 === 0))
      continue;
    const dictations = 5 + (i % 10);
    const words = dictations * (25 + (i % 31));
    days.push({
      date: localDateKey(date),
      words,
      dictations,
      audio_ms: Math.round((words / (130 + (i % 25))) * 60_000),
    });
  }
  const totalWords = days.reduce((sum, day) => sum + day.words, 0);
  const appWords = days
    .filter((day) => !since || day.date >= since)
    .reduce((sum, day) => sum + day.words, 0);
  const shares = since
    ? [0.42, 0.2, 0.14, 0.1, 0.07, 0.03, 0.02, 0.01, 0.01]
    : [0.34, 0.22, 0.15, 0.11, 0.08, 0.04, 0.03, 0.02, 0.01];
  const names = [
    "Slack",
    "Cursor",
    "Outlook",
    "Notes",
    "Terminal",
    "Pages",
    "Chrome",
    "Linear",
    "Other",
  ];
  let allocated = 0;
  return {
    first_use: days[0]?.date ?? null,
    total_words: totalWords,
    total_dictations: days.reduce((sum, day) => sum + day.dictations, 0),
    total_audio_ms: days.reduce((sum, day) => sum + day.audio_ms, 0),
    polished_dictations: 312,
    days,
    apps: names.map((name, i) => {
      const words =
        i === names.length - 1 ? appWords - allocated : Math.round(appWords * shares[i]);
      allocated += words;
      return { name, words };
    }),
  };
}
