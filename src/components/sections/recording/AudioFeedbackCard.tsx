import { SettingsCard, SettingRow } from "@/components/settings/settings-ui";
import { Switch } from "@/components/settings/SettingsSwitch";
import { useSettings } from "@/contexts/SettingsContext";

export function AudioFeedbackCard() {
  const { settings } = useSettings();
  if (!settings) return null;
  return (
    <SettingsCard
      title="Sounds"
      description="Choose recording, transcript and paste sounds in More."
      className="rounded-[12px] px-[14px] py-3 [&_h2]:font-medium"
    />
  );
}

export function AudioFeedbackDetailRows() {
  const { settings, updateSettings } = useSettings();
  if (!settings) return null;
  return (
    <>
      <SettingRow
        title="Recording started"
        description="Play a sound when the microphone is ready for speech."
        htmlFor="sound-on-recording"
        control={
          <Switch
            id="sound-on-recording"
            checked={settings.play_sound_on_recording ?? true}
            onCheckedChange={(checked) => void updateSettings({ play_sound_on_recording: checked })}
          />
        }
      />
      <SettingRow
        title="Transcript ready"
        htmlFor="sound-on-transcription-complete"
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
        htmlFor="sound-on-paste-success"
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
