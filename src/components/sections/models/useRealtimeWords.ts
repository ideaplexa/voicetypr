import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import type { ActiveStreamCapabilities, SpeechModelEngine } from "@/types";
import { getErrorMessage } from "@/utils/error";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

const log = createLogger("models");

/** Whether the active model streams, and the Real-time words switch (transcription_mode). */
export function useRealtimeWords(engine: SpeechModelEngine, modelName: string) {
  const { settings, updateSettings, refreshSettings } = useSettings();
  const [capabilities, setCapabilities] = useState<ActiveStreamCapabilities | null>(null);
  const [isActivating, setIsActivating] = useState(false);
  const transcriptionMode = settings?.transcription_mode ?? "regular";
  const requestRevision = useRef(0);

  const refreshCapabilities = useCallback(() => {
    const revision = ++requestRevision.current;
    return invoke<ActiveStreamCapabilities>("get_active_stream_capabilities")
      .then((next) => {
        if (revision === requestRevision.current) setCapabilities(next);
      })
      .catch((error) => {
        log.warn("Failed to load stream capabilities:", error);
        if (revision === requestRevision.current) setCapabilities(null);
      });
  }, []);

  useEffect(() => {
    void refreshCapabilities();
    return () => {
      requestRevision.current++;
    };
  }, [engine, modelName, refreshCapabilities]);

  // Both the tray and remote-selection commands emit this, even if the local
  // model/engine settings themselves did not change.
  useTauriEvent("sharing-status-changed", () => {
    setCapabilities(null);
    return refreshCapabilities();
  });

  const setRealtime = async (on: boolean) => {
    if (!on) {
      try {
        await updateSettings({ transcription_mode: "regular" });
        await refreshSettings();
        await refreshCapabilities();
      } catch (error) {
        log.error("Failed to switch transcription mode:", error);
        toast.error("Failed to switch real-time words");
      }
      return;
    }
    setIsActivating(true);
    try {
      await invoke("activate_live_preview");
      await refreshSettings();
      await refreshCapabilities();
      toast.success("Real-time words on");
    } catch (error) {
      await refreshSettings();
      await refreshCapabilities();
      toast.error(getErrorMessage(error, "Real-time words could not be turned on"));
    } finally {
      setIsActivating(false);
    }
  };

  const streams = capabilities?.capabilities?.supports_streaming === true;
  return {
    streams,
    isActivating,
    realtimeOn: streams && transcriptionMode === "live_preview",
    setRealtime,
  };
}
