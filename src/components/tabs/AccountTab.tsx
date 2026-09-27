import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";
import { toast } from "sonner";
import { AccountSection } from "../sections/AccountSection";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { createLogger } from "@/lib/logger";

const log = createLogger("account-tab");

export function AccountTab() {
  const { registerEvent } = useEventCoordinator("main");

  // Initialize account tab
  useEffect(() => {
    const init = async () => {
      try {
        // Listen for license-required event
        registerEvent<{ title: string; message: string; action: string }>(
          "license-required",
          async (event) => {
            log.debug("License required event:", event);

            // Small delay to prevent window animation conflicts
            await new Promise((resolve) => setTimeout(resolve, 100));

            // Focus main window (already on account section)
            try {
              await invoke("focus_main_window");

              // Show toast after window is focused to ensure it appears on top
              setTimeout(() => {
                toast.error(event.message, {
                  duration: 2000,
                });
              }, 200);
            } catch (error) {
              log.error("Failed to focus window:", error);
              // If window focus fails, still show the toast
              toast.error(event.message);
            }
          },
        );
      } catch (error) {
        log.error("Failed to initialize account tab:", error);
      }
    };

    init();
  }, [registerEvent]);

  return <AccountSection />;
}
