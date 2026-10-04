import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));

import { fetchModelStatus } from "./model-status";

describe("fetchModelStatus", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async () => ({ models: [] }));
  });
  afterEach(() => vi.useRealTimers());

  it("shares one backend call between callers in the same tick", async () => {
    const [a, b] = await Promise.all([fetchModelStatus(), fetchModelStatus()]);
    expect(a).toBe(b);
    expect(mockInvoke).toHaveBeenCalledTimes(1);
  });

  it("gives a later caller one fresh trailing call, shared by all late callers", async () => {
    let release: (value: { models: never[] }) => void = () => undefined;
    mockInvoke.mockImplementationOnce(() => new Promise((resolve) => (release = resolve)));
    const first = fetchModelStatus();
    await new Promise((resolve) => setTimeout(resolve, 5));
    const lateA = fetchModelStatus();
    const lateB = fetchModelStatus();
    expect(lateA).toBe(lateB);
    release({ models: [] });
    await Promise.all([first, lateA, lateB]);
    expect(mockInvoke).toHaveBeenCalledTimes(2);
  });

  it("calls again once the previous request settled", async () => {
    await fetchModelStatus();
    await fetchModelStatus();
    expect(mockInvoke).toHaveBeenCalledTimes(2);
  });

  it("schedules a trailing call for a refresh arriving after the backend call started", async () => {
    vi.useFakeTimers();
    let release: (value: { models: never[] }) => void = () => undefined;
    mockInvoke.mockImplementationOnce(() => new Promise((resolve) => (release = resolve)));
    const first = fetchModelStatus();
    const sameTick = fetchModelStatus();
    expect(sameTick).toBe(first);
    await vi.advanceTimersByTimeAsync(0);
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    const late = fetchModelStatus();
    expect(late).not.toBe(first);
    release({ models: [] });
    await vi.advanceTimersByTimeAsync(0);
    await Promise.all([first, late]);
    expect(mockInvoke).toHaveBeenCalledTimes(2);
  });
});
