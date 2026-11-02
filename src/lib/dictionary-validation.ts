import { languages } from "@/components/languages";
import type { CustomWord, Snippet, TextReplacementRule, WritingSettings } from "@/types/writing";

export type DictionaryKind = "words" | "corrections" | "snippets";
export type DictionaryEntry = CustomWord | TextReplacementRule | Snippet;

// Library matching uses Rust's eq_ignore_ascii_case after the sanitizer trims.
const normalizedText = (value: string) => value.trim().replace(/[A-Z]/g, (letter) => letter.toLowerCase());
// Keep this grammar aligned with Rust's supported_language_base: supported base,
// optionally followed by a two-letter or three-digit ISO region.
function supportedLanguageBase(value: string): string | null {
  const match = /^([a-z]{2,3})(?:[-_](?:[a-z]{2}|[0-9]{3}))?$/.exec(normalizedText(value));
  const base = match?.[1];
  return base && languages.some((language) => language.value === base) ? base : null;
}

export function normalizeDictionaryLanguage(value: string | null | undefined): string | null {
  if (!value?.trim()) return null;
  return supportedLanguageBase(value) ?? value;
}
const languageScope = normalizeDictionaryLanguage;

export function dictionaryLanguageError(value: string | null | undefined): string | null {
  return !value?.trim() || supportedLanguageBase(value) !== null
    ? null
    : "Use a supported language code (e.g. en, de or de-AT), or leave empty for all languages.";
}
const scopesOverlap = (a: string | null, b: string | null) => a === null || b === null || a === b;

export function dictionaryEntryError(kind: DictionaryKind, entry: DictionaryEntry, settings: WritingSettings, original: DictionaryEntry | null): string | null {
  const languageError = dictionaryLanguageError(entry.language);
  const identity = (value: DictionaryEntry) => kind === "words"
    ? (value as CustomWord).phrase.trim()
    : kind === "corrections" ? (value as TextReplacementRule).from.trim() : (value as Snippet).trigger.trim();
  const preservedLegacyScope = original !== null &&
    entry.language === original.language && identity(entry) === identity(original);
  if (languageError && !preservedLegacyScope) return languageError;
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
