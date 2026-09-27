import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { updateService } from "@/services/updateService";
import type { AppSettings } from "@/types";
import { loadApiKeysToCache } from "@/utils/keyring";
import { createLogger } from "@/lib/logger";

const log = createLogger("app");

export function useAppBootstrap(settings: AppSettings | null) {
  const [justUpdatedVersion, setJustUpdatedVersion] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    const timeoutId = window.setTimeout(() => {
      if (cancelled) return;
      loadApiKeysToCache().catch((error) => {
        log.error("Failed to load API keys to cache:", error);
      });
    }, 100);

    const init = async () => {
      try {
        // Run cleanup if enabled
        if (settings?.transcription_cleanup_days) {
          await invoke("cleanup_old_transcriptions", {
            days: settings.transcription_cleanup_days,
          });
        }

        // Initialize update service for automatic update checks
        if (settings) {
          await updateService.initialize(settings);
        }

        // Read the running version before consuming the one-shot update marker.
        // Settings hydration can restart this effect; a cancelled run must leave
        // the marker available for the replacement run.
        let currentVersion: string;
        try {
          currentVersion = await getVersion();
        } catch (error) {
          log.error("Failed to read current app version:", error);
          return;
        }

        if (cancelled) return;

        // Check if this exact app version was just installed. A retained marker
        // from an interrupted older update must not announce the wrong release.
        const updatedVersion = updateService.getJustUpdatedVersion?.();
        if (updatedVersion) {
          if (currentVersion !== updatedVersion) {
            log.warn(
              `Ignoring stale update marker for ${updatedVersion}; running ${currentVersion}`,
            );
          } else {
            setJustUpdatedVersion(updatedVersion);
            try {
              await invoke("focus_main_window");
            } catch {
              // Window focus is best-effort; dialog still renders.
            }
          }
        }
      } catch (error) {
        log.error("Failed to initialize:", error);
      }
    };

    void init();

    return () => {
      cancelled = true;
      clearTimeout(timeoutId);
      updateService.dispose();
    };
  }, [settings]);

  return { justUpdatedVersion, setJustUpdatedVersion };
}
