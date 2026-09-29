import { SettingsCard, SettingRow } from "@/components/settings/settings-ui";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/settings/SettingsSwitch";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import type { ActiveStreamCapabilities, SpeechModelEngine } from "@/types";
import { getErrorMessage } from "@/utils/error";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

const log = createLogger("models");

export function TranscriptionControls({
  engine,
  modelName,
  speedOnly = false,
}: {
  engine: SpeechModelEngine;
  modelName: string;
  speedOnly?: boolean;
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

  const previewAvailable = capabilities?.capabilities.supports_streaming === true;
  return speedOnly ? (
    engine === "whisper" ? (
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
    ) : null
  ) : (
    <section
      data-pencil-name="Opt Live preview"
      className="rounded-[14px] border border-border bg-card px-4 py-3"
    >
      <SettingRow
        title="Live preview"
        className="border-0 p-0! flex-row! items-center! gap-4! [&>div:first-child]:flex-1"
        description={
          previewAvailable
            ? "See words in the pill as you talk."
            : "This engine cannot show words while you speak."
        }
        control={
          <Switch
            id="live-preview"
            aria-label="Live preview"
            checked={previewAvailable && transcriptionMode === "live_preview"}
            disabled={!previewAvailable || isActivating}
            onCheckedChange={(checked) => {
              void changeMode(checked ? "live_preview" : "regular");
            }}
          />
        }
      />
      {isActivating ? (
        <p className="text-xs text-muted-foreground">
          <Spinner className="inline size-3" /> Enabling live preview…
        </p>
      ) : null}
    </section>
  );
}
