import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { StackView } from '@/pill/stack-view';
import type { FeedbackCard } from '@/pill/feedback';

let root: HTMLElement, view: StackView;
const card = (key: string): FeedbackCard => ({key,title:key,sub:'',tone:'neutral',duration:null,actions:[]});
const step = () => view.step(1,true,true);
const el = (key: string) => root.querySelector<HTMLElement>(`[data-key="${key}"]`)!;
beforeEach(() => {
  vi.useFakeTimers(); root=document.createElement('div'); document.body.append(root);
  view=new StackView(root,{wake:()=>{},changed:()=>{},call:async()=>{},announce:()=>{},setTimeout:window.setTimeout,clearTimeout:window.clearTimeout});
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
