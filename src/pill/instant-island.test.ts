import { afterEach, describe, expect, it, vi } from 'vitest';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => { h?.destroy(); });
const mount = async (settings: Record<string, unknown> = {}) => { h = pillHarness({ pill_indicator_mode: 'when_recording', ...settings }); await h.settle(0); return h; };
const surface = () => h.get('.pill-surface');
const rootData = () => h.get('.pill-root').dataset;
const starting = () => h.emit('recording-state-changed', { state: 'starting', error: null });
const recording = () => h.emit('recording-state-changed', { state: 'recording', error: null });
const timer = () => h.get('.pill-timer').textContent;

describe('instant island on keypress', () => {
  it('shows the starting look at Starting: listening layout, clock at 0:00, never a live take', async () => {
    await mount();
    expect(surface().hidden).toBe(true);
    starting();
    expect(surface().hidden).toBe(false);
    expect(rootData().state).toBe('listening'); expect(rootData().starting).toBe('true');
    await h.settle(3000);
    expect(timer()).toBe('0:00');
    expect(h.get('[data-layer="row"]').getAttribute('aria-hidden')).toBe('false');
  });
  it('shows no recording look before Recording: levels are ignored and the bars stay empty', async () => {
    await mount();
    starting();
    h.emit('audio-level', { generation: 1, level: 1 }); await h.settle(1500);
    expect(timer()).toBe('0:00'); expect(rootData().starting).toBe('true');
    recording();
    expect(rootData().starting).toBe('false');
  });
  it('goes live at Recording with the clock starting then, not at the keypress', async () => {
    await mount();
    starting(); await h.settle(1500);
    recording(); await h.settle(2050);
    expect(timer()).toBe('0:02');
    // The legacy recording-started event after Recording must not restart the clock.
    h.emit('recording-started'); await h.settle(1000);
    expect(timer()).toBe('0:03');
  });
  it('flows starting -> live -> transcribing -> done', async () => {
    await mount();
    starting(); await h.settle(300); recording(); await h.settle(2050);
    h.emit('recording-state-changed', { state: 'stopping', error: null }); await h.settle(200);
    expect(rootData().state).toBe('transcribing'); expect(h.get('.work-time').textContent).toBe('0:02');
    h.emit('paste-outcome', { outcome: 'pasted', words: 4 }); await h.settle(100);
    expect(rootData().state).toBe('pasted');
  });
  it('folds back when the start is cancelled before Recording', async () => {
    await mount();
    starting(); await h.settle(100);
    h.emit('recording-state-changed', { state: 'idle', error: null }); await h.settle(1000);
    expect(rootData().state).toBe('idle'); expect(rootData().starting).toBe('false'); expect(surface().hidden).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });
  it('folds back without an error flash when the start fails before Recording', async () => {
    await mount();
    starting(); await h.settle(100);
    h.emit('recording-state-changed', { state: 'error', error: 'Mic unavailable' }); await h.settle(2000);
    expect(rootData().state).toBe('idle'); expect(surface().hidden).toBe(true);
    expect(h.root.textContent).not.toContain('Recording failed');
  });
  it('asks the backend to hide the early window when a start ends before Recording, and only then', async () => {
    await mount();
    const hides = () => (h.invoke.mock.calls as unknown as Array<[string, { visible?: boolean }?]>).filter(([cmd, args]) => cmd === 'pill_feedback_visible' && args?.visible === false).length;
    starting(); await h.settle(50);
    h.emit('recording-state-changed', { state: 'idle', error: null }); await h.settle(50);
    expect(hides()).toBe(1);
    starting(); recording(); await h.settle(300);
    h.emit('recording-state-changed', { state: 'stopping', error: null });
    h.emit('recording-state-changed', { state: 'idle', error: null }); await h.settle(50);
    expect(hides()).toBe(1);
  });
  it('keeps an always-visible island as the rest dot after a cancelled start', async () => {
    await mount({ pill_indicator_mode: 'always' });
    starting(); await h.settle(100);
    h.emit('recording-state-changed', { state: 'idle', error: null }); await h.settle(1000);
    expect(rootData().state).toBe('idle'); expect(surface().hidden).toBe(false);
  });
  it('stays hidden for Starting in never mode', async () => {
    await mount({ pill_indicator_mode: 'never' });
    starting(); recording(); await h.settle(500);
    expect(surface().hidden).toBe(true);
  });
  it('hydrates a take that is still starting', async () => {
    h = pillHarness({ pill_indicator_mode: 'when_recording' }, { state: 'starting', error: null }); await h.settle(0);
    expect(rootData().starting).toBe('true'); expect(surface().hidden).toBe(false);
  });
});

describe('compositor-only island motion', () => {
  it('computes hit regions from the springs without measuring the DOM on any frame', async () => {
    await mount({ pill_indicator_mode: 'always' });
    await h.settle(500);
    const measure = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect');
    const stringify = vi.spyOn(JSON, 'stringify');
    const regions = () => h.invoke.mock.calls.filter(([cmd]) => cmd === 'pill_set_hit_regions').length;
    const before = regions();
    starting(); recording();
    h.emit('transcription-started');
    const stateChanges = stringify.mock.calls.length; // serialisation on a state change is fine; per frame it is not
    await h.settle(3000); // two full morphs, ~180 frames
    expect(measure).not.toHaveBeenCalled();
    expect(stringify.mock.calls.length).toBe(stateChanges);
    expect(regions()).toBeGreaterThan(before); // regions follow the morph
  });
  it('sends hit regions only when they change', async () => {
    await mount({ pill_indicator_mode: 'always' });
    starting(); recording(); await h.settle(1500);
    const sent = h.invoke.mock.calls.filter(([cmd]) => cmd === 'pill_set_hit_regions').length;
    await h.settle(1000); // settled and recording: frames still run, regions are unchanged
    expect(h.invoke.mock.calls.filter(([cmd]) => cmd === 'pill_set_hit_regions').length).toBe(sent);
  });
  it('reads hover from cached button rects instead of measuring on every pointer event', async () => {
    await mount({ pill_indicator_mode: 'always' });
    starting(); recording(); await h.settle(1500);
    h.emit('pill-pointer', { inside: true, x: 200, y: 400 }); await h.settle(300);
    const measure = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect');
    for (let i = 0; i < 20; i++) h.emit('pill-pointer', { inside: true, x: 200 + i, y: 400 });
    expect(measure.mock.calls.length).toBeLessThanOrEqual(h.root.querySelectorAll('.pill-surface button').length);
  });
  it('moves the content by transform only: layers carry a translate, never a per-frame left/top', async () => {
    await mount({ pill_indicator_mode: 'always' });
    starting(); recording(); await h.settle(50);
    expect(h.get('.pill-content').style.width).toMatch(/px$/);
    expect(h.get('[data-layer="row"]').style.translate).toMatch(/px/);
  });
});
