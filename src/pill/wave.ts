import { clamp, ease } from '@/pill/spring';
export const BAR_SECONDS = .07, BAR_PITCH = 4;
const CAPACITY = 96;
/** Fixed ring; only elapsed slices exist. Peak samples are never synthetic. */
export class WaveBuffer {
  readonly times = new Float64Array(CAPACITY);
  readonly levels = new Float32Array(CAPACITY);
  count = 0;
  head = 0;
  next = BAR_SECONDS;
  peak = 0;
  level = 0;
  envelope = 0;
  private gradient: CanvasGradient | undefined;
  private gradientWidth = -1;
  readonly heights = new Float32Array(CAPACITY);
  reset() { this.count = this.head = 0; this.next = BAR_SECONDS; this.peak = this.level = this.envelope = 0; }
  sample(level: number) { this.level = clamp(level); this.peak = Math.max(this.peak, this.level); }
  advance(seconds: number, dt: number) {
    this.envelope += (this.level - this.envelope) * (1 - Math.exp(-dt / (this.level > this.envelope ? .03 : .18)));
    while (seconds + 1e-9 >= this.next) {
      this.times[this.head] = this.next; this.levels[this.head] = this.peak;
      const seed = Math.sin(Math.round(this.next / BAR_SECONDS) * 12.9898) * 43758.5453;
      this.heights[this.head] = this.peak > .02 ? 14 * this.peak ** .7 * (.8 + .2 * (seed - Math.floor(seed))) : 0;
      this.head = (this.head + 1) % CAPACITY; this.count = Math.min(CAPACITY, this.count + 1);
      this.peak = 0; this.next += BAR_SECONDS;
    }
  }
  draw(g: CanvasRenderingContext2D, seconds: number, width: number, reduced: boolean, alpha = 1, arc = 0) {
    g.save(); g.beginPath(); g.rect(0, 0, width, 24); g.clip();
    if (reduced) {
      const n = Math.max(5, Math.min(15, Math.floor(width / 8))), x0 = (width - (n - 1) * 8) / 2;
      g.fillStyle = '#fff';
      for (let i = 0; i < n; i++) {
        const lv = clamp(this.envelope * 1.6) * (.5 + .5 * Math.sin(Math.PI * (i + .5) / n));
        const h = 2 + 14 * lv ** .7;
        g.globalAlpha = alpha * (lv > .03 ? .93 : .34); g.beginPath(); g.roundRect(x0 + i * 8 - 1.2, 12 - h / 2, 2.4, h, 1.2); g.fill();
      }
    } else {
      if (!this.gradient || this.gradientWidth !== width) {
        this.gradientWidth = width; this.gradient = g.createLinearGradient(0, 0, Math.max(1, width), 0);
        this.gradient.addColorStop(0, 'rgba(255,255,255,0)');
        const fade = Math.min(1, 24 / Math.max(1, width));
        this.gradient.addColorStop(fade, `rgba(255,255,255,${.45 + .55 * fade})`); this.gradient.addColorStop(1, '#fff');
      }
      g.fillStyle = this.gradient;
      for (let i = 0; i < this.count; i++) {
        const ix = (this.head - 1 - i + CAPACITY) % CAPACITY;
        const age = Math.max(0, seconds - this.times[ix]);
        const x0 = width - 2.2 - age * BAR_PITCH / BAR_SECONDS;
        if (x0 < -2) break;
        const x = x0 + (11 - x0) * arc, lv = this.levels[ix];
        const h = 2 + this.heights[ix] * ease(age / .06) * (1 - arc);
        g.globalAlpha = alpha * (lv > .02 ? .93 : .34); g.beginPath(); g.roundRect(x - 1.2, 12 - h / 2, 2.4, h, 1.2); g.fill();
      }
    }
    g.restore();
  }
}
