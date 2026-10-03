import { afterEach, expect, it, vi } from "vitest";
import { applyPillGeometry, reportPillHitRegions } from "@/pill-geometry";

afterEach(() => { vi.unstubAllGlobals(); document.body.replaceChildren(); });

it("coalesces changed hit regions into one frame and stops observing on teardown", () => {
  let resize: ResizeObserverCallback | undefined;
  let mutate: MutationCallback | undefined;
  const disconnect = vi.fn();
  vi.stubGlobal("ResizeObserver", class { constructor(callback: ResizeObserverCallback) { resize = callback; } observe() {} disconnect = disconnect; });
  vi.stubGlobal("MutationObserver", class { constructor(callback: MutationCallback) { mutate = callback; } observe() {} disconnect = disconnect; });
  const frames: FrameRequestCallback[] = [];
  const raf = vi.fn((callback: FrameRequestCallback) => { frames.push(callback); return frames.length; });
  const cancel = vi.fn();
  vi.stubGlobal("requestAnimationFrame", raf);
  vi.stubGlobal("cancelAnimationFrame", cancel);
  const root = document.createElement("div");
  const surface = document.createElement("div");
  root.append(surface);
  document.body.append(root);
  vi.spyOn(surface, "getBoundingClientRect").mockReturnValue({ x: 100, y: 380, width: 120, height: 34 } as DOMRect);
  const send = vi.fn();
  const stop = reportPillHitRegions(surface, send);
  resize?.([], {} as ResizeObserver);
  mutate?.([], {} as MutationObserver);
  expect(raf).toHaveBeenCalledTimes(1);
  frames.shift()?.(0);
  expect(send).toHaveBeenLastCalledWith([{ x: 100, y: 380, width: 120, height: 34 }]);
  resize?.([], {} as ResizeObserver);
  frames.shift()?.(16);
  expect(send).toHaveBeenCalledTimes(1);
  surface.hidden = true;
  mutate?.([], {} as MutationObserver);
  frames.shift()?.(32);
  expect(send).toHaveBeenLastCalledWith([]);
  mutate?.([], {} as MutationObserver);
  stop();
  expect(disconnect).toHaveBeenCalledTimes(2);
  expect(cancel).toHaveBeenCalledOnce();
});

it("applies the native anchor in window coordinates", () => {
  const root = document.createElement("div");
  applyPillGeometry(root, { anchor: "top-right", anchorX: 440, anchorY: 6 });
  expect(root.dataset.pillPosition).toBe("top-right");
  expect(root.style.getPropertyValue("--pill-anchor-x")).toBe("440px");
  expect(root.style.getPropertyValue("--pill-anchor-y")).toBe("6px");
});
