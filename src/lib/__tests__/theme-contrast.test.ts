import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(process.cwd(), "src/globals.css"), "utf8");

function tokens(selector: string): Record<string, string> {
  const block = css.match(new RegExp(`${selector}\\s*\\{([^}]+)\\}`))?.[1];
  if (!block) throw new Error(`Missing ${selector} tokens`);
  return Object.fromEntries([...block.matchAll(/--([\w-]+):\s*(#[\da-fA-F]{6})\s*;/g)].map((match) => [match[1], match[2]]));
}

function luminance(hex: string): number {
  const [red, green, blue] = [1, 3, 5].map((index) => {
    const value = Number.parseInt(hex.slice(index, index + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return red * 0.2126 + green * 0.7152 + blue * 0.0722;
}

function contrast(foreground: string, background: string): number {
  const [lighter, darker] = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (lighter + 0.05) / (darker + 0.05);
}

describe("small UI text contrast", () => {
  const pairs = [
    ...["background", "card", "muted", "sage-bg", "warn-bg"].map((background) => ["foreground", background]),
    ...["background", "card", "muted"].map((background) => ["muted-foreground", background]),
    ...["background", "card"].map((background) => ["sage", background]),
  ];

  it.each([":root", "\\.dark"])("keeps all small text token pairs at 4.5:1 in %s", (selector) => {
    const theme = tokens(selector);
    for (const [foreground, background] of pairs) {
      expect(theme[foreground], `missing ${foreground}`).toBeDefined();
      expect(theme[background], `missing ${background}`).toBeDefined();
      expect(contrast(theme[foreground], theme[background]), `${selector}: ${foreground} on ${background}`).toBeGreaterThanOrEqual(4.5);
    }
  });
});
