import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { recordEvent, type ObservabilityEvent } from "./observability";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
it("serializes every frontend event using closed categories only", () => {
  const events: ObservabilityEvent[] = [
    { name: "insights_viewed", properties: { period: "week" } },
    { name: "share_card_action", properties: { action: "copy" } },
    { name: "share_card_action", properties: { action: "save" } },
    { name: "share_card_action", properties: { action: "post_x" } },
    { name: "settings_opened", properties: { pane: "privacy", source: "app" } },
    { name: "quick_setting_changed", properties: { setting: "engine", source: "app" } },
    { name: "island_peek_opened", properties: {} },
  ];
  for (const event of events) {
    recordEvent(event);
    const serialized = JSON.stringify(event);
    expect(serialized).not.toMatch(/\/Users\/|transcript|audio|clipboard|title|prompt|app_name/);
    expect(invoke).toHaveBeenCalledWith("record_observability_event", event);
  }
});
