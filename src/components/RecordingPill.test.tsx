import { fireEvent } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { pillHarness } from '@/pill/test-harness';
const platform = vi.hoisted(() => ({ mac: true }));
vi.mock('@/lib/platform', () => ({ get isMacOS() { return platform.mac; } }));
let h: ReturnType<typeof pillHarness>;
afterEach(() => { h?.destroy(); platform.mac = true; });
async function mount(settings: Record<string, unknown> = {}) { h = pillHarness(settings); await h.settle(0); return h; }
const state = () => h.get('.pill-root').dataset.state;
const layer = (name: string) => h.get(`[data-layer="${name}"]`);

describe('Morphing RecordingPill', () => {
  it('hides idle in when_recording mode', async () => { await mount({ pill_indicator_mode: 'when_recording' }); expect(h.get('.pill-surface').hidden).toBe(true); });
  it('shows the 12 px rest dot in always mode', async () => { await mount(); expect(h.get('.pill-rest-dot').hidden).toBe(false); expect(h.get('.pill-surface').style.width).toBe('12px'); });
  it('re-reads settings when settings change', async () => { await mount(); h.emit('settings-changed'); await h.settle(0); expect(h.invoke.mock.calls.filter(([cmd]) => cmd === 'get_settings')).toHaveLength(2); });
  it('shows elapsed recording timer and real cancel button', async () => { await mount(); h.emit('recording-started'); await h.settle(3050); expect(h.get('.pill-timer').textContent).toBe('0:03'); expect(h.get('.cancel').getAttribute('aria-label')).toBe('Cancel recording'); });
  it('uses the same three zones for legacy compact mode', async () => { await mount({ pill_indicator_style: 'compact' }); h.emit('recording-started'); await h.settle(); expect(layer('row').hidden).toBe(false); expect(h.get('.pill-timer').textContent).toBe('0:00'); });
  it.each(['top-left','top-center','top-right','bottom-left','bottom-center','bottom-right'])('honours the %s P1 anchor', async anchor => { await mount({ pill_indicator_position: anchor }); expect(h.get('.pill-root').dataset.pillPosition).toBe(anchor); });
  it('hydrates an already running recording', async () => { h = pillHarness({}, { state: 'recording', error: null }); await h.settle(); expect(state()).toBe('listening'); });
  it('ignores audio at rest and stale generations while recording', async () => { await mount(); h.emit('audio-level',{ generation:1,level:1 }); expect(state()).toBe('idle'); h.context({ generation:2 }); h.emit('recording-started'); h.emit('audio-level',{ generation:1,level:1 }); expect(state()).toBe('listening'); });
  it('registers the level transport exactly once across starts', async () => { await mount(); h.emit('recording-started'); h.emit('transcription-started'); h.emit('recording-started'); expect(h.handlers.has('audio-level')).toBe(true); });
  it('invokes cancel only once until the state changes', async () => { await mount(); h.emit('recording-started'); h.emit('pill-pointer',{ inside:true }); await h.settle(); fireEvent.click(h.get('.cancel')); fireEvent.click(h.get('.cancel')); expect(h.invoke.mock.calls.filter(([cmd]) => cmd === 'cancel_recording')).toHaveLength(1); expect(h.get<HTMLButtonElement>('.cancel').disabled).toBe(true); });
  it('invokes the real stop command', async () => { await mount(); h.emit('recording-started'); h.emit('pill-pointer',{ inside:true }); await h.settle(); fireEvent.click(h.get('.stop')); expect(h.invoke).toHaveBeenCalledWith('stop_recording'); });
  it('freezes the timer through stopping and transcribing', async () => { await mount(); h.emit('recording-started'); await h.settle(2050); h.emit('recording-state-changed',{ state:'stopping',error:null }); await h.settle(1000); expect(h.get('.work-time').textContent).toBe('0:02'); expect(h.get('.work-label').textContent).toBe('Transcribing'); });
  it.each(['compact','full'])('shows processing labels and hides unavailable Skip in %s mode', async style => { await mount({ pill_indicator_style:style }); h.emit('transcription-started'); await h.settle(); expect(layer('work').hidden).toBe(false); h.emit('enhancing-started'); await h.settle(); expect(h.get('.work-label').textContent).toBe('Polishing'); expect(h.get('.skip').hidden).toBe(true); });
  it('holds Polish precedence until enhancement completes', async () => { await mount(); h.emit('enhancing-started'); h.emit('recording-state-changed',{state:'transcribing',error:null}); expect(state()).toBe('polishing'); h.emit('enhancing-completed'); expect(state()).toBe('transcribing'); });
  it('flashes too-short with toggle copy', async () => { await mount(); h.emit('recording-too-short',{generation:1,mode:'toggle'}); await h.settle(); expect(h.get('.card-title').textContent).toBe('Too short — talk a bit longer'); await h.settle(2400); expect(state()).toBe('idle'); });
  it('flashes recording errors briefly', async () => { await mount(); h.emit('recording-state-changed',{state:'error',error:'Mic unavailable'}); await h.settle(); expect(h.get('.notice-title').textContent).toBe('Recording failed'); await h.settle(1500); expect(state()).toBe('idle'); });
  it.each(['pasted','copied','no_permission'])('renders %s feedback across backend idle', async outcome => { await mount(); h.emit('paste-outcome',{outcome,words:38}); h.emit('recording-state-changed',{state:'idle',error:null}); await h.settle(); expect(state()).toBe(outcome); expect(h.get('.pill-announcement').textContent).toContain(outcome === 'pasted' ? '38 words' : 'Copied'); });
  it.each(['pasted','copied','no_permission'])('new recording interrupts %s feedback', async outcome => { await mount(); h.emit('paste-outcome',{outcome,words:2}); h.emit('recording-started'); await h.settle(2600); expect(state()).toBe('listening'); });
  it('never mode hides every state', async () => { await mount({pill_indicator_mode:'never'}); for (const event of ['recording-started','transcription-started','recording-too-short']) { h.emit(event,event === 'recording-too-short' ? {generation:1,mode:'toggle'} : undefined); expect(h.get('.pill-surface').hidden).toBe(true); } h.emit('paste-outcome',{outcome:'copied',words:2}); expect(h.get('.pill-surface').hidden).toBe(true); });
  it('pasted lasts exactly 2.4 seconds then returns to rest', async () => { await mount(); h.emit('paste-outcome',{outcome:'pasted',words:2}); await h.settle(2399); expect(state()).toBe('pasted'); await h.settle(1); expect(state()).toBe('idle'); });
  it('uses Windows paste copy and bottom-right anchor', async () => { platform.mac = false; await mount(); h.emit('paste-outcome',{outcome:'copied',words:2}); expect(h.get('.notice-title').textContent).toBe('Copied — press Ctrl+V'); expect(h.get('.pill-root').dataset.pillPosition).toBe('bottom-right'); });
  it('preserves no-permission feedback through backend error', async () => { await mount(); h.emit('paste-outcome',{outcome:'no_permission',words:2}); h.emit('recording-state-changed',{state:'error',error:'No accessibility permission'}); expect(state()).toBe('no_permission'); });
  it('shows Esc hint from the real escape-hint event then clears after two seconds', async () => { await mount(); h.emit('recording-started'); h.emit('escape-hint',{generation:1,phase:'recording'}); await h.settle(); expect(layer('hint').hidden).toBe(false); expect(h.get('.hint-row > span').textContent).toBe('Press Esc again to cancel'); await h.settle(2000); expect(layer('hint').getAttribute('aria-hidden')).toBe('true'); });
  it('uses discard copy for Esc during processing', async () => { await mount(); h.emit('transcription-started'); h.emit('escape-hint',{generation:1,phase:'recording'}); expect(h.get('.hint-row > span').textContent).toBe('Press Esc again to discard'); });
  it('maps retained no speech to an actionable card', async () => { await mount(); h.emit('dictation-recovery',{generation:1,id:'clip',kind:'no_speech',engine_short:'Parakeet',alt_engine_short:null,expires_in_ms:30000});await h.settle();expect(h.get('.card-title').textContent).toBe('No speech heard');expect(h.get('.card-bottom').textContent).toContain('Audio kept for 30 s');expect(h.get('.card-actions').textContent).toContain('Transcribe anyway'); });
  it('keeps copied card sticky and dismisses without deleting words', async () => { await mount(); h.emit('paste-outcome',{outcome:'copied',words:2}); await h.settle(10000); expect(h.get('.card-title').textContent).toBe('Copied — press ⌘V'); fireEvent.click(h.get('.card-dismiss')); expect(state()).toBe('idle'); expect(h.invoke.mock.calls.some(([cmd]) => cmd === 'cancel_recording')).toBe(false); });
  it('stops all frames and timers after settling at rest', async () => { await mount(); h.emit('recording-started'); await h.settle(); h.emit('recording-state-changed',{state:'idle',error:null}); await h.settle(3000); expect(vi.getTimerCount()).toBe(0); });
  it('holds the real start card for 800 ms and shows context chips', async () => {
    await mount(); h.context({show_start_card:true,app:{name:'Editor',icon_key:'opaque'},mic:{name:'Microphone (Shure MV7)',tooltip:'Full mic name',ok:true},engine:{short_name:'Parakeet',kind:'local'}}); h.emit('recording-started');
    expect(state()).toBe('start'); expect(h.get('.letter').textContent).toBe('E'); expect(h.get('.mic-chip').title).toBe('Full mic name'); expect(h.get('.mic-chip').textContent).toBe('Shure MV7');
    expect(h.invoke).toHaveBeenCalledWith('pill_app_icon',{iconKey:'opaque'}); await h.settle(799); expect(state()).toBe('start'); await h.settle(1); expect(state()).toBe('listening');
  });
  it('shows the amber badge for unavailable selected Polish and none when Polish is off', async () => {
    await mount(); h.context({polish:{will_run:false,style:'message',key_ok:false,keep_words:true}}); h.emit('recording-started'); await h.settle();
    expect(h.get('.aibadge').hidden).toBe(false); expect(h.get('.aibadge').classList.contains('amber')).toBe(true); expect(h.get('.aibadge svg')).toBeTruthy();
    h.context({generation:2,polish:{will_run:false,style:null,key_ok:true,keep_words:true}}); await h.settle(); expect(h.get('.aibadge').hidden).toBe(true);
  });
  it('uses native pointer coordinates to reveal controls without DOM mousemove', async () => {
    await mount(); h.emit('recording-started'); vi.spyOn(h.get('.pill-surface'),'getBoundingClientRect').mockReturnValue({x:100,y:380,left:100,right:336,top:380,bottom:414,width:236,height:34} as DOMRect);
    h.emit('pill-pointer',{inside:true,x:200,y:390}); await h.settle(); expect(h.get('.ctl').inert).toBe(false);
    h.emit('pill-pointer',{inside:true,x:400,y:390}); await h.settle(); expect(h.get('.ctl').inert).toBe(true);
  });
  it('opens peek after dwell and folds 400 ms after leaving', async () => {
    await mount(); h.emit('pill-pointer',{inside:true}); await h.settle(449); expect(state()).toBe('idle'); await h.settle(1); expect(state()).toBe('peek');
    h.emit('pill-pointer',{inside:false}); await h.settle(399); expect(state()).toBe('peek'); await h.settle(1); expect(state()).toBe('idle');
  });
  it('uses bounded cross-fades and no continuous rAF under reduced motion', async () => {
    vi.spyOn(window,'matchMedia').mockReturnValue({matches:true,addEventListener:vi.fn(),removeEventListener:vi.fn()} as unknown as MediaQueryList);
    await mount(); h.emit('recording-started'); await h.settle(1000); expect(vi.getTimerCount()).toBe(0);
    h.emit('transcription-started'); await h.settle(1000); expect(vi.getTimerCount()).toBe(0); expect(h.root.querySelector('.island-ghost')).toBeNull();
  });
  it('never mode leaves no frames running even while the backend transcribes', async () => { await mount({pill_indicator_mode:'never'}); h.emit('transcription-started'); await h.settle(1000); expect(vi.getTimerCount()).toBe(0); });
  it('announces a duplicate outcome only once', async () => {
    await mount(); const announcement=h.get('.pill-announcement'); const records: MutationRecord[]=[]; const observer=new MutationObserver(items=>records.push(...items)); observer.observe(announcement,{childList:true});
    h.emit('paste-outcome',{outcome:'pasted',words:2}); await h.settle(0); h.emit('paste-outcome',{outcome:'pasted',words:2}); await h.settle(0); observer.disconnect(); expect(records).toHaveLength(1);
  });
  it('wires the existing privacy settings action without inventing an Undo command', async () => { await mount(); h.emit('paste-outcome',{outcome:'no_permission',words:2}); expect(h.get('.privacy-settings').hidden).toBe(false); fireEvent.click(h.get('.privacy-settings')); expect(h.invoke).toHaveBeenCalledWith('open_accessibility_settings'); expect(h.root.textContent).not.toContain('Undo'); });

  it.each([true, false])('keeps the copied identity on row one and the detail on row two (mac=%s)', async mac => {
    platform.mac = mac; await mount(); h.context({app:{name:'Editor',icon_key:null}});
    h.emit('paste-outcome',{outcome:'copied',words:38}); await h.settle(1000);
    expect(h.get('.notice-title').textContent).toBe(`Copied — press ${mac ? '⌘V' : 'Ctrl+V'}`);
    expect(h.get('.notice-sub').textContent).toBe("Editor didn't accept the paste");
    expect(h.get('.notice-title').parentElement).toBe(h.get('.notice-count').parentElement);
    expect(h.get('.notice-sub').parentElement).toBe(h.get('.dismiss').parentElement);
    expect(parseFloat(h.get('.appico').style.top)).toBe(7);
    expect(parseFloat(h.get('.glyph').style.top)).toBe(4);
    expect(h.get('.pill-surface').classList.contains('amber')).toBe(true);
    await h.settle(2500);
    expect(h.get('.card-title').textContent).toBe(`Copied — press ${mac ? '⌘V' : 'Ctrl+V'}`);
    expect(vi.getTimerCount()).toBe(0);
  });
  it('expands live words and all row zones to 300 px then holds processing at 236 px', async () => {
    await mount(); h.emit('recording-started');
    h.emit('transcription-stream',{type:'partial',session_id:1,revision:1,committed:'hello ',tentative:'world'});
    await h.settle(2000);
    expect(h.get('.pill-surface').style.width).toBe('300px');
    expect(h.get('.listening-row').style.width).toBe('298px');
    h.emit('transcription-started'); await h.settle(2000);
    expect(h.get('.pill-surface').style.width).toBe('236px');
  });

});
