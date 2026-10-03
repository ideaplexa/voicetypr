import type { DictationContext } from '@/pill/contracts';
import { icon } from '@/pill/icons';
import { clamp, ease } from '@/pill/spring';

export class Badge {
  alpha = 0; to = 0; flight = 1;
  private amber = false;
  private lastX = NaN; private lastY = NaN; private lastAlpha = -1; private lastFlight = -1;
  constructor(readonly el: HTMLElement) {}
  target(context: DictationContext, visible: boolean, fromStart: boolean) {
    // Invalid selected Polish is amber even though availability makes will_run false.
    const amber = !context.polish.key_ok && context.polish.style !== null;
    this.to = visible && (context.polish.will_run || amber) ? 1 : 0;
    if (amber !== this.amber || !this.el.firstChild) this.el.innerHTML = icon(amber ? 'bang' : 'star',7);
    this.amber = amber; this.el.classList.toggle('amber', amber);
    if (fromStart && this.to) this.flight = 0;
  }
  step(dt: number, x: number, y: number, reduced: boolean, fromX: number, fromY: number) {
    this.alpha += Math.sign(this.to - this.alpha) * Math.min(Math.abs(this.to - this.alpha), dt / (this.to ? .15 : .08));
    this.flight = reduced ? 1 : clamp(this.flight + dt / .42);
    const f = ease(this.flight);
    if (this.lastFlight !== f) { this.el.style.transform = f < 1 ? `scale(${1.45 - .45 * f})` : ''; this.lastFlight = f; }
    if (this.lastAlpha !== this.alpha) { this.el.hidden = this.alpha === 0; this.el.style.opacity = String(this.alpha); this.lastAlpha = this.alpha; }
    const px = fromX + (x - fromX) * f, py = fromY + (y - fromY) * f;
    if (this.lastX !== px) { this.el.style.left = `${px}px`; this.lastX = px; }
    if (this.lastY !== py) { this.el.style.top = `${py}px`; this.lastY = py; }
    return this.alpha !== this.to || this.flight < 1;
  }
}
