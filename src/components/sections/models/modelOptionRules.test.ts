import { describe, expect, it } from "vitest";
import { islandLiveWordsActive, supportsFillerRemoval, supportsParagraphs } from "./modelOptionRules";

describe("model option rules", () => {
  it("offers paragraphs on Whisper, Parakeet TDT and Deepgram only", () => {
    expect(supportsParagraphs("whisper", "base.en")).toBe(true);
    expect(supportsParagraphs("parakeet", "parakeet-tdt-0.6b-v3")).toBe(true);
    expect(supportsParagraphs("parakeet", "parakeet-tdt-0.6b-v2")).toBe(true);
    expect(supportsParagraphs("parakeet", "parakeet-unified-640ms")).toBe(false);
    expect(supportsParagraphs("parakeet", "parakeet-nemotron-multilingual-1120ms")).toBe(false);
    expect(supportsParagraphs("deepgram", "deepgram")).toBe(true);
    for (const engine of ["soniox", "openai", "groq", "cohere"] as const) {
      expect(supportsParagraphs(engine, engine)).toBe(false);
    }
  });

  it("offers filler removal for English only", () => {
    expect(supportsFillerRemoval("en", false)).toBe(true);
    expect(supportsFillerRemoval("en-US", false)).toBe(true);
    expect(supportsFillerRemoval("fr", false)).toBe(false);
    expect(supportsFillerRemoval("fr", true)).toBe(true);
  });

  it("activates island live words only while streaming is on", () => {
    expect(islandLiveWordsActive(true, true)).toBe(true);
    expect(islandLiveWordsActive(true, false)).toBe(false);
    expect(islandLiveWordsActive(false, true)).toBe(false);
  });
});
