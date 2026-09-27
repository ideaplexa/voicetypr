import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { useSettings } from "@/contexts/SettingsContext";

export function AudioFeedbackCard() {
  const { settings, updateSettings } = useSettings();
  if (!settings) return null;

  return (
    <FieldSet className="gap-4 rounded-xl border border-border bg-card p-4">
      <FieldLegend className="mb-1 text-base font-semibold">Audio feedback</FieldLegend>

      <Field orientation="responsive" className="items-center gap-3">
        <FieldContent>
          <FieldLabel htmlFor="sound-on-recording">Recording started</FieldLabel>
          <FieldDescription>Play a sound when the microphone is ready for speech.</FieldDescription>
        </FieldContent>
        <Switch
          id="sound-on-recording"
          checked={settings.play_sound_on_recording ?? true}
          onCheckedChange={async (checked) =>
            await updateSettings({
              play_sound_on_recording: checked,
            })
          }
        />
      </Field>

      <Field orientation="responsive" className="items-center gap-3">
        <FieldContent>
          <FieldLabel htmlFor="sound-on-transcription-complete">Transcript ready</FieldLabel>
          <FieldDescription>
            Play a sound after transcription and optional AI formatting finish.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="sound-on-transcription-complete"
          checked={settings.play_sound_on_transcription_complete ?? true}
          onCheckedChange={async (checked) =>
            await updateSettings({
              play_sound_on_transcription_complete: checked,
            })
          }
        />
      </Field>

      <Field orientation="responsive" className="items-center gap-3">
        <FieldContent>
          <FieldLabel htmlFor="sound-on-paste-success">Paste completed</FieldLabel>
          <FieldDescription>
            Play a sound after VoiceTypr successfully sends the paste command.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="sound-on-paste-success"
          checked={settings.play_sound_on_paste_success ?? true}
          onCheckedChange={async (checked) =>
            await updateSettings({
              play_sound_on_paste_success: checked,
            })
          }
        />
      </Field>
    </FieldSet>
  );
}
