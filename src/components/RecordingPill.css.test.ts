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
      ".pill-dots[hidden]",
      ".pill-bars[hidden]",
      ".pill-status[hidden]",
      ".pill-preview[hidden]",
    ]) {
      expect(guardBlock).toContain(selector);
    }
    expect(guardBlock).toMatch(/display:\s*none/);
  });
});
