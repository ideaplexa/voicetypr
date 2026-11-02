import { describe, expect, it } from "vitest";
import { languages } from "@/components/languages";
import { dictionaryEntryError, dictionaryLanguageError } from "@/lib/dictionary-validation";
import { defaultWritingSettings } from "@/types/writing";

describe("dictionary language validation", () => {
  it("accepts empty scopes and every picker language case-insensitively", () => {
    for (const code of [null, undefined, "", "  ", ...languages.map(({ value }) => value)]) {
      expect(dictionaryLanguageError(code)).toBeNull();
      if (code) expect(dictionaryLanguageError(` ${code.toUpperCase()} `)).toBeNull();
    }
  });

  it.each(["d", "unknown", "auto", "de-AT", "de_CH", "english", "KO"])(
    "rejects unsupported code %s",
    (code) => expect(dictionaryLanguageError(code)).toContain("supported language code"),
  );

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
    expect(dictionaryEntryError("words", existing, settings, existing))
      .toContain("supported language code");
  });
});
