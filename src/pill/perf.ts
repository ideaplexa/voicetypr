/**
 * Debug-only, content-free island measurement (plan 082 F0). The probe exists only
 * when the backend reports VOICETYPR_PERF=1 on a debug build; otherwise nothing is
 * created, so the pill pays no per-frame cost. It records numbers only: never text,
 * audio, window titles or paths.
 */
export interface FrameSummary {
  frames: number; p50_ms: number; p95_ms: number; max_ms: number; over16: number; over33: number; over50: number;
}
export interface PerfSummary extends FrameSummary {
  generation: number;
  level_events: number; level_gap_p95_ms: number;
  pill_dom_nodes: number; stack_items: number;
  /** null when the WebView has no longtask observer. */
  longtasks: number | null;
}
export interface PerfSink {
  visible(unixMs: number): Promise<unknown> | void;
  summary(summary: PerfSummary): Promise<unknown> | void;
}
export interface PerfDeps {
  sink: PerfSink;
  now: () => number;
  timeOrigin: () => number;
  raf: (callback: (timestamp: number) => void) => number;
  caf: (id: number) => void;
  /** True once the island is actually on screen in its recording state. */
  visible: () => boolean;
  domNodes: () => number;
  stackItems: () => number;
  observeLongTasks?: (onTask: () => void) => (() => void) | null;
}

/** One 60 Hz frame is 16.7 ms; 17 ms tolerates timer jitter so only a missed frame counts. */
const OVER16_MS = 17;
const MAX_SAMPLES = 200_000;
const MAX_VISIBLE_WAIT_FRAMES = 180;

/** Nearest-rank percentile of an ascending list; 0 for an empty list. */
export function percentile(sorted: readonly number[], p: number): number {
  if (!sorted.length) return 0;
  const rank = Math.ceil((p / 100) * sorted.length);
  return sorted[Math.min(sorted.length - 1, Math.max(0, rank - 1))];
}
const ascending = (values: readonly number[]) => [...values].sort((a, b) => a - b);

/** Frame-interval statistics. Inputs are consecutive requestAnimationFrame deltas in ms. */
export function summarizeFrames(deltas: readonly number[]): FrameSummary {
  const sorted = ascending(deltas);
  return {
    frames: sorted.length,
    p50_ms: percentile(sorted, 50),
    p95_ms: percentile(sorted, 95),
    max_ms: sorted.length ? sorted[sorted.length - 1] : 0,
    over16: sorted.filter(value => value > OVER16_MS).length,
    over33: sorted.filter(value => value > 33).length,
    over50: sorted.filter(value => value > 50).length,
  };
}
export const summarizeGaps = (gaps: readonly number[]): number => percentile(ascending(gaps), 95);

/** Observe long tasks when WebKit supports them; never throws. */
export function observeLongTasks(onTask: () => void): (() => void) | null {
  try {
    if (typeof PerformanceObserver === 'undefined' || !PerformanceObserver.supportedEntryTypes?.includes('longtask')) return null;
    const observer = new PerformanceObserver(list => { for (let i = 0; i < list.getEntries().length; i++) onTask(); });
    observer.observe({ entryTypes: ['longtask'] });
    return () => observer.disconnect();
  } catch { return null; }
}

export class PerfProbe {
  private active = false;
  private frames: number[] = [];
  private gaps: number[] = [];
  private lastFrame = -1;
  private lastLevel = -1;
  private levels = 0;
  private generation = 0;
  private longtasks = 0;
  private visibleRaf: number | undefined;
  private stopObserver: (() => void) | null;
  constructor(private readonly deps: PerfDeps) {
    this.stopObserver = (deps.observeLongTasks ?? observeLongTasks)(() => { if (this.active) this.longtasks++; });
  }
  /** A take entered its recording state: reset counters and time the first visible frame. */
  begin() {
    this.cancelVisible();
    this.active = true;
    this.frames = []; this.gaps = []; this.lastFrame = this.lastLevel = -1; this.levels = 0; this.longtasks = 0; this.generation = 0;
    let waited = 0;
    const check = (timestamp: number) => {
      this.visibleRaf = undefined;
      if (!this.active) return;
      if (this.deps.visible()) { void this.deps.sink.visible(this.deps.timeOrigin() + timestamp); return; }
      if (++waited < MAX_VISIBLE_WAIT_FRAMES) this.visibleRaf = this.deps.raf(check);
    };
    this.visibleRaf = this.deps.raf(check);
  }
  frame(now: number) {
    if (!this.active) return;
    if (this.lastFrame >= 0 && this.frames.length < MAX_SAMPLES) this.frames.push(now - this.lastFrame);
    this.lastFrame = now;
  }
  level(now: number, generation: number) {
    if (!this.active) return;
    this.levels++; this.generation = generation;
    if (this.lastLevel >= 0 && this.gaps.length < MAX_SAMPLES) this.gaps.push(now - this.lastLevel);
    this.lastLevel = now;
  }
  /** The take left its recording state: send one summary. Idempotent. */
  end(fallbackGeneration: number) {
    if (!this.active) return;
    this.active = false;
    this.cancelVisible();
    const summary: PerfSummary = {
      ...summarizeFrames(this.frames),
      generation: this.generation || fallbackGeneration,
      level_events: this.levels,
      level_gap_p95_ms: summarizeGaps(this.gaps),
      pill_dom_nodes: this.deps.domNodes(),
      stack_items: this.deps.stackItems(),
      longtasks: this.stopObserver ? this.longtasks : null,
    };
    void this.deps.sink.summary(summary);
  }
  destroy() { this.active = false; this.cancelVisible(); this.stopObserver?.(); this.stopObserver = null; }
  private cancelVisible() { if (this.visibleRaf !== undefined) this.deps.caf(this.visibleRaf); this.visibleRaf = undefined; }
}

/** Disabled means no probe object, no observer and no frame callbacks at all. */
export function createPerfProbe(enabled: boolean, deps: PerfDeps): PerfProbe | undefined {
  return enabled ? new PerfProbe(deps) : undefined;
}
