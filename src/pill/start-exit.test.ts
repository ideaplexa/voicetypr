import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
const start = () => h.get('[data-layer="start"]');
const width = (el: HTMLElement) => parseFloat(el.style.width || el.parentElement?.style.width || '0');
beforeEach(() => {
  h=pillHarness();
  // Model the start border box independently of the shared content box, as the browser does.
  vi.spyOn(HTMLElement.prototype,'offsetWidth','get').mockImplementation(function(this:HTMLElement){return width(this);});
  vi.spyOn(HTMLElement.prototype,'offsetHeight','get').mockImplementation(function(this:HTMLElement){return parseFloat(this.style.height || '83');});
});
afterEach(() => h?.destroy());

it.each(['bottom-center','top-center'])('holds outgoing start dimensions through every opaque fold frame at %s', async anchor => {
  await h.settle(0); h.emit('pill-geometry',{anchor,anchorX:231,anchorY:anchor.startsWith('top')?17:425});
  h.context({generation:1,show_start_card:true}); h.emit('recording-started'); await h.settle(790);
  const before=width(start()); expect(before).toBe(298);
  await h.settle(10); // normal 800 ms fold
  expect(h.get('.pill-content').style.width).toBe('234px');
  expect(start().style.width).toBe(`${before}px`); expect(start().style.height).toBe('83px');
  let opaqueFrames=0;
  for(let frame=0;frame<6;frame++){
    await h.settle(16);
    if(Number(start().style.opacity)>0){opaqueFrames++;expect(width(start())).toBe(before);}
  }
  expect(opaqueFrames).toBeGreaterThan(0);
  expect(Number(start().style.opacity)).toBe(0); expect(start().hidden).toBe(true);
});

it('clears the outgoing freeze on re-entry even if the exit fade has not finished', async () => {
  await h.settle(0);
  h.context({generation:1,show_start_card:true}); h.emit('recording-started'); await h.settle(800);
  expect(start().style.width).toBe('298px');
  h.emit('recording-state-changed',{state:'idle',generation:1,error:null});
  h.emit('recording-state-changed',{state:'starting',generation:2,error:null});
  h.context({generation:2,show_start_card:true});
  expect(start().style.width).toBe(''); expect(start().style.height).toBe('');
  expect(width(start())).toBe(298);
});

it('measures incoming start height at its intended width before laying out the shared box', async () => {
  await h.settle(0);
  h.emit('recording-state-changed',{state:'starting',generation:1,error:null});
  expect(h.get('.pill-content').style.width).toBe('234px');
  const measured: string[]=[];
  vi.spyOn(HTMLElement.prototype,'scrollHeight','get').mockImplementation(function(this:HTMLElement){
    if(this.dataset.layer!=='start')return 0;
    measured.push(this.style.width);
    return this.style.width==='298px'?83:110;
  });
  h.context({generation:1,show_start_card:true});
  expect(measured.length).toBeGreaterThan(0); expect(new Set(measured)).toEqual(new Set(['298px']));
  expect(h.get('.pill-content').style.height).toBe('83px');
  expect(start().style.width).toBe(''); expect(start().style.height).toBe('');
});
