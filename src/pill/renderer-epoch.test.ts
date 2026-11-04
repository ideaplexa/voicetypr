import { afterEach, expect, it } from 'vitest';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const kept = {
  generation: 6, id: 'kept', kind: 'cloud_failed', engine_short: 'Whisper',
  alt_engine_short: 'Parakeet', alt_engine_id: 'parakeet-tdt-0.6b-v3', expires_in_ms: 600_000,
};

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
