import type { QuickOptions } from '@/pill/quick-settings';
import type { DictationBlocked, DictationRecovery, DictationNote, RecordingTooShort, EscapeHint, IslandNotice } from '@/types/island-events';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { isMacOS } from '@/lib/platform';
import { TRANSCRIPTION_STREAM_EVENT, type TranscriptionStreamEvent } from '@/types/streaming';
import type { PasteOutcomePayload } from '@/types/paste-outcome';
import type { PillAudioLevel } from '@/types';
import type { PillGeometry } from '@/pill-geometry';
import type { DictationContext, PillPointer, PillSettings, RecordingStatePayload } from '@/pill/contracts';
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
  let destroyed = false, settingsRevision = 0, recordingEventReceived = false, geometryReceived = false;
  const subscriptions: Promise<void>[] = [];
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
  let feedbackQueued: boolean | undefined, feedbackSending = false;
  const feedbackVisible = (visible: boolean): Promise<void> => {
    feedbackQueued=visible;
    if(feedbackSending)return Promise.resolve();
    feedbackSending=true;
    return (async()=>{while(feedbackQueued!==undefined){const latest=feedbackQueued;feedbackQueued=undefined;try{await call('pill_feedback_visible',{visible:latest});}catch{ /* A later state change can retry. */ }}feedbackSending=false;})();
  };
  let streamUnlisten: (() => void) | undefined, streamPending = false, streamEnabled = false;
  const island = createIsland(root, {
    mac: isMacOS,
    actions: {
      start: () => call('start_recording', { source: 'pointer' }), stop: () => call('stop_recording'),
      cancel: () => call('cancel_recording'), dismiss: () => call('hide_pill_widget'),
      original: () => call('copy_last_original'),
      card: c => call(c.command, c.command === 'island_action' ? {action:c.action} : c.command === 'retry_kept_dictation' ? {id:c.id,engine:c.engine} : {id:c.id}),
      feedbackVisible,
      quickSet: async (kind, id) => { await call('island_quick_set', { kind, id }); await readQuick(); },
      openSettings: () => call('island_action', { action: 'open_settings' }),
      openPrivacySettings: () => call('open_accessibility_settings'),
    },
    icon: key => call<string | null>('pill_app_icon', { iconKey: key }),
    hitRegions: sendRegions,
    setTimeout: ((handler: () => void, delay: number) => (deps.setTimeout ?? window.setTimeout.bind(window))(handler, delay)) as typeof window.setTimeout,
    clearTimeout: handle => (deps.clearTimeout ?? window.clearTimeout.bind(window))(handle),
  });
  const subscribe = <T,>(event: string, handler: (payload: T) => void) => {
    subscriptions.push(on<T>(event, ({ payload }) => { if (!destroyed) handler(payload); }).then(stop => {
      if (destroyed) stop(); else unlisteners.push(stop);
    }).catch(() => {}));
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
  let quickRevision = 0;
  const readQuick = async () => {
    const revision = ++quickRevision;
    try { const options = await call<QuickOptions>('island_quick_options'); if (!destroyed && revision === quickRevision) island.quickOptions(options); } catch { /* Change events retry without logging IDs. */ }
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
      if (!geometryReceived) island.geometry({ anchor, anchorX: anchor.endsWith('-left') ? 11 : anchor.endsWith('-right') ? 451 : 231, anchorY: anchor.startsWith('top-') ? 17 : 425 });
      const geometry = await call<PillGeometry>('pill_get_geometry');
      if (!destroyed && revision === settingsRevision && geometry?.anchor) { geometryReceived = true; island.geometry(geometry); }
    } catch { /* Subsequent settings/geometry events recover without sensitive logs. */ }
  };
  const recordingState = (payload: RecordingStatePayload) => {
    if (payload.state === 'starting' || payload.state === 'recording') island.start();
    else if (payload.state === 'stopping' || payload.state === 'transcribing') {
      if (island.machine.state !== 'polishing') island.work('transcribing');
    } else if (!island.machine.terminal) {
      if (payload.state === 'error' && !island.feedbackActive) island.flash('error', 'Recording failed');
      else if (!['error', 'too_short', 'nospeech'].includes(island.machine.state)) island.rest();
    }
  };
  subscribe<PillGeometry>('pill-geometry', value => { geometryReceived = true; island.geometry(value); });
  subscribe<PillPointer>('pill-pointer', value => island.pointer(value));
  subscribe<DictationContext>('dictation-context', value => island.context(value));
  subscribe<PillAudioLevel>('audio-level', value => island.level(value));
  subscribe<RecordingStatePayload>('recording-state-changed', value => { recordingEventReceived = true; recordingState(value); });
  subscribe('recording-started', () => { recordingEventReceived = true; island.start(); });
  subscribe('transcription-started', () => { recordingEventReceived = true; island.work('transcribing'); });
  subscribe('enhancing-started', () => { recordingEventReceived = true; island.work('polishing'); });
  for (const name of ['enhancing-completed', 'enhancing-failed']) subscribe(name, () => { if (island.machine.state === 'polishing') island.work('transcribing'); });
  subscribe<RecordingTooShort>('recording-too-short', value => island.tooShort(value));
  subscribe<DictationBlocked>('dictation-blocked', value => island.blocked(value));
  subscribe<DictationRecovery>('dictation-recovery', value => island.recovery(value));
  subscribe<DictationNote>('dictation-note', value => island.note(value));
  subscribe<EscapeHint>('escape-hint', value => { if(island.machine.observeGeneration(value.generation)) island.escapeHint(); });
  subscribe<IslandNotice>('island-notice', value => island.notice(value));
  subscribe<{id:string}>('kept-dictation-expired', value => island.expired(value.id));
  subscribe<{kind: IslandNotice['kind']}>('island-notice-clear', value => island.clearNotice(value.kind));
  subscribe<PasteOutcomePayload>('paste-outcome', value => island.outcome(value));
  subscribe('settings-changed', () => { void readSettings(); void readQuick(); });
  for (const event of ['model-changed', 'audio-device-changed', 'polish-options-changed', 'ai-enabled-changed', 'sharing-status-changed', 'shortcut-settings-changed']) subscribe(event, () => { void readQuick(); });
  subscribe('island-escape', () => island.closeQuick());
  void Promise.resolve().then(async () => {
    await Promise.all(subscriptions);
    await call('pill_feedback_ready').catch(() => {});
    const statePromise = call<RecordingStatePayload>('get_current_recording_state');
    void readSettings();
    void readQuick();
    try {
      const state = await statePromise;
      // Hydration cannot overwrite an event received while the command was in flight.
      if (!destroyed && !recordingEventReceived && state?.state) recordingState(state);
    } catch { /* Real events remain authoritative. */ }
  });
  return { destroy() { destroyed = true; settingsRevision++; quickRevision++; streamUnlisten?.(); unlisteners.forEach(stop => stop()); island.destroy(); } };
}
const root = document.getElementById('root');
if (root) createRecordingPill(root);
