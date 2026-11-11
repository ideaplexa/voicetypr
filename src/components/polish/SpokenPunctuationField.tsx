import {
  Field,
  FieldContent,
  FieldDescription,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";

interface SpokenPunctuationFieldProps {
  enabled: boolean;
  disabled: boolean;
  onChange: (enabled: boolean) => void;
}

export function SpokenPunctuationField({ enabled, disabled, onChange }: SpokenPunctuationFieldProps) {
  return (
    <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
      <Field orientation="horizontal">
        <FieldContent>
          <FieldTitle>Spoken punctuation (German)</FieldTitle>
          <FieldDescription>
            Works offline, with or without Polish. Say “Beistrich”, “Strichpunkt”,
            “Doppelpunkt”, “Fragezeichen”, “Rufzeichen”, “Gedankenstrich”, “neue Zeile”,
            “neuer Absatz”, “Anführungszeichen auf/zu”, “Klammer auf/zu” or symbol names
            such as “Paragraphzeichen” and “Schrägstrich”. “Komma” and “Punkt” stay words.
          </FieldDescription>
        </FieldContent>
        <Switch
          aria-label="Spoken punctuation (German)"
          checked={enabled}
          disabled={disabled}
          onCheckedChange={onChange}
        />
      </Field>
    </FieldSet>
  );
}
