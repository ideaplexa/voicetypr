import { SettingsCard, SettingRow } from "@/components/settings/settings-ui";
import { Switch } from "@/components/settings/SettingsSwitch";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import { toast } from "sonner";

const log = createLogger("models");

/** Whisper-only speed card. Language and real-time words live under the selected model. */
export function WhisperSpeedCard() {
  const { settings, updateSettings } = useSettings();
  return (
    <SettingsCard title="Whisper performance">
      <SettingRow
        title="Speed mode"
        description="Faster transcription (flash attention); pairs best with Large v3 Turbo."
        control={
          <Switch
            id="whisper-speed-mode"
            checked={settings?.whisper_speed_mode ?? false}
            onCheckedChange={(checked) => {
              void updateSettings({ whisper_speed_mode: checked }).catch((error) => {
                log.error("Failed to update Whisper speed mode:", error);
                toast.error("Failed to update speed mode");
              });
            }}
            aria-label="Speed mode"
          />
        }
      />
    </SettingsCard>
  );
}
