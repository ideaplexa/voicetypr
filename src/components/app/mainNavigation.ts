import {
  footerNavScreens,
  mainNavScreens,
  resolveScreen,
  screenAliases,
  tuningNavScreens,
  type ScreenId,
  type SettingsPane,
} from "@/components/navigation";
import type { SourceFilter } from "@/components/sections/models/types";

// Native event payloads are validated here, including the legacy string events.
export interface MainNavigate {
  screen: string;
  telemetry_source?: "tray" | "island" | "app";
  pane?: string | null;
  source?: SourceFilter | null;
}
export type IslandNavigate = "open_models" | "open_cloud_keys" | "choose_mic";
export function islandDestination(action: IslandNavigate): MainNavigate {
  switch (action) {
    case "open_models":
      return { screen: "transcription", source: "local" };
    case "open_cloud_keys":
      return { screen: "transcription", source: "cloud" };
    case "choose_mic":
      return { screen: "recording" };
  }
}

const screenIds = new Set<string>([
  ...mainNavScreens.map(({ id }) => id),
  ...tuningNavScreens.map(({ id }) => id),
  ...footerNavScreens.map(({ id }) => id),
  ...Object.keys(screenAliases),
  "license",
]);
const settingsPanes: SettingsPane[] = [
  "general",
  "shortcuts",
  "privacy",
  "storage",
  "network",
  "agent",
  "advanced",
  "license",
  "about",
];

export function routeMainNavigation(
  destination: MainNavigate,
  setScreen: (screen: ScreenId) => void,
  setSource: (source: SourceFilter) => void,
  openSettingsPane?: (pane: SettingsPane) => void,
) {
  if (!screenIds.has(destination.screen)) return;
  const screen = destination.screen as ScreenId;
  const resolved = resolveScreen(screen);
  if (resolved.screen === "settings") {
    const requestedPane = settingsPanes.find((id) => id === destination.pane);
    const pane = requestedPane ?? resolved.pane ?? "general";
    if (openSettingsPane) openSettingsPane(pane);
    else setScreen(requestedPane ?? resolved.pane ?? "settings");
  } else {
    if (destination.source) setSource(destination.source);
    // Preserve aliases such as audio, which also opens the upload dialog.
    setScreen(screen);
  }
}
