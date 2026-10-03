import { describe, expect, it, vi } from "vitest";
import { islandDestination, routeMainNavigation, type MainNavigate } from "@/components/app/mainNavigation";
describe("native menu and island navigation", () => {
  it.each([
    ["open_models", { screen: "transcription", source: "local" }],
    ["open_cloud_keys", { screen: "transcription", source: "cloud" }],
    ["choose_mic", { screen: "recording" }],
  ] as const)("routes %s", (action, destination) => {
    expect(islandDestination(action)).toEqual(destination);
  });
  it.each(["home", "polish", "transcription", "audio", "help"] as const)("routes tray destination %s", (screen) => {
    const navigate = vi.fn();
    routeMainNavigation({ screen }, navigate, vi.fn());
    expect(navigate).toHaveBeenCalledWith(screen);
  });
  it("routes settings directly to the requested pane", () => {
    const navigate = vi.fn(); const pane = vi.fn();
    routeMainNavigation({ screen: "settings", pane: "shortcuts" }, navigate, vi.fn(), pane);
    expect(pane).toHaveBeenCalledWith("shortcuts");
    expect(navigate).not.toHaveBeenCalled();
  });
  it("sets the source before visiting transcription", () => {
    const calls: string[] = [];
    const destination: MainNavigate = { screen: "transcription", source: "cloud" };
    routeMainNavigation(destination, (s) => calls.push(s), (s) => calls.push(s));
    expect(calls).toEqual(["cloud", "transcription"]);
  });
});
