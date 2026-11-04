import { afterEach, expect, it } from 'vitest';
import { pillHarness } from '@/pill/test-harness';
import { FeedbackStack } from '@/pill/stack';
import { blockedCard, noteCard, recoveryCard } from '@/pill/feedback';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const state = (state: string, generation: number) => h.emit('recording-state-changed', {state, generation, error:null});

it.each(['mic_silent', 'mic_dropped', 'mic_dropped_empty'] as const)('retires finished-take %s when a new take begins', async kind => {
  h = pillHarness(); await h.settle(0); h.context({generation:1});
  if (kind === 'mic_dropped_empty') h.emit('dictation-recovery', {generation:1,id:'empty',kind,expires_in_ms:30000});
  else h.emit('dictation-note', {generation:1,kind,captured_ms:42000});
  await h.settle(1000);
  state('starting',2); h.context({generation:2}); state('recording',2); await h.settle(5000);
  state('idle',2); await h.settle(1200);
  expect(h.get('.feedback-card')).toBeNull();
});

it.each(['mic_permission_denied', 'mic_missing', 'mic_busy'] as const)('resolves %s only after the mic actually opens', async kind => {
  h = pillHarness(); await h.settle(0);
  h.emit('dictation-blocked', {generation:1,kind,action:'choose_mic'}); await h.settle(1000);
  state('starting',2); await h.settle(100);
  expect(h.get(`[data-key="blocked:${kind}"]`)).not.toBeNull();
  h.context({generation:2}); state('recording',2); await h.settle(1000);
  state('idle',2); await h.settle(1200);
  expect(h.get('.feedback-card')).toBeNull();
});

it('retires only mic status, keeping retry and no-speech leases intact', () => {
  const s = new FeedbackStack();
  const recovery = {generation:1,id:'kept',kind:'integrity' as const,engine_short:'Local',alt_engine_short:'Cloud',alt_engine_id:'cloud',expires_in_ms:30000};
  s.push(recoveryCard(recovery,0),0);
  s.push(recoveryCard({...recovery,id:'speech',kind:'no_speech'},0),0);
  s.push(recoveryCard({...recovery,id:'empty',kind:'mic_dropped_empty'},0),0);
  s.push(blockedCard({generation:1,kind:'trial_ended',action:'open_license'},true),0);
  s.push(blockedCard({generation:1,kind:'mic_busy',action:'choose_mic'},true),0);
  s.push(noteCard({generation:1,kind:'mic_silent'}),0);
  s.resolveMicStatus('new_take',2); s.resolveMicStatus('recording',2);
  expect(s.items.map(i=>i.key)).toEqual(['recovery:kept','recovery:speech','blocked:trial_ended']);
  expect(s.items[0].actions.map(a=>a.call.command)).toEqual(['retry_kept_dictation']);
  expect(s.items[1].actions[0].call.command).toBe('transcribe_anyway');
  expect(s.items[0].expiresAt).toBe(30000);
});
