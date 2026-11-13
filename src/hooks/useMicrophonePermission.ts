import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createLogger } from "@/lib/logger";

const log = createLogger("permissions");

interface MicrophonePermissionOptions {
  // When false, skip the automatic check on mount and rely on explicit calls.
  checkOnMount?: boolean;
}

export function useMicrophonePermission(options?: MicrophonePermissionOptions) {
  const { checkOnMount = true } = options || {};
  const [hasPermission, setHasPermission] = useState<boolean | null>(null);
  const [isChecking, setIsChecking] = useState(false);
  const [permissionStatus, setPermissionStatus] = useState<"checking" | "granted" | "not_determined" | "denied" | "restricted">("checking");

  const checkPermission = useCallback(async () => {
    setIsChecking(true);
    try {
      const status = await invoke<"granted" | "not_determined" | "denied" | "restricted">("get_microphone_permission_status");
      const result = status === "granted";
      setPermissionStatus(status);
      setHasPermission(result);
      return result;
    } catch (error) {
      log.error("Failed to check microphone permission:", error);
      setHasPermission(false);
      setPermissionStatus("denied");
      return false;
    } finally {
      setIsChecking(false);
    }
  }, []);

  const requestPermission = useCallback(async () => {
    try {
      const result = await invoke<boolean>("request_microphone_permission");
      setHasPermission(result);
      setPermissionStatus(result ? "granted" : "denied");
      return result;
    } catch (error) {
      log.error("Failed to request microphone permission:", error);
      return false;
    }
  }, []);

  // Optionally check permission on mount
  useEffect(() => {
    if (!checkOnMount) return;
    void (async () => {
      await Promise.resolve();
      await checkPermission();
    })();
  }, [checkPermission, checkOnMount]);

  // Listen for permission changes
  useEffect(() => {
    const unlistenGranted = listen("microphone-granted", () => {
      log.info("[useMicrophonePermission] Permission granted event received");
      setHasPermission(true);
      setPermissionStatus("granted");
    });

    const unlistenDenied = listen("microphone-denied", () => {
      log.info("[useMicrophonePermission] Permission denied event received");
      setHasPermission(false);
      setPermissionStatus("denied");
    });

    return () => {
      Promise.all([unlistenGranted, unlistenDenied]).then((unsubs) => {
        unsubs.forEach((unsub) => unsub());
      });
    };
  }, []);

  return {
    hasPermission,
    permissionStatus,
    isChecking,
    checkPermission,
    requestPermission,
  };
}
