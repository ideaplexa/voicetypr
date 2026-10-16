"use client";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { SpeechModelEngine } from "@/types";
import { cn } from "@/lib/utils";
import { Check, ChevronsUpDown } from "lucide-react";
import * as React from "react";
import { languages } from "./languages";

// Spoken Auto is deliberately separate from the shared list of explicit
// languages, which also supplies translation-target selectors.
const autoLanguage = { value: "auto", label: "Auto" };

interface LanguageSelectionProps {
  value: string;
  onValueChange: (value: string) => void;
  className?: string;
  engine?: SpeechModelEngine;
  englishOnly?: boolean;
  supportedLanguages?: readonly string[];
}

export function LanguageSelection({
  value,
  onValueChange,
  className,
  engine = "whisper",
  englishOnly = false,
  supportedLanguages,
}: LanguageSelectionProps) {
  const [open, setOpen] = React.useState(false);

  // Parakeet v3 supports 25 European languages
  const parakeetAllowed = React.useMemo(
    () =>
      new Set([
        "bg",
        "cs",
        "da",
        "de",
        "el",
        "en",
        "es",
        "et",
        "fi",
        "fr",
        "hr",
        "hu",
        "it",
        "lt",
        "lv",
        "mt",
        "nl",
        "pl",
        "pt",
        "ro",
        "ru",
        "sk",
        "sl",
        "sv",
        "uk",
      ]),
    [],
  );

  // Soniox supported languages (static list per docs). Keep in sync with codes in `languages` above.
  const sonioxAllowed = React.useMemo(
    () =>
      new Set<string>([
        "en",
        "es",
        "fr",
        "de",
        "it",
        "pt",
        "nl",
        "ru",
        "zh",
        "ja",
        "ko",
        "ar",
        "hi",
        "tr",
        "pl",
        "sv",
        "no",
        "da",
        "fi",
        "el",
        "cs",
        "ro",
        "hu",
        "sk",
        "uk",
        "he",
        "id",
        "vi",
        "th",
        "ms",
        "tl",
        "fa",
        "ur",
        "bn",
        "ta",
        "te",
        "gu",
        "pa",
        "bg",
        "hr",
        "sr",
        "sl",
        "lv",
        "lt",
        "et",
        "is",
        "ca",
        "gl",
      ]),
    [],
  );

  // Cohere Transcribe supports 14 languages and does not auto-detect unsupported languages.
  const cohereAllowed = React.useMemo(
    () =>
      new Set<string>([
        "en",
        "de",
        "fr",
        "it",
        "es",
        "pt",
        "el",
        "nl",
        "pl",
        "vi",
        "zh",
        "ar",
        "ja",
        "ko",
      ]),
    [],
  );

  const displayed = React.useMemo(() => {
    if (englishOnly) {
      return languages.filter((l) => l.value === "en");
    }
    if (supportedLanguages) {
      return languages.filter((l) => supportedLanguages.includes(l.value));
    }
    if (engine === "parakeet") {
      return languages.filter((l) => parakeetAllowed.has(l.value));
    }
    if (engine === "soniox") {
      return languages.filter((l) => sonioxAllowed.has(l.value));
    }
    if (engine === "cohere") {
      return languages.filter((l) => cohereAllowed.has(l.value));
    }
    return languages;
  }, [engine, parakeetAllowed, sonioxAllowed, cohereAllowed, englishOnly, supportedLanguages]);

  const supportsAuto =
    !englishOnly &&
    supportedLanguages?.length !== 1 &&
    (engine === "whisper" || engine === "parakeet" || engine === "crispasr");
  const choices = supportsAuto ? [autoLanguage, ...displayed] : displayed;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        render={
          <Button
            variant="outline"
            role="combobox"
            aria-label="Spoken language"
            aria-expanded={open}
            disabled={englishOnly}
            className={cn("w-48 justify-between", className)}
          />
        }
      >
        {englishOnly
          ? "English"
          : value === "auto"
            ? autoLanguage.label
            : value
              ? languages.find((language) => language.value === value)?.label
              : "Select language"}
        <ChevronsUpDown className="opacity-50" />
      </PopoverTrigger>
      <PopoverContent className="w-[200px] p-0">
        <Command>
          <CommandInput placeholder="Search language..." className="h-9" />
          <CommandList>
            <CommandEmpty>No language found.</CommandEmpty>
            <CommandGroup>
              {choices.map((language) => (
                <CommandItem
                  key={language.value}
                  // Use label for search instead of value so users can search by language name
                  value={language.label}
                  onSelect={() => {
                    // Pass the actual language code (value) when selected
                    onValueChange(language.value);
                    setOpen(false);
                  }}
                >
                  {language.label}
                  <Check
                    className={cn(
                      "ml-auto",
                      value === language.value ? "opacity-100" : "opacity-0",
                    )}
                  />
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
