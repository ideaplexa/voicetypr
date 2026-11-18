import { recordEvent } from "@/lib/observability";
import { MicrophoneSelection } from "@/components/MicrophoneSelection";
import { SettingsPaneCard } from "@/components/settings/settings-ui";
import { useSettings } from "@/contexts/SettingsContext";
import { invoke } from "@tauri-apps/api/core";
import { Mic } from "lucide-react";
import { toast } from "sonner";
import { useMicrophoneTest } from "@/components/onboarding/useMicrophoneTest";

/** The one Microphone block: permission, device, live level. */
export function MicrophoneCheck({ allowed = false }: { allowed?: boolean }) {
  const { settings, refreshSettings } = useSettings();
  const { level, status, visible } = useMicrophoneTest(settings?.selected_microphone || undefined);
  return (
    <SettingsPaneCard className="w-full !p-[18px]">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-[14px]">
          <span className="flex size-[34px] shrink-0 items-center justify-center rounded-[9px] bg-sage-bg">
            <Mic className="size-[18px] text-sage" />
          </span>
          <div className="space-y-0.5">
            <h3 className="flex items-center gap-2 text-sm leading-[normal] font-semibold">
              Microphone
              {allowed ? (
                <span className="flex items-center gap-[5px] rounded-full bg-sage-bg px-[9px] py-[3px] text-[11.5px] font-medium text-muted-foreground">
                  <span className="size-1.5 rounded-full bg-sage" />
                  Allowed
                </span>
              ) : null}
            </h3>
            <p className="text-[12.5px] leading-[normal] text-muted-foreground">So Voicetypr can hear you.</p>
          </div>
        </div>
        <MicrophoneSelection
          value={settings?.selected_microphone || undefined}
          className="h-[34px] w-auto max-w-[360px] rounded-[10px] px-[10px] text-[13px] text-muted-foreground hover:text-muted-foreground aria-expanded:text-muted-foreground"
          onValueChange={async (deviceName) => {
            try {
              await invoke("set_audio_device", { deviceName: deviceName || null });
                recordEvent({ name: "quick_setting_changed", properties: { setting: "mic", source: "app" } });
              await refreshSettings();
            } catch {
              toast.error("Failed to change microphone");
            }
          }}
        />
      </div>
      <div className="mt-3 flex h-[18px] items-center gap-[3px]" role="meter"
        aria-label="Microphone input level" aria-valuemin={0} aria-valuemax={100}
        aria-valuenow={Math.round(level * 100)}>
        {Array.from({ length: 24 }, (_, index) => (
          <span key={index} className={`h-2 flex-1 rounded-sm ${index < Math.round(level * 24) ? "bg-sage" : "bg-muted"}`} />
        ))}
      </div>
      <p role="status" className="mt-3 text-xs text-muted-foreground">
        {!visible || status === "paused" ? "Microphone test paused." : status === "opening" ? "Opening microphone…"
          : status === "heard" ? "We can hear you"
          : status === "quiet" ? "No input yet — try another microphone"
          : status === "error" ? "Could not open this microphone — try another microphone"
          : "Speak to check your microphone."}
      </p>
      <p className="mt-2 text-xs text-muted-foreground">
        {visible && (status === "listening" || status === "heard" || status === "quiet")
          ? "Microphone on for this check. Nothing is saved."
          : "This check saves nothing."}
      </p>
    </SettingsPaneCard>
  );
}
