import { describe, it, expect } from "vitest";
import { TranscriptionHistory } from "@/types";
import { applyHistoryFilters, sourceLabel, formatDurationMs } from "./recentRecordingsHelpers";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function makeItem(
  overrides: Partial<TranscriptionHistory> & Pick<TranscriptionHistory, "id" | "text">,
): TranscriptionHistory {
  return {
    timestamp: new Date("2026-06-16T10:00:00.000Z"),
    model: "base.en",
    ...overrides,
  };
}

/** "now" anchor for date-filter tests — treat 2026-06-16 as today. */
const NOW = new Date("2026-06-16T12:00:00.000Z");
const TODAY_TS = new Date("2026-06-16T10:00:00.000Z");
const YESTERDAY_TS = new Date("2026-06-15T10:00:00.000Z");
const OLD_TS = new Date("2026-06-01T10:00:00.000Z");

const desktopWithApp = makeItem({
  id: "a",
  text: "desktop dictation text",
  timestamp: TODAY_TS,
  writing: {
    source: "desktop_recording",
    engine: "parakeet",
    audio_duration_ms: 5000,
    diarized: false,
    context_hint: { app_name: "Notion" },
  },
});

const uploadEntry = makeItem({
  id: "b",
  text: "uploaded audio text",
  timestamp: TODAY_TS,
  writing: {
    source: "audio_file",
    engine: "whisper",
    audio_duration_ms: 90_000,
    diarized: true,
  },
});

const remoteEntry = makeItem({
  id: "c",
  text: "remote server text",
  timestamp: YESTERDAY_TS,
  writing: {
    source: "remote_server",
    engine: "soniox",
    audio_duration_ms: 3000,
    diarized: false,
  },
});

const oldEntry = makeItem({
  id: "d",
  text: "old entry with no writing metadata",
  timestamp: OLD_TS,
  // no writing field — legacy row
});

const history = [desktopWithApp, uploadEntry, remoteEntry, oldEntry];

// ---------------------------------------------------------------------------
// sourceLabel
// ---------------------------------------------------------------------------

describe("sourceLabel", () => {
  it("maps desktop_recording to This device", () => {
    expect(sourceLabel("desktop_recording")).toBe("This device");
  });
  it("maps audio_file to Upload", () => {
    expect(sourceLabel("audio_file")).toBe("Upload");
  });
  it("maps audio_bytes to Upload", () => {
    expect(sourceLabel("audio_bytes")).toBe("Upload");
  });
  it("maps remote_server to Remote", () => {
    expect(sourceLabel("remote_server")).toBe("Remote");
  });
  it("defaults unknown/undefined source to This device", () => {
    expect(sourceLabel(undefined)).toBe("This device");
    expect(sourceLabel("something_else")).toBe("This device");
  });
});

// ---------------------------------------------------------------------------
// formatDurationMs
// ---------------------------------------------------------------------------

describe("formatDurationMs", () => {
  it("formats sub-minute durations", () => {
    expect(formatDurationMs(5_000)).toBe("0:05");
  });
  it("formats exactly 1 minute", () => {
    expect(formatDurationMs(60_000)).toBe("1:00");
  });
  it("formats multi-minute durations", () => {
    expect(formatDurationMs(90_000)).toBe("1:30");
  });
  it("zero-pads seconds < 10", () => {
    expect(formatDurationMs(62_000)).toBe("1:02");
  });
});

// ---------------------------------------------------------------------------
// applyHistoryFilters — source filter
// ---------------------------------------------------------------------------

describe("applyHistoryFilters — source filter", () => {
  it("passes all items when filter is 'all'", () => {
    const result = applyHistoryFilters(history, "all", "all", "all", NOW);
    expect(result).toHaveLength(4);
  });

  it("shows only desktop_recording entries; legacy rows excluded", () => {
    const result = applyHistoryFilters(history, "desktop_recording", "all", "all", NOW);
    // oldEntry has no writing.source — excluded under a specific source filter
    expect(result.map((i) => i.id)).toEqual(["a"]);
  });

  it("shows only audio_file entries; legacy rows excluded", () => {
    const result = applyHistoryFilters(history, "audio_file", "all", "all", NOW);
    expect(result.map((i) => i.id)).toEqual(["b"]);
  });

  it("shows only remote_server entries; legacy rows excluded", () => {
    const result = applyHistoryFilters(history, "remote_server", "all", "all", NOW);
    expect(result.map((i) => i.id)).toEqual(["c"]);
  });

  it("legacy rows (no writing) are excluded under any specific source filter", () => {
    for (const src of ["desktop_recording", "audio_file", "remote_server"]) {
      const result = applyHistoryFilters(history, src, "all", "all", NOW);
      expect(result.some((i) => i.id === "d")).toBe(false);
    }
  });

  it("legacy rows appear when filter is 'all'", () => {
    const result = applyHistoryFilters(history, "all", "all", "all", NOW);
    expect(result.some((i) => i.id === "d")).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// applyHistoryFilters — app filter
// ---------------------------------------------------------------------------

describe("applyHistoryFilters — app filter", () => {
  it("passes all items when filter is 'all'", () => {
    const result = applyHistoryFilters(history, "all", "all", "all", NOW);
    expect(result).toHaveLength(4);
  });

  it("shows only entries with the matching app name", () => {
    const result = applyHistoryFilters(history, "all", "Notion", "all", NOW);
    expect(result.map((i) => i.id)).toEqual(["a"]);
  });

  it("hides old/no-writing entries when a specific app is selected", () => {
    const result = applyHistoryFilters(history, "all", "Notion", "all", NOW);
    expect(result.some((i) => i.id === "d")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// applyHistoryFilters — date filter
// ---------------------------------------------------------------------------

describe("applyHistoryFilters — date filter", () => {
  it("today filter shows only today's entries", () => {
    const result = applyHistoryFilters(history, "all", "all", "today", NOW);
    // desktopWithApp (today) + uploadEntry (today)
    // remoteEntry (yesterday) excluded; oldEntry (old) excluded
    expect(result.map((i) => i.id).sort()).toEqual(["a", "b"]);
  });

  it("last7 filter includes today and yesterday but not old entries", () => {
    const result = applyHistoryFilters(history, "all", "all", "last7", NOW);
    expect(result.map((i) => i.id).sort()).toEqual(["a", "b", "c"]);
    expect(result.some((i) => i.id === "d")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// applyHistoryFilters — combined filters
// ---------------------------------------------------------------------------

describe("applyHistoryFilters — combined filters", () => {
  it("source + date together narrow correctly", () => {
    // Only desktop entries from today (legacy rows from today would also pass,
    // but oldEntry is not today, so it's excluded by date)
    const result = applyHistoryFilters(history, "desktop_recording", "all", "today", NOW);
    expect(result.map((i) => i.id)).toEqual(["a"]);
  });

  it("source + app together work", () => {
    const result = applyHistoryFilters(history, "desktop_recording", "Notion", "all", NOW);
    expect(result.map((i) => i.id)).toEqual(["a"]);
  });
});

// ---------------------------------------------------------------------------
// Text search (component-level pattern)
// ---------------------------------------------------------------------------

describe("text search alongside structural filters", () => {
  // Mirror the component's filteredHistory predicate (raw model + display name + text)
  function filterWithText(
    items: TranscriptionHistory[],
    query: string,
    sourceFilter = "all",
    appFilter = "all",
    dateFilter = "all",
  ): TranscriptionHistory[] {
    const structural = applyHistoryFilters(items, sourceFilter, appFilter, dateFilter, NOW);
    if (!query.trim()) return structural;
    const q = query.trim().toLowerCase();
    return structural.filter(
      (item) =>
        item.text.toLowerCase().includes(q) || (item.model && item.model.toLowerCase().includes(q)),
    );
  }

  it("text search filters by transcript content", () => {
    const result = filterWithText(history, "desktop");
    expect(result.map((i) => i.id)).toEqual(["a"]);
  });

  it("text search works alongside source filter (legacy rows excluded by source filter)", () => {
    // audio_file source filter: only uploadEntry (b) passes (oldEntry excluded — no source)
    // 'text' also matches uploadEntry's content
    const result = filterWithText(history, "text", "audio_file");
    expect(result.map((i) => i.id)).toEqual(["b"]);
  });

  it("old entries still render when text search matches and filter is 'all'", () => {
    const result = filterWithText(history, "old entry");
    expect(result.map((i) => i.id)).toEqual(["d"]);
  });

  it("empty search returns all (structural) results", () => {
    const result = filterWithText(history, "");
    expect(result).toHaveLength(4);
  });

  it("raw model id search matches (e.g. 'base.en')", () => {
    // All fixtures use model "base.en"; all 4 should match when no other filter is active
    const result = filterWithText(history, "base.en");
    expect(result).toHaveLength(4);
  });

  it("raw model id search works alongside source filter", () => {
    // 'base.en' matches all; desktop_recording source filter passes only desktopWithApp (a)
    const result = filterWithText(history, "base.en", "desktop_recording");
    expect(result.map((i) => i.id)).toEqual(["a"]);
  });
});
