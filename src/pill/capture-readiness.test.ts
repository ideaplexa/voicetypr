import { afterEach, expect, it } from 'vitest';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const state = (state: string, generation: number) => h.emit('recording-state-changed',{state,generation,error:null});
const blocker = () => h.get('[data-key="blocked:mic_missing"]');

it('does not resolve a mic blocker for kept-file retry Recording, but does for ready capture', async () => {
  h=pillHarness(); await h.settle(0);
  h.context({generation:1}); // even a healthy old capture is not evidence for the file retry
  h.emit('dictation-blocked',{generation:1,kind:'mic_missing',action:'choose_mic'}); await h.settle(1000);
  for(const phase of ['starting','recording','stopping','transcribing'])state(phase,2);
  await h.settle(1000);
  expect(blocker()).not.toBeNull();
  state('idle',2); await h.settle(1000);
  expect(blocker().hidden).toBe(false);
  state('starting',3);
  h.context({generation:3});
  await h.settle(100); expect(blocker()).not.toBeNull(); // context cannot confirm Recording while pending
  state('recording',3); await h.settle(300);
  expect(blocker()).toBeNull();
});

it.each([true,false])('resolves only healthy, matching context received after Recording (mic ok=%s)', async ok => {
  h=pillHarness(); await h.settle(0);
  h.emit('dictation-blocked',{generation:1,kind:'mic_missing',action:'choose_mic'}); await h.settle(1000);
  state('starting',2); state('recording',2); await h.settle(200);
  expect(blocker()).not.toBeNull();
  h.context({generation:1}); await h.settle(100);
  expect(blocker()).not.toBeNull();
  h.context({generation:2,mic:{name:'Microphone',tooltip:'Microphone',ok}});
  await h.settle(300);
  expect(blocker()===null).toBe(ok);
});

it('resolves a ready capture even when its matching context arrives after the take stops', async () => {
  h=pillHarness(); await h.settle(0);
  h.emit('dictation-blocked',{generation:1,kind:'mic_missing',action:'choose_mic'}); await h.settle(1000);
  state('starting',2); state('recording',2); state('idle',2);
  h.context({generation:2}); await h.settle(1000);
  expect(blocker()).toBeNull();
});
