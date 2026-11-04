import { afterEach, expect, it, vi } from 'vitest';
import type { RecordingStatePayload } from '@/pill/contracts';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const warning = { generation: 7, kind: 'mic_silent_live', renderer_epoch: 3 };
const recovery = {
  generation: 6, id: 'kept', kind: 'cloud_failed', engine_short: 'Whisper',
  alt_engine_short: null, alt_engine_id: null, expires_in_ms: 10_000, renderer_epoch: 3,
};
const visibility = () => h.invoke.mock.calls.filter(([command]) => command === 'pill_feedback_visible');
const reports = (epoch: number, ...values: boolean[]) => values.map(visible => ['pill_feedback_visible', { visible, epoch }]);
const recording = (generation = 7): RecordingStatePayload => ({ state: 'recording', generation, error: null });
function replacement(retry: () => Promise<RecordingStatePayload> = async () => { throw new Error('Unavailable'); }, ttl?: number) {
  h = pillHarness();
  let reads = 0;
  h.invoke.mockImplementation(async (command, args?: { epoch?: number }) => {
    if (command === 'pill_feedback_ready') {
      if (args?.epoch !== undefined) {
        if (ttl !== undefined) h.emit('dictation-recovery', { ...recovery, expires_in_ms: ttl });
        h.emit('dictation-note', warning);
      }
      return 3;
    }
    if (command === 'get_current_recording_state') {
      if (++reads === 1) throw new Error('Transient startup failure');
      return retry();
    }
    return null;
  });
  return () => reads;
}

it.each(['expiry', 'removal', 'dismissal'])('does not release when the last recovery disappears by %s while a warning is held', async removal => {
  replacement(undefined, removal === 'expiry' ? 150 : 10_000); await h.settle(0);
  expect(visibility()).toEqual(reports(3, true)); // The epoch binds to all sources, not an empty intermediate state.
  if (removal === 'removal') h.emit('kept-dictation-expired', { id: recovery.id, renderer_epoch: 3 });
  if (removal === 'dismissal') h.get<HTMLButtonElement>('.card-dismiss').click();
  await h.settle(250);
  expect(h.get('[data-key="recovery:kept"]')).toBeNull();
  expect(visibility()).toEqual(reports(3, true));
  await h.settle(1750);
  expect(visibility()).toEqual(reports(3, true, false));
});

it('does not release when a held warning is withdrawn but a recovery remains', async () => {
  replacement(undefined, 10_000); await h.settle(0);
  h.emit('dictation-note-withdraw', warning); await h.settle(500);
  expect(visibility()).toEqual(reports(3, true));
  h.emit('kept-dictation-expired', { id: recovery.id, renderer_epoch: 3 }); await h.settle(250);
  expect(visibility()).toEqual(reports(3, true, false));
});

it('reports only transitions across repeated callbacks, held replacement and promotion', async () => {
  replacement(); await h.settle(0);
  h.emit('dictation-note', { ...warning, generation: 8 });
  h.emit('dictation-note', warning); // Older than the held item.
  h.emit('recording-state-changed', recording(8));
  h.emit('recording-state-changed', recording(8));
  await h.settle(0);
  expect(h.get('[data-key="mic_silent_live:8"]')).not.toBeNull();
  expect(visibility()).toEqual(reports(3, true));
  h.emit('dictation-note-withdraw', warning); await h.settle(250);
  expect(visibility()).toEqual(reports(3, true));
  h.emit('dictation-note-withdraw', { ...warning, generation: 8 }); await h.settle(250);
  h.emit('dictation-note-withdraw', { ...warning, generation: 8 });
  h.emit('recording-state-changed', recording(8)); await h.settle(1000);
  expect(visibility()).toEqual(reports(3, true, false));
});

it.each([
  { state: 'idle', generation: 7 }, { state: 'transcribing', generation: 7 }, { state: 'recording', generation: 8 },
] as const)('drops an obsolete held warning on the one retry: $state / $generation', async state => {
  const reads = replacement(async () => ({ ...state, error: null }));
  await h.settle(499);
  expect(reads()).toBe(1);
  expect(visibility()).toEqual(reports(3, true));
  await h.settle(1);
  expect(h.get('[data-key="mic_silent_live:7"]')).toBeNull();
  expect(h.get('.pill-root').dataset.state).toBe('idle');
  expect(visibility()).toEqual(reports(3, true, false));
  await h.settle(5000); expect(reads()).toBe(2);
});

it('does not let an older retry snapshot discard a newer held warning', async () => {
  replacement(async () => recording(6)); await h.settle(500);
  expect(visibility()).toEqual(reports(3, true));
  await h.settle(1500);
  expect(visibility()).toEqual(reports(3, true, false));
});

it.each(['withdrawal', 'new take', 'idle', 'expiry'])('ignores a pending retry after held-warning %s', async discard => {
  let resolve!: (state: RecordingStatePayload) => void;
  const reads = replacement(() => new Promise(done => { resolve = done; }));
  await h.settle(500);
  if (discard === 'withdrawal') h.emit('dictation-note-withdraw', warning);
  if (discard === 'new take') h.emit('recording-state-changed', recording(8));
  if (discard === 'idle') h.emit('recording-state-changed', { state: 'idle', generation: 7, error: null });
  if (discard === 'expiry') await h.settle(1500);
  await h.settle(0);
  const state = h.get('.pill-root').dataset.state;
  resolve(recording()); await h.settle(0);
  expect(h.get('.pill-root').dataset.state).toBe(state);
  expect(h.get('[data-key="mic_silent_live:7"]')).toBeNull();
  expect(visibility()).toEqual(reports(3, true, false));
  expect(reads()).toBe(2);
});

it('recovers the missed Recording warning on retry without reopening state after recovery handoff', async () => {
  const reads = replacement(async () => recording(), 150); await h.settle(500);
  expect(reads()).toBe(2);
  expect(h.get('[data-key="recovery:kept"]')).toBeNull();
  expect(h.get('[data-key="mic_silent_live:7"]')).not.toBeNull();
  expect(h.get('.pill-root').dataset.state).toBe('idle');
  expect(visibility()).toEqual(reports(3, true));
  h.emit('dictation-note-withdraw', warning); await h.settle(250);
  expect(visibility()).toEqual(reports(3, true, false));
});

it.each(['held', 'card', 'terminal', 'empty'])('reconciles %s feedback on teardown exactly once', async source => {
  if (source === 'held') replacement();
  else h = pillHarness();
  await h.settle(0);
  const epoch = source === 'held' ? 3 : 0;
  if (source === 'card') h.emit('dictation-recovery', { ...recovery, renderer_epoch: epoch });
  if (source === 'terminal') h.emit('paste-outcome', { outcome: 'pasted', generation: 7, words: 1, polished: false });
  await h.settle(0); h.invoke.mockClear();
  h.controller.destroy(); await h.settle(0);
  h.controller.destroy(); await h.settle(5000);
  expect(visibility()).toEqual(source === 'empty' ? [] : reports(epoch, false));
  expect(vi.getTimerCount()).toBe(0);
});

it('serializes teardown release after an in-flight visibility acquisition', async () => {
  h = pillHarness();
  let acknowledge!: (value: null) => void;
  h.invoke.mockImplementation(async (command, args?: { epoch?: number; visible?: boolean }) => {
    if (command === 'pill_feedback_ready') {
      if (args?.epoch !== undefined) h.emit('dictation-note', warning);
      return 3;
    }
    if (command === 'get_current_recording_state') throw new Error('Unavailable');
    if (command === 'pill_feedback_visible' && args?.visible) return new Promise(resolve => { acknowledge = resolve; });
    return null;
  });
  await h.settle(0);
  h.controller.destroy(); await h.settle(0);
  expect(visibility()).toEqual(reports(3, true));
  acknowledge(null); await h.settle(0);
  expect(visibility()).toEqual(reports(3, true, false));
});
