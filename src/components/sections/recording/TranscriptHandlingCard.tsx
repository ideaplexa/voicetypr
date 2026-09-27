import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLegend,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { useSettings } from "@/contexts/SettingsContext";

export function TranscriptHandlingCard() {
  const { settings, updateSettings } = useSettings();
  if (!settings) return null;

  return (
    <FieldSet className="gap-4 rounded-xl border border-border bg-card p-4">
      <FieldLegend className="mb-1 text-base font-semibold">Transcript handling</FieldLegend>

      <Field orientation="responsive" className="items-center gap-3">
        <FieldContent>
          <FieldTitle>Keep Transcript in Clipboard</FieldTitle>
          <FieldDescription>Leave transcribed text available for manual pastes</FieldDescription>
        </FieldContent>
        <Switch
          id="clipboard-retain"
          checked={settings.keep_transcription_in_clipboard ?? false}
          onCheckedChange={async (checked) =>
            await updateSettings({
              keep_transcription_in_clipboard: checked,
            })
          }
        />
      </Field>

      <Field orientation="responsive" className="items-center gap-3">
        <FieldContent>
          <FieldTitle>Auto-paste transcript</FieldTitle>
          <FieldDescription>
            Insert completed text automatically. Turn off to copy for manual paste.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="auto-paste-transcription"
          checked={settings.auto_paste_transcription ?? true}
          onCheckedChange={async (checked) =>
            await updateSettings({
              auto_paste_transcription: checked,
            })
          }
        />
      </Field>

      <Field orientation="responsive" className="items-center gap-3">
        <FieldContent>
          <FieldTitle>Pause media during recording</FieldTitle>
          <FieldDescription>
            Automatically pause playing music or videos while recording.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="pause-media"
          checked={settings.pause_media_during_recording ?? false}
          onCheckedChange={async (checked) =>
            await updateSettings({
              pause_media_during_recording: checked,
            })
          }
        />
      </Field>
    </FieldSet>
  );
}
