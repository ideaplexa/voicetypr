import { SettingsCard, SettingRow } from "@/components/settings/settings-ui";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import type { ActiveStreamCapabilities, SpeechModelEngine } from "@/types";
import { getErrorMessage } from "@/utils/error";
import { invoke } from "@tauri-apps/api/core";
import { Zap } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

const log = createLogger("models");

export function TranscriptionControls({
  engine,
  modelName,
}: {
  engine: SpeechModelEngine;
  modelName: string;
}) {
  const { settings, updateSettings, refreshSettings } = useSettings();
  const [capabilities, setCapabilities] = useState<ActiveStreamCapabilities | null>(null);
  const [isActivating, setIsActivating] = useState(false);
  const transcriptionMode = settings?.transcription_mode ?? "regular";

  const refreshCapabilities = useCallback(async () => {
    try {
      setCapabilities(await invoke<ActiveStreamCapabilities>("get_active_stream_capabilities"));
    } catch (error) {
      log.warn("Failed to load stream capabilities:", error);
      setCapabilities(null);
    }
  }, []);

  useEffect(() => {
    let active = true;
    void invoke<ActiveStreamCapabilities>("get_active_stream_capabilities")
      .then((next) => {
        if (active) setCapabilities(next);
      })
      .catch((error) => {
        log.warn("Failed to load stream capabilities:", error);
        if (active) setCapabilities(null);
      });
    return () => {
      active = false;
    };
  }, [engine, modelName]);

  const changeMode = async (value: string) => {
    if (!value || value === transcriptionMode) return;
    if (value === "regular") {
      try {
        await updateSettings({ transcription_mode: "regular" });
        await refreshSettings();
        await refreshCapabilities();
      } catch (error) {
        log.error("Failed to switch transcription mode:", error);
        toast.error("Failed to switch transcription mode");
      }
      return;
    }

    setIsActivating(true);
    try {
      await invoke("activate_live_preview");
      await refreshSettings();
      await refreshCapabilities();
      toast.success("Live preview enabled");
    } catch (error) {
      await refreshSettings();
      await refreshCapabilities();
      toast.error(getErrorMessage(error, "Live preview could not be enabled"));
    } finally {
      setIsActivating(false);
    }
  };

  return (
    <>
      {engine === "whisper" && (
        <SettingsCard icon={Zap} title="Whisper performance">
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
      )}
      {capabilities?.capabilities.supports_streaming === true && (
        <SettingsCard
          icon={Zap}
          title="Transcription mode"
          description="Live preview shows text as you speak; your final text still uses your selected model."
        >
          <SettingRow
            title="Mode"
            description="Regular waits for the final transcript. Live preview shows text locally as you speak."
            control={
              <div className="flex flex-col items-end gap-2">
                <ToggleGroup
                  variant="outline"
                  size="sm"
                  spacing={0}
                  value={[transcriptionMode]}
                  onValueChange={(values) =>
                    void changeMode(values.find((value) => value !== transcriptionMode) ?? "")
                  }
                  aria-label="Transcription mode"
                  disabled={isActivating}
                >
                  <ToggleGroupItem value="regular">Regular</ToggleGroupItem>
                  <ToggleGroupItem value="live_preview">Live preview</ToggleGroupItem>
                </ToggleGroup>
                {isActivating && (
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Spinner className="size-3.5" /> Enabling live preview…
                  </div>
                )}
              </div>
            }
          />
        </SettingsCard>
      )}
    </>
  );
}
