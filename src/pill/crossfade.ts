/** Reduced motion keeps the outgoing geometry intact for a 150 ms cross-fade. */
export class Crossfade {
  private old: HTMLElement | undefined;
  private time = .15;
  constructor(readonly surface: HTMLElement) {}
  capture() {
    this.old?.remove();
    if (this.surface.hidden) return;
    const copy = this.surface.cloneNode(true) as HTMLElement;
    copy.classList.add('island-ghost'); copy.inert = true;
    copy.setAttribute('aria-hidden', 'true');
    for (const el of copy.querySelectorAll('[data-testid]')) el.removeAttribute('data-testid');
    const originalCanvas = this.surface.querySelector('canvas'), canvas = copy.querySelector('canvas');
    if (originalCanvas && canvas) canvas.getContext('2d')?.drawImage(originalCanvas,0,0);
    this.surface.after(copy); this.old = copy; this.time = 0;
    this.surface.style.opacity = '0';
  }
  step(dt: number) {
    if (!this.old) return false;
    this.time = Math.min(.15, this.time + dt);
    this.old.style.opacity = String(1 - this.time / .15);
    this.surface.style.opacity = String(this.time / .15);
    if (this.time === .15) this.clear();
    return this.old !== undefined;
  }
  clear() { this.old?.remove(); this.old = undefined; this.surface.style.opacity = ''; }
}
