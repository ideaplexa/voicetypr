import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent } from '@testing-library/react';
import { createIsland } from '@/pill/renderer';
import { quickFixture } from '@/ui-preview/pill-quick';
import { PickGuard, pickPlacement, quickRows, dotCentre } from '@/pill/quick-settings';
import { pillHarness } from '@/pill/test-harness';
import type { PillIndicatorPosition } from '@/types';

let cleanup: (() => void) | undefined;
afterEach(() => { cleanup?.(); cleanup = undefined; vi.useRealTimers(); vi.restoreAllMocks(); });
function renderer(mac = true) {
  vi.useFakeTimers(); vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue(null);
  const host=document.createElement('div'); document.body.append(host);
  const start=vi.fn(async()=>true), quickSet=vi.fn(async()=>{}), regions=vi.fn();
  const island=createIsland(host,{mac, actions:{start,quickSet,stop:async()=>{},cancel:async()=>{},dismiss:async()=>{}},icon:async()=>null,hitRegions:regions,setTimeout:window.setTimeout.bind(window),clearTimeout:window.clearTimeout.bind(window)});
  island.settings({pill_indicator_mode:'always'}); island.quickOptions(quickFixture(!mac));
  const get=<T extends HTMLElement=HTMLElement>(selector:string)=>host.querySelector<T>(selector)!;
  const settle=async(ms=800)=>{await vi.advanceTimersByTimeAsync(ms);};
  const peek=async()=>{island.pointer({inside:true});await settle(900);};
  cleanup=()=>{island.destroy();host.remove();vi.clearAllTimers();};
  return {island,host,start,quickSet,regions,get,settle,peek};
}
describe('P5 quick settings',()=>{
  it('dwells 450ms, guards early clicks, then permits a quick dot click',async()=>{
    const h=renderer();h.island.pointer({inside:true});fireEvent.click(h.get('.pill-rest-dot'));expect(h.start).not.toHaveBeenCalled();
    await h.settle(120);fireEvent.click(h.get('.pill-rest-dot'));expect(h.start).toHaveBeenCalledOnce();
    expect(h.island.machine.state).toBe('rest'); // no optimistic generation/start
    await h.settle(329);expect(h.island.machine.state).toBe('rest');await h.settle(1);expect(h.island.machine.state).toBe('peek');
  });
  it('fast motion resets slow dwell and stationary native events can open',async()=>{
    const h=renderer();vi.spyOn(h.get('.pill-surface'),'getBoundingClientRect').mockReturnValue(new DOMRect(100,100,12,12));
    h.island.pointer({inside:true,x:101,y:101});await h.settle(300);
    h.island.pointer({inside:true,x:111,y:101});await h.settle(10);
    h.island.pointer({inside:true,x:101,y:101});await h.settle(449);expect(h.island.machine.state).toBe('rest');await h.settle(1);expect(h.island.machine.state).toBe('peek');
  });
  it('closes exactly 400ms after leaving and cancels close on reentry',async()=>{
    const h=renderer();await h.peek();h.island.pointer({inside:false});await h.settle(399);expect(h.island.machine.state).toBe('peek');
    h.island.pointer({inside:true});await h.settle(50);expect(h.island.machine.state).toBe('peek');h.island.pointer({inside:false});await h.settle(400);expect(h.island.machine.state).toBe('rest');
  });
  it('empty peek and record click use the same real start action',async()=>{
    const h=renderer();await h.peek();fireEvent.click(h.get('.record'));await h.settle(0);fireEvent.click(h.get('.peek-row'));expect(h.start).toHaveBeenCalledTimes(2);
  });
  it.each(['polish','engine','mic','language'] as const)('picks %s through shared IPC after the 250ms guard',async kind=>{
    const h=renderer();await h.peek();fireEvent.click(h.get(kind==='polish'?'.peek-style':`.peek-${kind}`));
    const row=h.get<HTMLButtonElement>('.pick-option:not(:disabled)');fireEvent.click(row);await h.settle(249);fireEvent.click(row);expect(h.quickSet).not.toHaveBeenCalled();
    await h.settle(1);fireEvent.click(row);await h.settle(0);expect(h.quickSet).toHaveBeenCalledWith(kind,row.dataset.id);
    expect(h.get(`[data-id="${row.dataset.id}"]`).getAttribute('aria-selected')).toBe('true');h.island.pointer({inside:true});await h.settle(350);expect(h.get('.pill-root').dataset.state).toBe('peek');
    if(kind==='polish') { expect(h.get('.peek-style .quick-label').textContent).toBe('Off'); expect(h.get('.peek-style .label-ghost').textContent).toBe('Message'); }
  });
  it('moving 4px unlocks a pick early',async()=>{
    const h=renderer();await h.peek();fireEvent.click(h.get('.peek-style'));h.island.pointer({inside:true,x:4,y:0});fireEvent.click(h.get('.pick-option'));await h.settle(0);expect(h.quickSet).toHaveBeenCalledOnce();
  });
  it('errors retain the selection, reenable rows and show content-free copy',async()=>{
    const h=renderer();h.quickSet.mockRejectedValueOnce(new Error('secret'));await h.peek();fireEvent.click(h.get('.peek-style'));await h.settle(250);fireEvent.click(h.get('.pick-option'));await h.settle(0);
    expect(h.get('.pick-error').hidden).toBe(false);expect(h.get('.pick-error').textContent).not.toContain('secret');expect(h.get<HTMLButtonElement>('.pick-option').disabled).toBe(false);
  });
  it('shows disabled engine group headers and back returns to peek',async()=>{
    const h=renderer(false);await h.peek();fireEvent.click(h.get('.peek-engine'));expect([...h.host.querySelectorAll<HTMLButtonElement>('.pick-option:disabled')].map(el=>el.textContent)).toEqual(['On this PC✓','Cloud✓','Network✓']);
    fireEvent.click(h.get('.pick-back'));expect(h.get('.pill-root').dataset.state).toBe('peek');
  });
  it('default style pick refreshes the effective app style instead of changing the take',async()=>{
    const h=renderer();
    const options=quickFixture();options.polish.current='Notes';
    const effective={generation:1,app_name:'Editor',style:'writing',will_run:true,overridden:true};
    h.island.context({...h.island.machine.context,generation:1,app:{name:'Editor',icon_key:null},show_start_card:true,polish:{will_run:true,style:'writing',key_ok:true,keep_words:false}});
    h.island.quickOptions({...options,polish_context:effective});h.island.start();await h.settle(300);
    fireEvent.click(h.get('.style-chip'));
    expect(h.get('.pick-title').textContent).toBe('Default style');
    const hint=h.host.querySelector<HTMLButtonElement>('[data-id="header:app-style"]')!;
    expect(hint.disabled).toBe(true);expect(hint.textContent).toBe('Editor uses Writing✓');
    h.quickSet.mockImplementationOnce(async()=>{h.island.quickOptions({...options,polish_context:effective});});
    await h.settle(250);fireEvent.click(h.get('[data-id="style_Notes"]'));await h.settle(0);
    expect(h.quickSet).toHaveBeenCalledWith('polish','style_Notes');
    expect(h.island.machine.context.polish.style).toBe('writing');
    await h.settle(1000);expect(h.get('.style-name').textContent).toBe('Writing');
  });
  it.each(['toggle','push_to_talk'])('the next island click stops a pointer take in %s mode',async recording_mode=>{
    const h=pillHarness({recording_mode});cleanup=()=>h.destroy();await h.settle(0);
    h.emit('pill-pointer',{inside:true});await h.settle(120);fireEvent.click(h.get('.pill-rest-dot'));await h.settle(0);
    h.emit('recording-started');await h.settle(500);fireEvent.click(h.get('.listening-row'));await h.settle(0);
    expect(h.invoke).toHaveBeenCalledWith('stop_recording');
    expect(h.invoke.mock.calls.filter(([cmd])=>cmd==='start_recording')).toHaveLength(1);
  });
  it.each(['toggle','push_to_talk'])('pointer start supplies its source and surfaces failures in %s mode',async recording_mode=>{
    const h=pillHarness({recording_mode});cleanup=()=>h.destroy();await h.settle(0);
    const invoke=h.invoke.getMockImplementation()!;
    h.invoke.mockImplementation(async cmd=>{if(cmd==='start_recording')throw new Error('private provider payload');return invoke(cmd);});
    h.emit('pill-pointer',{inside:true});await h.settle(120);fireEvent.click(h.get('.pill-rest-dot'));await h.settle(1500);
    expect(h.invoke).toHaveBeenCalledWith('start_recording',{source:'pointer'});
    expect(h.root.textContent).toContain('Recording failed');expect(h.root.textContent).not.toContain('private provider payload');
  });
  it('same Polish list opens from start without ending recording',async()=>{
    const h=renderer();h.island.context({...h.island.machine.context,generation:1,show_start_card:true,polish:{will_run:true,style:'message',key_ok:true,keep_words:false}});h.island.start();await h.settle(300);fireEvent.click(h.get('.style-chip'));
    expect(h.get('.pill-root').dataset.state).toBe('pick');expect(h.island.machine.recording).toBe(true);await h.settle(1000);expect(h.island.machine.state).toBe('start');
    expect(h.host.querySelectorAll('.pick-option')).toHaveLength(quickFixture().polish.options.length);h.island.closeQuick();await h.settle(800);expect(h.island.machine.state).toBe('listening');
  });
  it('Esc observes without swallowing and pointer leave closes lists',async()=>{
    const h=renderer();await h.peek();fireEvent.click(h.get('.peek-style'));const event=new KeyboardEvent('keydown',{key:'Escape',cancelable:true});window.dispatchEvent(event);expect(event.defaultPrevented).toBe(false);expect(h.get('.pill-root').dataset.state).toBe('peek');
    fireEvent.click(h.get('.peek-language'));h.island.pointer({inside:false});await h.settle(400);expect(h.island.machine.state).toBe('rest');
  });
  it('no-mic copy and colour update from live options',async()=>{
    const h=renderer();await h.peek();h.island.quickOptions(quickFixture(false,true));expect(h.get('.peek-mic .quick-label').textContent).toBe('No mic');expect(h.get('.peek-mic').classList.contains('bad')).toBe(true);expect(h.get('.record').classList.contains('bad')).toBe(true);expect(h.get('.peek-hint').textContent).toBe('No mic — pick one below');
  });
  it.each(['top-left','top-center','top-right','bottom-left','bottom-center','bottom-right'] as PillIndicatorPosition[])('pins record to actual dot centre at %s within 1px',async anchor=>{
    const h=renderer();const geometry={anchor,anchorX:anchor.endsWith('-left')?11:anchor.endsWith('-right')?451:231,anchorY:anchor.startsWith('top-')?17:425};h.island.geometry(geometry);await h.peek();
    const surface=h.get('.pill-surface'),record=h.get('.record'),dot=dotCentre(geometry);
    // DOM local coordinates include the surface's 1px border.
    expect(Math.abs(parseFloat(surface.style.left)+1+parseFloat(record.style.left)+13-dot.x)).toBeLessThanOrEqual(1);
    expect(Math.abs(parseFloat(surface.style.top)+1+parseFloat(record.style.top)+13-dot.y)).toBeLessThanOrEqual(1);
  });
  it.each([true,false])('uses real hold/toggle caps on mac=%s',async mac=>{
    const h=renderer(mac);await h.peek();expect(h.get('.peek-hint').textContent).toBe(mac ? 'Hold ⌥Space to talk' : 'Hold CtrlAltSpace to talk');
    const options=quickFixture(!mac);options.shortcut_caps={mode:'toggle',keys:mac ? ['⌘','⇧','D'] : ['Ctrl','Shift','D']};h.island.quickOptions(options);
    expect(h.get('.peek-hint').textContent).toBe(mac ? 'Press ⌘⇧D to talk' : 'Press CtrlShiftD to talk');
  });
  it('keeps native anchor authority while settings reload',async()=>{
    const h=pillHarness();cleanup=()=>h.destroy();await h.settle(1);h.emit('pill-geometry',{anchor:'top-right',anchorX:451,anchorY:17});h.emit('settings-changed');await h.settle(0);
    expect(h.get('.pill-root').dataset.pillPosition).toBe('top-right');expect(h.get('.pill-root').style.getPropertyValue('--pill-anchor-x')).toBe('451px');
  });
  it('hit target extends only upward and includes every expanded shape',async()=>{
    const h=renderer();vi.spyOn(h.get('.pill-surface'),'getBoundingClientRect').mockReturnValue(new DOMRect(100,100,12,12));await h.settle();
    expect(h.regions).toHaveBeenLastCalledWith([{x:96,y:92,width:20,height:20}]);await h.peek();expect(h.regions).toHaveBeenLastCalledWith([{x:100,y:100,width:12,height:12}]);
  });
  it('settled peek and rest do not maintain requestAnimationFrame loops',async()=>{
    const h=renderer();const raf=vi.spyOn(window,'requestAnimationFrame');await h.peek();await h.settle(2000);raf.mockClear();await h.settle(500);expect(raf).not.toHaveBeenCalled();h.island.rest();await h.settle(2000);raf.mockClear();await h.settle(500);expect(raf).not.toHaveBeenCalled();
  });
  it('transport hydrates and syncs all shared events, and dispatches IPC',async()=>{
    const h=pillHarness();cleanup=()=>h.destroy();let options=quickFixture();h.invoke.mockImplementation(async(cmd:string)=>cmd==='island_quick_options'?options:cmd==='get_settings'?{pill_indicator_mode:'always'}:null);await h.settle(1);
    for(const name of ['settings-changed','model-changed','audio-device-changed','polish-options-changed','ai-enabled-changed','sharing-status-changed','shortcut-settings-changed']) {
      options={...options,engine:{...options.engine,current:name}};h.emit(name);await h.settle(0);expect(h.get('.peek-engine').textContent).toBe(name);
    }
    h.emit('pill-pointer',{inside:true});await h.settle(900);fireEvent.click(h.get('.peek-style'));await h.settle(250);fireEvent.click(h.get('.pick-option'));await h.settle(0);expect(h.invoke).toHaveBeenCalledWith('island_quick_set',{kind:'polish',id:'style_Off'});
    h.emit('island-escape');expect(h.get('.pill-root').dataset.state).toBe('peek');fireEvent.click(h.get('.peek-gear'));expect(h.invoke).toHaveBeenCalledWith('island_action',{action:'open_settings'});
  });
});

describe('pick contracts',()=>{
  it('guard measures distance from opening point, not accumulated jitter',()=>{const guard=new PickGuard();guard.open(100,10,10);guard.pointer(12,12);expect(guard.allows(349)).toBe(false);expect(guard.allows(350)).toBe(true);guard.pointer(14,10);expect(guard.allows(101)).toBe(true);});
  it('selected row aligns on chip unless a canvas edge pulls the list in',()=>{const rows=quickRows(quickFixture(),'polish');const p=pickPlacement(new DOMRect(0,210,70,22),rows,false,442);expect(p.y+46+p.selected*30+15).toBe(221);const edge=pickPlacement(new DOMRect(0,10,70,22),rows,true,442);expect(edge.y).toBe(2);});
});
