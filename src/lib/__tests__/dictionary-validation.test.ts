import { describe, expect, it } from "vitest";
import { languages } from "@/components/languages";
import { dictionaryEntryError, dictionaryLanguageError, normalizeDictionaryLanguage } from "@/lib/dictionary-validation";
import { defaultWritingSettings } from "@/types/writing";

describe("dictionary language validation", () => {
  it("accepts empty scopes and every picker language case-insensitively", () => {
    for (const code of [null, undefined, "", "  ", ...languages.map(({ value }) => value)]) {
      expect(dictionaryLanguageError(code)).toBeNull();
      if (code) expect(dictionaryLanguageError(` ${code.toUpperCase()} `)).toBeNull();
    }
  });

  it.each(["d", "unknown", "auto", "de-", "de_CH_extra", "english", "KO", "en-1A", "de-äö", "zh-Hant"])(
    "rejects unsupported code %s",
    (code) => expect(dictionaryLanguageError(code)).toContain("supported language code"),
  );

  it.each(["de-AT", "de-CH", "de_DE", " DE-at "])("normalizes regional code %s to German", (code) => {
    expect(dictionaryLanguageError(code)).toBeNull();
    expect(normalizeDictionaryLanguage(code)).toBe("de");
  });

  it("supports English and numeric regions and detects overlapping regional scopes", () => {
    expect(normalizeDictionaryLanguage("en-GB")).toBe("en");
    expect(normalizeDictionaryLanguage("es-419")).toBe("es");
    const existing = { phrase: "Alpha", language: "de-AT", enabled: true };
    const settings = { ...defaultWritingSettings, custom_words: [existing] };
    expect(dictionaryEntryError("words", { ...existing, language: "de-CH" }, settings, null))
      .toContain("already exists");
  });

  it("normalizes case and whitespace when detecting duplicate scopes", () => {
    const existing = { phrase: "Alpha", language: "de", enabled: true };
    const settings = { ...defaultWritingSettings, custom_words: [existing] };
    expect(dictionaryEntryError("words", { ...existing, language: " DE " }, settings, null))
      .toContain("already exists");
  });

  it("does not reinterpret a legacy invalid scope as English", () => {
    const existing = { phrase: "Alpha", language: "d", enabled: true };
    const settings = { ...defaultWritingSettings, custom_words: [existing] };
    expect(dictionaryEntryError("words", { ...existing, language: "en" }, settings, null)).toBeNull();
    expect(dictionaryEntryError("words", existing, settings, existing)).toBeNull();
    expect(dictionaryEntryError("words", { ...existing, spoken_form: "alternate" }, settings, existing)).toBeNull();
    expect(dictionaryEntryError("words", { ...existing, language: "D" }, settings, existing))
      .toContain("supported language code");
    expect(dictionaryEntryError("words", { ...existing, phrase: "New" }, settings, existing))
      .toContain("supported language code");
  });
});
