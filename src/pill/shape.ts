import type { PillGeometry } from '@/pill-geometry';

/**
 * Island geometry as pure arithmetic: where the surface sits for a given size, derived
 * from the spring values. Hit regions, hover and the shadow use it, so nothing measures
 * the DOM per frame. Nothing in this file reads the DOM.
 */
export interface Box { left: number; top: number; width: number; height: number }
export interface Point { x: number; y: number }
export const newBox = (): Box => ({ left: 0, top: 0, width: 12, height: 12 });

/**
 * Where a surface of `width x height` sits in viewport px. Mirrors the CSS anchor rules
 * (centre / left / right by the anchor's suffix, top / bottom by its prefix) and, for the
 * peek and pick panels, the dot-relative placement. Writes into `out`; allocates nothing.
 */
export function boxFor(g: PillGeometry, panel: boolean, dot: Point, top: boolean, width: number, height: number, panelOffset: number, out: Box): Box {
  if (panel) {
    const insetX = Math.min(17, width / 2), insetY = Math.min(17, height / 2);
    out.left = g.anchor.endsWith('-left') ? dot.x - insetX : g.anchor.endsWith('-right') ? dot.x - width + insetX : dot.x - width / 2;
    out.top = (top ? dot.y - insetY : dot.y - height + insetY) + panelOffset;
  } else {
    out.left = g.anchor.endsWith('-left') ? g.anchorX : g.anchor.endsWith('-right') ? g.anchorX - width : g.anchorX - width / 2;
    out.top = top ? g.anchorY : g.anchorY - height;
  }
  out.width = width; out.height = height;
  return out;
}

export interface Rect { x: number; y: number; width: number; height: number }
/** Numeric comparison with a sub-pixel tolerance; no serialisation. */
export function sameRects(a: readonly Rect[], b: readonly Rect[], epsilon = .01): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    const p = a[i], q = b[i];
    if (Math.abs(p.x - q.x) > epsilon || Math.abs(p.y - q.y) > epsilon || Math.abs(p.width - q.width) > epsilon || Math.abs(p.height - q.height) > epsilon) return false;
  }
  return true;
}
