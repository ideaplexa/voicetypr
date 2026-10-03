import { clamp } from '@/pill/spring';
import { WaveBuffer } from '@/pill/wave';
import type { IslandState } from '@/pill/island';
const TAU = Math.PI * 2;
type Color = readonly [number,number,number,number];
const WHITE: Color = [255,255,255,.95], SAGE: Color = [143,209,168,1], AMBER: Color = [242,179,91,1], RED: Color = [242,139,122,1];
const color = (from: Color, to: Color, t: number) => `rgba(${Math.round(from[0]+(to[0]-from[0])*t)},${Math.round(from[1]+(to[1]-from[1])*t)},${Math.round(from[2]+(to[2]-from[2])*t)},${from[3]+(to[3]-from[3])*t})`;
type Kind = 'wave' | 'spin' | 'check' | 'clip' | 'quiet' | 'bang' | 'none';
/** One shared canvas: bars converge at the arc, then its actual tail closes into the outcome. */
export class Glyph {
  readonly wave = new WaveBuffer();
  private g: CanvasRenderingContext2D | null;
  private scale = 0;
  kind: Kind = 'none';
  arc = 0; targetArc = 0; end = 1; angle = -Math.PI / 2; captured = -Math.PI / 2;
  private sage = false;
  private spinAge = 1;
  private spinFrom: Color = WHITE;
  private capturedColor: Color = WHITE;
  private capturedLength = 0;
  private alpha = 0;
  private visible = true;
  private error = false;
  constructor(readonly canvas: HTMLCanvasElement) { this.g = canvas.getContext('2d'); }
  target(state: IslandState, visible = true) {
    const kind: Kind = ['start', 'listening', 'live'].includes(state) ? 'wave' : state === 'transcribing' || state === 'polishing' ? 'spin' : state === 'pasted' ? 'check' : state === 'copied' || state === 'no_permission' ? 'clip' : state === 'nospeech' ? 'quiet' : state === 'error' ? 'bang' : 'none';
    // On a new take, keep the outgoing mark while its arc unfolds back into bars.
    const drawKind = kind === 'wave' && this.arc > .01 && this.alpha > .05 ? this.kind : kind;
    if (drawKind !== this.kind) {
      this.capturedLength = this.kind === 'spin' && this.arc > .3 && this.alpha > .3 ? 1.9 : 0;
      this.capturedColor = this.spinColor();
      this.captured = this.angle; this.end = 0;
      this.kind = drawKind;
    }
    this.visible = visible; this.error = state === 'error';
    const sage = state === 'polishing';
    if (sage !== this.sage) { this.spinFrom = this.spinColor(); this.spinAge = 0; this.sage = sage; } this.targetArc = kind === 'wave' || kind === 'none' ? 0 : 1;
  }
  private spinColor(): Color {
    const to = this.sage ? SAGE : WHITE, t = this.spinAge;
    return [this.spinFrom[0]+(to[0]-this.spinFrom[0])*t,this.spinFrom[1]+(to[1]-this.spinFrom[1])*t,this.spinFrom[2]+(to[2]-this.spinFrom[2])*t,this.spinFrom[3]+(to[3]-this.spinFrom[3])*t];
  }
  step(dt: number, reduced: boolean) {
    this.spinAge = reduced ? 1 : Math.min(1,this.spinAge + dt / .2);
    this.arc += reduced ? Math.sign(this.targetArc - this.arc) * Math.min(Math.abs(this.targetArc - this.arc), dt / .15) : (this.targetArc - this.arc) * (1 - Math.exp(-dt / .09));
    if (Math.abs(this.targetArc - this.arc) < .001) this.arc = this.targetArc;
    const target = this.kind === 'none' || !this.visible ? 0 : 1;
    this.alpha += Math.sign(target - this.alpha) * Math.min(Math.abs(target - this.alpha), dt / .15);
    this.end = reduced ? 1 : Math.min(1, this.end + dt / .45);
    if (this.kind === 'spin' && !reduced) this.angle = (this.angle + dt * (this.sage ? 4.2 : 6.5)) % TAU;
    return this.arc !== this.targetArc || this.alpha !== target || this.end < 1 || (this.kind === 'spin' && (!reduced || this.spinAge < 1));
  }
  draw(seconds: number, width: number, reduced: boolean) {
    const g = this.g; if (!g) return;
    const scale = 2 * (window.devicePixelRatio || 1);
    if (this.scale !== scale) { this.scale = scale; this.canvas.width = Math.ceil(260 * scale); this.canvas.height = Math.ceil(24 * scale); }
    g.setTransform(scale, 0, 0, scale, 0, 0); g.clearRect(0, 0, 260, 24);
    if (this.alpha === 0) return;
    const ba = this.alpha * (1 - this.arc) ** 3;
    if (ba > .01) this.wave.draw(g, seconds, width, reduced, ba, reduced ? 0 : this.arc);
    if (this.arc <= .01) return;
    g.globalAlpha = this.alpha * this.arc; g.lineWidth = 2; g.lineCap = 'round'; g.lineJoin = 'round';
    const ring = clamp(this.end / .6), stroke = clamp((this.end - .55) / .45), cx = 11, cy = 12;
    const resultColor = this.kind === 'check' ? SAGE : this.error ? RED : this.kind === 'clip' || this.kind === 'bang' ? AMBER : WHITE;
    const strokeColor = this.kind === 'spin' ? this.spinAge === 1 ? this.sage ? '#8FD1A8' : 'rgba(255,255,255,.95)' : color(this.spinFrom,this.sage ? SAGE : WHITE,this.spinAge) : color(this.capturedColor,resultColor,clamp(this.end / .25));
    if (this.kind === 'clip') {
      g.fillStyle = 'rgba(242,179,91,.16)'; g.beginPath(); g.arc(cx, cy, 10, 0, TAU); g.fill();
      if (this.capturedLength > 0 && ring < 1 && !reduced) { g.globalAlpha *= 1 - ring; g.strokeStyle = strokeColor; g.beginPath(); g.arc(cx,cy,7,this.captured + this.capturedLength * ring,this.captured + this.capturedLength); g.stroke(); g.globalAlpha = this.alpha * this.arc; }
      g.strokeStyle = strokeColor; g.lineWidth = 1.8; g.setLineDash([46 * ring, 46]); g.beginPath(); g.roundRect(cx - 5, cy - 5.5, 10, 13, 2); g.stroke(); g.setLineDash([]);
      g.globalAlpha *= stroke; g.fillStyle = strokeColor; g.beginPath(); g.roundRect(cx - 2.8, cy - 7.6, 5.6, 3.6, 1.2); g.fill(); g.fillRect(cx - 2.6, cy - .2, 5.2, 1.4); g.fillRect(cx - 2.6, cy + 2.8, 3.4, 1.4);
    } else {
      if (this.kind === 'quiet' || this.kind === 'bang') { g.fillStyle = this.kind === 'quiet' ? 'rgba(255,255,255,.16)' : this.error ? 'rgba(242,139,122,.16)' : 'rgba(242,179,91,.16)'; g.globalAlpha *= ring; g.beginPath(); g.arc(cx,cy,10,0,TAU); g.fill(); g.globalAlpha = this.alpha * this.arc; }
      if (this.kind !== 'quiet') { g.strokeStyle = 'rgba(255,255,255,.14)'; g.beginPath(); g.arc(cx, cy, 7, 0, TAU); g.stroke(); }
      g.strokeStyle = this.kind === 'quiet' ? 'rgba(255,255,255,.62)' : strokeColor;
      if (this.kind === 'quiet') g.setLineDash([2.6, 2.4]);
      g.beginPath();
      if (this.kind === 'spin') g.arc(cx, cy, 7, reduced ? -Math.PI / 2 : this.angle, (reduced ? -Math.PI / 2 : this.angle) + 1.9);
      else g.arc(cx, cy, 7, this.captured, this.captured + this.capturedLength + (TAU - this.capturedLength) * ring);
      g.stroke(); g.setLineDash([]);
      if (this.kind === 'check' && stroke > 0) {
        const l1 = Math.hypot(2.2, 2.4), l2 = Math.hypot(4.4, 5), d = (l1 + l2) * stroke;
        g.lineWidth = 1.8; g.beginPath(); g.moveTo(cx - 3.2, cy + .2);
        if (d < l1) g.lineTo(cx - 3.2 + 2.2 * d / l1, cy + .2 + 2.4 * d / l1);
        else { g.lineTo(cx - 1, cy + 2.6); g.lineTo(cx - 1 + 4.4 * (d - l1) / l2, cy + 2.6 - 5 * (d - l1) / l2); } g.stroke();
      }
      if (this.kind === 'bang') { g.beginPath(); g.moveTo(cx, cy - 3.8); g.lineTo(cx, cy - 3.8 + 4.4 * stroke); g.stroke(); if (stroke > .8) { g.fillStyle = strokeColor; g.beginPath(); g.arc(cx, cy + 3.3, 1.15, 0, TAU); g.fill(); } }
    }
    g.globalAlpha = 1;
  }
}
