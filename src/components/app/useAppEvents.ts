import {
  useEffect,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";
import { toast } from "sonner";
import {
  sendNotification,
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import type { ScreenId } from "../navigation";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { updateService } from "@/services/updateService";
import { createLogger } from "@/lib/logger";
import { usePolishErrorEvents } from "@/components/polish/usePolishErrorEvents";
import { useEnhancementsStore } from "@/state/enhancements";

import type { SourceFilter } from "../sections/models/types";

const log = createLogger("app");

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

interface UseAppEventsOptions {
  checkModels: () => Promise<{ hasModels: boolean | null }>;
  setActiveSection: Dispatch<SetStateAction<ScreenId>>;
  setSourceFilter: (filter: SourceFilter) => void;
  setForceShowOnboarding: Dispatch<SetStateAction<boolean>>;
  forceOnboardingNeedsFreshAvailabilityRef: MutableRefObject<boolean>;
}

export function useAppEvents({
  checkModels,
  setActiveSection,
  setSourceFilter,
  setForceShowOnboarding,
  forceOnboardingNeedsFreshAvailabilityRef,
}: UseAppEventsOptions) {
  const { registerEvent } = useEventCoordinator("main");
  const checkModelsRef = useRef(checkModels);
  useEffect(() => {
    checkModelsRef.current = checkModels;
  }, [checkModels]);
  usePolishErrorEvents();

  useEffect(() => {
    let isMounted = true;
    const unlisteners: Array<() => void> = [];

    const register = async <T>(
      eventName: string,
      handler: (payload: T) => void | Promise<void>,
    ) => {
      if (!isMounted) return;
      const unlisten = await registerEvent<T>(eventName, handler);
      if (typeof unlisten !== "function") {
        return;
      }
      if (!isMounted) {
        unlisten();
        return;
      }
      unlisteners.push(unlisten);
    };

    const setup = async () => {
      try {
        await register("navigate-to-overview", () => {
          setActiveSection("home");
        });

        await register<string | undefined>("navigate-to-settings", (pane) => {
          const panes = [
            "general",
            "shortcuts",
            "privacy",
            "storage",
            "network",
            "agent",
            "advanced",
            "license",
            "about",
          ] as const;
          setActiveSection(panes.find((id) => id === pane) ?? "settings");
        });
        await register<string>("navigate-to-section", (section) => {
          const destinations = [
            "home",
            "history",
            "insights",
            "transcription",
            "polish",
            "dictionary",
            "recording",
            "help",
            "settings",
            "license",
            "general",
            "shortcuts",
            "privacy",
            "storage",
            "network",
            "agent",
            "advanced",
            "about",
          ] as const;
          const destination = destinations.find((id) => id === section);
          if (destination) setActiveSection(destination);
        });

        await register<ErrorEventPayload>("hotkey-registration-failed", (data) => {
          log.error("Hotkey registration failed:", data);
          toast.error("Hotkey Registration Failed", {
            description: data.suggestion || "The hotkey is in use by another application",
            duration: 10000,
          });
        });

        await register<string>("ai-enhancement-auth-error", (message) => {
          log.error("AI authentication error:", message);
          if (typeof message === "string") {
            useEnhancementsStore.getState().setPolishError("auth", message);
          }
          toast.error(message, {
            description: "Please update your API key in the Polish section",
          });
        });

        await register<string>("ai-enhancement-error", (message) => {
          log.warn("Polish error:", message);
          if (typeof message === "string") {
            useEnhancementsStore.getState().setPolishError("generic", message);
          }
          toast.warning(message);
        });

        await register("tray-check-updates", async () => {
          try {
            await updateService.checkForUpdatesManually();
          } catch (e) {
            log.error("Manual update check failed:", e);
            toast.error("Failed to check for updates");
          }
        });

        await register<string>("tray-action-error", (message) => {
          log.error("Tray action error:", message);
          toast.error(message);
        });

        await register<string>("parakeet-unavailable", (message) => {
          const description =
            typeof message === "string" && message.trim().length > 0
              ? message
              : "Parakeet is unavailable on this Mac. Please reinstall Voicetypr or remove the quarantine flag.";
          log.error("Parakeet unavailable:", description);
          toast.error("Parakeet Unavailable", {
            description,
            duration: 8000,
          });
        });

        const getRemoteServerErrorCopy = (data: {
          title?: string;
          message?: string;
          can_retry_from_history?: boolean;
        }) => {
          const title = data.title?.trim() || "Remote Server Unreachable";
          const message =
            data.message?.trim() || "The remote server could not complete this recording.";
          const historyGuidance = data.can_retry_from_history
            ? "Go to History to re-transcribe this recording, or select a different model."
            : "";

          return {
            title,
            message: historyGuidance ? `${message} ${historyGuidance}` : message,
          };
        };

        await register<{
          title?: string;
          message?: string;
          can_retry_from_history?: boolean;
        }>("remote-server-error", async (data) => {
          log.error("Remote server error:", data);

          // Show toast with backend-owned error details and clear action
          const { title, message } = getRemoteServerErrorCopy(data);
          toast.error(title, {
            description: message,
            duration: 8000,
          });

          // Also show system notification so user sees it even if app is not focused
          try {
            let permitted = await isPermissionGranted();
            if (!permitted) {
              const permission = await requestPermission();
              permitted = permission === "granted";
            }
            if (permitted) {
              sendNotification({
                title,
                body: message,
              });
            }
          } catch (err) {
            log.error("Failed to send system notification:", err);
          }
        });

        await register<{ title: string; message: string; action?: string }>(
          "license-required",
          async (data) => {
            log.debug("License required event received in AppContainer:", data);
            // Navigate to License section to show license management
            setActiveSection("license");
            // Show a toast to inform the user
            toast.error(data.title || "License Required", {
              description: data.message || "Please purchase or restore a license to continue",
              duration: 5000,
            });
          },
        );

        await register<{ title?: string; message?: string; autoHealed?: boolean }>(
          "soniox-storage-limit",
          (data) => {
            log.info("Soniox storage limit event received");
            // Same escalation as license-required: the backend already
            // focused the main window; land on the page with the Soniox
            // stored-files card and explain inline.
            setSourceFilter("cloud");
            setActiveSection("transcription");
            toast.error(data.title || "Soniox storage limit reached", {
              description:
                data.message ||
                "Automatic cleanup could not free enough space. Delete stored files and try again.",
              duration: 8000,
            });
          },
        );

        await register<ErrorEventPayload>("no-models-error", async (data) => {
          log.error("No models available:", data);
          setForceShowOnboarding(true);
          forceOnboardingNeedsFreshAvailabilityRef.current = true;
          const refreshedAvailability = await checkModelsRef.current();
          if (refreshedAvailability.hasModels === true) {
            forceOnboardingNeedsFreshAvailabilityRef.current = false;
            setForceShowOnboarding(false);
          }
          toast.error(data.title || "No Models Available", {
            description:
              data.suggestion ??
              data.message ??
              "Connect a cloud provider or download a local model in Models before recording.",
            duration: 8000,
          });
        });
      } catch (error) {
        log.error("Failed to register app event listeners:", error);
      }
    };

    void setup();

    return () => {
      isMounted = false;
      unlisteners.forEach((unlisten) => {
        if (typeof unlisten === "function") {
          unlisten();
        }
      });
    };
  }, [
    registerEvent,
    setActiveSection,
    setSourceFilter,
    setForceShowOnboarding,
    forceOnboardingNeedsFreshAvailabilityRef,
  ]);
}
