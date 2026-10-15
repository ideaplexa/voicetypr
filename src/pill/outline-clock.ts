const SVG = 'http://www.w3.org/2000/svg';
export type ClockTone = 'neutral' | 'amber' | 'red' | 'sage';

/**
 * A card's own outline as its countdown (plan 082 G1). One rounded-rect stroke hugging the card's
 * padding box drains over the lease; the stroke is a CSS animation on `stroke-dashoffset`, so
 * nothing runs per frame in JS. Reduced motion shows the static outline (see pill.css).
 */
export function outlinePath(boxWidth: number, boxHeight: number, radius: number) {
  // The stroke sits one px inside the padding box (the card has a 1 px border outside it).
  const w = Math.max(0, boxWidth - 2), h = Math.max(0, boxHeight - 2);
  const r = Math.max(0, Math.min(radius, boxWidth / 2, boxHeight / 2) - 2);
  const x0 = 1, y0 = 1, x1 = w - 1, y1 = h - 1, cx = w / 2;
  const n = (v: number) => String(Math.round(v * 100) / 100);
  const arc = (x: number, y: number) => `A${n(r)} ${n(r)} 0 0 1 ${n(x)} ${n(y)}`;
  // From the top centre, clockwise.
  return `M${n(cx)} ${n(y0)}H${n(x1 - r)}${arc(x1, y0 + r)}V${n(y1 - r)}${arc(x1 - r, y1)}H${n(x0 + r)}${arc(x0, y1 - r)}V${n(y0 + r)}${arc(x0 + r, y0)}H${n(cx)}`;
}

export class OutlineClock {
  readonly el = document.createElementNS(SVG, 'svg');
  private path: SVGPathElement | undefined;
  private key = '';
  private d = '';
  private paused = false;
  constructor(tone: ClockTone = 'neutral') {
    this.el.setAttribute('class', `outline-clock ${tone}`); this.el.setAttribute('aria-hidden', 'true');
  }
  /** Fit the stroke to a card of this border-box size and corner radius. Cheap when nothing changed. */
  size(boxWidth: number, boxHeight: number, radius: number) {
    const key = `${Math.round(boxWidth * 10)}|${Math.round(boxHeight * 10)}|${radius}`;
    if (key === this.key) return; this.key = key;
    this.el.setAttribute('width', String(Math.max(0, boxWidth - 2))); this.el.setAttribute('height', String(Math.max(0, boxHeight - 2)));
    this.d = outlinePath(boxWidth, boxHeight, radius);
    this.path?.setAttribute('d', this.d);
  }
  /** Restart: `remainingMs` of a `totalMs` lease are left, so the stroke starts partly drained. */
  start(totalMs: number, remainingMs = totalMs) {
    const path = document.createElementNS(SVG, 'path');
    path.setAttribute('d', this.d); path.setAttribute('pathLength', '1');
    path.style.animationDuration = `${Math.max(0, Math.round(remainingMs))}ms`;
    path.style.setProperty('--clock-from', String(Math.min(1, Math.max(0, 1 - remainingMs / Math.max(1, totalMs)))));
    path.style.animationPlayState = this.paused ? 'paused' : 'running';
    if (this.path) this.path.replaceWith(path); else this.el.append(path);
    this.path = path;
  }
  /** Holds the countdown (hover) and resumes it from the same point. */
  pause(paused: boolean) {
    this.paused = paused;
    if (this.path) this.path.style.animationPlayState = paused ? 'paused' : 'running';
  }
}
