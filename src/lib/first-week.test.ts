import { describe, expect, it } from "vitest";
import { advanceFirstWeek, firstWeekActive, firstWeekCard, localDate, retireFirstWeekCard, type FirstWeekState } from "@/lib/first-week";
import type { FirstWeekProgress } from "@/types";

const ready: FirstWeekState = {
  licenseValid: true,
  holdToTalk: false,
  dictionaryEmpty: true,
  polishOn: false,
  voiceCommandsOn: false,
  sourceReady: true,
  streams: true,
  realtimeOn: false,
  canShareThisComputer: true,
  sharingOn: false,
};
const onDay = (day: number, dismissed: string[] = []): FirstWeekProgress => ({ day, last_seen: "2026-11-17", dismissed });

describe("first-week cards on Home", () => {
  it("shows one card a day in the designed order", () => {
    expect([1, 2, 3, 4, 5, 6, 7].map((day) => firstWeekCard(onDay(day), ready))).toEqual([
      "hold", "dictionary", "polish", "voice_commands", "realtime", "file", "share",
    ]);
  });

  it("shows nothing when the licence or trial doesn't allow dictation, or isn't known yet", () => {
    expect(firstWeekCard(onDay(1), { ...ready, licenseValid: false })).toBeNull();
    expect(firstWeekCard(onDay(3), { ...ready, licenseValid: null })).toBeNull();
  });

  it("shows nothing before the first day and after the week ends", () => {
    expect(firstWeekCard(onDay(0), ready)).toBeNull();
    expect(firstWeekCard(onDay(8), ready)).toBeNull();
    expect(firstWeekCard(undefined, ready)).toBeNull();
  });

  it("offers Hold to talk only to people still on press-to-toggle", () => {
    expect(firstWeekCard(onDay(1), { ...ready, holdToTalk: true })).toBeNull();
    expect(firstWeekCard(onDay(1), { ...ready, holdToTalk: null })).toBeNull();
  });

  it("hides Real-time words unless the current source streams, or when it's already on", () => {
    expect(firstWeekCard(onDay(5), { ...ready, streams: false })).toBeNull();
    expect(firstWeekCard(onDay(5), { ...ready, streams: null })).toBeNull();
    expect(firstWeekCard(onDay(5), { ...ready, realtimeOn: true })).toBeNull();
  });

  it("shows nothing more that day once today's card is put away, and never brings it back", () => {
    const progress = retireFirstWeekCard(onDay(2), "dictionary");
    expect(firstWeekCard(progress, ready)).toBeNull();
    expect(firstWeekCard({ ...progress, day: 3 }, ready)).toBe("polish");
    expect(firstWeekCard({ ...progress, day: 2 }, ready)).toBeNull();
  });

  it("never shows an earlier day's card in place of today's", () => {
    expect(firstWeekCard(onDay(2), { ...ready, dictionaryEmpty: false })).toBeNull();
  });

  it("skips features already in use", () => {
    expect(firstWeekCard(onDay(3), { ...ready, polishOn: true })).toBeNull();
    expect(firstWeekCard(onDay(4), { ...ready, voiceCommandsOn: true })).toBeNull();
  });

  it("offers Transcribe a file only with a ready source", () => {
    expect(firstWeekCard(onDay(6), { ...ready, sourceReady: false })).toBeNull();
  });

  it("offers sharing only with a selected on-device model and sharing off", () => {
    expect(firstWeekCard(onDay(7), { ...ready, canShareThisComputer: false })).toBeNull();
    expect(firstWeekCard(onDay(7), { ...ready, sharingOn: true })).toBeNull();
    expect(firstWeekCard(onDay(7), { ...ready, sharingOn: null })).toBeNull();
  });
});

describe("first-week day counter", () => {
  it("starts on day 1 the first time Home opens and advances once per local date", () => {
    const first = advanceFirstWeek(undefined, "2026-11-17");
    expect(first).toEqual({ day: 1, last_seen: "2026-11-17", dismissed: [] });
    expect(advanceFirstWeek(first ?? undefined, "2026-11-17")).toBeNull();
    expect(advanceFirstWeek(first ?? undefined, "2026-11-20")).toEqual({ day: 2, last_seen: "2026-11-20", dismissed: [] });
  });

  it("stops counting once the week is over", () => {
    const last = advanceFirstWeek(onDay(7), "2026-11-18");
    expect(last?.day).toBe(8);
    expect(firstWeekActive(last ?? undefined)).toBe(false);
    expect(advanceFirstWeek(last ?? undefined, "2026-11-19")).toBeNull();
    expect(firstWeekActive(onDay(0))).toBe(true);
    expect(firstWeekActive(undefined)).toBe(false);
  });

  it("doesn't count a day when the clock or time zone moves back", () => {
    const today = advanceFirstWeek(onDay(2), "2026-11-18");
    expect(advanceFirstWeek(today ?? undefined, "2026-11-17")).toBeNull();
    expect(advanceFirstWeek(today ?? undefined, "2026-11-17")).toBeNull();
    expect(advanceFirstWeek(today ?? undefined, "2026-11-19")?.day).toBe(4);
  });

  it("uses the local calendar date", () => {
    expect(localDate(new Date(2026, 0, 5, 23, 59))).toBe("2026-01-05");
  });
});
