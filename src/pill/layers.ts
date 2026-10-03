import { clamp, ease } from '@/pill/spring';
import type { LayerName } from '@/pill/island';
export const layerNames: readonly LayerName[] = ['start', 'words', 'row', 'hint', 'work', 'done', 'notice', 'note', 'peek', 'pick'];
class Layer {
  on = false; alpha = 0; from = 0; time = 0; secondTime = 0;
  private second: HTMLElement | null;
  constructor(readonly el: HTMLElement) { el.hidden = true; this.second = el.querySelector<HTMLElement>('.chips,.acts'); }
  target(on: boolean) {
    if (on === this.on) return;
    this.on = on; this.from = this.alpha; this.time = this.secondTime = 0;
    this.el.hidden = false;
    this.el.inert = !on; this.el.setAttribute('aria-hidden', String(!on));
  }
  step(dt: number, reduced: boolean, room = true, visibleRoom = true) {
    if (!this.on && this.alpha === 0 && this.el.hidden || this.on && this.alpha === 1 && (!this.second || this.second.style.opacity === '1')) return false;
    this.time += dt;
    if (this.on && !reduced && this.from <= .05 && this.alpha < .01 && !visibleRoom) { this.time = Math.min(this.time,.06); return true; }
    const t = this.on ? reduced ? clamp(this.time / .15) : ease((this.time - (this.from > .05 ? 0 : .06)) / .2) : clamp(this.time / (reduced ? .15 : .08));
    this.alpha = this.on ? reduced ? Math.min(1,this.from + this.time / .15) : this.from + (1 - this.from) * t : Math.max(0,this.from - this.time / (reduced ? .15 : .08));
    this.el.style.opacity = String(this.alpha);
    const lift = this.on && !reduced ? 4 * (1 - t) : 0;
    this.el.style.transform = lift ? `translateY(${lift}px)` : '';
    this.el.style.filter = reduced ? '' : this.on ? lift ? `blur(${lift * .75}px)` : '' : this.alpha > 0 ? `blur(${2 * (1 - this.alpha)}px)` : '';
    if (!this.on && this.alpha === 0) this.el.hidden = true;
    let secondMoving = false;
    if (this.second) {
      if (room) this.secondTime += dt;
      const a = reduced ? 1 : this.on ? Math.min(ease((this.time - .18) / .2), ease((this.secondTime - .06) / .2)) : this.alpha * this.alpha;
      this.second.style.opacity = String(a); secondMoving = this.on && a < 1;
    }
    return this.alpha !== (this.on ? 1 : 0) || secondMoving;
  }
}
export class Layers {
  readonly entries: Record<LayerName, Layer>;
  private swaps: Array<{ old: HTMLElement; current: HTMLElement; time: number }> = [];
  constructor(surface: HTMLElement) {
    this.entries = Object.fromEntries(layerNames.map(name => [name, new Layer(surface.querySelector<HTMLElement>(`[data-layer="${name}"]`)!)])) as Record<LayerName, Layer>;
  }
  target(names: readonly LayerName[]) { for (const name of layerNames) this.entries[name].target(names.includes(name)); }
  swapText(el: HTMLElement, value: string) {
    if (el.textContent === value) return;
    if (!el.parentElement || el.closest('[hidden]')) { el.textContent = value; return; }
    const old = el.cloneNode(true) as HTMLElement;
    old.className = 'label-ghost'; old.setAttribute('aria-hidden', 'true'); old.inert = true;
    old.style.left = `${el.offsetLeft}px`; old.style.top = `${el.offsetTop}px`;
    old.style.width = `${el.offsetWidth}px`;
    el.parentElement.append(old); el.textContent = value; el.style.opacity = '0';
    this.swaps.push({old,current:el,time:0});
  }
  step(dt: number, reduced: boolean, width = Infinity, targetWidth = 0, height = Infinity, targetHeight = 0) {
    let moving = false;
    for (const name of layerNames) {
      const fits = name === 'start' ? height >= 49 : name === 'words' ? height >= targetHeight - 6 : name === 'peek' ? height >= targetHeight - 1 : name === 'row' || name === 'hint' || width >= targetWidth - 4;
      moving = this.entries[name].step(dt, reduced, name !== 'start' || width >= targetWidth - 16, fits) || moving;
    }
    for (let i = this.swaps.length - 1; i >= 0; i--) {
      const swap = this.swaps[i]; swap.time += dt;
      swap.old.style.opacity = String(1 - clamp(swap.time / (reduced ? .15 : .08)));
      swap.current.style.opacity = String(ease((swap.time - (reduced ? 0 : .06)) / (reduced ? .15 : .2)));
      if (swap.time >= (reduced ? .15 : .26)) { swap.old.remove(); swap.current.style.opacity = ''; this.swaps.splice(i,1); }
      else moving = true;
    }
    return moving;
  }
}
