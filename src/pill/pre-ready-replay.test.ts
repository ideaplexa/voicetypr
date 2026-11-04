import { afterEach, expect, it } from 'vitest';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const state = (state: string) => h.emit('recording-state-changed', { state, generation: 7, error: null });
const kept = {
  generation: 6, id: 'kept', kind: 'cloud_failed',
  engine_short: 'Whisper', alt_engine_short: 'Parakeet',
  alt_engine_id: 'parakeet-tdt-0.6b-v3', expires_in_ms: 600_000,
};

// Rust's outbox tests pause after taking a batch, race each operation against
// it, and prove these wire orders. The renderer deliberately has no tombstone
// for a withdrawn live warning: a later warning in the SAME take is legitimate.
it.each(['discard', 'retry success', 'expiry'])(
  'does not leave a retry action after %s races captured pre-ready recovery',
  async () => {
    h = pillHarness(); await h.settle(0);
    h.emit('dictation-recovery', kept); // captured batch must be emitted first
    h.emit('kept-dictation-expired', { id: kept.id }); // racing removal is the next batch
    await h.settle(1000);
    expect(h.get('[data-key="recovery:kept"]')).toBeNull();
    expect(h.get('.feedback-card')).toBeNull();
  },
);

it('withdraws the captured warning, but accepts a genuinely newer warning in the same take', async () => {
  h = pillHarness(); await h.settle(0);
  h.context({ generation: 7 }); state('starting'); state('recording');
  const warning = { generation: 7, kind: 'mic_silent_live' };
  h.emit('dictation-note', warning); // first batch
  h.emit('dictation-note-withdraw', warning); // withdrawal raced the first emit
  await h.settle(200);
  expect(h.get('[data-key="mic_silent_live:7"]')).toBeNull();
  h.emit('dictation-note', warning); // silence recurs: not stale replay
  await h.settle(200);
  expect(h.get('[data-key="mic_silent_live:7"]')).not.toBeNull();
  h.emit('dictation-note-withdraw', warning); await h.settle(200);
  expect(h.get('.feedback-card')).toBeNull();
});

it('keeps the newer recovery actions when publication races captured replay', async () => {
  h = pillHarness(); await h.settle(0);
  h.emit('dictation-recovery', kept);
  h.emit('dictation-recovery', {
    ...kept, generation: 7, alt_engine_short: 'Whisper Large', alt_engine_id: 'large-v3',
  });
  await h.settle(1000);
  const card = h.get('[data-key="recovery:kept"]');
  expect(card.textContent).toContain('Retry with Whisper Large');
  expect(card.textContent).not.toContain('Retry with Parakeet');
});

it('does not acquire frontend ownership for a clear-only replay', async () => {
  h = pillHarness({ pill_indicator_mode: 'when_recording' }); await h.settle(0);
  h.invoke.mockClear();
  h.emit('island-notice-clear', { kind: 'long_silence' });
  await h.settle(500);
  expect(h.get('.feedback-card')).toBeNull();
  expect(h.get('.pill-surface').hidden).toBe(true);
  expect(h.invoke.mock.calls.filter(([command]) => command === 'pill_feedback_visible')).toEqual([]);
});
