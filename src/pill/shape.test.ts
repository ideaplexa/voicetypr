import { describe, expect, it } from 'vitest';
import { boxFor, newBox, sameRects } from '@/pill/shape';
import type { PillGeometry } from '@/pill-geometry';

const at = (anchor: PillGeometry['anchor'], anchorX: number, anchorY: number): PillGeometry => ({ anchor, anchorX, anchorY });
const dot = { x: 231, y: 419 };

describe('island shape geometry', () => {
  it('mirrors the CSS anchor rules for every position', () => {
    // centre: x - w/2; left: x; right: x - w. top: y; bottom: y - h.
    expect(boxFor(at('bottom-center', 231, 425), false, dot, false, 236, 34, 0, newBox())).toMatchObject({ left: 113, top: 391, width: 236, height: 34 });
    expect(boxFor(at('bottom-right', 451, 425), false, dot, false, 236, 34, 0, newBox())).toMatchObject({ left: 215, top: 391 });
    expect(boxFor(at('bottom-left', 11, 425), false, dot, false, 236, 34, 0, newBox())).toMatchObject({ left: 11, top: 391 });
    expect(boxFor(at('top-center', 231, 17), false, dot, true, 300, 64, 0, newBox())).toMatchObject({ left: 81, top: 17 });
    expect(boxFor(at('top-right', 451, 17), false, dot, true, 300, 64, 0, newBox())).toMatchObject({ left: 151, top: 17 });
  });
  it('places panels from the dot and applies the pick offset', () => {
    expect(boxFor(at('bottom-center', 231, 425), true, dot, false, 378, 70, 0, newBox())).toMatchObject({ left: 42, top: 366 });
    expect(boxFor(at('bottom-center', 231, 425), true, dot, false, 378, 70, -20, newBox()).top).toBe(346);
  });
  it('writes into the box it is given and allocates nothing', () => {
    const out = newBox();
    expect(boxFor(at('bottom-center', 231, 425), false, dot, false, 12, 12, 0, out)).toBe(out);
  });
  it('compares rectangles numerically within a tolerance', () => {
    const a = [{ x: 1, y: 2, width: 3, height: 4 }];
    expect(sameRects(a, [{ x: 1, y: 2, width: 3, height: 4 }])).toBe(true);
    expect(sameRects(a, [{ x: 1.004, y: 2, width: 3, height: 4 }])).toBe(true);
    expect(sameRects(a, [{ x: 1.004, y: 2, width: 3, height: 4 }], 0)).toBe(false);
    expect(sameRects(a, [])).toBe(false);
    expect(sameRects([], [])).toBe(true);
  });
});
