import { invoke } from "@tauri-apps/api/core";

export interface TrayStatus {
  available: boolean;
  attempts: number;
  lastError: string | null;
}

export function getTrayStatus(): Promise<TrayStatus> {
  return invoke<TrayStatus>("get_tray_status");
}

export function retryTrayCreation(): Promise<TrayStatus> {
  return invoke<TrayStatus>("retry_tray_creation");
}
