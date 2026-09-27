import type { BareModifierSpec } from "@/components/HotkeyInput";
import type { SavedConnection } from "@/components/RemoteServerCard";
import type { useModelManagement } from "@/hooks/useModelManagement";
import { ValidationPresets } from "@/lib/keyboard-normalizer";
import { isMacOS } from "@/lib/platform";

export interface OnboardingDesktopProps {
  onCompletionStart?: () => void;
  onCompletionError?: () => void;
  onComplete: () => void;
  modelManagement: ReturnType<typeof useModelManagement>;
}

export type Step = "welcome" | "source" | "permissions" | "readiness" | "hotkey" | "success";

export type SourceType = "local" | "cloud" | "remote";
export type PermissionStatus = "checking" | "granted" | "denied" | "error";

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
