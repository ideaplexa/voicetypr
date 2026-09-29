import { SettingsCard, SettingRow } from "@/components/settings/settings-ui";
import { Switch } from "@/components/ui/switch";
import { useSettings } from "@/contexts/SettingsContext";

export function TranscriptHandlingCard() {
  const { settings, updateSettings } = useSettings();
  if (!settings) return null;
  return (
    <>
      <SettingsCard title="Paste automatically" description="Type the text where your cursor is.">
        <div className="mt-3 flex justify-end">
          <Switch
            id="auto-paste-transcription"
            aria-label="Paste automatically"
            checked={settings.auto_paste_transcription ?? true}
            onCheckedChange={(checked) =>
              void updateSettings({ auto_paste_transcription: checked })
            }
          />
        </div>
      </SettingsCard>
      <SettingsCard title="Keep in clipboard" description="Also copy it, so you can paste again.">
        <div className="mt-3 flex justify-end">
          <Switch
            id="clipboard-retain"
            aria-label="Keep in clipboard"
            checked={settings.keep_transcription_in_clipboard ?? false}
            onCheckedChange={(checked) =>
              void updateSettings({ keep_transcription_in_clipboard: checked })
            }
          />
        </div>
      </SettingsCard>
    </>
  );
}

export function PauseMediaRow() {
  const { settings, updateSettings } = useSettings();
  if (!settings) return null;
  return (
    <SettingRow
      title="Pause media during recording"
      htmlFor="pause-media"
      description="Automatically pause playing music or videos while recording."
      control={
        <Switch
          id="pause-media"
          checked={settings.pause_media_during_recording ?? false}
          onCheckedChange={(checked) =>
            void updateSettings({ pause_media_during_recording: checked })
          }
        />
      }
    />
  );
}
