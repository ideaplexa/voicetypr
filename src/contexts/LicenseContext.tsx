import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { LicenseStatus } from "@/types";
import { toast } from "sonner";
import { getErrorMessage } from "@/utils/error";
import { createLogger } from "@/lib/logger";

const log = createLogger("license");
const LICENSE_COMMAND_TIMEOUT_MS = 60_000;

interface LicenseContextValue {
  status: LicenseStatus | null;
  isLoading: boolean;
  checkStatus: () => Promise<void>;
  revalidateLicense: () => Promise<void>;
  restoreLicense: () => Promise<void>;
  activateLicense: (key: string) => Promise<void>;
  deactivateLicense: () => Promise<void>;
  openPurchasePage: () => Promise<void>;
}

const LicenseContext = createContext<LicenseContextValue | undefined>(undefined);

const withTimeout = async <T,>(promise: Promise<T>, timeoutMs: number): Promise<T> => {
  let timeoutHandle: ReturnType<typeof setTimeout> | undefined;
  const timeoutPromise = new Promise<never>((_, reject) => {
    timeoutHandle = setTimeout(
      () => reject(new Error("License status check timed out")),
      timeoutMs,
    );
  });

  try {
    return await Promise.race([promise, timeoutPromise]);
  } finally {
    clearTimeout(timeoutHandle);
  }
};

const getFriendlyLicenseError = (action: "activate" | "restore", rawMessage?: string) => {
  const lower = rawMessage?.toLowerCase() ?? "";
  const actionLabel = action === "activate" ? "activate license" : "restore license";

  if (lower.includes("network error") || lower.includes("error sending request")) {
    return `Failed to ${actionLabel}. Please check your connection and try again.`;
  }

  if (lower.includes("already activated on another device")) {
    return "This license is already activated on another device";
  }

  if (lower.includes("maximum number of devices")) {
    return "This license has reached its device activation limit";
  }

  if (lower.includes("invalid license key")) {
    return "Invalid license key";
  }

  if (action === "restore" && lower.includes("no license found")) {
    return "No license found. Please enter your license key manually.";
  }

  return rawMessage || `Failed to ${actionLabel}`;
};

const openPurchasePage = async () => {
  try {
    await invoke("open_purchase_page");
  } catch (error) {
    log.error("Failed to open purchase page:", error);
    // Fallback to window.open
    window.open("https://voicetypr.com/#pricing", "_blank", "noopener,noreferrer");
  }
};

export function LicenseProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<LicenseStatus | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const latestCheckStatusId = useRef(0);

  const checkStatus = useCallback(async () => {
    const checkId = ++latestCheckStatusId.current;
    try {
      setIsLoading(true);
      log.debug("Checking license status...");

      const invokePromise = invoke<LicenseStatus>("check_license_status");
      invokePromise.catch(() => {
        // Prevent unhandled rejections if we time out and ignore the result.
      });

      const licenseStatus = await withTimeout(invokePromise, LICENSE_COMMAND_TIMEOUT_MS);

      if (checkId !== latestCheckStatusId.current) return;
      log.debug("License status received:", {
        status: licenseStatus.status,
        trial_days_left: licenseStatus.trial_days_left,
        license_type: licenseStatus.license_type,
        expires_at: licenseStatus.expires_at,
        verification_state: licenseStatus.verification_state,
      });
      setStatus(licenseStatus);
    } catch (error) {
      if (checkId !== latestCheckStatusId.current) return;

      if (error instanceof Error && error.message === "License status check timed out") {
        toast.error("License status check timed out. Please try again.");
        return;
      }

      const message = getErrorMessage(error, "Failed to check license status");
      log.error("Failed to check license status:", error);
      toast.error(message);
    } finally {
      if (checkId === latestCheckStatusId.current) {
        setIsLoading(false);
      }
    }
  }, []);

  const revalidateLicense = useCallback(async () => {
    const checkId = ++latestCheckStatusId.current;
    try {
      setIsLoading(true);
      const invokePromise = invoke<LicenseStatus>("revalidate_license");
      invokePromise.catch(() => {
        // Prevent unhandled rejections if the UI timeout wins the race.
      });
      const licenseStatus = await withTimeout(invokePromise, LICENSE_COMMAND_TIMEOUT_MS);
      if (checkId !== latestCheckStatusId.current) return;
      setStatus(licenseStatus);

      if (licenseStatus.verification_state === "verified") {
        toast.success("License verified");
      } else {
        toast.info("Couldn’t verify the license yet. Offline access remains available.");
      }
    } catch (error: unknown) {
      if (checkId !== latestCheckStatusId.current) return;
      const message = getErrorMessage(error, "Failed to revalidate license");
      log.error("Failed to revalidate license:", error);
      toast.error(message);
    } finally {
      if (checkId === latestCheckStatusId.current) {
        setIsLoading(false);
      }
    }
  }, []);

  const restoreLicense = useCallback(async () => {
    try {
      const licenseStatus = await invoke<LicenseStatus>("restore_license");
      setStatus(licenseStatus);
      toast.success("License restored successfully");
    } catch (error: unknown) {
      const message = getErrorMessage(error);
      log.error("Failed to restore license:", error);
      toast.error(getFriendlyLicenseError("restore", message));
    }
  }, []);

  const activateLicense = useCallback(async (key: string) => {
    try {
      const licenseStatus = await invoke<LicenseStatus>("activate_license", { licenseKey: key });
      setStatus(licenseStatus);
      toast.success("License activated successfully");
    } catch (error: unknown) {
      const message = getErrorMessage(error);
      log.error("Failed to activate license:", error);
      toast.error(getFriendlyLicenseError("activate", message));
    }
  }, []);

  const deactivateLicense = useCallback(async () => {
    try {
      await invoke("deactivate_license");
      // Re-check status after deactivation
      await checkStatus();
      toast.success("License deactivated successfully");
    } catch (error: unknown) {
      const message = getErrorMessage(error, "Failed to deactivate license");
      log.error("Failed to deactivate license:", error);
      toast.error(message);
    }
  }, [checkStatus]);

  // Check license status on mount
  useEffect(() => {
    log.debug("LicenseProvider mounted, checking status...");
    void (async () => {
      await Promise.resolve();
      await checkStatus();
    })();
  }, [checkStatus]);

  const value = useMemo<LicenseContextValue>(
    () => ({
      status,
      isLoading,
      checkStatus,
      revalidateLicense,
      restoreLicense,
      activateLicense,
      deactivateLicense,
      openPurchasePage,
    }),
    [
      status,
      isLoading,
      checkStatus,
      revalidateLicense,
      restoreLicense,
      activateLicense,
      deactivateLicense,
    ],
  );

  return <LicenseContext.Provider value={value}>{children}</LicenseContext.Provider>;
}

export function useLicense() {
  const context = useContext(LicenseContext);
  if (!context) {
    throw new Error("useLicense must be used within a LicenseProvider");
  }
  return context;
}
