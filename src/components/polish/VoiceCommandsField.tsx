import {
  Field,
  FieldContent,
  FieldDescription,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";

const COMMANDS: [say: string, does: string][] = [
  ["new line", "Starts a new line"],
  ["new paragraph", "Starts a new paragraph"],
  ["undo that", "Undoes the last change"],
  ["delete that", "Deletes the dictation you just made, if the cursor is still right after it (Mac)"],
  ["select all", "Selects everything in the field"],
  ["send", "Presses Return to send"],
];

interface VoiceCommandsFieldProps {
  enabled: boolean;
  disabled: boolean;
  onChange: (enabled: boolean) => void;
}

export function VoiceCommandsField({ enabled, disabled, onChange }: VoiceCommandsFieldProps) {
  return (
    <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
      <Field orientation="horizontal">
        <FieldContent>
          <FieldTitle>Voice commands</FieldTitle>
          <FieldDescription>
            Say a command on its own, as a whole dictation, and Voicetypr presses the keys instead
            of typing the words. The same words inside a sentence are typed as words (with Spoken
            punctuation on, “new line” and “new paragraph” still break the line there). Works
            offline. To type a command as words, say “literal” first, for example “literal send”.
          </FieldDescription>
        </FieldContent>
        <Switch
          aria-label="Voice commands"
          checked={enabled}
          disabled={disabled}
          onCheckedChange={onChange}
        />
      </Field>
      <table className="mt-3 w-full text-left text-xs">
        <thead className="text-muted-foreground">
          <tr>
            <th className="py-1 pr-3 font-medium">Say</th>
            <th className="py-1 font-medium">Does</th>
          </tr>
        </thead>
        <tbody>
          {COMMANDS.map(([say, does]) => (
            <tr key={say} className="border-t border-border/60 align-top">
              <td className="py-1 pr-3">{say}</td>
              <td className="py-1 text-muted-foreground">{does}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </FieldSet>
  );
}
