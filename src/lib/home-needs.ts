import type { ScreenId, SettingsPane } from "@/components/navigation";
import type { SourceFilter } from "@/components/sections/models/types";
import type { HomeStatus } from "@/lib/home-readiness";
import type { RecoveryKind } from "@/types/island-events";

/** A failed dictation whose audio is still kept, as reported by `list_kept_dictations`. */
export interface KeptRecording {
  id: string;
  engine_short: string;
  alt_engine_short: string | null;
  alt_engine_id: string | null;
  remaining_ms: number;
  busy: boolean;
  retry_ready?: boolean;
  retry_failed?: boolean;
  kind?: RecoveryKind;
}

export type NeedAction =
  | { kind: "retry"; id: string; engine: string | null; anyway?: boolean }
  | { kind: "discard"; id: string }
  | { kind: "install" }
  | { kind: "restart" }
  | { kind: "screen"; screen: ScreenId; source?: SourceFilter }
  | { kind: "pane"; pane: SettingsPane };

export interface NeedRow {
  id: string;
  icon: "retry" | "update" | "license" | "microphone" | "accessibility" | "source";
  title: string;
  description: string;
  button: string;
  disabled?: boolean;
  action: NeedAction;
  discard?: { kind: "discard"; id: string };
  discardLabel?: "Delete audio";
  alternative?: { kind: "retry"; id: string; engine: string };
}

function minutesLeft(ms: number): string {
  if (ms > 24 * 60 * 60 * 1000) {
    const days = Math.ceil(ms / (24 * 60 * 60 * 1000));
    return `${days} more days`;
  }
  const minutes = Math.max(1, Math.ceil(ms / 60_000));
  return `${minutes} more ${minutes === 1 ? "minute" : "minutes"}`;
}

export function homeNeeds({
  kept,
  updateVersion,
  updateInstall = "idle",
  status,
  accessibilityMissing,
}: {
  kept: KeptRecording[];
  updateVersion: string | null;
  /** Shared install state: the row never offers a second install while one runs. */
  updateInstall?: "idle" | "prompting" | "installing" | "restart_pending";
  status: HomeStatus;
  accessibilityMissing: boolean;
}): NeedRow[] {
  const rows: NeedRow[] = kept.map((clip) => ({
    id: `kept:${clip.id}`,
    icon: "retry",
    title: clip.retry_failed ? `${clip.engine_short} couldn't transcribe this` : "A recording was kept",
    description: clip.kind === "no_speech" && clip.remaining_ms <= 30_000 ? "No speech heard. Audio kept temporarily for 30 seconds." : `${clip.retry_failed ? "Recording kept." : clip.kind === "unknown" ? "Recovered after an interrupted save." : `${clip.engine_short} couldn't finish it.`} Audio kept for ${minutesLeft(clip.remaining_ms)}.`,
    button: clip.busy ? "Retrying…" : clip.retry_ready === false ? "Finishing…" : clip.retry_failed ? "Retry again" : clip.kind === "no_speech" ? "Transcribe anyway" : "Retry",
    disabled: clip.busy || clip.retry_ready === false,
    // Null reuses the last failed selection, not the current Settings model.
    action: { kind: "retry", id: clip.id, engine: clip.retry_failed || clip.kind === "unknown" ? null : clip.alt_engine_id, ...(clip.kind === "no_speech" ? { anyway: true } : {}) },
    discard: { kind: "discard", id: clip.id },
    ...(clip.retry_failed ? { discardLabel: "Delete audio" as const } : {}),
    ...(clip.retry_failed && clip.alt_engine_short && clip.alt_engine_id ? { alternative: { kind: "retry" as const, id: clip.id, engine: clip.alt_engine_id } } : {}),
  }));
  if (updateVersion) {
    const busy = updateInstall === "prompting" || updateInstall === "installing";
    rows.push(
      updateInstall === "restart_pending"
        ? {
            id: "update",
            icon: "update",
            title: `Update installed · ${updateVersion}`,
            description: "Restart Voicetypr to finish updating.",
            button: "Restart",
            action: { kind: "restart" },
          }
        : {
            id: "update",
            icon: "update",
            title: `Update ready · ${updateVersion}`,
            description: busy ? "Installing the update…" : "Installs in a few seconds and restarts Voicetypr.",
            button: busy ? "Installing…" : "Install",
            disabled: busy,
            action: { kind: "install" },
          },
    );
  }
  if (!status.ready && !status.pending) {
    const action: NeedAction = status.pane
      ? { kind: "pane", pane: status.pane }
      : { kind: "screen", screen: status.screen ?? "transcription", source: status.source };
    rows.push(
      status.kind === "license"
        ? { id: "license", icon: "license", title: status.label, description: "Check or activate your license.", button: "Open", action }
        : status.kind === "microphone"
          ? { id: "microphone", icon: "microphone", title: status.label, description: "Voicetypr can't hear you until it's allowed.", button: "Fix", action }
          : { id: "source", icon: "source", title: status.label, description: "Pick a transcription source to start dictating.", button: "Set up", action },
    );
  }
  if (accessibilityMissing)
    rows.push({
      id: "accessibility",
      icon: "accessibility",
      title: "Accessibility access needed",
      description: "Voicetypr can't paste into other apps until it's allowed.",
      button: "Fix",
      action: { kind: "pane", pane: "advanced" },
    });
  return rows;
}

export interface TipRow {
  id: "polish" | "dictionary";
  title: string;
  description: string;
  button: string;
  screen: ScreenId;
}

export function homeTips({ polishOn, dictionaryEmpty }: { polishOn: boolean | null; dictionaryEmpty: boolean | null }): TipRow[] {
  const tips: TipRow[] = [];
  if (polishOn === false)
    tips.push({ id: "polish", title: "Turn on Polish", description: "Clean up fillers and punctuation as you paste.", button: "Set up", screen: "polish" });
  if (dictionaryEmpty === true)
    tips.push({ id: "dictionary", title: "Teach it your words", description: "Names and terms you use, spelled right every time.", button: "Add words", screen: "dictionary" });
  return tips;
}
