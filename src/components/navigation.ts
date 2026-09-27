import type { LucideIcon } from "lucide-react";
import {
  Activity,
  Bug,
  Clock,
  Cpu,
  FileAudio,
  Home,
  Keyboard,
  Key,
  Mic,
  Settings2,
  Share2,
  Sparkles,
  Terminal,
} from "lucide-react";

export type ScreenId =
  | "overview"
  | "recordings"
  | "audio"
  | "recording"
  | "general"
  | "shortcuts"
  | "models"
  | "network"
  | "formatting"
  | "license"
  | "agent"
  | "advanced"
  | "report-problem";

export interface ScreenDefinition {
  id: ScreenId;
  label: string;
  icon: LucideIcon;
  description: string;
}

export const primaryScreens: ScreenDefinition[] = [
  {
    id: "overview",
    label: "Overview",
    icon: Home,
    description: "Readiness, recent activity, and quick next steps.",
  },
  {
    id: "recordings",
    label: "History",
    icon: Clock,
    description: "Past transcriptions and retry actions.",
  },
  {
    id: "audio",
    label: "Upload",
    icon: FileAudio,
    description: "Transcribe existing audio files.",
  },
  {
    id: "recording",
    label: "Recording",
    icon: Mic,
    description: "Microphone, primary shortcut, feedback, and recording behavior.",
  },
  {
    id: "models",
    label: "Sources",
    icon: Cpu,
    description: "Choose local, cloud, or remote transcription.",
  },
  {
    id: "network",
    label: "Network sharing",
    icon: Share2,
    description: "Let other devices use this device's transcription engine.",
  },
  {
    id: "formatting",
    label: "Polish",
    icon: Sparkles,
    description: "Configure AI cleanup, dictionary, corrections, snippets, and modes.",
  },
  {
    id: "general",
    label: "General",
    icon: Settings2,
    description: "Appearance, startup, privacy, and update options.",
  },
  {
    id: "shortcuts",
    label: "Shortcuts",
    icon: Keyboard,
    description: "Additional shortcuts for history, Polish, and app actions.",
  },
  {
    id: "license",
    label: "License",
    icon: Key,
    description: "Trial status, license activation, and purchase access.",
  },
  {
    id: "agent",
    label: "CLI",
    icon: Terminal,
    description: "Drive Voicetypr from scripts and agents via the CLI and local API.",
  },
];

export const secondaryScreens: ScreenDefinition[] = [
  {
    id: "advanced",
    label: "Quick help",
    icon: Activity,
    description: "Permissions, troubleshooting, reset tools, and app diagnostics.",
  },
  {
    id: "report-problem",
    label: "Report a problem",
    icon: Bug,
    description: "Send an issue with diagnostic logs.",
  },
];

export const screens = [...primaryScreens, ...secondaryScreens] as const;

const screenById = (id: ScreenId): ScreenDefinition =>
  screens.find((screen) => screen.id === id) as ScreenDefinition;

// Main navigation stays focused on everyday workflows.
export const navScreens: ScreenDefinition[] = [
  screenById("overview"),
  screenById("general"),
  screenById("recordings"),
  screenById("audio"),
  screenById("models"),
  screenById("recording"),
  screenById("formatting"),
  screenById("shortcuts"),
  screenById("network"),
  screenById("agent"),
  screenById("license"),
];

// Support and troubleshooting remain fixed at the bottom.
export const footerNavScreens: ScreenDefinition[] = [
  screenById("advanced"),
  screenById("report-problem"),
];
