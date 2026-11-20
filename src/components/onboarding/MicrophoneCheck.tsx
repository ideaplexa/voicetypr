import { recordEvent } from "@/lib/observability";
import { MicrophoneSelection } from "@/components/MicrophoneSelection";
import { LevelBars } from "@/components/onboarding/LevelBars";
import { useSettings } from "@/contexts/SettingsContext";
import { invoke } from "@tauri-apps/api/core";
import { CircleCheck } from "lucide-react";
import { toast } from "sonner";
import { useMicrophoneTest } from "@/components/onboarding/useMicrophoneTest";

/** 05 on Windows: the live level, whether we hear you, and the microphone to use. */
export function MicrophoneCheck() {
  const { settings, refreshSettings } = useSettings();
  const { level, status, visible } = useMicrophoneTest(settings?.selected_microphone || undefined);
  return (
    <div className="flex flex-col items-center gap-[22px] self-stretch rounded-2xl border border-border bg-card p-7">
      <LevelBars level={level} size="lg" />
      <p role="status" className="flex items-center gap-2 text-base font-semibold text-foreground">
        {status === "heard" && visible ? <CircleCheck aria-hidden="true" className="size-[18px] text-sage" /> : null}
        {!visible || status === "paused" ? "Microphone test paused." : status === "opening" ? "Opening microphone…"
          : status === "heard" ? "We can hear you"
          : status === "quiet" ? "No input yet — try another microphone"
          : status === "error" ? "Could not open this microphone — try another microphone"
          : "Say a few words."}
      </p>
      <MicrophoneSelection
        value={settings?.selected_microphone || undefined}
        className="h-11 w-full rounded-[10px] px-3.5 text-[15px]"
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
  );
}
