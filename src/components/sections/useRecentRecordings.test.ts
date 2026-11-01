import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { TranscriptionHistory } from "@/types";
import { useRecentRecordings } from "@/components/sections/useRecentRecordings";

vi.mock("@/components/sections/useRecentRecordingsActions", () => ({
  useRecentRecordingsActions: () => ({}),
}));

function item(id: string, timestamp: unknown): TranscriptionHistory {
  // Malformed persisted data can violate the declared Date type.
  return { id, text: id, model: "base", ...(timestamp === undefined ? {} : { timestamp }) } as TranscriptionHistory;
}

describe("History timestamp recovery", () => {
  it.each(["", "garbage", "123", undefined])("keeps %s in an Unknown date group after dated entries", async (timestamp) => {
    const bad = item("bad", timestamp);
    const dated = item("dated", new Date("2026-10-01T10:00:00Z"));
    const history = [bad, dated];
    const { result } = renderHook(() => useRecentRecordings({ history }));
    await act(async () => {});
    expect(result.current.filteredHistory.map((entry) => entry.id)).toEqual(["dated", "bad"]);
    const groupKeys = Object.keys(result.current.groupedHistory);
    expect(groupKeys[groupKeys.length - 1]).toBe("Unknown date");
    expect(result.current.groupedHistory["Unknown date"]).toEqual([bad]);
    expect(history).toEqual([bad, dated]);
    act(() => result.current.setDateFilter("last7"));
    expect(result.current.filteredHistory).not.toContain(bad);
  });

  it("sorts unknown dates behind all valid dates before pagination", async () => {
    const dated = Array.from({ length: 60 }, (_, index) =>
      item(`dated-${index}`, new Date(Date.UTC(2026, 9, 1, 0, index))),
    );
    const history = [item("bad", new Date("garbage")), ...dated];
    const { result } = renderHook(() => useRecentRecordings({ history }));
    await act(async () => {});
    expect(result.current.filteredHistory[0].id).toBe("dated-59");
    expect(result.current.groupedHistory["Unknown date"]).toBeUndefined();
    act(() => result.current.setVisibleCount(120));
    expect(result.current.groupedHistory["Unknown date"]).toHaveLength(1);
  });
});
