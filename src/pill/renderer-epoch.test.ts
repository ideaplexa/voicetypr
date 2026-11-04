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
