import type { BareModifierSpec } from "@/components/HotkeyInput";
import type { SavedConnection } from "@/components/RemoteServerCard";
import type { useModelManagement } from "@/hooks/useModelManagement";
import { ValidationPresets } from "@/lib/keyboard-normalizer";
import { isMacOS } from "@/lib/platform";
import type { LicenseStatus, TrialStartError } from "@/types";

export type Step = "promise" | "trial" | "source" | "setup" | "try" | "polish" | "done";

export interface OnboardingDesktopProps {
  /** Screenshot fixtures only; production onboarding always starts at the promise. */
  previewStep?: Step;
  onCompletionStart?: () => void;
  onCompletionError?: () => void;
  onComplete: () => void;
  /** The licence as last checked; null when the check has not answered. */
  licenseStatus?: LicenseStatus | null;
  onStartTrial?: (name: string, email: string) => Promise<TrialStartError | null>;
  /** Resolves to a message the user can act on, or null once the key is active. */
  onActivateLicense?: (key: string) => Promise<string | null>;
  onRecheckLicense?: () => Promise<void>;
  onOpenPricing?: () => Promise<void>;
  modelManagement: ReturnType<typeof useModelManagement>;
}

/**
 * Whether new users get Polish without their own key. Undecided by the founder;
 * nothing backs it yet, so the first sentence shows the engine's own text unless
 * this user has Polish on.
 */
export const NEW_USERS_GET_POLISH = false;

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
