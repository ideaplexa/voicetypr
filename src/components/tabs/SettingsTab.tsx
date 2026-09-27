import { useEffect } from "react";
import { toast } from "sonner";
import { GeneralSettings } from "../sections/GeneralSettings";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { createLogger } from "@/lib/logger";

const log = createLogger("settings-tab");

interface ErrorEventPayload {
  title?: string;
  message: string;
  severity?: "info" | "warning" | "error";
  actions?: string[];
  details?: string;
  hotkey?: string;
  error?: string;
  suggestion?: string;
}

export function SettingsTab() {
  const { registerEvent } = useEventCoordinator("main");

  // Initialize settings tab
  useEffect(() => {
    const init = async () => {
      try {
        // Listen for hotkey registration failures
        registerEvent<ErrorEventPayload>("hotkey-registration-failed", (data) => {
          log.error("Hotkey registration failed:", data);

          toast.error("Hotkey Registration Failed", {
            description: data.suggestion || "The hotkey is in use by another application",
            duration: 10000, // Persistent for important errors
          });
        });

        // Listen for no speech detected events with settings action
        registerEvent<ErrorEventPayload>("no-speech-detected", (data) => {
          log.warn("No speech detected:", data);

          // Determine toast type based on severity
          const toastFn = data.severity === "error" ? toast.error : toast.warning;

          toastFn(data.title || "No Speech Detected", {
            description: data.message || "Please check your microphone and speak clearly",
            duration: data.severity === "error" ? 8000 : 5000,
          });
        });
      } catch (error) {
        log.error("Failed to initialize settings tab:", error);
      }
    };

    init();
  }, [registerEvent]);

  return <GeneralSettings />;
}
