import { invoke } from "@tauri-apps/api/core";
import type { SettingsPane } from "@/components/navigation";
export type ObservabilityEvent =
  | { name: "insights_viewed"; properties: { period: "week" | "month" | "all" } }
  | { name: "share_card_action"; properties: { action: "copy" | "save" | "post_x" } }
  | { name: "settings_opened"; properties: { pane: SettingsPane; source: "island" | "tray" | "app" } }
  | { name: "quick_setting_changed"; properties: { setting: "polish" | "engine" | "mic" | "language" | "mode" | "live_preview"; source: "island" | "tray" | "app" } }
  | { name: "island_peek_opened"; properties: Record<string, never> };
export function recordEvent(event: ObservabilityEvent): void {
  void invoke("record_observability_event", event).catch(() => {});
}

let settingsSource: "app" | "tray" | "island" = "app";
export function markSettingsSource(source: unknown) {
  settingsSource = source === "tray" || source === "island" ? source : "app";
}
export function takeSettingsSource(): "app" | "tray" | "island" {
  const source = settingsSource;
  settingsSource = "app";
  return source;
}
