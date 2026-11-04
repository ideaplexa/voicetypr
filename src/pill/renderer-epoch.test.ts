import { afterEach, expect, it, vi } from 'vitest';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const kept = {
  generation: 6, id: 'kept', kind: 'cloud_failed', engine_short: 'Whisper',
  alt_engine_short: 'Parakeet', alt_engine_id: 'parakeet-tdt-0.6b-v3', expires_in_ms: 600_000,
};
function failedLiveHydration(failure: 'reject' | 'timeout') {
  h = pillHarness();
  h.invoke.mockImplementation(async (command, args?: { epoch?: number }) => {
    if (command === 'pill_feedback_ready') {
      if (args?.epoch !== undefined) h.emit('dictation-note', { generation: 7, kind: 'mic_silent_live', renderer_epoch: 1 });
      return 1;
    }
    if (command === 'get_current_recording_state') {
      if (failure === 'reject') throw new Error('Hydration unavailable');
      return new Promise(() => {});
    }
    return null;
  });
}

it.each(['reject', 'timeout'] as const)('briefly retains a live warning after hydration %s until Recording becomes known', async failure => {
  failedLiveHydration(failure); await h.settle(failure === 'timeout' ? 1800 : 0);
  expect(h.get('[data-key="mic_silent_live:7"]')).toBeNull();
  await h.settle(100);
  h.emit('recording-state-changed', { state: 'recording', generation: 7, error: null }); await h.settle(0);
  expect(h.get('[data-key="mic_silent_live:7"]')).not.toBeNull();
  h.emit('recording-state-changed', { state: 'recording', generation: 7, error: null }); await h.settle(0);
  expect(h.root.textContent).not.toContain('×2');
});

it.each(['idle', 'newer take', 'withdrawal', 'expiry'])('does not revive a deferred warning after %s', async obsolete => {
  failedLiveHydration('reject'); await h.settle(0);
  if (obsolete === 'idle') h.emit('recording-state-changed', { state: 'idle', generation: 7, error: null });
  if (obsolete === 'newer take') h.emit('recording-state-changed', { state: 'recording', generation: 8, error: null });
  if (obsolete === 'withdrawal') h.emit('dictation-note-withdraw', { generation: 7, kind: 'mic_silent_live', renderer_epoch: 1 });
  if (obsolete === 'expiry') await h.settle(2001);
  h.emit('recording-state-changed', { state: 'recording', generation: 7, error: null }); await h.settle(0);
  expect(h.get('[data-key="mic_silent_live:7"]')).toBeNull();
});

it.each([0, 1800])('cleans up startup timers on destruction after %i ms', async delay => {
  failedLiveHydration('timeout'); await h.settle(delay);
  h.controller.destroy(); await h.settle(0);
  expect(vi.getTimerCount()).toBe(0);
});

it.each(['reject', 'timeout'] as const)('retries once at 500 ms after %s, promoting only the warning without changing island state', async failure => {
  h = pillHarness();
  let reads = 0, late!: (state: { state: string; generation: number; error: null }) => void;
  h.invoke.mockImplementation(async (command, args?: { epoch?: number }) => {
    if (command === 'pill_feedback_ready') {
      if (args?.epoch !== undefined) h.emit('dictation-note', { generation: 7, kind: 'mic_silent_live', renderer_epoch: 1 });
      return 1;
    }
    if (command === 'get_current_recording_state') {
      if (++reads === 1) {
        if (failure === 'reject') throw new Error('Hydration unavailable');
        return new Promise(resolve => { late = resolve; });
      }
      return { state: 'recording', generation: 7, error: null };
    }
    return null;
  });
  await h.settle(failure === 'timeout' ? 1800 : 0);
  if (failure === 'timeout') late({ state: 'recording', generation: 7, error: null });
  await h.settle(0);
  expect(reads).toBe(1);
  expect(h.get('.pill-root').dataset.state).toBe('idle');
  expect(h.get('[data-key="mic_silent_live:7"]')).toBeNull();
  await h.settle(499);
  expect(reads).toBe(1);
  await h.settle(1);
  expect(reads).toBe(2);
  expect(h.get('.pill-root').dataset.state).toBe('idle');
  expect(h.get('[data-key="mic_silent_live:7"]')).not.toBeNull();
  await h.settle(5000);
  expect(reads).toBe(2);
  expect(h.root.textContent).not.toContain('×2');
});

it.each(['transcribing', 'recording'])('ignores late same-generation %s hydration after timeout and recovery handoff', async state => {
  h = pillHarness();
  let hydrate!: (value: { state: string; generation: number; error: null }) => void;
  h.invoke.mockImplementation(async (command, args?: { epoch?: number }) => {
    if (command === 'pill_feedback_ready') {
      if (args?.epoch !== undefined) h.emit('dictation-recovery', { ...kept, generation: 7, renderer_epoch: 1 });
      return 1;
    }
    if (command === 'get_current_recording_state') return new Promise(resolve => { hydrate = resolve; });
    return null;
  });
  await h.settle(1800);
  expect(h.get('[data-key="recovery:kept"]')).not.toBeNull();
  expect(h.get('.pill-root').dataset.state).toBe('idle');
  hydrate({ state, generation: 7, error: null }); await h.settle(0);
  expect(h.get('.pill-root').dataset.state).toBe('idle');
  expect(h.get('[data-key="recovery:kept"]')).not.toBeNull();
});

it.each([
  ['expiry', false], ['withdrawal', false], ['expiry', true], ['withdrawal', true],
] as const)('reconciles ownership after held-warning %s (other feedback: %s)', async (discard, otherFeedback) => {
  failedLiveHydration('reject'); await h.settle(0);
  if (otherFeedback) { h.emit('dictation-recovery', { ...kept, renderer_epoch: 1 }); await h.settle(0); }
  h.invoke.mockClear();
  if (discard === 'expiry') await h.settle(2000);
  else {
    h.emit('dictation-note-withdraw', { generation: 7, kind: 'mic_silent_live', renderer_epoch: 1 });
    await h.settle(0);
  }
  const releases = h.invoke.mock.calls.filter(([command]) => command === 'pill_feedback_visible');
  expect(releases).toEqual(otherFeedback ? [] : [['pill_feedback_visible', { visible: false, epoch: 1 }]]);
  expect(h.get('[data-key="mic_silent_live:7"]')).toBeNull();
  if (otherFeedback) expect(h.get('[data-key="recovery:kept"]')).not.toBeNull();
});

it.each(['idle', 'newer take'])('releases ownership when known state discards the held warning: %s', async state => {
  failedLiveHydration('reject'); await h.settle(0); h.invoke.mockClear();
  h.emit('recording-state-changed', { state: state === 'idle' ? 'idle' : 'recording', generation: state === 'idle' ? 7 : 8, error: null });
  await h.settle(0);
  expect(h.invoke).toHaveBeenCalledWith('pill_feedback_visible', { visible: false, epoch: 1 });
  expect(h.get('[data-key="mic_silent_live:7"]')).toBeNull();
});

it('does not release ownership when a recording error replaces the held warning', async () => {
  failedLiveHydration('reject'); await h.settle(0); h.invoke.mockClear();
  h.emit('recording-state-changed', { state: 'error', generation: 7, error: 'unavailable' }); await h.settle(0);
  expect(h.get('.pill-root').dataset.state).toBe('error');
  expect(h.invoke.mock.calls.filter(([command]) => command === 'pill_feedback_visible')).toEqual([]);
  expect(h.invoke).not.toHaveBeenCalledWith('pill_feedback_visible', { visible: false, epoch: 1 });
});

it('does not release ownership while replacing or displaying a held warning', async () => {
  failedLiveHydration('reject'); await h.settle(1000); h.invoke.mockClear();
  h.emit('dictation-note', { generation: 8, kind: 'mic_silent_live', renderer_epoch: 1 });
  await h.settle(1000);
  h.emit('recording-state-changed', { state: 'recording', generation: 8, error: null }); await h.settle(0);
  expect(h.get('[data-key="mic_silent_live:8"]')).not.toBeNull();
  expect(h.invoke).not.toHaveBeenCalledWith('pill_feedback_visible', { visible: false, epoch: 1 });
});

it('buffers tagged replay until readiness identifies the renderer epoch', async () => {
  h = pillHarness();
  let ready!: (epoch: number) => void;
  h.invoke.mockImplementation(async (command, args?: { epoch?: number }) => command === 'pill_feedback_ready' && args?.epoch === undefined
    ? new Promise<number>(resolve => { ready = resolve; }) : null);
  await h.settle(0);
  h.emit('dictation-recovery', { ...kept, renderer_epoch: 0, id: 'dead' });
  h.emit('dictation-recovery', { ...kept, renderer_epoch: 1 });
  expect(h.get('.feedback-card')).toBeNull();
  ready(1); await h.settle(500);
  expect(h.get('[data-key="recovery:dead"]')).toBeNull();
  expect(h.get('[data-key="recovery:kept"]')).not.toBeNull();
  expect(h.invoke).toHaveBeenCalledWith('pill_feedback_visible', { visible: true, epoch: 1 });
});

it('hydrates Recording before replaying a rebuilt live warning in a replacement renderer', async () => {
  h = pillHarness();
  let hydrate!: (state: { state: string; generation: number; error: null }) => void;
  h.invoke.mockImplementation(async (command, args?: { epoch?: number }) => {
    if (command === 'pill_feedback_ready') {
      if (args?.epoch !== undefined) h.emit('dictation-note', { generation: 7, kind: 'mic_silent_live', renderer_epoch: 1 });
      return 1;
    }
    if (command === 'get_current_recording_state') return new Promise(resolve => { hydrate = resolve; });
    return null;
  });
  await h.settle(0);
  expect(h.get('[data-key="mic_silent_live:7"]')).toBeNull();
  // No Recording event was received by this renderer; only hydration can
  // establish the state required to accept the backend's rebuilt warning.
  hydrate({ state: 'recording', generation: 7, error: null }); await h.settle(100);
  expect(h.get('.pill-root').dataset.state).toBe('listening');
  expect(h.get('[data-key="mic_silent_live:7"]')).not.toBeNull();
});

it('releases buffered and subsequent feedback after bounded, never-settling hydration', async () => {
  h = pillHarness();
  h.invoke.mockImplementation(async (command, args?: { epoch?: number }) => {
    if (command === 'pill_feedback_ready') {
      if (args?.epoch !== undefined) {
        h.emit('dictation-recovery', { ...kept, renderer_epoch: 1 });
        h.emit('dictation-blocked', { generation: 7, kind: 'mic_missing', action: 'choose_mic', renderer_epoch: 1 });
      }
      return 1;
    }
    if (command === 'get_current_recording_state') return new Promise(() => {});
    return null;
  });
  await h.settle(1799);
  expect(h.get('.feedback-card')).toBeNull();
  await h.settle(1);
  expect(h.get('[data-key="recovery:kept"]')).not.toBeNull();
  expect(h.get('[data-key="blocked:mic_missing"]')).not.toBeNull();
  expect(h.invoke).toHaveBeenCalledWith('pill_feedback_visible', { visible: true, epoch: 1 });
  h.emit('dictation-recovery', { ...kept, id: 'later', renderer_epoch: 1 }); await h.settle(0);
  expect(h.get('[data-key="recovery:later"]')).not.toBeNull();
});

it('releases on an authoritative event first and never reapplies the batch or late hydration', async () => {
  h = pillHarness();
  let hydrate!: (state: { state: string; generation: number; error: null }) => void;
  h.invoke.mockImplementation(async (command, args?: { epoch?: number }) => {
    if (command === 'pill_feedback_ready') {
      if (args?.epoch !== undefined) {
        h.emit('dictation-recovery', { ...kept, renderer_epoch: 1 });
        h.emit('dictation-note', { generation: 7, kind: 'mic_silent_live', renderer_epoch: 1 });
      }
      return 1;
    }
    if (command === 'get_current_recording_state') return new Promise(resolve => { hydrate = resolve; });
    return null;
  });
  await h.settle(0);
  h.emit('recording-state-changed', { state: 'recording', generation: 7, error: null }); await h.settle(0);
  expect(h.get('[data-key="recovery:kept"]')).not.toBeNull();
  expect(h.get('[data-key="mic_silent_live:7"]')).not.toBeNull();
  hydrate({ state: 'idle', generation: 7, error: null }); await h.settle(2000);
  expect(h.get('.pill-root').dataset.state).toBe('listening');
  expect(h.get('[data-key="mic_silent_live:7"]')).not.toBeNull();
  expect(h.root.textContent).not.toContain('×2');
});

it('uses a newer stop event instead of stale Recording hydration before replay', async () => {
  h = pillHarness();
  let hydrate!: (state: { state: string; generation: number; error: null }) => void;
  h.invoke.mockImplementation(async (command, args?: { epoch?: number }) => {
    if (command === 'pill_feedback_ready') {
      if (args?.epoch !== undefined) h.emit('dictation-note', { generation: 7, kind: 'mic_silent_live', renderer_epoch: 1 });
      return 1;
    }
    if (command === 'get_current_recording_state') return new Promise(resolve => { hydrate = resolve; });
    return null;
  });
  await h.settle(0);
  h.emit('recording-state-changed', { state: 'idle', generation: 7, error: null });
  hydrate({ state: 'recording', generation: 7, error: null }); await h.settle(100);
  expect(h.get('[data-key="mic_silent_live:7"]')).toBeNull();
  expect(h.get('.pill-root').dataset.state).toBe('idle');
});

it('rejects old-epoch publications and removals after replacement readiness', async () => {
  h = pillHarness();
  h.invoke.mockImplementation(async command => command === 'pill_feedback_ready' ? 2 : null);
  await h.settle(0);
  h.emit('dictation-recovery', { ...kept, renderer_epoch: 2 });
  h.emit('kept-dictation-expired', { id: kept.id, renderer_epoch: 1 });
  h.emit('dictation-blocked', { generation: 7, kind: 'mic_missing', action: 'choose_mic', renderer_epoch: 1 });
  await h.settle(500);
  expect(h.get('[data-key="recovery:kept"]')).not.toBeNull();
  expect(h.root.textContent).not.toContain('No microphone');
  h.emit('kept-dictation-expired', { id: kept.id, renderer_epoch: 2 });
  await h.settle(500);
  expect(h.get('.feedback-card')).toBeNull();
});

it('does not apply buffered feedback or report ownership after destruction', async () => {
  h = pillHarness();
  let ready!: (epoch: number) => void;
  h.invoke.mockImplementation(async (command, args?: { epoch?: number }) => command === 'pill_feedback_ready' && args?.epoch === undefined
    ? new Promise<number>(resolve => { ready = resolve; }) : null);
  await h.settle(0);
  h.emit('dictation-recovery', { ...kept, renderer_epoch: 1 });
  h.controller.destroy(); h.invoke.mockClear();
  ready(1); await h.settle(500);
  expect(h.invoke.mock.calls.filter(([command]) => command === 'pill_feedback_visible')).toEqual([]);
  expect(h.invoke.mock.calls.filter(([command]) => command === 'pill_feedback_ready')).toEqual([]);
});
