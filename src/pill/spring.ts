/** Island Lab v3.2 response .34 / bounce .22, integrated in <=6 ms steps. */
export class Spring {
  x: number;
  v = 0;
  to: number;
  readonly k: number;
  readonly z: number;
  constructor(x: number, response = .34, bounce = .22) {
    this.x = this.to = x;
    this.k = (Math.PI * 2 / response) ** 2;
    this.z = 1 - bounce;
  }
  step(dt: number, reduced = false) {
    if (reduced) { this.snap(); return false; }
    if (dt <= 0) return this.x !== this.to || this.v !== 0;
    const n = Math.max(1, Math.ceil(dt / .006)), h = dt / n;
    const c = 2 * Math.sqrt(this.k) * this.z;
    for (let i = 0; i < n; i++) {
      this.v += (this.k * (this.to - this.x) - c * this.v) * h;
      this.x += this.v * h;
    }
    if (Math.abs(this.to - this.x) < .01 && Math.abs(this.v) < .01) this.snap();
    return this.x !== this.to || this.v !== 0;
  }
  snap() { this.x = this.to; this.v = 0; }
}
export const clamp = (x: number, lo = 0, hi = 1) => Math.max(lo, Math.min(hi, x));
export const ease = (x: number) => 1 - (1 - clamp(x)) ** 3;
