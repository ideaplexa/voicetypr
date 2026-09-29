import type { ShortcutBinding } from "@/types/shortcuts";

export type PrimaryMode = "toggle" | "push_to_talk";

export interface PrimaryShortcut {
  binding: ShortcutBinding | null;
  hotkey: string | undefined;
  mode: PrimaryMode;
}

/** Resolve the recording trigger and behavior that the user actually has active. */
export function findActivePrimaryBinding(bindings: ShortcutBinding[]): ShortcutBinding | null {
  const eligible = bindings.filter((binding) =>
    binding.enabled &&
    (binding.action === "hold_to_record" || binding.action === "toggle_recording") &&
    (binding.trigger_kind === "modifier_hold" || binding.trigger_kind === "isolated_tap"),
  );
  return eligible.find((binding) => binding.id === "onboarding-primary-hold") ?? eligible[0] ?? null;
}

export function resolvePrimaryShortcut(
  settings: { hotkey?: string; recording_mode?: PrimaryMode } | null | undefined,
  bindings: ShortcutBinding[],
): PrimaryShortcut {
  const hotkey = settings?.hotkey || undefined;
  const binding = hotkey ? null : findActivePrimaryBinding(bindings);
  return {
    binding,
    hotkey,
    mode: binding?.action === "hold_to_record" ? "push_to_talk" : binding ? "toggle" : settings?.recording_mode ?? "toggle",
  };
}
