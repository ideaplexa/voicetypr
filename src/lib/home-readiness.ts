import { isCloudEngine } from "@/lib/cloudProviders";
import { getModelDisplayName } from "@/lib/model-display";
import { isMacOS } from "@/lib/platform";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import type { SourceFilter } from "@/components/sections/models/types";

export interface HomeStatus {
  label: string;
  ready: boolean;
  screen?: ScreenId;
  pane?: SettingsPane;
  source?: SourceFilter;
}

export function homeStatus({
  model,
  engine,
  modelAvailable,
  canRecord,
  remoteSelected,
  remoteAvailable,
  remoteLabel,
  downloadProgress,
  licenseValid,
  licenseStatus,
  hasMicrophonePermission,
}: {
  model: string;
  engine: string;
  modelAvailable: boolean | null;
  canRecord: boolean;
  remoteSelected: boolean;
  remoteAvailable: boolean | null;
  remoteLabel: string | null;
  downloadProgress: number | null;
  licenseValid: boolean;
  licenseStatus: string | null;
  hasMicrophonePermission: boolean | null;
}): HomeStatus {
  const modelLabel = getModelDisplayName(model) ?? model;
  if (!licenseValid && licenseStatus)
    return { label: "License needs attention", ready: false, screen: "license" };
  if (hasMicrophonePermission === false)
    return isMacOS
      ? { label: "Microphone access needed", ready: false, screen: "settings", pane: "advanced" }
      : { label: "Microphone access needed", ready: false, screen: "recording" };
  if (remoteSelected)
    return canRecord
      ? { label: `Ready · ${remoteLabel ?? "Remote Voicetypr"}`, ready: true }
      : {
          label: remoteAvailable === false ? "Remote source unavailable" : "Needs attention",
          ready: false,
          screen: "transcription",
          source: "remote",
        };
  if (!model) return { label: "No model yet", ready: false, screen: "transcription" };
  if (isCloudEngine(engine))
    return modelAvailable === false
      ? { label: "Needs an API key", ready: false, screen: "transcription", source: "cloud" }
      : canRecord
        ? { label: `Ready · ${modelLabel.replace(/ \(Cloud\)$/, "")} (cloud)`, ready: true }
        : { label: "Needs attention", ready: false, screen: "transcription" };
  if (downloadProgress !== null && modelAvailable !== true)
    return {
      label: `Downloading ${modelLabel} · ${Math.round(downloadProgress)}%`,
      ready: false,
      screen: "transcription",
    };
  if (modelAvailable === false)
    return { label: "No model yet", ready: false, screen: "transcription" };
  if (!canRecord) return { label: "Needs attention", ready: false, screen: "transcription" };
  return { label: `Ready · ${modelLabel} runs on this ${isMacOS ? "Mac" : "PC"}`, ready: true };
}
