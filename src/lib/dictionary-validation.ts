import { languages } from "@/components/languages";
import type { CustomWord, Snippet, TextReplacementRule, WritingSettings } from "@/types/writing";

export type DictionaryKind = "words" | "corrections" | "snippets";
export type DictionaryEntry = CustomWord | TextReplacementRule | Snippet;

// Library matching uses Rust's eq_ignore_ascii_case after the sanitizer trims.
const normalizedText = (value: string) => value.trim().replace(/[A-Z]/g, (letter) => letter.toLowerCase());
// Dictionary scopes use the same supported base codes as the spoken-language picker.
// Rust also trims and folds ASCII case; regional forms are not supported scopes.
const languageScope = (value: string | null | undefined) => value ? normalizedText(value) || null : null;

export function dictionaryLanguageError(value: string | null | undefined): string | null {
  const code = languageScope(value);
  return code === null || languages.some((language) => language.value === code)
    ? null
    : "Use a supported language code (e.g. en or de), or leave empty for all languages.";
}
const scopesOverlap = (a: string | null, b: string | null) => a === null || b === null || a === b;

export function dictionaryEntryError(kind: DictionaryKind, entry: DictionaryEntry, settings: WritingSettings, original: DictionaryEntry | null): string | null {
  const languageError = dictionaryLanguageError(entry.language);
  if (languageError) return languageError;
  if (kind === "words") {
    const word = entry as CustomWord;
    if (!word.phrase.trim()) return "Enter a word or phrase.";
    if (settings.custom_words.some((candidate) => candidate !== original &&
      normalizedText(candidate.phrase) === normalizedText(word.phrase) &&
      scopesOverlap(languageScope(candidate.language), languageScope(word.language)))) {
      return "This word already exists for the same language.";
    }
  } else if (kind === "corrections") {
    const rule = entry as TextReplacementRule;
    if (!rule.from.trim()) return "Enter text to match.";
    if (!rule.to.length) return "Enter replacement text.";
    if (settings.replacements.some((candidate) => candidate !== original &&
      normalizedText(candidate.from) === normalizedText(rule.from) &&
      scopesOverlap(languageScope(candidate.language), languageScope(rule.language)))) {
      return "This correction already exists for the same language.";
    }
  } else {
    const snippet = entry as Snippet;
    if (!snippet.trigger.trim()) return "Enter a snippet trigger.";
    if (!snippet.body.trimEnd()) return "Enter text to insert.";
    if (settings.snippets.some((candidate) => candidate !== original &&
      normalizedText(candidate.trigger) === normalizedText(snippet.trigger) &&
      scopesOverlap(languageScope(candidate.language), languageScope(snippet.language)))) {
      return "This trigger already exists for the same language.";
    }
  }
  return null;
}
