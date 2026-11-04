import type { LucideIcon } from "lucide-react";
import {
  AudioLines,
  BadgeCheck,
  ChartNoAxesColumnIncreasing,
  BookA,
  History,
  House,
  LifeBuoy,
  Mic,
  Settings2,
  Sparkles,
} from "lucide-react";

export type MainScreenId =
  | "home"
  | "history"
  | "insights"
  | "transcription"
  | "polish"
  | "dictionary"
  | "recording"
  | "settings"
  | "help"
  | "license";
export type LegacyScreenId =
  | "overview"
  | "recordings"
  | "audio"
  | "models"
  | "formatting"
  | "general"
  | "shortcuts"
  | "network"
  | "agent"
  | "advanced"
  | "privacy"
  | "storage"
  | "about"
  | "report-problem";
export type ScreenId = MainScreenId | LegacyScreenId;
export type SettingsPane =
  | "general"
  | "shortcuts"
  | "storage"
  | "network"
  | "agent"
  | "advanced"
  | "license";

export const settingsPaneIds: readonly SettingsPane[] = [
  "general",
  "shortcuts",
  "storage",
  "network",
  "agent",
  "advanced",
  "license",
];

/** Panes folded into General in beta.5; old deep links still land somewhere sensible. */
const retiredSettingsPanes: Record<string, SettingsPane> = { privacy: "general", about: "general" };

export function parseSettingsPane(value: unknown): SettingsPane | undefined {
  if (typeof value !== "string") return undefined;
  return settingsPaneIds.find((id) => id === value) ?? retiredSettingsPanes[value];
}

export interface ScreenDefinition {
  id: MainScreenId;
  label: string;
  icon: LucideIcon;
  description: string;
}

export const mainNavScreens: ScreenDefinition[] = [
  { id: "home", label: "Home", icon: House, description: "Your shortcut, what needs you, and ways to get more out of Voicetypr." },
  {
    id: "history",
    label: "History",
    icon: History,
    description: "Past transcriptions and file transcription.",
  },
  {
    id: "insights",
    label: "Insights",
    icon: ChartNoAxesColumnIncreasing,
    description: "Your dictation activity and progress.",
  },
];

export const tuningNavScreens: ScreenDefinition[] = [
  {
    id: "recording",
    label: "Recording",
    icon: Mic,
    description: "Microphone, shortcut, and recording behavior.",
  },
  {
    id: "transcription",
    label: "Transcription",
    icon: AudioLines,
    description: "Choose a transcription source.",
  },
  {
    id: "dictionary",
    label: "Dictionary",
    icon: BookA,
    description: "Words, corrections, and snippets.",
  },
  {
    id: "polish",
    label: "Polish",
    icon: Sparkles,
    description: "Configure AI cleanup and writing tools.",
  },
];

export const footerNavScreens: ScreenDefinition[] = [
  {
    id: "settings",
    label: "Settings",
    icon: Settings2,
    description: "Appearance, updates, privacy, and advanced options.",
  },
  {
    id: "help",
    label: "Help & feedback",
    icon: LifeBuoy,
    description: "Report a problem and find help.",
  },
];

export const licenseScreen: ScreenDefinition = {
  id: "license",
  label: "License",
  icon: BadgeCheck,
  description: "Trial status and license activation.",
};

export const screenAliases: Record<
  LegacyScreenId,
  { screen: MainScreenId; pane?: SettingsPane; openUpload?: boolean }
> = {
  overview: { screen: "home" },
  recordings: { screen: "history" },
  audio: { screen: "history", openUpload: true },
  models: { screen: "transcription" },
  formatting: { screen: "polish" },
  general: { screen: "settings", pane: "general" },
  shortcuts: { screen: "settings", pane: "shortcuts" },
  network: { screen: "settings", pane: "network" },
  agent: { screen: "settings", pane: "agent" },
  advanced: { screen: "settings", pane: "advanced" },
  privacy: { screen: "settings", pane: "general" },
  storage: { screen: "settings", pane: "storage" },
  about: { screen: "settings", pane: "general" },
  "report-problem": { screen: "help" },
};

export function resolveScreen(id: ScreenId): {
  screen: MainScreenId;
  pane?: SettingsPane;
  openUpload?: boolean;
} {
  if (id === "license") return { screen: "settings", pane: "license" };
  return id in screenAliases ? screenAliases[id as LegacyScreenId] : { screen: id as MainScreenId };
}
