import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

describe("RecordingPill CSS hidden guards", () => {
  it("keeps author display rules from overriding hidden-toggled pill elements", () => {
    const css = readFileSync(join(process.cwd(), "src/pill.css"), "utf8");
    const guardStart = css.indexOf(".pill-surface[hidden]");
    expect(guardStart).toBeGreaterThanOrEqual(0);

    const guardBlock = css.slice(guardStart, css.indexOf("}", guardStart) + 1);

    for (const selector of [
      ".pill-surface[hidden]",
      ".pill-rest-dot[hidden]",
      ".pill-bars[hidden]",
      ".pill-status[hidden]",
      ".pill-preview[hidden]",
      ".pill-timer[hidden]",
      ".pill-text-primary[hidden]",
      ".pill-text-pair[hidden]",
    ]) {
      expect(guardBlock).toContain(selector);
    }
    expect(guardBlock).toMatch(/display:\s*none/);
  });
  it("locks the spec surface, bars, type and error colors", () => {
    const css = readFileSync(join(process.cwd(), "src/pill.css"), "utf8");
    for (const token of [
      "#121316",
      "#ffffff1f",
      "#8fd1a8",
      "#ffffffe6",
      "#ffffff80",
      "#2a1618",
      "#f28b7a55",
      "#fecaca",
      "0 8px 24px #00000030",
      "width: 2.5px",
      "border-radius: 18px",
      "font-size: 13px",
      "Geist Mono Variable",
    ])
      expect(css).toContain(token);
  });
});

describe("RecordingPill live preview text order", () => {
  it("wraps committed + tentative in one ltr isolate inside the rtl preview box", () => {
    const css = readFileSync(join(process.cwd(), "src/pill.css"), "utf8");
    const start = css.indexOf(".pill-preview-line {");
    expect(start).toBeGreaterThanOrEqual(0);
    const block = css.slice(start, css.indexOf("}", start) + 1);
    expect(block).toMatch(/direction:\s*ltr/);
    expect(block).toMatch(/unicode-bidi:\s*isolate/);
    // The spans themselves must not force ltr at the rtl paragraph level again.
    const spans = css.slice(css.indexOf(".pill-committed,"), css.indexOf("}", css.indexOf(".pill-committed,")) + 1);
    expect(spans).not.toMatch(/direction:\s*ltr/);
  });
});
