import type { AppSettings } from "@/types";

export function recordingAudioValue(settings: AppSettings): string {
  if (!settings.save_recordings) return settings.save_failed_recordings === false ? "off" : "failed";
  return settings.recording_retention_days === null ? "forever" : String(settings.recording_retention_days ?? 30);
}

export function recordingAudioUpdate(value: string): Partial<AppSettings> {
  if (value === "off" || value === "failed") {
    return { save_recordings: false, save_failed_recordings: value === "failed" };
  }
  return { save_recordings: true, recording_retention_days: value === "forever" ? null : Number(value) };
}

export function recordingAudioDescription(value: string): string {
  if (value === "off") return "Failed takes can be retried for 10 minutes, until you quit. No speech is kept for 30 seconds.";
  if (value === "failed") return "Only failed takes are saved, up to 20 for 7 days. Retry from Home, even after restarting. A successful retry or Delete audio removes them.";
  return "Each dictation's audio is saved for your chosen period, so you can re-transcribe it from History.";
}
