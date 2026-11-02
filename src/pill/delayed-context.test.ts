import { afterEach, expect, it } from 'vitest';
import { IslandMachine } from '@/pill/island';
import { emptyContext } from '@/pill/contracts';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const state = (state: string) => h.emit('recording-state-changed',{state,generation:1,error:null});

it('hydrates delayed start context without bouncing live words through the start layer', async () => {
  h=pillHarness({transcription_mode:'live_preview'}); await h.settle(0);
  state('starting'); state('recording');
  h.emit('transcription-stream',{type:'partial',session_id:1,revision:1,committed:'Live words',tentative:''});
  await h.settle(100);
  expect(h.get('.pill-root').dataset.state).toBe('live');
  h.context({generation:1,show_start_card:true,engine:{short_name:'Hydrated',kind:'local'}});
  expect(h.get('.engine-chip').textContent).toBe('Hydrated');
  for(let frame=0;frame<70;frame++){
    expect(h.get('.pill-root').dataset.state).toBe('live');
    expect(h.get('[data-layer="start"]').hidden).toBe(true);
    await h.settle(16);
  }
});

it('still opens normal listening context and folds its start details after 800 ms', async () => {
  h=pillHarness(); await h.settle(0);
  state('starting'); state('recording');
  h.context({generation:1,show_start_card:true});
  expect(h.get('.pill-root').dataset.state).toBe('start');
  await h.settle(799); expect(h.get('.pill-root').dataset.state).toBe('start');
  await h.settle(1); expect(h.get('.pill-root').dataset.state).toBe('listening');
});

it('does not reopen details when live words already appeared and then cleared', () => {
  const m=new IslandMachine(); m.arm(0,1); m.start(10,1);
  m.stream({type:'partial',session_id:1,revision:1,committed:'',tentative:'words'});
  m.stream({type:'cancelled',session_id:1,revision:2});
  expect(m.state).toBe('listening');
  expect(m.contextEvent({...emptyContext(),generation:1,show_start_card:true})).toBe(true);
  expect(m.state).toBe('listening');
});

it.each(['transcribing','polishing','rest'] as const)('hydrates context after leaving listening for %s without reopening', state => {
  const m=new IslandMachine(); m.arm(0,1); m.start(10,1);
  if(state==='rest')m.rest();else m.work(state,100);
  expect(m.contextEvent({...emptyContext(),generation:1,show_start_card:true})).toBe(true);
  expect(m.state).toBe(state);
});
