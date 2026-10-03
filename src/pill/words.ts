import { clamp } from '@/pill/spring';
/** Persistent committed node, FLIP line offset, and new ink easing out of its tentative slant. */
export class Words {
  private committed = '';
  private tentative = '';
  private offset = 0;
  private age = 1;
  private prefix = document.createTextNode('');
  private fresh = document.createElement('span');
  constructor(readonly line: HTMLElement, readonly committedEl: HTMLElement, readonly tentativeEl: HTMLElement) {
    this.fresh.className = 'fresh'; committedEl.replaceChildren(this.prefix, this.fresh);
  }
  update(committed: string, tentative: string, reduced: boolean) {
    if (committed === this.committed && tentative === this.tentative) return;
    const oldWidth = this.line.offsetWidth;
    if (committed.startsWith(this.committed) && committed.length > this.committed.length && !reduced) {
      this.prefix.data = this.committed; this.fresh.textContent = committed.slice(this.committed.length); this.age = 0;
    } else { this.prefix.data = committed; this.fresh.textContent = ''; this.age = 1; }
    this.committed = committed; this.tentative = tentative; this.tentativeEl.textContent = tentative;
    if (!reduced && oldWidth) this.offset += Math.max(0, this.line.offsetWidth - oldWidth);
  }
  step(dt: number, reduced: boolean) {
    if (this.offset === 0 && this.age === 1) return false;
    this.offset = reduced || Math.abs(this.offset) < .05 ? 0 : this.offset * Math.exp(-dt / .06);
    this.line.style.transform = this.offset ? `translateX(${this.offset}px)` : '';
    this.age = Math.min(1, this.age + dt / .2);
    this.fresh.style.color = this.age < 1 ? `rgba(255,255,255,${.45 + .47 * this.age})` : '';
    this.fresh.style.transform = this.age < 1 ? `skewX(${-12 * (1 - clamp(this.age))}deg)` : '';
    if (this.age === 1 && this.fresh.textContent) { this.prefix.data = this.committed; this.fresh.textContent = ''; }
    return this.offset !== 0 || this.age < 1;
  }
  clear() { if (!this.committed && !this.tentative && this.offset === 0 && this.age === 1) return; this.committed = this.tentative = ''; this.offset = 0; this.age = 1; this.prefix.data = ''; this.fresh.textContent = ''; this.tentativeEl.textContent = ''; this.line.style.transform = ''; }
}
