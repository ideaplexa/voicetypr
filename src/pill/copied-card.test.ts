import { afterEach, expect, it } from 'vitest';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const state = (state: string, generation: number) => h.emit('recording-state-changed', { state, generation, error: null });
const outcome = (outcome: string, generation: number) => h.emit('paste-outcome', { outcome, generation, words: 3, polished: false });
const card = (key: string) => h.get(`[data-key="${key}"]`);
async function copied(kind = 'copied') {
  h = pillHarness(); await h.settle(0); h.context({ generation: 1 });
  state('recording', 1); state('transcribing', 1); outcome(kind, 1);
  await h.settle(3000);
  expect(card(kind).hidden).toBe(false);
}

it.each(['copied', 'no_permission'])('removes the sticky %s card once a later dictation pastes', async kind => {
  await copied(kind);
  state('recording', 2); state('transcribing', 2); outcome('pasted', 2);
  await h.settle(3000);
  expect(card(kind)).toBeNull();
  expect(h.get('.pill-root').dataset.state).toBe('idle');
});

it('removes the card when the paste lands behind a newer recording', async () => {
  await copied();
  state('recording', 2); state('transcribing', 2); state('recording', 3); outcome('pasted', 2);
  await h.settle(500);
  expect(card('copied')).toBeNull();
  expect(h.get('.pill-root').dataset.state).toBe('listening');
});

it('keeps the card when the paste belongs to an earlier dictation or the next one is cancelled', async () => {
  await copied();
  outcome('pasted', 0);
  state('recording', 2); state('idle', 2);
  await h.settle(3000);
  expect(card('copied').hidden).toBe(false);
});
