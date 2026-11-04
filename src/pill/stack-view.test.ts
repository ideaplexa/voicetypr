import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { StackView } from '@/pill/stack-view';
import { recoveryCard, type CardCommand, type FeedbackCard } from '@/pill/feedback';

let root: HTMLElement, view: StackView;
const call = vi.fn(async (_command: CardCommand): Promise<void> => {});
const card = (key: string): FeedbackCard => ({key,title:key,sub:'',tone:'neutral',duration:null,actions:[]});
const step = () => view.step(1,true,true);
const el = (key: string) => root.querySelector<HTMLElement>(`[data-key="${key}"]`)!;
beforeEach(() => {
  vi.useFakeTimers(); root=document.createElement('div'); document.body.append(root);
  call.mockReset();call.mockResolvedValue();
  view=new StackView(root,{wake:()=>{},changed:()=>{},call,announce:()=>{},setTimeout:window.setTimeout,clearTimeout:window.clearTimeout});
  vi.spyOn(HTMLElement.prototype,'offsetWidth','get').mockImplementation(function(this:HTMLElement){
    expect(this.hidden).toBe(false); expect(this.classList.contains('back')).toBe(false);
    return 320;
  });
  vi.spyOn(HTMLElement.prototype,'offsetHeight','get').mockImplementation(function(this:HTMLElement){
    expect(this.hidden).toBe(false); expect(this.style.width).toBe('320px');
    return 76;
  });
});
afterEach(() => {view.destroy();root.remove();vi.restoreAllMocks();vi.useRealTimers();});

it.each([1,2,3])('measures an unscaled border box when repeating a card at depth %s, including hidden overflow', depth => {
  view.tuck(false);
  view.push(card('promoted')); step();
  for(let n=0;n<depth;n++){view.push(card(`front${n}`));step();}
  expect(el('promoted').hidden).toBe(depth>2);
  expect(el('promoted').style.transform).toContain(`scale(${1-Math.min(2,depth)*.05})`);
  view.push(card('promoted')); step();
  expect(el('promoted').dataset.naturalWidth).toBe('320');
  expect(el('promoted').dataset.naturalHeight).toBe('76');
  expect(el('promoted').style.width).toBe('320px');
  expect(el('promoted').style.height).toBe('76px');
  expect(el('promoted').hidden).toBe(false);
});

it.each([1,2])('measures a card promoted from depth %s by dismissal', depth => {
  view.tuck(false); view.push(card('promoted')); step();
  for(let n=0;n<depth;n++){view.push(card(`front${n}`));step();}
  for(let n=0;n<depth;n++)view.remove(`front${n}`);
  step();
  expect(el('promoted').dataset.naturalWidth).toBe('320');
  expect(el('promoted').dataset.naturalHeight).toBe('76');
});

it('temporarily unhides a tucked card for natural measurement on return', () => {
  view.push(card('tucked')); step();
  expect(el('tucked').hidden).toBe(true);
  view.tuck(false); step();
  expect(el('tucked').hidden).toBe(false);
  expect(el('tucked').style.width).toBe('320px');
  expect(el('tucked').style.height).toBe('76px');
});

it('keeps a retry pending through stack promotion, then re-enables it on rejection', async () => {
  let reject = (_reason: Error) => {};
  call.mockImplementationOnce(() => new Promise<void>((_resolve, fail) => { reject = fail; }));
  view.tuck(false);
  view.push(recoveryCard({generation:1,id:'clip',kind:'cloud_failed',engine_short:'Soniox',alt_engine_short:'Parakeet',alt_engine_id:'parakeet',expires_in_ms:600000},performance.now()));
  step();
  const retry=el('recovery:clip').querySelector<HTMLButtonElement>('.act')!;
  expect(retry.disabled).toBe(false);expect(el('recovery:clip').inert).toBe(false);
  retry.click();retry.click();
  expect(retry.disabled).toBe(true);expect(call).toHaveBeenCalledTimes(1);
  view.push(card('cover'));step();
  expect(el('recovery:clip').inert).toBe(true);
  view.remove('cover');step();
  expect(el('recovery:clip').inert).toBe(false);
  expect(el('recovery:clip').querySelector('.act')).toBe(retry);
  expect(retry.disabled).toBe(true);
  reject(new Error('Unavailable'));await Promise.resolve();await Promise.resolve();
  expect(retry.disabled).toBe(false);
  expect(el('retry-error').textContent).toContain("Couldn't retry");
  expect(el('retry-error').textContent).not.toContain('Unavailable');
  view.remove('retry-error');step();
  retry.click();await Promise.resolve();
  expect(call).toHaveBeenCalledTimes(2);
  expect(view.stack.items).toHaveLength(0);
});

it('replaces an action when readiness or engine changes without a new expiry/title', () => {
  const recovery={generation:1,id:'clip',kind:'integrity' as const,engine_short:'Parakeet',alt_engine_short:'Parakeet',alt_engine_id:'v3',expires_in_ms:600000};
  view.push(recoveryCard({...recovery,retry_ready:false},0));step();
  expect(el('recovery:clip').querySelector('.act')).toBeNull();
  view.push(recoveryCard({...recovery,retry_ready:true},0));step();
  expect(el('recovery:clip').querySelector('.act')?.textContent).toBe('Retry with Parakeet');
  view.push(recoveryCard({...recovery,alt_engine_id:'v2'},0));step();
  el('recovery:clip').querySelector<HTMLButtonElement>('.act')!.click();
  expect(call).toHaveBeenCalledWith({command:'retry_kept_dictation',id:'clip',engine:'v2'});
  expect(view.stack.items[0].count).toBe(1);
});

it('disables all pending actions but not X, and wakes cursor targets on generic rejection', async () => {
  let reject = (_reason: Error) => {};
  call.mockImplementationOnce(() => new Promise<void>((_resolve, fail) => { reject = fail; }));
  const wake = vi.fn();
  const other = new StackView(root, {wake,changed:()=>{},call,announce:()=>{},setTimeout:window.setTimeout,clearTimeout:window.clearTimeout});
  try {
    other.tuck(false);
    other.push({...card('two'), actions: [
      {label:'Choose mic',call:{command:'island_action',action:'choose_mic'}},
      {label:'Other mic',call:{command:'island_action',action:'choose_mic'}},
    ]});
    other.step(1,true,true);
    const buttons = el('two').querySelectorAll<HTMLButtonElement>('.act');
    buttons[0].click(); buttons[1].click();
    expect(call).toHaveBeenCalledTimes(1);
    expect([...buttons].every(button => button.disabled)).toBe(true);
    expect(el('two').querySelector<HTMLButtonElement>('.card-dismiss')!.disabled).toBe(false);
    wake.mockClear();
    reject(new Error('Unavailable')); await Promise.resolve(); await Promise.resolve();
    expect([...buttons].every(button => !button.disabled)).toBe(true);
    expect(wake).toHaveBeenCalled();
  } finally { other.destroy(); }
});

it('keeps Retry disabled across a native recovery refresh until the pending invocation settles', async () => {
  let reject = (_reason: Error) => {};
  call.mockImplementationOnce(() => new Promise<void>((_resolve, fail) => { reject = fail; }));
  const recovery={generation:1,id:'clip',kind:'integrity' as const,engine_short:'Parakeet',alt_engine_short:'Parakeet',alt_engine_id:'v3',expires_in_ms:600000};
  view.push(recoveryCard(recovery,0));step();
  el('recovery:clip').querySelector<HTMLButtonElement>('.act')!.click();
  view.push(recoveryCard({...recovery,alt_engine_id:'v2'},0));step();
  const updated=el('recovery:clip').querySelector<HTMLButtonElement>('.act')!;
  expect(updated.disabled).toBe(true);
  updated.click();expect(call).toHaveBeenCalledTimes(1);
  reject(new Error('private error'));await Promise.resolve();await Promise.resolve();
  expect(updated.disabled).toBe(false);
  expect(el('retry-error').textContent).toContain("Couldn't retry");
});
