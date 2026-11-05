/** Pill-only recovery contracts. No transcript, audio, path, title or key. */
export type RecoveryKind =
  | "cloud_failed" | "network_offline" | "model_missing" | "remote_offline"
  | "unknown" | "no_speech" | "integrity" | "mic_dropped_empty";
export type IslandAction =
  | "open_settings" | "recheck_license" | "open_license" | "open_models" | "open_cloud_keys"
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
  alt_engine_id: string | null;
  expires_in_ms: number;
  /** False while the retained WAV is still finalizing. Older producers omit it. */
  retry_ready?: boolean;
  retry_failed?: boolean;
}
export interface DictationBlocked { generation: number; kind: BlockedKind; action: IslandAction }
export type PolishSkippedReason = "timeout" | "rate_limited" | "network" | "guard" | "error";
export type DictationNote = { generation: number } & (
  | { kind: "mic_dropped"; captured_ms: number }
  | { kind: "mic_silent" | "mic_silent_live" | "translate_failed" | "storage_failed" }
  | { kind: "model_fallback"; engine_short: string; alt_engine_short: string }
  | { kind: "gpu_fallback"; engine_short: string }
  | { kind: "polish_skipped"; reason: PolishSkippedReason }
);
export interface DictationNoteWithdraw { generation: number; kind: "mic_silent_live" }
export interface RecordingTooShort { generation: number; mode: "hold" | "toggle" }
export interface EscapeHint { generation: number; phase: "recording" | "transcribing" }
export interface IslandProblemEvents {
  "dictation-recovery": DictationRecovery;
  "dictation-blocked": DictationBlocked;
  "dictation-note": DictationNote;
  "dictation-note-withdraw": DictationNoteWithdraw;
  "recording-too-short": RecordingTooShort;
  "escape-hint": EscapeHint;
}
/** Main-window navigation handoff for P3; emitted only after a button action. */
export type IslandNavigateAction = "open_models" | "open_cloud_keys" | "choose_mic";
export interface IslandNotice {
  kind: 'finishing' | 'polish_on' | 'polish_off' | 'polish_setup' | 'shortcuts_retired'
    | 'long_silence' | 'silence_stopped' | 'silence_discarded' | 'recording_failed'
    | 'copy_failed' | 'transcription_failed' | 'history_retry'
    | 'shortcut_throttled' | 'no_speech';
}
