import { afterEach, expect, it } from 'vitest';
import { pillHarness } from '@/pill/test-harness';
import type { DictationRecovery } from '@/types/island-events';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const recovery = (kind: DictationRecovery['kind'] = 'no_speech'): DictationRecovery => ({
  generation:1,id:'kept',kind,engine_short:'Local',alt_engine_short:'Cloud',alt_engine_id:'cloud',expires_in_ms:30000,
});
const state = (state: string, generation = 2) => h.emit('recording-state-changed',{state,generation,error:null});

it.each(['no_speech','cloud_failed','network_offline','model_missing','remote_offline','integrity'] as const)('keeps queued %s recovery after a newer Starting overtakes readiness', async kind => {
  h=pillHarness();
  let flush: (() => void) | undefined;
  h.invoke.mockImplementation(async command => {
    if(command==='pill_feedback_ready') return new Promise(resolve => {flush=()=>{h.emit('dictation-recovery',recovery(kind));resolve(null);};});
    if(command==='get_settings')return {pill_indicator_mode:'always'};
    return null;
  });
  await h.settle(0); expect(flush).toBeDefined();
  state('starting'); flush!(); await h.settle(100);
  expect(h.get('.pill-root').dataset.state).toBe('listening');
  expect(h.get('.pill-root').dataset.starting).toBe('true');
  const card=h.get('[data-key="recovery:kept"]');
  expect(card.querySelector('.act')?.textContent).toBe(kind==='no_speech'?'Transcribe anyway':'Retry with Cloud');
  h.context({generation:2}); state('recording'); await h.settle(1050);
  expect(h.get('.pill-timer').textContent).toBe('0:01');
  state('idle'); await h.settle(1200);
  expect(card.hidden).toBe(false);
  (card.querySelector('.act') as HTMLButtonElement).click(); await h.settle(0);
  expect(h.invoke).toHaveBeenCalledWith(kind==='no_speech'?'transcribe_anyway':'retry_kept_dictation',kind==='no_speech'?{id:'kept'}:{id:'kept',engine:'cloud'});
});

it('does not replace the newer take processing state when an older clip arrives', async () => {
  h=pillHarness(); await h.settle(0);
  state('starting'); state('recording'); state('transcribing');
  h.emit('dictation-recovery',recovery()); await h.settle(100);
  expect(h.get('.pill-root').dataset.state).toBe('transcribing');
  h.context({generation:2}); // the recovery cannot advance or reset the take watermark
  expect(h.get('.pill-root').dataset.state).toBe('transcribing');
});

it('uses clip expiry and explicit id retirement rather than generation to retire retained actions', async () => {
  h=pillHarness(); await h.settle(0); state('starting');
  h.emit('dictation-recovery',{...recovery(),expires_in_ms:0});
  h.emit('dictation-recovery',{...recovery(),id:''});
  h.emit('dictation-recovery',recovery('mic_dropped_empty'));
  expect(h.get('.feedback-card')).toBeNull();
  h.emit('dictation-recovery',{...recovery(),expires_in_ms:100});
  await h.settle(250); expect(h.get('.feedback-card')).toBeNull();
  h.emit('dictation-recovery',recovery());
  h.emit('kept-dictation-expired',{id:'kept'});
  await h.settle(250); expect(h.get('.feedback-card')).toBeNull();
});
