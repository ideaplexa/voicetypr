import { SettingsCard, SettingRow } from "@/components/settings/settings-ui";
import { Switch } from "@/components/ui/switch";
import { useSettings } from "@/contexts/SettingsContext";

export function AudioFeedbackCard() {
  const { settings, updateSettings } = useSettings();
  if (!settings) return null;
  return (
    <SettingsCard title="Sounds" description="A soft tick when recording starts and ends.">
      <div className="mt-3 flex justify-end">
        <Switch
          id="sound-on-recording"
          aria-label="Sounds"
          checked={settings.play_sound_on_recording ?? true}
          onCheckedChange={(checked) => void updateSettings({ play_sound_on_recording: checked })}
        />
      </div>
    </SettingsCard>
  );
}

export function AudioFeedbackDetailRows() {
  const { settings, updateSettings } = useSettings();
  if (!settings) return null;
  return (
    <>
      <SettingRow
        title="Transcript ready"
        description="Play a sound after transcription and optional AI formatting finish."
        control={
          <Switch
            id="sound-on-transcription-complete"
            checked={settings.play_sound_on_transcription_complete ?? true}
            onCheckedChange={(checked) =>
              void updateSettings({ play_sound_on_transcription_complete: checked })
            }
          />
        }
      />
      <SettingRow
        title="Paste completed"
        description="Play a sound after Voicetypr successfully sends the paste command."
        control={
          <Switch
            id="sound-on-paste-success"
            checked={settings.play_sound_on_paste_success ?? true}
            onCheckedChange={(checked) =>
              void updateSettings({ play_sound_on_paste_success: checked })
            }
          />
        }
      />
    </>
  );
}
