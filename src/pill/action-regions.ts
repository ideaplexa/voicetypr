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
    let left = b.left, top = b.top, right = b.right, bottom = b.bottom;
    for (let el = button.parentElement; el && el !== root.parentElement; el = el.parentElement) {
      const style = getComputedStyle(el);
      const clipsX = /^(auto|scroll|hidden|clip)$/.test(style.overflowX || style.overflow);
      const clipsY = /^(auto|scroll|hidden|clip)$/.test(style.overflowY || style.overflow);
      if (!clipsX && !clipsY) continue;
      const viewport = el.getBoundingClientRect();
      if (clipsX) { left = Math.max(left, viewport.left); right = Math.min(right, viewport.right); }
      if (clipsY) { top = Math.max(top, viewport.top); bottom = Math.min(bottom, viewport.bottom); }
    }
    if (right <= left || bottom <= top) continue;
    let clipped: Rect | undefined;
    for (const surface of surfaces) {
      const x = Math.max(left, surface.x), y = Math.max(top, surface.y);
      const width = Math.min(right, surface.x + surface.width) - x, height = Math.min(bottom, surface.y + surface.height) - y;
      if (width > 0 && height > 0 && (!clipped || width * height > clipped.width * clipped.height)) clipped = { x, y, width, height };
    }
    if (clipped) rects.push(clipped); // Overlapping back surfaces must not duplicate a front button.
  }
  return rects;
}
