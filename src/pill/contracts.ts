import type { PillIndicatorPosition } from '@/types';
export interface DictationContext {
  generation: number;
  app: { name: string; icon_key: string | null };
  polish: { will_run: boolean; style: string | null; key_ok: boolean; keep_words: boolean };
  mic: { name: string; tooltip: string; ok: boolean };
  engine: { short_name: string; kind: string };
  language: { code: string; label: string };
  mode: 'hold' | 'toggle';
  changed_since_last: boolean;
  show_start_card: boolean;
}
export interface PillPointer { inside: boolean; x?: number; y?: number }
export interface PillSettings {
  pill_indicator_mode?: 'never' | 'always' | 'when_recording';
  pill_indicator_position?: PillIndicatorPosition;
  transcription_mode?: 'regular' | 'live_preview';
  streaming_preview_enabled?: boolean;
  hotkey?: string;
  ptt_hotkey?: string;
  recording_mode?: string;
}
export interface RecordingStatePayload {
  state: 'idle' | 'starting' | 'recording' | 'stopping' | 'transcribing' | 'error';
  error: string | null;
}
export interface ToastPayload { id?: number; message: string; duration_ms: number; action?: 'show' | 'hide' }
/** Future slice actions are capabilities, never buttons that invoke missing commands. */
export interface IslandActions {
  start(): Promise<unknown>;
  stop(): Promise<unknown>;
  cancel(): Promise<unknown>;
  dismiss(): Promise<unknown>;
  openPrivacySettings?: () => Promise<unknown>;
  skip?: () => Promise<unknown>;
  undo?: () => Promise<unknown>;
  original?: () => Promise<unknown>;
  retry?: (engine: string) => Promise<unknown>;
  transcribeAnyway?: () => Promise<unknown>;
  quickSetting?: (kind: 'polish' | 'mic' | 'engine' | 'language') => void;
  feedback?: (kind: string) => void;
}
export const emptyContext = (): DictationContext => ({
  generation: 0, app: { name: '', icon_key: null },
  polish: { will_run: false, style: null, key_ok: true, keep_words: true },
  mic: { name: 'Microphone', tooltip: 'Microphone', ok: true },
  engine: { short_name: 'Voice engine', kind: 'local' },
  language: { code: 'auto', label: 'Auto' }, mode: 'toggle',
  changed_since_last: false, show_start_card: false,
});
