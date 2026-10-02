export type SpeechModelEngine =
  | "whisper"
  | "parakeet"
  | "soniox"
  | "openai"
  | "groq"
  | "deepgram"
  | "cohere";
export type ModelKind = "local" | "cloud";

/** A downloaded shareable model exposed by a remote Voicetypr host. */
export interface RemoteShareableModel {
  id: string;
  display_name: string;
  engine: Extract<SpeechModelEngine, "whisper" | "parakeet">;
  recommended?: boolean | null;
  speed_score?: number | null;
  accuracy_score?: number | null;
}

/** Remote-control-lite snapshot for changing the host's shared transcription model. */
export interface RemoteModelControlSnapshot {
  current: RemoteShareableModel;
  available: RemoteShareableModel[];
}

interface BaseModelInfo {
  name: string;
  display_name: string;
  engine: SpeechModelEngine;
  kind: ModelKind;
  recommended: boolean;
  downloaded: boolean;
  requires_setup: boolean;
  speed_score?: number; // Optional for cloud providers
  accuracy_score?: number; // Optional for cloud providers
  size?: number;
  url?: string;
  sha256?: string;
  supported_languages?: string[] | null;
}

export interface LocalModelInfo extends BaseModelInfo {
  kind: "local";
  size: number;
  url: string;
  sha256: string;
  speed_score: number;
  accuracy_score: number;
}

export interface CloudSttModel {
  id: string;
  display_name: string;
}

export interface CloudModelInfo extends BaseModelInfo {
  kind: "cloud";
  /** Backend-sourced transcription model id used by this cloud provider. */
  underlying_model?: string | null;
  /** Curated, friendly-labeled API models for this provider. */
  available_models?: CloudSttModel[] | null;
}

export type ModelInfo = LocalModelInfo | CloudModelInfo;

export const isCloudModel = (model: ModelInfo): model is CloudModelInfo => model.kind === "cloud";

export const isLocalModel = (model: ModelInfo): model is LocalModelInfo => model.kind === "local";

export type RecordingMode = "toggle" | "push_to_talk";
export type PillIndicatorMode = "never" | "always" | "when_recording";
export type PillIndicatorStyle = "compact" | "full";
export type PillIndicatorPosition =
  | "top-left"
  | "top-center"
  | "top-right"
  | "bottom-left"
  | "bottom-center"
  | "bottom-right";
export type TranscriptionAcceleration = "auto" | "gpu" | "cpu";
export type UpdateChannel = "stable" | "beta";
export type TranscriptionMode = "regular" | "live_preview";

export interface EngineStreamCapabilities {
  supports_streaming: boolean;
  supports_committed_prefix: boolean;
  supports_tentative_tail: boolean;
  supports_endpointing: boolean;
  final_only: boolean;
}

export interface ActiveStreamCapabilities {
  active_engine: string;
  capabilities: EngineStreamCapabilities;
  eou_model_downloaded: boolean;
  eou_model_path?: string | null;
  eou_chunk_ms: number;
  transcription_mode: TranscriptionMode;
}

export interface AppSettings {
  polish_keep_words?: boolean;
  hotkey: string;
  current_model: string;
  speech_language: string;
  transcription_task?: "transcribe" | "translate_to_english";
  final_text_language?: string;
  theme: string;
  transcription_cleanup_days?: number | null;
  launch_at_startup?: boolean;
  onboarding_completed?: boolean;
  check_updates_automatically?: boolean;
  selected_microphone?: string | null;
  // Push-to-talk support
  recording_mode?: RecordingMode;
  use_different_ptt_key?: boolean;
  ptt_hotkey?: string;
  current_model_engine?: SpeechModelEngine;
  auto_paste_transcription?: boolean;
  keep_transcription_in_clipboard?: boolean;
  // Audio feedback
  play_sound_on_recording?: boolean;
  play_sound_on_transcription_complete?: boolean;
  play_sound_on_paste_success?: boolean;
  settings_mode?: string;
  // Pill indicator visibility mode
  pill_indicator_mode?: PillIndicatorMode;
  // Pill indicator detail level
  pill_indicator_style?: PillIndicatorStyle;
  // Pill indicator screen position
  pill_indicator_position?: PillIndicatorPosition;
  // Pill indicator offset from screen edge in pixels (10-50)
  pill_indicator_offset?: number;
  pill_position?: [number, number] | null;
  // Pause system media during recording
  pause_media_during_recording?: boolean;
  // Network sharing settings
  sharing_port?: number;
  sharing_password?: string;
  // Recording persistence settings
  save_recordings?: boolean;
  recording_retention_days?: number | null; // null = keep forever
  // Transcription acceleration (Windows only; stored-but-ignored on other platforms)
  transcription_acceleration?: TranscriptionAcceleration;
  // Direct-install update feed (Store/MSIX ignores this setting)
  update_channel?: UpdateChannel;
  whisper_speed_mode?: boolean;
  transcription_mode?: TranscriptionMode;
}

/** Writing-step outcome attached to a history row (mirrors the backend `writing` metadata blob). */
export interface TranscriptionWritingMeta {
  /** True when a required AI translation failed and the row holds the raw, untranslated transcript. */
  translation_failed?: boolean;
  /** The output language the failed translation targeted. */
  target_language?: string;
  /** Transcription source: 'desktop_recording' | 'audio_file' | 'audio_bytes' | 'remote_server' */
  source?: string;
  /** Engine/provider used (e.g. 'whisper', 'parakeet', 'soniox'). */
  engine?: string;
  /** Audio length in milliseconds. */
  audio_duration_ms?: number;
  /** Processing time in milliseconds. */
  processing_duration_ms?: number;
  /** True when speaker-diarization word segments are present. */
  diarized?: boolean;
  /** AI writing mode applied (desktop only). */
  mode?: string;
  /** True when an AI enhancement was applied (desktop only). */
  ai_applied?: boolean;
  /** Pre-AI raw transcript saved when AI formatting changed the text (desktop only). Never logged; local history only. */
  original_text?: string;
  /** App active when desktop recording started. Captured locally whether Polish is on or off. */
  context_hint?: { app_name?: string; process_path?: string; category?: string };
  ai_provider?: string;
  ai_model?: string;
}

export interface TranscriptionHistory {
  id: string;
  text: string;
  timestamp: Date;
  model: string;
  recording_file?: string; // Filename of the saved recording (not full path)
  source_recording_id?: string; // For re-transcriptions, references original transcription
  status?: "completed" | "in_progress" | "failed";
  writing?: TranscriptionWritingMeta;
}

export interface LicenseStatus {
  status: "licensed" | "trial" | "expired" | "none";
  trial_days_left?: number;
  license_type?: string;
  license_key?: string;
  expires_at?: string;
  verification_state?: "verified" | "offline_grace" | "needs_revalidation";
  verification_expires_at?: string;
}
