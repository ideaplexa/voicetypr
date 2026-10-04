import { describe, expect, it, vi } from 'vitest';
import { createPerfProbe, percentile, summarizeFrames, summarizeGaps, type PerfDeps, type PerfSummary } from '@/pill/perf';

describe('perf summary math', () => {
  it('uses nearest-rank percentiles', () => {
    expect(percentile([], 95)).toBe(0);
    expect(percentile([5], 50)).toBe(5);
    const hundred = Array.from({ length: 100 }, (_, i) => i + 1);
    expect(percentile(hundred, 50)).toBe(50);
    expect(percentile(hundred, 95)).toBe(95);
    expect(percentile(hundred, 100)).toBe(100);
  });
  it('summarises frame deltas and counts long frames', () => {
    const deltas = [...Array.from({ length: 90 }, () => 16.6), 20, 25, 34, 40, 51, 60, 80, 100, 120, 200];
    const s = summarizeFrames(deltas);
    expect(s.frames).toBe(100);
    expect(s.p50_ms).toBe(16.6);
    expect(s.p95_ms).toBe(51);
    expect(s.max_ms).toBe(200);
    expect(s.over16).toBe(10);
    expect(s.over33).toBe(8);
    expect(s.over50).toBe(6);
  });
  it('does not mutate its input and handles no samples', () => {
    const input = [30, 10, 20];
    summarizeFrames(input);
    expect(input).toEqual([30, 10, 20]);
    expect(summarizeFrames([])).toEqual({ frames: 0, p50_ms: 0, p95_ms: 0, max_ms: 0, over16: 0, over33: 0, over50: 0 });
    expect(summarizeGaps([])).toBe(0);
    expect(summarizeGaps([16, 17, 16, 90])).toBe(90);
  });
});

const make = (visible = () => true) => {
  const callbacks: Array<(t: number) => void> = [];
  const sink = { visible: vi.fn(), summary: vi.fn() };
  const deps: PerfDeps = {
    sink, now: () => 0, timeOrigin: () => 1_000_000, raf: cb => callbacks.push(cb), caf: vi.fn(),
    visible, domNodes: () => 42, stackItems: () => 2, observeLongTasks: () => () => {},
  };
  return { deps, sink, callbacks };
};

describe('perf probe', () => {
  it('is inert when disabled', () => {
    const { deps, sink, callbacks } = make();
    const observe = vi.fn(() => null);
    expect(createPerfProbe(false, { ...deps, observeLongTasks: observe })).toBeUndefined();
    expect(observe).not.toHaveBeenCalled();
    expect(callbacks).toHaveLength(0);
    expect(sink.visible).not.toHaveBeenCalled();
    expect(sink.summary).not.toHaveBeenCalled();
  });
  it('reports the first visible frame in unix ms', () => {
    const { deps, sink, callbacks } = make();
    const probe = createPerfProbe(true, deps)!;
    probe.begin();
    callbacks[0](250);
    expect(sink.visible).toHaveBeenCalledWith(1_000_250);
  });
  it('waits until the island is visible', () => {
    let on = false;
    const { deps, sink, callbacks } = make(() => on);
    createPerfProbe(true, deps)!.begin();
    callbacks[0](10);
    expect(sink.visible).not.toHaveBeenCalled();
    on = true; callbacks[1](26);
    expect(sink.visible).toHaveBeenCalledWith(1_000_026);
  });
  it('sends one summary per take and ignores events outside a take', () => {
    const { deps, sink } = make();
    const probe = createPerfProbe(true, deps)!;
    probe.frame(1); probe.level(1, 9);
    probe.begin();
    for (const t of [100, 116, 133, 183]) probe.frame(t);
    for (const t of [100, 116, 200]) probe.level(t, 7);
    probe.end(3); probe.end(3);
    expect(sink.summary).toHaveBeenCalledTimes(1);
    const s = sink.summary.mock.calls[0][0] as PerfSummary;
    expect(s).toMatchObject({ generation: 7, frames: 3, max_ms: 50, over33: 1, over50: 0, level_events: 3, level_gap_p95_ms: 84, pill_dom_nodes: 42, stack_items: 2, longtasks: 0 });
  });
  it('reports unsupported long-task observation as null and falls back to the machine generation', () => {
    const { deps, sink } = make();
    const probe = createPerfProbe(true, { ...deps, observeLongTasks: () => null })!;
    probe.begin(); probe.end(5);
    expect(sink.summary.mock.calls[0][0]).toMatchObject({ generation: 5, longtasks: null, frames: 0 });
  });
});
