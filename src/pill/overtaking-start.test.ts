import { afterEach, expect, it } from 'vitest';
import { IslandMachine } from '@/pill/island';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const state = (state: string, generation: number) => h.emit('recording-state-changed',{state,generation,error:null});

it.each(['pending','cancelled','live','recording_over_pending','recording_over_live'])('lets take 2 supersede take 1 (%s) without accepting its delayed idle or warnings', async scenario => {
  h=pillHarness(); await h.settle(0);
  state('starting',1);
  if(scenario==='live'||scenario==='recording_over_live')state('recording',1);
  h.emit('dictation-note',{generation:1,kind:'mic_silent_live'}); await h.settle(100);
  if(scenario==='cancelled')state('idle',1);
  const direct=scenario.startsWith('recording_over');
  state(direct?'recording':'starting',2);
  state('idle',1); // the old cancellation arrives after the new start
  if(!direct){
    expect(h.get('.pill-root').dataset.starting).toBe('true');
    state('recording',2);
  }
  expect(h.get('.pill-root').dataset.starting).toBe('false');
  h.context({generation:2});
  h.emit('dictation-note',{generation:2,kind:'mic_silent_live'});
  h.emit('dictation-note',{generation:1,kind:'mic_silent_live'});
  await h.settle(1050);
  expect(h.get('.pill-timer').textContent).toBe('0:01');
  expect(h.get('[data-key="mic_silent_live:2"]').hidden).toBe(false);
  expect(h.get('[data-key="mic_silent_live:1"]')).toBeNull();
});

it('updates the active generation on overtaking Starting, deduping only the current take', () => {
  const m=new IslandMachine();
  expect(m.arm(0,1)).toBe(true);
  expect(m.arm(100,2)).toBe(true); expect(m.activeTakeGeneration).toBe(2);
  expect(m.arm(200,2)).toBe(false); expect(m.startedAt).toBe(100);
  expect(m.arm(300,1)).toBe(false); expect(m.observeGeneration(1)).toBe(false);
  expect(m.start(400,2)).toBe(true); expect(m.pending).toBe(false);
  expect(m.start(500,2)).toBe(false); expect(m.startedAt).toBe(400);
});

it('scopes a new take Escape hint before delayed context arrives', async () => {
  h=pillHarness(); await h.settle(0); h.context({generation:1});
  state('starting',1); state('starting',2); state('recording',2);
  h.emit('escape-hint',{generation:2,phase:'recording'}); await h.settle(100);
  expect(h.get('[data-key="escape_hint:2"]')).not.toBeNull();
  state('stopping',2); await h.settle(100);
  expect(h.get('[data-key="escape_hint:2"] .card-title').textContent).toBe('Press Esc again to discard');
});
