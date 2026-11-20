import type { BareModifierSpec } from "@/components/HotkeyInput";
import type { SavedConnection } from "@/components/RemoteServerCard";
import type { useModelManagement } from "@/hooks/useModelManagement";
import { ValidationPresets } from "@/lib/keyboard-normalizer";
import { isMacOS } from "@/lib/platform";
import type { ScreenId } from "@/components/navigation";
import type { LicenseStatus, TrialStartError } from "@/types";

export type Step =
  | "promise"
  | "source"
  | "language"
  | "engine"
  | "permissions"
  | "shortcut"
  | "polish"
  | "try"
  | "result"
  | "fork"
  | "use"
  | "tour"
  | "buy"
  | "trial"
  | "done";

export interface OnboardingDesktopProps {
  /** Screenshot fixtures only; production onboarding always starts at the promise. */
  previewStep?: Step;
  onCompletionStart?: () => void;
  onCompletionError?: () => void;
  /** Opens the app, on the section the tour remembered when there is one. */
  onComplete: (open?: ScreenId) => void;
  /** The licence as last checked; null when the check has not answered. */
  licenseStatus?: LicenseStatus | null;
  /** The latest licence check failed, so the status may be stale. */
  licenseCheckFailed?: boolean;
  onStartTrial?: (name: string, email: string) => Promise<TrialStartError | null>;
  /** Resolves to a message the user can act on, or null once the key is active. */
  onActivateLicense?: (key: string) => Promise<string | null>;
  onRecheckLicense?: () => Promise<void>;
  onOpenPricing?: () => Promise<void>;
  modelManagement: ReturnType<typeof useModelManagement>;
}

export type SourceType = "local" | "cloud" | "remote";
export type PermissionStatus = "checking" | "granted" | "not_determined" | "denied" | "restricted" | "error";

export interface PermissionState {
  status: PermissionStatus;
  error?: string;
}

export interface DiscoveredRemoteServer {
  name: string;
  host: string;
  port: number;
  model: string;
  auth_required: boolean;
  machine_id: string;
}

export const READINESS_COPY: Record<SourceType, { title: string; description: string }> = {
  local: {
    title: "Choose a local model",
    description: "Select a downloaded model, or download one now.",
  },
  cloud: {
    title: "Connect a cloud provider",
    description: "Add an API key, then select that provider for transcription.",
  },
  remote: {
    title: "Connect a remote Voicetypr",
    description: "Choose an online Voicetypr server on your network.",
  },
};

export const isRemoteServerOnline = (server?: SavedConnection | null) =>
  server?.status === "Online";

export const sourceLabel = (sourceType: SourceType) =>
  sourceType === "local" ? "Local setup" : sourceType === "cloud" ? "Cloud setup" : "Remote setup";

export const ONBOARDING_HOTKEY_VALIDATION = ValidationPresets.custom({
  minKeys: 1,
  requireModifier: false,
  requireModifierForMultiKey: true,
});

/** Format a bare modifier spec as a short human-readable label, e.g. "Right ⌥". */
export function formatBareModifierLabel({ modifier, side }: BareModifierSpec): string {
  const sideStr = side === "right" ? "Right " : side === "left" ? "Left " : "";
  const macIcons: Record<string, string> = {
    alt: "⌥",
    meta: "⌘",
    control: "⌃",
    shift: "⇧",
  };
  const modStr = isMacOS
    ? (macIcons[modifier] ?? modifier)
    : modifier.charAt(0).toUpperCase() + modifier.slice(1);
  return `${sideStr}${modStr}`;
}
