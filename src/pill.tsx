import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { isMacOS } from '@/lib/platform';
import { TRANSCRIPTION_STREAM_EVENT, type TranscriptionStreamEvent } from '@/types/streaming';
import type { PasteOutcomePayload } from '@/types/paste-outcome';
import type { PillAudioLevel } from '@/types';
import type { PillGeometry } from '@/pill-geometry';
import type { DictationContext, PillPointer, PillSettings, RecordingStatePayload, ToastPayload } from '@/pill/contracts';
import { createIsland } from '@/pill/renderer';
import '@fontsource-variable/geist';
import '@fontsource-variable/geist-mono';
import './pill.css';

type Invoke = <T = unknown>(command: string, args?: Record<string, unknown>) => Promise<T>;
type Listen = <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>;
export interface RecordingPillController { destroy(): void }
interface RecordingPillDeps {
  invoke?: Invoke;
  listen?: Listen;
  setTimeout?: (handler: () => void, delay: number) => number | ReturnType<typeof setTimeout>;
  clearTimeout?: (handle: number | ReturnType<typeof setTimeout>) => void;
}
/** Only event transport and startup hydration live here. The pill has no React runtime. */
export function createRecordingPill(root: HTMLElement, deps: RecordingPillDeps = {}): RecordingPillController {
  const call = deps.invoke ?? invoke, on = deps.listen ?? listen;
  let destroyed = false, settingsRevision = 0, recordingEventReceived = false;
  const unlisteners: Array<() => void> = [];
  type Regions = Array<{ x: number; y: number; width: number; height: number }>;
  let queuedRegions: Regions | undefined, sendingRegions = false;
  const sendRegions = (rects: Regions) => {
    queuedRegions = rects;
    if (sendingRegions) return;
    sendingRegions = true;
    void (async () => {
      while (queuedRegions) {
        const latest = queuedRegions; queuedRegions = undefined;
        try { await call('pill_set_hit_regions', { rects: latest }); } catch { /* No logging of geometry. */ }
      }
      sendingRegions = false;
    })();
  };
  let streamUnlisten: (() => void) | undefined, streamPending = false, streamEnabled = false;
  const island = createIsland(root, {
    mac: isMacOS,
    actions: {
      start: () => call('start_recording'), stop: () => call('stop_recording'),
      cancel: () => call('cancel_recording'), dismiss: () => call('hide_pill_widget'),
      openPrivacySettings: () => call('open_accessibility_settings'),
    },
    icon: key => call<string | null>('pill_app_icon', { iconKey: key }),
    hitRegions: sendRegions,
    setTimeout: ((handler: () => void, delay: number) => (deps.setTimeout ?? window.setTimeout.bind(window))(handler, delay)) as typeof window.setTimeout,
    clearTimeout: handle => (deps.clearTimeout ?? window.clearTimeout.bind(window))(handle),
  });
  const subscribe = <T,>(event: string, handler: (payload: T) => void) => {
    void on<T>(event, ({ payload }) => { if (!destroyed) handler(payload); }).then(stop => {
      if (destroyed) stop(); else unlisteners.push(stop);
    }).catch(() => {});
  };
  const syncStream = () => {
    if (!streamEnabled) { streamUnlisten?.(); streamUnlisten = undefined; return; }
    if (streamPending || streamUnlisten) return;
    streamPending = true;
    void on<TranscriptionStreamEvent>(TRANSCRIPTION_STREAM_EVENT, ({ payload }) => { if (!destroyed && streamEnabled) island.stream(payload); }).then(stop => {
      streamPending = false;
      if (destroyed || !streamEnabled) stop(); else streamUnlisten = stop;
    }).catch(() => { streamPending = false; });
  };
  const readSettings = async () => {
    const revision = ++settingsRevision;
    try {
      const settings = await call<PillSettings>('get_settings');
      if (destroyed || revision !== settingsRevision) return;
      streamEnabled = settings.transcription_mode === 'live_preview' || settings.streaming_preview_enabled === true;
      island.settings(settings); syncStream();
      // Geometry events are authoritative; this fallback also supports old settings fixtures.
      const anchor = isMacOS ? settings.pill_indicator_position ?? 'bottom-center' : 'bottom-right';
      island.geometry({ anchor, anchorX: anchor.endsWith('-left') ? 0 : anchor.endsWith('-right') ? 440 : 220, anchorY: anchor.startsWith('top-') ? 6 : 414 });
      const geometry = await call<PillGeometry>('pill_get_geometry');
      if (!destroyed && revision === settingsRevision && geometry?.anchor) island.geometry(geometry);
    } catch { /* Subsequent settings/geometry events recover without sensitive logs. */ }
  };
  const recordingState = (payload: RecordingStatePayload) => {
    if (payload.state === 'starting' || payload.state === 'recording') island.start();
    else if (payload.state === 'stopping' || payload.state === 'transcribing') {
      if (island.machine.state !== 'polishing') island.work('transcribing');
    } else if (!island.machine.terminal) {
      if (payload.state === 'error') island.flash('error', payload.error || 'Recording failed');
      else if (!['error', 'too_short', 'nospeech'].includes(island.machine.state)) island.rest();
    }
  };
  subscribe<PillGeometry>('pill-geometry', value => island.geometry(value));
  subscribe<PillPointer>('pill-pointer', value => island.pointer(value));
  subscribe<DictationContext>('dictation-context', value => island.context(value));
  subscribe<PillAudioLevel>('audio-level', value => island.level(value));
  subscribe<RecordingStatePayload>('recording-state-changed', value => { recordingEventReceived = true; recordingState(value); });
  subscribe('recording-started', () => { recordingEventReceived = true; island.start(); });
  subscribe('transcription-started', () => { recordingEventReceived = true; island.work('transcribing'); });
  subscribe('enhancing-started', () => { recordingEventReceived = true; island.work('polishing'); });
  for (const name of ['enhancing-completed', 'enhancing-failed']) subscribe(name, () => { if (island.machine.state === 'polishing') island.work('transcribing'); });
  subscribe('recording-too-short', () => island.flash('too_short'));
  subscribe<PasteOutcomePayload>('paste-outcome', value => island.outcome(value));
  // Existing backend Esc and speech-gate events share the toast transport.
  subscribe<ToastPayload>('toast', value => {
    if (value.action === 'hide') return;
    if (/^Press ESC again to cancel$/i.test(value.message)) island.escapeHint();
    else if (/^(?:Recording )?too short/i.test(value.message)) island.flash('too_short');
    else if (/^No speech detected/i.test(value.message)) island.flash('nospeech');
  });
  subscribe('settings-changed', () => { void readSettings(); });
  void Promise.resolve().then(async () => {
    const statePromise = call<RecordingStatePayload>('get_current_recording_state');
    void readSettings();
    try {
      const state = await statePromise;
      // Hydration cannot overwrite an event received while the command was in flight.
      if (!destroyed && !recordingEventReceived && state?.state) recordingState(state);
    } catch { /* Real events remain authoritative. */ }
  });
  return { destroy() { destroyed = true; settingsRevision++; streamUnlisten?.(); unlisteners.forEach(stop => stop()); island.destroy(); } };
}
const root = document.getElementById('root');
if (root) createRecordingPill(root);
