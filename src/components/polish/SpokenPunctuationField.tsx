import { invoke } from "@tauri-apps/api/core";
import { ChevronDown } from "lucide-react";
import { useState } from "react";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type {
  SpokenPunctuationCommand,
  SpokenPunctuationLanguage,
} from "@/types/writing";

const LANGUAGE_NAMES: Record<string, string> = {
  en: "English",
  de: "German",
  fr: "French",
  es: "Spanish",
  it: "Italian",
  pt: "Portuguese",
  nl: "Dutch",
  pl: "Polish",
};

function triggerNote(command: SpokenPunctuationCommand, language: SpokenPunctuationLanguage) {
  switch (command.trigger) {
    case "not_between_digits":
      return "Not between numbers";
    case "phrase_end":
      return "Only at the end of a phrase";
    case "prefix_only":
      return `Say “${language.prefix}” first`;
    default:
      return null;
  }
}

function CommandReference({ languages }: { languages: SpokenPunctuationLanguage[] }) {
  const [code, setCode] = useState(languages[0]?.code ?? "en");
  const language = languages.find((item) => item.code === code) ?? languages[0];
  if (!language) return null;
  return (
    <div className="mt-3 flex flex-col gap-3">
      <Select value={language.code} onValueChange={(value) => value && setCode(value)}>
        <SelectTrigger size="sm" className="w-48" aria-label="Command language">
          <SelectValue>{LANGUAGE_NAMES[language.code] ?? language.code}</SelectValue>
        </SelectTrigger>
        <SelectContent>
          {languages.map((item) => (
            <SelectItem key={item.code} value={item.code}>
              {LANGUAGE_NAMES[item.code] ?? item.code}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <p className="text-xs text-muted-foreground">
        Say “{language.prefix}” before a command to force it, or “{language.escape}” to keep it as
        words. A command right after an article (“a”, “ein”, “une”…) stays a word.
      </p>
      <table className="w-full text-left text-xs">
        <thead className="text-muted-foreground">
          <tr>
            <th className="py-1 pr-3 font-medium">Say</th>
            <th className="py-1 pr-3 font-medium">Types</th>
            <th className="py-1 font-medium">Note</th>
          </tr>
        </thead>
        <tbody>
          {language.commands.map((command) => (
            <tr key={command.phrases[0]} className="border-t border-border/60 align-top">
              <td className="py-1 pr-3">{command.phrases.join(" · ")}</td>
              <td className="py-1 pr-3 font-mono">
                {command.symbol
                  .replace(/\n\n/g, " ↵↵ new paragraph")
                  .replace(/\n/g, " ↵ new line")
                  .trim()}
              </td>
              <td className="py-1 text-muted-foreground">{triggerNote(command, language)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

interface SpokenPunctuationFieldProps {
  enabled: boolean;
  disabled: boolean;
  onChange: (enabled: boolean) => void;
}

export function SpokenPunctuationField({ enabled, disabled, onChange }: SpokenPunctuationFieldProps) {
  const [languages, setLanguages] = useState<SpokenPunctuationLanguage[] | null>(null);
  const [loadError, setLoadError] = useState(false);

  const loadCommands = (open: boolean) => {
    if (!open || languages) return;
    invoke<SpokenPunctuationLanguage[]>("get_spoken_punctuation_commands")
      .then((result) => {
        setLanguages(result);
        setLoadError(false);
      })
      .catch(() => setLoadError(true));
  };

  return (
    <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
      <Field orientation="horizontal">
        <FieldContent>
          <FieldTitle>Spoken punctuation</FieldTitle>
          <FieldDescription>
            Say punctuation while dictating in English, German, French, Spanish, Italian,
            Portuguese, Dutch or Polish, for example “comma”, “question mark” or “new paragraph”.
            Works offline, with or without Polish.
          </FieldDescription>
        </FieldContent>
        <Switch
          aria-label="Spoken punctuation"
          checked={enabled}
          disabled={disabled}
          onCheckedChange={onChange}
        />
      </Field>
      <Collapsible onOpenChange={loadCommands}>
        <CollapsibleTrigger className="mt-2 flex items-center gap-1 text-xs font-medium text-muted-foreground hover:text-foreground">
          Show commands
          <ChevronDown className="size-3.5" />
        </CollapsibleTrigger>
        <CollapsibleContent>
          {languages ? (
            <CommandReference languages={languages} />
          ) : (
            <p className="mt-3 text-xs text-muted-foreground">
              {loadError ? "Could not load the command list." : "Loading commands…"}
            </p>
          )}
        </CollapsibleContent>
      </Collapsible>
    </FieldSet>
  );
}
