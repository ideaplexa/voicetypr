import { describe, expect, it } from "vitest";
import {
  formatNativeEffort,
  isNativeProvider,
  nativeEffortLevels,
  resolveNativeEffort,
} from "./nativeProvider";

describe("isNativeProvider", () => {
  it("matches the direct API key providers only", () => {
    expect(isNativeProvider("openai")).toBe(true);
    expect(isNativeProvider("anthropic")).toBe(true);
    expect(isNativeProvider("gemini")).toBe(true);
    expect(isNativeProvider("claude-code")).toBe(false);
    expect(isNativeProvider("openrouter")).toBe(false);
    expect(isNativeProvider("custom")).toBe(false);
  });
});

describe("nativeEffortLevels", () => {
  it("keeps only contract levels, preserving order", () => {
    expect(nativeEffortLevels(["xhigh", "low", "high", "max", "none"])).toEqual([
      "low",
      "high",
      "none",
    ]);
    expect(nativeEffortLevels(undefined)).toEqual([]);
    expect(nativeEffortLevels(["xhigh"])).toEqual([]);
  });
});

describe("resolveNativeEffort", () => {
  it("returns the stored level when the model supports it", () => {
    expect(resolveNativeEffort("high", ["none", "low", "medium", "high"])).toBe("high");
    expect(resolveNativeEffort("none", ["none", "minimal", "low"])).toBe("none");
  });
  it("picks the highest supported level below medium", () => {
    expect(resolveNativeEffort("none", ["low", "high"])).toBe("low");
    expect(resolveNativeEffort(undefined, ["none", "minimal", "high"])).toBe("minimal");
    expect(resolveNativeEffort(undefined, ["none", "high"])).toBe("none");
  });

  it("falls back to medium when the stored level is unsupported or missing", () => {
    expect(resolveNativeEffort("xhigh", ["none", "low", "medium", "high"])).toBe("medium");
    expect(resolveNativeEffort(undefined, ["minimal", "medium", "high"])).toBe("medium");
    expect(resolveNativeEffort("off", ["low", "medium"])).toBe("medium");
  });

  it("falls back to the lowest supported level", () => {
    expect(resolveNativeEffort("low", ["high"])).toBe("high");
    expect(resolveNativeEffort(undefined, ["high"])).toBe("high");
    expect(resolveNativeEffort(undefined, ["minimal", "high"])).toBe("minimal");
  });

  it("returns undefined when the model supports no effort", () => {
    expect(resolveNativeEffort("medium", [])).toBeUndefined();
    expect(resolveNativeEffort(undefined, [])).toBeUndefined();
  });
});

describe("formatNativeEffort", () => {
  it("labels none as Off and capitalizes the rest", () => {
    expect(formatNativeEffort("none")).toBe("Off");
    expect(formatNativeEffort("minimal")).toBe("Minimal");
    expect(formatNativeEffort("low")).toBe("Low");
    expect(formatNativeEffort("medium")).toBe("Medium");
    expect(formatNativeEffort("high")).toBe("High");
  });
});
