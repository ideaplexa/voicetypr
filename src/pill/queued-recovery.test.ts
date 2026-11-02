import { afterEach, expect, it } from 'vitest';
import { pillHarness } from '@/pill/test-harness';
import type { DictationRecovery } from '@/types/island-events';
import { IslandMachine } from '@/pill/island';

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

it.each(['stopping','transcribing'] as const)('keeps cold-renderer %s state when an older recovery flushes at readiness', async phase => {
  h=pillHarness();
  let flush: (() => void) | undefined;
  h.invoke.mockImplementation(async command => {
    if(command==='pill_feedback_ready')return new Promise(resolve=>{flush=()=>{h.emit('dictation-recovery',recovery());resolve(null);};});
    if(command==='get_settings')return {pill_indicator_mode:'always'};
    return null;
  });
  await h.settle(0); expect(flush).toBeDefined();
  state(phase); // no Starting/Recording or context has been received
  flush!(); await h.settle(1000);
  expect(h.get('.pill-root').dataset.state).toBe('transcribing');
  expect(h.get('.pill-surface').style.width).toBe('236px');
  expect(h.get('[data-key="recovery:kept"] .act').textContent).toBe('Transcribe anyway');
  h.emit('dictation-recovery',recovery('mic_dropped_empty'));
  await h.settle(200);
  expect(h.get('.pill-root').dataset.state).toBe('transcribing');
  expect(h.get('[data-key="recovery:kept"] .card-title').textContent).toBe('No speech heard');
  state('idle'); await h.settle(1200);
  expect(h.get('[data-key="recovery:kept"]').hidden).toBe(false);
});

it.each(['idle','transcribing'] as const)('uses hydrated %s generation to reject old status while retaining old clip actions', async phase => {
  h=pillHarness();
  h.invoke.mockImplementation(async command => command==='get_current_recording_state'?{state:phase,generation:2,error:null}:command==='get_settings'?{pill_indicator_mode:'always'}:null);
  await h.settle(0);
  h.emit('dictation-recovery',recovery('mic_dropped_empty'));
  expect(h.get('.feedback-card')).toBeNull();
  h.emit('dictation-recovery',recovery()); await h.settle(100);
  expect(h.get('.pill-root').dataset.state).toBe(phase==='idle'?'idle':'transcribing');
  expect(h.get('[data-key="recovery:kept"] .act').textContent).toBe('Transcribe anyway');
});

it('still hands off the current cold-renderer take to its recovery card', async () => {
  h=pillHarness(); await h.settle(0); state('transcribing');
  h.emit('dictation-recovery',{...recovery(),generation:2}); await h.settle(1200);
  expect(h.get('.pill-root').dataset.state).toBe('idle');
  expect(h.get('[data-key="recovery:kept"]').hidden).toBe(false);
});

it('tracks all accepted state generations independently of context and rejects regressions', () => {
  const machine=new IslandMachine();
  expect(machine.observeRecordingGeneration(2)).toBe(true);
  expect(machine.takeGeneration).toBe(0); expect(machine.latestRecordingGeneration).toBe(2);
  expect(machine.observeRecordingGeneration(1)).toBe(false);
  expect(machine.latestRecordingGeneration).toBe(2);
});
