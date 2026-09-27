import { open } from "@tauri-apps/plugin-shell";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";
import type { LicenseStatus } from "@/types";

const log = createLogger("account");

export async function openExternalLink(url: string) {
  try {
    await open(url);
  } catch (error) {
    log.error("Failed to open external link:", error);
    toast.error("Failed to open link");
  }
}

export function formatLicenseStatus(status: LicenseStatus | null, isLoading: boolean): string {
  if (isLoading) return "Loading...";
  if (!status) return "Unknown";

  switch (status.status) {
    case "licensed":
      return `Licensed`;
    case "trial":
      return status.trial_days_left !== undefined
        ? status.trial_days_left > 0
          ? `Trial - ${status.trial_days_left} day${status.trial_days_left > 1 ? "s" : ""}`
          : "Trial expires today"
        : "Trial (3-day limit)";
    case "expired":
      return "Trial Expired";
    case "none":
      return "No License";
    default:
      return "Unknown";
  }
}

export function getStatusBadgeVariant(
  status: LicenseStatus | null,
): "default" | "secondary" | "destructive" | "outline" {
  if (!status) return "secondary";

  switch (status.status) {
    case "licensed":
      return "default";
    case "trial":
      return "secondary";
    case "expired":
      return "destructive";
    case "none":
      return "outline";
    default:
      return "secondary";
  }
}
