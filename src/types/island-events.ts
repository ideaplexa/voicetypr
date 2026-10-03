/** Pill-only recovery contracts. No transcript, audio, path, title or key. */
export type RecoveryKind =
  | "cloud_failed" | "network_offline" | "model_missing" | "remote_offline"
  | "no_speech" | "integrity" | "mic_dropped_empty";
export type IslandAction =
  | "recheck_license" | "open_license" | "open_models" | "open_cloud_keys"
  | "open_storage" | "open_mic_settings" | "choose_mic" | "open_accessibility";
export type BlockedKind =
  | "license_check_failed" | "license_verify_required" | "trial_ended" | "no_engine"
  | "cloud_key_missing" | "cloud_key_rejected" | "soniox_storage_full"
  | "mic_permission_denied" | "mic_missing" | "mic_busy" | "accessibility_off" | "starting_up";
export interface DictationRecovery {
  generation: number;
  id: string;
  kind: RecoveryKind;
  engine_short: string;
  alt_engine_short: string | null;
  expires_in_ms: number;
}
export interface DictationBlocked { generation: number; kind: BlockedKind; action: IslandAction }
export type PolishSkippedReason = "timeout" | "rate_limited" | "network" | "guard" | "error";
export type DictationNote = { generation: number } & (
  | { kind: "mic_dropped"; captured_ms: number }
  | { kind: "mic_silent" | "translate_failed" }
  | { kind: "model_fallback"; engine_short: string; alt_engine_short: string }
  | { kind: "gpu_fallback"; engine_short: string }
  | { kind: "polish_skipped"; reason: PolishSkippedReason }
);
export interface RecordingTooShort { generation: number; mode: "hold" | "toggle" }
export interface EscapeHint { generation: number; phase: "recording" | "transcribing" }
export interface IslandProblemEvents {
  "dictation-recovery": DictationRecovery;
  "dictation-blocked": DictationBlocked;
  "dictation-note": DictationNote;
  "recording-too-short": RecordingTooShort;
  "escape-hint": EscapeHint;
}
/** Main-window navigation handoff for P3; emitted only after a button action. */
export type IslandNavigateAction = "open_models" | "open_cloud_keys" | "choose_mic";
