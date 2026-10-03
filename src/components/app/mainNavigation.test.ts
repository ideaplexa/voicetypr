import { describe, expect, it, vi } from "vitest";
import {
  islandDestination,
  routeMainNavigation,
  type MainNavigate,
} from "@/components/app/mainNavigation";
describe("native menu and island navigation", () => {
  it.each([
    ["open_models", { screen: "transcription", source: "local" }],
    ["open_cloud_keys", { screen: "transcription", source: "cloud" }],
    ["choose_mic", { screen: "recording" }],
  ] as const)("routes %s", (action, destination) => {
    expect(islandDestination(action)).toEqual(destination);
  });
  it.each([
    "home",
    "history",
    "insights",
    "models",
    "polish",
    "transcription",
    "recording",
    "dictionary",
    "audio",
    "help",
  ] as const)("routes tray destination %s", (screen) => {
    const navigate = vi.fn();
    routeMainNavigation({ screen }, navigate, vi.fn());
    expect(navigate).toHaveBeenCalledWith(screen);
  });
  it("routes settings directly to the requested pane", () => {
    const navigate = vi.fn();
    const pane = vi.fn();
    routeMainNavigation({ screen: "settings", pane: "shortcuts" }, navigate, vi.fn(), pane);
    expect(pane).toHaveBeenCalledWith("shortcuts");
    expect(navigate).not.toHaveBeenCalled();
  });
  it("sets the source before visiting transcription", () => {
    const calls: string[] = [];
    const destination: MainNavigate = { screen: "transcription", source: "cloud" };
    routeMainNavigation(
      destination,
      (s) => calls.push(s),
      (s) => calls.push(s),
    );
    expect(calls).toEqual(["cloud", "transcription"]);
  });
  it.each([
    "general",
    "shortcuts",
    "privacy",
    "storage",
    "network",
    "agent",
    "advanced",
    "license",
    "about",
  ] as const)("opens the %s modal pane for both explicit panes and legacy ids", (id) => {
    const navigate = vi.fn();
    const pane = vi.fn();
    routeMainNavigation({ screen: "settings", pane: id }, navigate, vi.fn(), pane);
    expect(pane).toHaveBeenLastCalledWith(id);
    routeMainNavigation({ screen: id }, navigate, vi.fn(), pane);
    expect(pane).toHaveBeenLastCalledWith(id);
    expect(navigate).not.toHaveBeenCalled();
  });
  it("defaults Settings to General and rejects unknown screens", () => {
    const navigate = vi.fn();
    const pane = vi.fn();
    routeMainNavigation({ screen: "settings", pane: "unknown" }, navigate, vi.fn(), pane);
    expect(pane).toHaveBeenCalledExactlyOnceWith("general");
    routeMainNavigation({ screen: "unknown" }, navigate, vi.fn(), pane);
    expect(navigate).not.toHaveBeenCalled();
    expect(pane).toHaveBeenCalledOnce();
  });
});
