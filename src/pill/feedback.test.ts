import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent } from '@testing-library/react';
import { FeedbackStack } from '@/pill/stack';
import { blockedCard, recoveryCard, noteCard, shortCard, noticeCard, noticeCopy, type FeedbackCard } from '@/pill/feedback';
import type { BlockedKind, IslandAction, RecoveryKind, DictationNote } from '@/types/island-events';
import { pillHarness } from '@/pill/test-harness';
const platform=vi.hoisted(()=>({mac:true}));
vi.mock('@/lib/platform',()=>({get isMacOS(){return platform.mac;}}));
const blocked: Record<BlockedKind,[IslandAction,string,string]> = {
  trial_ended:['open_license','Trial ended','Activate'],license_check_failed:['recheck_license',"Couldn't verify your license",'Recheck'],license_verify_required:['recheck_license',"Couldn't verify your license",'Recheck'],
  no_engine:['open_models','No voice engine set up','Set up'],cloud_key_missing:['open_cloud_keys','Cloud key missing','Fix key'],cloud_key_rejected:['open_cloud_keys','Cloud key rejected','Fix key'],soniox_storage_full:['open_storage','Soniox storage is full','Clean up'],
  mic_permission_denied:['open_mic_settings','Microphone access is off','Open System Settings'],mic_missing:['choose_mic','No microphone','Choose mic'],mic_busy:['choose_mic','Mic in use by another app','Choose mic'],accessibility_off:['open_accessibility','Allow Accessibility to paste automatically','Open System Settings'],starting_up:['open_models','Getting ready…',''],
};
const recoveries: RecoveryKind[]=['cloud_failed','network_offline','model_missing','remote_offline','integrity','no_speech','mic_dropped_empty'];
const notes: Array<[DictationNote,string]> = [
  [{generation:1,kind:'mic_dropped',captured_ms:42000},'Mic disconnected · using the 0:42 we got'],
  [{generation:1,kind:'storage_failed'},"Couldn't keep the recording"],
  [{generation:1,kind:'mic_silent'},'Your mic sent silence — is it muted?'],
  [{generation:1,kind:'translate_failed'},'Pasted untranslated · translation failed'],
  [{generation:1,kind:'model_fallback',engine_short:'Whisper',alt_engine_short:'Parakeet'},"Using Parakeet · Whisper isn't downloaded"],
  [{generation:1,kind:'gpu_fallback',engine_short:'Whisper'},'GPU unavailable · using the CPU'],
  ...(['timeout','rate_limited','network','guard','error'] as const).map((reason,i):[DictationNote,string]=>[{generation:1,kind:'polish_skipped',reason},`Pasted unpolished · ${['Polish timed out','Polish rate limited','Polish unavailable offline','Polish kept the original','Polish failed'][i]}`]),
];
const fixture=(key:string,duration:number|null=1000):FeedbackCard=>({key,title:key,sub:'',tone:'neutral',duration,actions:[]});
describe('feedback stack rules',()=>{
  it('shows newest first, at most three, and counts the overflow',()=>{const s=new FeedbackStack();for(let n=0;n<5;n++)s.push(fixture(String(n)),0);expect(s.visible.map(i=>i.key)).toEqual(['4','3','2']);expect(s.more).toBe(2);});
  it('retires timed items FIFO even when a newer short item runs out first',()=>{const s=new FeedbackStack();s.tuck(false,0);s.push(fixture('first',5000),0);s.push(fixture('second',100),0);s.advance(101);expect(s.items).toHaveLength(2);s.advance(5000);expect(s.items).toHaveLength(0);});
  it('sticky blockers stay while timed items retire',()=>{const s=new FeedbackStack();s.tuck(false,0);s.push(fixture('blocker',null),0);s.push(fixture('timed'),0);s.advance(10000);expect(s.items.map(i=>i.key)).toEqual(['blocker']);s.remove('blocker');expect(s.items).toHaveLength(0);});
  it('merges repeats, promotes them, and preserves FIFO serial order',()=>{const s=new FeedbackStack();s.tuck(false,0);s.push(fixture('a'),0);s.push(fixture('b'),0);s.push(fixture('a'),100);expect(s.visible[0].count).toBe(2);expect(s.visible[0].serial).toBe(1);expect(s.items).toHaveLength(2);});
  it('pauses clocks while tucked and resumes with the remaining duration',()=>{const s=new FeedbackStack();s.tuck(false,0);s.push(fixture('a'),0);s.tuck(true,400);s.advance(20000);expect(s.items[0].left).toBe(600);expect(s.nextDelay).toBeNull();s.tuck(false,20000);s.advance(20599);expect(s.items).toHaveLength(1);s.advance(20600);expect(s.items).toHaveLength(0);});
  it('real clip expiry continues even while tucked',()=>{const s=new FeedbackStack();s.push({...fixture('clip',null),expiresAt:30000},0);expect(s.nextDelay).toBe(30000);s.advance(30000);expect(s.items).toHaveLength(0);});
  it('dismissal of a recovery does not invoke audio deletion',()=>{const s=new FeedbackStack();s.push(fixture('clip',null),0);s.remove('clip');expect(s.items).toHaveLength(0);});
});
describe('complete event copy mapping',()=>{
  it.each(Object.entries(blocked))('maps blocked %s',(kind,[action,title,label])=>{const c=blockedCard({generation:1,kind:kind as BlockedKind,action},true);expect(c.title).toBe(title);expect(c.actions[0]?.label ?? '').toBe(label);expect(c.duration).toBe(kind==='starting_up'?3200:null);expect(c.tone).toBe(kind==='mic_busy'?'red':'amber');});
  it.each(recoveries)('maps recovery %s',kind=>{const c=recoveryCard({generation:1,id:'clip',kind,engine_short:'Soniox',alt_engine_short:'Parakeet',alt_engine_id:'parakeet-tdt-0.6b-v3',expires_in_ms:30000},10);expect(c.expiresAt).toBe(30010);if(kind==='no_speech'){expect(c.title).toBe('No speech heard');expect(c.sub).toBe('Audio kept for 30 s');expect(c.actions[0].label).toBe('Transcribe anyway');}else if(kind==='mic_dropped_empty'){expect(c.duration).toBe(3500);expect(c.actions).toHaveLength(0);}else{expect(c.title).toContain(' · Recording kept');expect(c.sub).toBe('Temporary audio · Manage in Home');expect(c.actions.map(a=>a.label)).toEqual(['Retry with Parakeet']);}});
  it.each(recoveries.filter(k=>!['no_speech','mic_dropped_empty'].includes(k)))('hides unavailable retry for %s',kind=>{expect(recoveryCard({generation:1,id:'clip',kind,engine_short:'Whisper',alt_engine_short:null,alt_engine_id:null,expires_in_ms:1000},0).actions).toEqual([]);});
  it.each(['integrity','no_speech'] as const)('does not offer %s retry before WAV finalization',kind=>{
    const c=recoveryCard({generation:1,id:'clip',kind,engine_short:'Parakeet',alt_engine_short:'Parakeet',alt_engine_id:'v3',expires_in_ms:30000,retry_ready:false},0);
    expect(c.actions).toEqual([]);expect(c.sub).toBe('Audio is still finishing…');expect(c.closeLabel).toBe('Close, recording kept');
  });
  it.each(notes)('maps note $0.kind',(note,copy)=>{expect(noteCard(note).title).toBe(copy);expect(noteCard(note).duration).not.toBeNull();});
  it.each(['hold','toggle'] as const)('uses %s too-short copy',mode=>{expect(shortCard({generation:1,mode}).title).toBe(`Too short — ${mode==='hold'?'hold':'talk'} a bit longer`);});
  it.each(Object.keys(noticeCopy) as Array<keyof typeof noticeCopy>)('maps enumerated notice %s',kind=>{expect(noticeCard({kind}).title).toBe(noticeCopy[kind]);});
  it.each(['mic_permission_denied','accessibility_off'] as const)('uses Windows privacy copy for %s',kind=>{expect(blockedCard({generation:1,kind,action:'open_mic_settings'},false).actions[0].label).toBe('Open Privacy settings');});
});
let h:ReturnType<typeof pillHarness>|undefined;
afterEach(()=>{h?.destroy();h=undefined;platform.mac=true;});
async function mount(mac=true){platform.mac=mac;h=pillHarness();await h.settle(0);return h;}
describe('feedback transport and actions',()=>{
  it('never releases sticky feedback ownership and requests native hide',async()=>{
    const h=await mount();h.emit('dictation-blocked',{generation:1,kind:'mic_missing',action:'choose_mic'});await h.settle(1500);
    expect(h.invoke).toHaveBeenCalledWith('pill_feedback_visible',{visible:true,epoch:0});
    h.invoke.mockImplementation(async cmd=>cmd==='get_settings'?{pill_indicator_mode:'never'}:null);
    h.emit('settings-changed');await h.settle(1000);
    expect(h.get('.pill-surface').hidden).toBe(true);
    expect(h.invoke.mock.calls.filter(([cmd])=>cmd==='pill_feedback_visible').slice(-1)[0]).toEqual(['pill_feedback_visible',{visible:false,epoch:0}]);
  });

  it.each(Object.entries(blocked).filter(([kind])=>kind!=='starting_up'))('calls the blocked %s action',async(kind,[action])=>{const h=await mount();h.emit('dictation-blocked',{generation:1,kind,action});await h.settle(1500);fireEvent.click(h.get('.card-actions .act'));await h.settle(0);expect(h.invoke).toHaveBeenCalledWith('island_action',{action});});
  it.each([true,false])('calls retained retry/transcribe on platform mac=%s',async mac=>{const h=await mount(mac);const recovery={generation:1,id:'clip',kind:'cloud_failed',engine_short:'Soniox',alt_engine_short:'Parakeet',alt_engine_id:'parakeet-tdt-0.6b-v3',expires_in_ms:600000};h.emit('dictation-recovery',recovery);await h.settle(1500);fireEvent.click(h.get('.card-actions .act'));await h.settle(0);expect(h.invoke).toHaveBeenCalledWith('retry_kept_dictation',{id:'clip',engine:'parakeet-tdt-0.6b-v3'});h.emit('dictation-recovery',{...recovery,kind:'no_speech'});await h.settle(1000);fireEvent.click(h.get('.card-actions .act'));await h.settle(0);expect(h.invoke).toHaveBeenCalledWith('transcribe_anyway',{id:'clip'});});
  it('dismisses a no-speech card without deleting audio',async()=>{const h=await mount();h.emit('dictation-recovery',{generation:1,id:'clip',kind:'no_speech',engine_short:'Soniox',alt_engine_short:null,alt_engine_id:null,expires_in_ms:10000});await h.settle(1500);fireEvent.click(h.get('.card-dismiss'));await h.settle(500);expect(h.root.querySelector('.feedback-card')).toBeNull();expect(h.invoke.mock.calls.some(([cmd])=>cmd==='discard_kept_dictation')).toBe(false);});
  it.each([true,false])('offers one removal control on retained failures, retry available=%s',async retry=>{
    const h=await mount();h.emit('dictation-recovery',{generation:1,id:'clip',kind:'cloud_failed',engine_short:'Soniox',alt_engine_short:retry?'Parakeet':null,alt_engine_id:retry?'parakeet':null,expires_in_ms:10000});await h.settle(1500);
    const card=h.get('.feedback-card');
    expect(card.textContent).not.toContain('Discard');
    const close=card.querySelector<HTMLButtonElement>('[aria-label="Close, recording kept"]')!;
    expect(card.querySelectorAll('button')).toHaveLength(retry?2:1);
    fireEvent.click(close);await h.settle(500);
    expect(h.invoke).not.toHaveBeenCalledWith('discard_kept_dictation',{id:'clip'});
    expect(h.root.querySelector('.feedback-card')).toBeNull();
  });
  it('removes expired ids by deadline and explicit expiry event',async()=>{const h=await mount();const e={generation:1,id:'clip',kind:'no_speech',engine_short:'Parakeet',alt_engine_short:null,alt_engine_id:null,expires_in_ms:30000};h.emit('dictation-recovery',e);await h.settle(30100);expect(h.root.querySelector('.feedback-card')).toBeNull();h.emit('dictation-recovery',{...e,id:'other'});await h.settle(1000);h.emit('kept-dictation-expired',{id:'other'});await h.settle(500);expect(h.root.querySelector('.feedback-card')).toBeNull();});
  it('tucks feedback throughout dictation, then resumes its clock',async()=>{const h=await mount();h.emit('island-notice',{kind:'polish_on'});await h.settle(1000);h.emit('recording-started');await h.settle(10000);expect(h.get('.feedback-stack').inert).toBe(true);expect(h.get('.card-title').textContent).toBe('Polish on');h.emit('recording-state-changed',{state:'idle',error:null});await h.settle(1600);expect(h.get('.feedback-stack').inert).toBe(false);await h.settle(5000);expect(h.root.querySelector('.feedback-card')).toBeNull();});
  it('reports card rectangles with the island and puts top stacks below it',async()=>{const h=await mount();h.emit('pill-geometry',{anchor:'top-center',anchorX:220,anchorY:6});h.emit('dictation-blocked',{generation:1,kind:'trial_ended',action:'open_license'});await h.settle(1500);expect(h.get('.feedback-stack').style.top).toBe('30px');expect(h.get('.feedback-stack').dataset.anchor).toBe('top-center');const calls=h.invoke.mock.calls.filter(([cmd])=>cmd==='pill_set_hit_regions') as unknown as Array<[string,{rects:unknown[]}]>;expect(calls[calls.length-1]?.[1].rects).toHaveLength(2);});
  it('keeps the live shape during the first frame of an island-to-card handoff',async()=>{const h=await mount();h.emit('transcription-started');await h.settle(1000);h.emit('dictation-recovery',{generation:1,id:'clip',kind:'cloud_failed',engine_short:'Soniox',alt_engine_short:'Parakeet',alt_engine_id:'parakeet-tdt-0.6b-v3',expires_in_ms:10000});expect(h.get('.pill-surface').style.width).toBe('236px');expect(h.get('.card-title').textContent).toBe('Soniox unreachable · Recording kept');await h.settle(2000);expect(h.get('.pill-surface').style.width).toBe('12px');expect(h.get('.feedback-stack').style.opacity).toBe('1');});
  it('merges repeats and shows three cards plus overflow',async()=>{const h=await mount();for(const kind of ['polish_on','polish_off','shortcuts_retired','polish_setup','polish_setup'])h.emit('island-notice',{kind});await h.settle(1000);expect(h.get('.stack-more').textContent).toBe('+1');expect(h.get('.card-count').textContent).toBe('×2');expect([...h.root.querySelectorAll<HTMLElement>('.feedback-card')].filter(e=>!e.hidden)).toHaveLength(3);});
  it('announces one condition once when a repeat merges',async()=>{const h=await mount();h.emit('island-notice',{kind:'polish_on'});const announcement=h.get('.pill-announcement');const spy=vi.spyOn(announcement,'textContent','set');h.emit('island-notice',{kind:'polish_on'});expect(spy).not.toHaveBeenCalled();});
  it('keeps blockers sticky through idle/error until dismissed and releases native ownership',async()=>{const h=await mount();h.emit('dictation-blocked',{generation:1,kind:'mic_busy',action:'choose_mic'});h.emit('recording-state-changed',{state:'error',error:'private detail'});await h.settle(20000);expect(h.get('.card-title').textContent).toBe('Mic in use by another app');expect(h.root.textContent).not.toContain('private detail');fireEvent.click(h.get('.card-dismiss'));await h.settle(500);expect(h.invoke).toHaveBeenCalledWith('pill_feedback_visible',{visible:false,epoch:0});});
  it('Original is hover-only, pauses the pasted clock and copies using the real command',async()=>{const h=await mount();h.invoke.mockImplementation(async(cmd:string):Promise<unknown>=>cmd==='copy_last_original'?'copied':null);h.emit('paste-outcome',{outcome:'pasted',words:38,polished:true});expect(h.get('.done-actions').hidden).toBe(true);await h.settle(1000);h.emit('pill-pointer',{inside:true});await h.settle(5000);expect(h.get('.done-actions').hidden).toBe(false);expect(h.get('.pill-root').dataset.state).toBe('pasted');fireEvent.click(h.get('.original'));await h.settle(0);expect(h.invoke).toHaveBeenCalledWith('copy_last_original');expect(h.get('.done-label').textContent).toBe('Original copied');expect(h.root.querySelector('.undo')).toBeNull();expect(h.root.querySelector('.retry')).toBeNull();h.emit('pill-pointer',{inside:false});await h.settle(1500);expect(h.get('.pill-root').dataset.state).toBe('idle');});
  it('hides Original for unchanged text and ignores stale problem generations',async()=>{const h=await mount();h.context({generation:2});h.emit('paste-outcome',{outcome:'pasted',words:1,polished:false});h.emit('pill-pointer',{inside:true});expect(h.get('.original').hidden).toBe(true);h.emit('dictation-blocked',{generation:1,kind:'mic_missing',action:'choose_mic'});expect(h.root.querySelector('.feedback-card')).toBeNull();});
  it('shows Finishing inline for 600 ms without queuing it behind the pasted outcome',async()=>{const h=await mount();h.emit('transcription-started');h.emit('island-notice',{kind:'finishing'});expect(h.get('.work-label').textContent).toBe('Finishing…');await h.settle(599);expect(h.get('.work-label').textContent).toBe('Finishing…');await h.settle(1);expect(h.get('.work-label').textContent).toBe('Transcribing');expect(h.root.querySelector('.feedback-card')).toBeNull();});
  it('announces a copied outcome once across handoff and keeps the panel through exit',async()=>{const h=await mount();h.invoke.mockClear();h.emit('paste-outcome',{outcome:'copied',words:2,polished:false});const spy=vi.spyOn(h.get('.pill-announcement'),'textContent','set');await h.settle(4000);expect(spy).not.toHaveBeenCalled();fireEvent.click(h.get('.card-dismiss'));expect(h.invoke).not.toHaveBeenCalledWith('pill_feedback_visible',{visible:false,epoch:0});await h.settle(500);expect(h.invoke).toHaveBeenCalledWith('pill_feedback_visible',{visible:false,epoch:0});});
  it('coalesces the permission blocker with its copied outcome, announcing it once',async()=>{const h=await mount();h.emit('paste-outcome',{outcome:'no_permission',words:2,polished:false});const spy=vi.spyOn(h.get('.pill-announcement'),'textContent','set');h.emit('dictation-blocked',{generation:1,kind:'accessibility_off',action:'open_accessibility'});await h.settle(4000);expect(h.root.querySelectorAll('.feedback-card')).toHaveLength(1);expect(spy).not.toHaveBeenCalled();expect(h.get('.card-title').textContent).toBe('Allow Accessibility to paste automatically');fireEvent.click(h.get('.card-actions .act'));await h.settle(0);expect(h.invoke).toHaveBeenCalledWith('island_action',{action:'open_accessibility'});});
  it('uses Choose mic from silence notes and cleans up every timer',async()=>{const h=await mount();h.emit('dictation-note',{generation:1,kind:'mic_silent'});await h.settle(1500);fireEvent.click(h.get('.card-actions .act'));await h.settle(0);expect(h.invoke).toHaveBeenCalledWith('island_action',{action:'choose_mic'});h.controller.destroy();await h.settle(10000);expect(vi.getTimerCount()).toBe(0);});
});


describe('feedback card layout',()=>{
  it.each([true,false])('keeps Pasted actions in their own hover row on mac=%s',async mac=>{
    const h=await mount(mac);
    h.emit('paste-outcome',{outcome:'pasted',words:38,polished:true});await h.settle(1000);
    const done=h.get('.done'),row=h.get('.done .result-row'),actions=h.get('.done-actions');
    expect([...done.children].slice(0,2)).toEqual([row,actions]);
    expect(row.contains(h.get('.done-count'))).toBe(true);
    expect(actions.contains(h.get('.original'))).toBe(true);
    expect(h.get('.pill-surface').style.height).toBe('34px');
    h.emit('pill-pointer',{inside:true});await h.settle(1000);
    expect(actions.hidden).toBe(false);expect(h.get('.pill-surface').style.height).toBe('70px');
    expect(h.get('.appico').style.top).toBe('7px');expect(h.get('.glyph').style.top).toBe('4px');
    h.emit('pill-pointer',{inside:false});await h.settle(500);
    expect(actions.hidden).toBe(true);expect(parseFloat(h.get('.pill-surface').style.height)).toBeCloseTo(34,2);
  });
  it('keeps the Pasted title and glyph at the top with a top anchor',async()=>{
    const h=await mount();h.emit('pill-geometry',{anchor:'top-center',anchorX:220,anchorY:6});
    h.emit('paste-outcome',{outcome:'pasted',words:38,polished:true});h.emit('pill-pointer',{inside:true});await h.settle(1000);
    expect(h.get('.appico').style.top).toBe('7px');expect(h.get('.glyph').style.top).toBe('4px');
  });
  it.each([
    ['dictation-note',{generation:1,kind:'mic_dropped',captured_ms:42000}],
    ['dictation-note',{generation:1,kind:'polish_skipped',reason:'timeout'}],
    ['island-notice',{kind:'polish_setup'}],
  ])('places simple %s feedback and dismiss on one row',async(event,payload)=>{
    const h=await mount();h.emit(event as string,payload);await h.settle(1000);
    const card=h.get('.feedback-card');expect(card.classList.contains('single-row')).toBe(true);
    expect(card.querySelector('.card-bottom')).toBeNull();expect(card.querySelector('.card-dismiss')?.parentElement).toBe(card);
    expect(h.get('.card-title').title).toBe(h.get('.card-title').textContent);
    fireEvent.click(h.get('.card-dismiss'));await h.settle(500);expect(h.root.querySelector('.feedback-card')).toBeNull();
  });
  it('caps stack width before measuring wrapped content height',async()=>{
    const h=await mount();
    vi.spyOn(HTMLElement.prototype,'offsetWidth','get').mockImplementation(function(this:HTMLElement){return this.classList.contains('feedback-card') ? 430 : 0;});
    vi.spyOn(HTMLElement.prototype,'offsetHeight','get').mockImplementation(function(this:HTMLElement){return this.classList.contains('feedback-card') && this.style.width==='360px' ? 108 : 0;});
    h.emit('dictation-blocked',{generation:1,kind:'accessibility_off',action:'open_accessibility'});await h.settle(1000);
    expect(h.get('.feedback-card').style.width).toBe('360px');expect(h.get('.feedback-card').style.height).toBe('108px');
    expect(h.get('.card-title').title).toBe('Allow Accessibility to paste automatically');
  });
  it.each([true,false])('puts Secure Input recovery instructions in the wrapping card body on mac=%s',async mac=>{
    const h=await mount(mac);
    vi.spyOn(HTMLElement.prototype,'offsetWidth','get').mockImplementation(function(this:HTMLElement){return this.classList.contains('feedback-card') ? 430 : 0;});
    vi.spyOn(HTMLElement.prototype,'offsetHeight','get').mockImplementation(function(this:HTMLElement){return this.classList.contains('feedback-card') && this.style.width==='360px' ? 108 : 0;});
    h.emit('island-notice',{kind:'secure_input'});await h.settle(1000);
    const card=h.get('.feedback-card'),instructions=h.get('.card-bottom small');
    expect(h.get('.card-title').textContent).toBe('Secure Input is on');
    expect(card.classList.contains('single-row')).toBe(false);
    expect(instructions.hidden).toBe(false);
    expect(instructions.textContent).toBe('If your shortcut is unavailable, start from the tray or mic. Use the island to stop or cancel.');
    expect(card.style.width).toBe('360px');expect(card.style.height).toBe('108px');
    expect(h.get('.card-dismiss').closest('.card-bottom')).not.toBeNull();
  });
  it.each([true,false])('caps the Accessibility island and measures wrapping on mac=%s',async mac=>{
    const h=await mount(mac);
    vi.spyOn(HTMLElement.prototype,'scrollHeight','get').mockImplementation(function(this:HTMLElement){return this.dataset.layer==='notice'&&this.style.width==='360px'?102:0;});
    h.emit('paste-outcome',{outcome:'no_permission',words:38,polished:false});await h.settle(1000);
    expect(h.get('.pill-surface').style.width).toBe('360px');expect(h.get('.pill-surface').style.height).toBe('104px');
    expect(h.get('.notice-title').title).toBe('Allow Accessibility to paste automatically');
  });
});

describe('live recording warnings', () => {
  it('shows live warnings while other cards are tucked and drops only live warnings', () => {
    const stack = new FeedbackStack();
    stack.push(noteCard({ generation: 1, kind: 'mic_silent' }), 0);
    stack.push(noteCard({ generation: 1, kind: 'mic_silent_live' }), 0);
    expect(stack.displayed.map(item => item.key)).toEqual(['mic_silent_live:1']);
    stack.dropLive();
    stack.tuck(false, 1);
    expect(stack.visible.map(item => item.key)).toEqual(['mic_silent']);
  });
  it('withdraws only the warning from the matching take when speech arrives', async () => {
    const h = pillHarness();
    await h.settle(); h.context(); h.emit('recording-started'); await h.settle(1000);
    h.emit('dictation-note', { generation: 1, kind: 'mic_silent_live' }); await h.settle();
    expect(h.get('.feedback-card').hidden).toBe(false);
    h.emit('dictation-note-withdraw', { generation: 0, kind: 'mic_silent_live' }); await h.settle();
    expect(h.get('.feedback-card')).not.toBeNull();
    h.emit('dictation-note-withdraw', { generation: 1, kind: 'mic_silent_live' }); await h.settle();
    expect(h.get('.feedback-card')).toBeNull(); h.destroy();
  });
  for (const end of ['stop', 'cancel', 'paste'] as const) it(`drops live warnings on ${end}, retaining post-take digital-zero feedback`, async () => {
    const h = pillHarness(); await h.settle(); h.context(); h.emit('recording-started'); await h.settle(1000);
    h.emit('dictation-note', { generation: 1, kind: 'mic_silent_live' }); await h.settle();
    if (end === 'stop') h.emit('recording-state-changed', { state: 'stopping', error: null });
    if (end === 'cancel') h.emit('recording-state-changed', { state: 'idle', error: null });
    if (end === 'paste') { h.emit('transcription-started'); h.emit('paste-outcome', { outcome: 'pasted', words: 12, polished: false }); }
    await h.settle(); expect(h.get('[data-key="mic_silent_live:1"]')).toBeNull();
    h.emit('dictation-note', { generation: 1, kind: 'mic_silent' });
    await h.settle(3000); expect(h.get('[data-key="mic_silent"]')).not.toBeNull(); h.destroy();
  });
});

it('drops long-silence feedback when recording ends and ignores late live warnings', async () => {
  const h = pillHarness(); await h.settle(); h.context(); h.emit('recording-started'); await h.settle(1000);
  h.emit('island-notice', { kind: 'long_silence' }); await h.settle();
  expect(h.get('[data-key="notice:long_silence"]').hidden).toBe(false);
  h.emit('recording-state-changed', {state:'idle',error:null}); await h.settle();
  h.emit('island-notice', { kind: 'long_silence' });
  h.emit('dictation-note', {generation:1,kind:'mic_silent_live'}); await h.settle();
  expect(h.get('.feedback-card')).toBeNull(); h.destroy();
});


it('retains the current mic warning when a previous outcome arrives during recording', async () => {
  const h = pillHarness(); await h.settle(); h.context(); h.emit('recording-started'); h.emit('transcription-started');
  h.context({generation:2}); h.emit('recording-started');
  h.emit('dictation-note', {generation:2,kind:'mic_silent_live'}); await h.settle();
  h.emit('paste-outcome', {generation:1,outcome:'pasted',words:12,polished:false}); await h.settle();
  expect(h.get('[data-key="mic_silent_live:2"]').hidden).toBe(false);
  expect(h.get('[data-layer="row"]').getAttribute('aria-hidden')).toBe('false'); h.destroy();
});

it('keeps a recovery handoff visible on every frame while the island contracts', async () => {
  const h = pillHarness(); await h.settle(); h.context(); h.emit('recording-started'); h.emit('transcription-started'); await h.settle(1000);
  h.emit('dictation-recovery', {generation:1,id:'handoff',kind:'cloud_failed',engine_short:'Soniox',alt_engine_short:null,expires_in_ms:10000});
  for (let frame=0;frame<90;frame++) {
    await h.settle(16);
    const card=h.get('[data-key="recovery:handoff"]');
    expect(card.hidden).toBe(false); expect(Number(card.style.opacity)).toBe(1);
    expect(Number(h.get('.feedback-stack').style.opacity)).toBe(1);
  }
  h.destroy();
});

it('scopes accepted-outcome live cleanup to its generation', () => {
  const stack = new FeedbackStack();
  stack.push(noteCard({generation:1,kind:'mic_silent_live'}),0);
  stack.push(noteCard({generation:2,kind:'mic_silent_live'}),0);
  stack.dropLive(1);
  expect(stack.items.map(item=>item.liveGeneration)).toEqual([2]);
});


it('rejects a previous generation outcome during processing and retains its live hint', async () => {
  const h = pillHarness(); await h.settle(); h.context({generation:2}); h.emit('recording-started'); h.emit('transcription-started');
  h.emit('escape-hint', {generation:2,phase:'processing'}); await h.settle();
  h.emit('paste-outcome', {generation:1,outcome:'pasted',words:12,polished:false}); await h.settle();
  expect(h.get('.escape-hint-card').hidden).toBe(false);
  expect(h.get('[data-layer="work"]').getAttribute('aria-hidden')).toBe('false');
  h.emit('paste-outcome', {generation:2,outcome:'pasted',words:12,polished:false}); await h.settle();
  expect(h.get('.escape-hint-card')).toBeNull(); h.destroy();
});


it('keeps the Escape deadline and drain when recording hands off to processing', async () => {
  const h=pillHarness(); await h.settle(); h.context(); h.emit('recording-started');
  h.emit('escape-hint',{generation:1,phase:'recording'}); await h.settle(1000);
  h.emit('transcription-started'); await h.settle(200);
  expect(h.get('.escape-hint-card .card-title').textContent).toBe('Press Esc again to discard');
  const outline=h.get('.escape-hint-card .outline-clock path');
  expect(outline.style.animationDuration).toBe('1000ms');
  expect(outline.style.getPropertyValue('--clock-from')).toBe('0.5');
  expect(h.root.querySelector('.drain')).toBeNull();
  await h.settle(1100); expect(h.get('.escape-hint-card')).toBeNull(); h.destroy();
});

describe('timed card outline countdown',()=>{
  it('pasted: the card outline drains over 2.4 s, pauses while hovered and restarts on a new outcome',async()=>{
    const h=await mount();
    h.emit('paste-outcome',{outcome:'pasted',words:38,polished:false});await h.settle(100);
    const path=()=>h.get('.done .outline-clock path');
    expect(path().style.animationDuration).toBe('2400ms');expect(path().style.animationPlayState).toBe('running');
    expect(h.root.querySelector('.hold')).toBeNull();
    h.emit('pill-pointer',{inside:true});expect(path().style.animationPlayState).toBe('paused');
    h.emit('pill-pointer',{inside:false});expect(path().style.animationPlayState).toBe('running');
    const before=path();h.emit('transcription-started');h.emit('paste-outcome',{outcome:'pasted',words:3,polished:false});
    expect(path()).not.toBe(before);
  });
  it('pasted: a new outcome does not inherit the previous take\'s hover pause',async()=>{
    const h=await mount();
    h.emit('paste-outcome',{outcome:'pasted',words:38,polished:false});await h.settle(100);
    const path=()=>h.get('.done .outline-clock path');
    h.emit('pill-pointer',{inside:true});expect(path().style.animationPlayState).toBe('paused');
    h.emit('transcription-started');h.emit('pill-pointer',{inside:false});
    h.emit('paste-outcome',{outcome:'pasted',words:3,polished:false});
    expect(path().style.animationPlayState).toBe('running');
    await h.settle(2500);expect(h.get('.pill-root').dataset.state).toBe('idle');
  });
  it('pasted: an outcome that lands under the pointer pauses the outline and the dismissal together',async()=>{
    const h=await mount();
    h.emit('pill-pointer',{inside:true});
    h.emit('paste-outcome',{outcome:'pasted',words:3,polished:false});
    const path=()=>h.get('.done .outline-clock path');
    expect(path().style.animationPlayState).toBe('paused');
    await h.settle(5000);expect(h.get('.pill-root').dataset.state).toBe('pasted');
    h.emit('pill-pointer',{inside:false});expect(path().style.animationPlayState).toBe('running');
    await h.settle(2500);expect(h.get('.pill-root').dataset.state).toBe('idle');
  });
  it('sizes the outline to the island surface',async()=>{
    const h=await mount();h.emit('paste-outcome',{outcome:'pasted',words:2,polished:false});await h.settle(300);
    const svg=h.get('.done .outline-clock');
    expect(svg.getAttribute('width')).toBe('234');expect(svg.getAttribute('height')).toBe('32');
    expect(svg.querySelector('path')!.getAttribute('d')).toMatch(/^M117 1H/);
  });
});
