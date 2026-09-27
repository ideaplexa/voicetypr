import { isMacOS } from "@/lib/platform";
import type { BareModifierSpec } from "./types";

const BARE_MOD_ICONS: Record<string, string> = {
  alt: "⌥",
  meta: "⌘",
  control: "⌃",
  shift: "⇧",
};

export function formatBareModifierDisplay(spec: BareModifierSpec): string {
  const sideLabel = spec.side === "right" ? "Right " : spec.side === "left" ? "Left " : "";
  const modLabel = isMacOS
    ? (BARE_MOD_ICONS[spec.modifier] ?? spec.modifier)
    : spec.modifier.charAt(0).toUpperCase() + spec.modifier.slice(1);
  return `${sideLabel}${modLabel} · hold to record`;
}
