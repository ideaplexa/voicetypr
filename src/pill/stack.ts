import type { FeedbackCard } from '@/pill/feedback';
export interface StackItem extends FeedbackCard { serial: number; count: number; left: number }
/** Arrival-order retirement, independent of display order. Expiry is a real clip lease. */
export class FeedbackStack {
  items: StackItem[] = [];
  tucked = true;
  private serial = 0;
  private last = 0;
  advance(now: number) {
    const dt = Math.max(0,now-this.last); this.last = now;
    if (!this.tucked) for (const i of this.items) if (i.duration !== null) i.left -= dt;
    this.items = this.items.filter(i => i.expiresAt === undefined || i.expiresAt > now);
    if (!this.tucked) for (;;) {
      const oldest = this.items.filter(i => i.duration !== null).sort((a,b) => a.serial-b.serial)[0];
      if (!oldest || oldest.left > 0) break;
      this.remove(oldest.key);
    }
  }
  tuck(value: boolean, now: number) { this.advance(now); this.tucked=value; }
  push(card: FeedbackCard, now: number) {
    this.advance(now);
    const existing = this.items.find(i => i.key === card.key);
    if (existing) { Object.assign(existing,card); existing.count++; existing.left=Math.max(existing.left,card.duration ?? 0); this.items=this.items.filter(i=>i!==existing).concat(existing); }
    else this.items.push({...card,serial:++this.serial,count:1,left:card.duration ?? 0});
  }
  remove(key: string) { this.items=this.items.filter(i=>i.key!==key); }
  get visible() { return this.items.slice(-3).reverse(); }
  get more() { return Math.max(0,this.items.length-3); }
  get nextDelay() {
    const leases = this.items.flatMap(i => i.expiresAt === undefined ? [] : [Math.max(0,i.expiresAt-this.last)]);
    const timed = this.items.filter(i=>i.duration !== null).sort((a,b)=>a.serial-b.serial)[0];
    if (!this.tucked && timed) leases.push(Math.max(0,timed.left));
    return leases.length ? Math.min(...leases) : null;
  }
}
