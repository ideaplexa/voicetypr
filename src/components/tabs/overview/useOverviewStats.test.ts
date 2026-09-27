import { describe, expect, it } from "vitest";
import { computeOverviewStats } from "./useOverviewStats";

describe("computeOverviewStats", () => {
  it("keeps the weekly maximum at zero for an empty history", () => {
    const stats = computeOverviewStats([], 0);

    expect(stats.weekCount).toBe(0);
    expect(stats.weekMax).toBe(0);
    expect(stats.weekDays.every((day) => day.count === 0)).toBe(true);
  });
});
