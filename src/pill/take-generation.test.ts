import { afterEach, expect, it } from 'vitest';
import { IslandMachine } from '@/pill/island';
import { emptyContext } from '@/pill/contracts';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const state = (state: string, generation: number) => h.emit('recording-state-changed', {state,generation,error:null});

it('rejects the old take warning in the Starting/context gap, accepting and withdrawing only the active take', async () => {
  h = pillHarness(); await h.settle(0);
  h.context({generation:1}); state('starting',1); state('recording',1); state('idle',1); await h.settle(1000);
  state('starting',2);
  h.emit('dictation-note',{generation:1,kind:'mic_silent_live'});
  h.emit('dictation-note',{generation:2,kind:'mic_silent_live'});
  await h.settle(100);
  expect(h.get('[data-key="mic_silent_live:1"]')).toBeNull();
  expect(h.get('[data-key="mic_silent_live:2"]')).not.toBeNull();
  h.context({generation:1,show_start_card:true});
  expect(h.get('.pill-root').dataset.state).toBe('listening');
  h.context({generation:2,show_start_card:true}); state('recording',2);
  expect(h.get('.pill-root').dataset.state).toBe('start');
  h.emit('dictation-note-withdraw',{generation:1,kind:'mic_silent_live'}); await h.settle(200);
  expect(h.get('[data-key="mic_silent_live:2"]')).not.toBeNull();
  h.emit('dictation-note-withdraw',{generation:2,kind:'mic_silent_live'}); await h.settle(200);
  expect(h.get('.feedback-card')).toBeNull();
});

it('keeps state generation separate so equal-generation context can hydrate once', () => {
  const m = new IslandMachine();
  m.contextEvent({...emptyContext(),generation:1});
  m.arm(0,2);
  expect(m.takeGeneration).toBe(2); expect(m.generation).toBe(1);
  expect(m.observeGeneration(1)).toBe(false);
  expect(m.start(100,1)).toBe(false); expect(m.pending).toBe(true);
  expect(m.contextEvent({...emptyContext(),generation:2,show_start_card:true})).toBe(true);
  expect(m.contextEvent({...emptyContext(),generation:2})).toBe(false);
  expect(m.start(200,2)).toBe(true);
});
