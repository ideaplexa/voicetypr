import { afterEach, describe, expect, it, vi } from 'vitest';
import { pillHarness } from '@/pill/test-harness';
import { groupLeft, timerChars, waveZone, ZONE } from '@/pill/zones';
import { OutlineClock, outlinePath } from '@/pill/outline-clock';
import { TRANSCRIPTION_STREAM_EVENT } from '@/types/streaming';

describe('island row zones', () => {
  it('centres the wave between the icon and the timer: equal gaps on both sides', () => {
    for (const width of [236, 300, 360]) {
      const { x, width: w } = waveZone(width, '0:07');
      const timerLeft = width - ZONE.border - ZONE.pad - timerChars('0:07') * ZONE.mono;
      expect(x - ZONE.iconEdge).toBe(ZONE.gap);
      expect(timerLeft - (x + w)).toBeCloseTo(ZONE.gap, 5);
    }
  });
  it('narrows the wave for longer times and never goes negative', () => {
    expect(waveZone(236, '10:00').width).toBeCloseTo(waveZone(236, '9:59').width - ZONE.mono, 5);
    expect(waveZone(236, '1:00:00').width).toBeLessThan(waveZone(236, '9:59').width);
    expect(waveZone(60, '1:00:00').width).toBe(0);
  });
  it('centres a result group in the row but never against the app icon', () => {
    expect(groupLeft(234, 130)).toBe(52);
    expect(groupLeft(234, 220)).toBe(ZONE.glyphMin);
    expect(groupLeft(234, 400)).toBe(ZONE.glyphMin);
  });
});

describe('outline path', () => {
  it('starts at the top centre, runs clockwise and ends where it started', () => {
    const d = outlinePath(236, 34, 17);
    expect(d.startsWith('M117 1H')).toBe(true);
    expect(d.endsWith('H117')).toBe(true);
    expect((d.match(/A/g) ?? []).length).toBe(4);
    expect(d).toContain('A15 15 0 0 1 233 16'); // full pill: radius is half the height, minus the stroke inset
  });
  it('clamps the corner radius to the card', () => {
    expect(outlinePath(40, 20, 20)).toContain('A8 8 0');
    expect(outlinePath(100, 50, 0)).toContain('A0 0 0');
  });
});

describe('outline clock', () => {
  it('drains over the remaining time from where it was handed off, and restarts on start()', () => {
    const clock = new OutlineClock('amber'); clock.size(300, 40, 20);
    expect(clock.el.getAttribute('class')).toContain('amber');
    clock.start(2000, 500);
    const first = clock.el.querySelector('path')!;
    expect(first.style.animationDuration).toBe('500ms');
    expect(first.style.getPropertyValue('--clock-from')).toBe('0.75');
    expect(first.getAttribute('pathLength')).toBe('1');
    clock.start(2000);
    expect(clock.el.querySelectorAll('path')).toHaveLength(1);
    expect(clock.el.querySelector('path')).not.toBe(first);
    expect(clock.el.querySelector('path')!.style.animationDuration).toBe('2000ms');
  });
  it('pauses and resumes the same stroke, and a restart keeps the pause', () => {
    const clock = new OutlineClock(); clock.size(300, 40, 20); clock.start(2400);
    const path = () => clock.el.querySelector('path')!;
    clock.pause(true); expect(path().style.animationPlayState).toBe('paused');
    clock.start(2400); expect(path().style.animationPlayState).toBe('paused');
    clock.pause(false); expect(path().style.animationPlayState).toBe('running');
  });
  it('refits the stroke when the card resizes', () => {
    const clock = new OutlineClock(); clock.size(236, 34, 17); clock.start(2400);
    const before = clock.el.querySelector('path')!.getAttribute('d');
    clock.size(236, 70, 20);
    expect(clock.el.querySelector('path')!.getAttribute('d')).not.toBe(before);
    expect(clock.el.getAttribute('height')).toBe('68');
  });
});

describe('island layout in the real renderer', () => {
  let h: ReturnType<typeof pillHarness>;
  afterEach(() => { h?.destroy(); Reflect.deleteProperty(Range.prototype, 'getBoundingClientRect'); });
  const glyphLeft = () => h.get('.glyph').style.left;
  it('listening: the wave sits in the middle zone, one gap right of the icon', async () => {
    h = pillHarness({ pill_indicator_mode: 'when_recording' }); await h.settle(0);
    h.emit('recording-started'); await h.settle(300);
    expect(glyphLeft()).toBe(`${ZONE.iconEdge + ZONE.gap}px`);
    h.emit('transcription-started'); await h.settle(300);
    expect(glyphLeft()).toBe(`${ZONE.glyphMin}px`);
  });
  it('listening with live words keeps the same zone', async () => {
    h = pillHarness({ pill_indicator_mode: 'when_recording' }); await h.settle(0);
    h.emit('recording-started'); h.emit(TRANSCRIPTION_STREAM_EVENT, { type: 'partial', session_id: 1, revision: 1, committed: 'hello ', tentative: 'there' }); await h.settle(300);
    expect(h.get('.pill-root').dataset.preview).toBe('true');
    expect(glyphLeft()).toBe(`${ZONE.iconEdge + ZONE.gap}px`);
  });
  it('pasted: check, label and count are one centred group', async () => {
    const widths: Record<string, number> = { 'Pasted': 40, '· 38 words': 60 };
    Object.defineProperty(Range.prototype, 'getBoundingClientRect', { configurable: true, value(this: Range) { return { width: widths[this.toString()] ?? 0 }; } });
    h = pillHarness({ pill_indicator_mode: 'when_recording' }); await h.settle(0);
    h.emit('paste-outcome', { outcome: 'pasted', words: 38, polished: false }); await h.settle(300);
    const left = groupLeft(234, ZONE.glyph + 40 + 5 + 60);
    expect(left).toBeGreaterThan(ZONE.glyphMin);
    expect(glyphLeft()).toBe(`${left}px`);
    expect(h.get('.done').style.getPropertyValue('--grp-x')).toBe(`${left + ZONE.glyph}px`);
  });
});

vi.mock('@/lib/platform', () => ({ isMacOS: true }));
