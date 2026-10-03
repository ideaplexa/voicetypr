import { resolveScreen, type ScreenId, type SettingsPane } from "@/components/navigation";
import type { SourceFilter } from "@/components/sections/models/types";
export interface MainNavigate {
  screen: ScreenId;
  pane?: SettingsPane | null;
  source?: SourceFilter | null;
}
export type IslandNavigate = "open_models" | "open_cloud_keys" | "choose_mic";
export function islandDestination(action: IslandNavigate): MainNavigate {
  switch (action) {
    case "open_models": return { screen: "transcription", source: "local" };
    case "open_cloud_keys": return { screen: "transcription", source: "cloud" };
    case "choose_mic": return { screen: "recording" };
  }
}
export function routeMainNavigation(
  destination: MainNavigate,
  setScreen: (screen: ScreenId) => void,
  setSource: (source: SourceFilter) => void,
  openSettingsPane?: (pane: SettingsPane) => void,
) {
  const resolved = resolveScreen(destination.screen);
  if (resolved.screen === "settings" && openSettingsPane) {
    openSettingsPane(destination.pane ?? resolved.pane ?? "general");
  } else {
    if (destination.source) setSource(destination.source);
    setScreen(destination.screen);
  }
}
