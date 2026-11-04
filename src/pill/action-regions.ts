import type { Rect } from '@/pill/shape';

/** Cursor targets are enabled buttons, not the broad click-through surfaces.
 * Clip to those surfaces while they animate; ghosts/back cards never qualify.
 */
export function enabledActionRects(root: HTMLElement, surfaces: readonly Rect[]): Rect[] {
  const rects: Rect[] = [];
  for (const button of root.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')) {
    if (button.closest('[hidden],[inert],[aria-hidden="true"],.back')) continue;
    let visible = true;
    for (let el: HTMLElement | null = button; el && el !== root.parentElement; el = el.parentElement) {
      if (el.inert || el.style.opacity === '0') { visible = false; break; }
    }
    if (!visible) continue;
    const style = getComputedStyle(button);
    if (style.visibility === 'hidden' || style.pointerEvents === 'none') continue;
    const b = button.getBoundingClientRect();
    if (b.width <= 0 || b.height <= 0) continue;
    let clipped: Rect | undefined;
    for (const surface of surfaces) {
      const x = Math.max(b.x, surface.x), y = Math.max(b.y, surface.y);
      const right = Math.min(b.right, surface.x + surface.width), bottom = Math.min(b.bottom, surface.y + surface.height);
      const width = right - x, height = bottom - y;
      if (width > 0 && height > 0 && (!clipped || width * height > clipped.width * clipped.height)) clipped = { x, y, width, height };
    }
    if (clipped) rects.push(clipped); // Overlapping back surfaces must not duplicate a front button.
  }
  return rects;
}
