import { MicrophoneSelection } from "@/components/MicrophoneSelection";
import { SettingsPaneCard } from "@/components/settings/settings-ui";
import { useSettings } from "@/contexts/SettingsContext";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";

export function MicrophoneCheck() {
  const { settings, refreshSettings } = useSettings();
  return (
    <SettingsPaneCard className="w-full !p-[18px]">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-semibold">Microphone</h3>
        <MicrophoneSelection
          value={settings?.selected_microphone || undefined}
          className="h-[34px] w-auto max-w-[360px] rounded-[10px] px-[10px] text-[13px] text-muted-foreground hover:text-muted-foreground aria-expanded:text-muted-foreground"
          onValueChange={async (deviceName) => {
            try {
              await invoke("set_audio_device", { deviceName: deviceName || null });
              await refreshSettings();
            } catch {
              toast.error("Failed to change microphone");
            }
          }}
        />
      </div>
      <p className="mt-3 text-xs text-muted-foreground">You'll test it in the next step.</p>
    </SettingsPaneCard>
  );
}
