import type { PillIndicatorPosition } from "@/types";

export interface PillGeometry {
  anchor: PillIndicatorPosition;
  anchorX: number;
  anchorY: number;
}
export function applyPillGeometry(root: HTMLElement, geometry: PillGeometry) {
  root.dataset.pillPosition = geometry.anchor;
  root.style.setProperty("--pill-anchor-x", `${geometry.anchorX}px`);
  root.style.setProperty("--pill-anchor-y", `${geometry.anchorY}px`);
}

/** Observe only layout/state changes; idle has no JS animation loop. */
export function reportPillHitRegions(
  surface: HTMLElement,
  send: (rects: Array<{ x: number; y: number; width: number; height: number }>) => void,
) {
  let frame: number | undefined;
  let previous = "";
  const report = () => {
    frame = undefined;
    const bounds = surface.getBoundingClientRect();
    const rects = surface.hidden ? [] : [{ x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height }];
    const signature = JSON.stringify(rects);
    if (signature !== previous) {
      previous = signature;
      send(rects);
    }
  };
  const schedule = () => {
    if (frame === undefined) frame = requestAnimationFrame(report);
  };
  const resize = new ResizeObserver(schedule);
  resize.observe(surface);
  const mutation = new MutationObserver(schedule);
  mutation.observe(surface.parentElement ?? surface, { attributes: true, subtree: true, attributeFilter: ["hidden", "style", "data-pill-position", "data-state", "data-preview"] });
  schedule();
  return () => {
    resize.disconnect();
    mutation.disconnect();
    if (frame !== undefined) cancelAnimationFrame(frame);
  };
}
