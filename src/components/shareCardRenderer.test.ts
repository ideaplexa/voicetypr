import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { drawShareCard, toShareCardStats } from "@/components/shareCardRenderer";
import { createUsageFixture } from "@/ui-preview/usageFixture";
const context = {
  resetTransform: vi.fn(),
  scale: vi.fn(),
  beginPath: vi.fn(),
  roundRect: vi.fn(),
  fill: vi.fn(),
  fillText: vi.fn(),
  measureText: vi.fn((value: string) => ({ width: value.length * 10 })),
  save: vi.fn(),
  translate: vi.fn(),
  restore: vi.fn(),
  stroke: vi.fn(),
  arc: vi.fn(),
};
const originalFonts = Object.getOwnPropertyDescriptor(document, "fonts");
beforeEach(() => {
  Object.defineProperty(document, "fonts", {
    configurable: true,
    value: { load: vi.fn().mockResolvedValue([]), ready: Promise.resolve() },
  });
  vi.stubGlobal(
    "Path2D",
    class {
      constructor(public path: string) {}
    },
  );
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
    context as unknown as CanvasRenderingContext2D,
  );
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue(
    "data:image/png;base64,c3RhdHM=",
  );
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  if (originalFonts) Object.defineProperty(document, "fonts", originalFonts);
  else Reflect.deleteProperty(document, "fonts");
});
describe("share card renderer", () => {
  it("exports exactly 1200×630, loads bundled fonts and strokes the real brandmark", async () => {
    const canvas = document.createElement("canvas");
    const stats = toShareCardStats(createUsageFixture());
    expect(await drawShareCard(canvas, stats, () => false)).toMatch(/^data:image\/png/);
    expect([canvas.width, canvas.height]).toEqual([1200, 630]);
    expect(document.fonts.load).toHaveBeenCalledWith('600 58px "Geist Mono Variable"');
    expect(context.stroke).toHaveBeenCalledWith(
      expect.objectContaining({ path: "M143 197 C166 288 201 350 240 393" }),
    );
    expect(context.stroke).toHaveBeenCalledTimes(7);
    expect(context.fillText).toHaveBeenCalledWith("words spoken, not typed", 30, 193);
    expect(context.fillText).toHaveBeenCalledWith("voicetypr.com", 570, 285);
  });
  it("draws 14 weeks of cells and leaves future dates blank", async () => {
    const stats = toShareCardStats(createUsageFixture());
    await drawShareCard(document.createElement("canvas"), stats, () => false);
    const cells = context.roundRect.mock.calls.filter((args) => args[2] === 9 && args[3] === 9);
    expect(cells.length).toBeGreaterThanOrEqual(92);
    expect(cells.length).toBeLessThanOrEqual(98);
    expect(Math.max(...cells.map((args) => Number(args[0])))).toBe(561);
  });
  it("omits speaking pace for missing audio", async () => {
    const stats = { ...toShareCardStats(createUsageFixture()), pace: null };
    await drawShareCard(document.createElement("canvas"), stats, () => false);
    expect(context.fillText.mock.calls.some((args) => args[0] === "speaking pace")).toBe(false);
  });
  it("does not export cancelled renders or missing canvas contexts", async () => {
    const stats = toShareCardStats(createUsageFixture());
    expect(await drawShareCard(document.createElement("canvas"), stats, () => true)).toBeNull();
    expect(HTMLCanvasElement.prototype.toDataURL).not.toHaveBeenCalled();
    vi.mocked(HTMLCanvasElement.prototype.getContext).mockReturnValue(null);
    expect(await drawShareCard(document.createElement("canvas"), stats, () => false)).toBeNull();
  });
});
