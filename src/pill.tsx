import type { QuickOptions } from '@/pill/quick-settings';
import type { DictationBlocked, DictationRecovery, DictationNote, RecordingTooShort, EscapeHint, IslandNotice } from '@/types/island-events';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { isMacOS } from '@/lib/platform';
import { TRANSCRIPTION_STREAM_EVENT, type TranscriptionStreamEvent } from '@/types/streaming';
import type { PasteOutcomePayload } from '@/types/paste-outcome';
import type { PillAudioLevel } from '@/types';
import type { PillGeometry } from '@/pill-geometry';
import { islandShowsLiveWords, type DictationContext, type PillPointer, type PillSettings, type RecordingStatePayload } from '@/pill/contracts';
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
  let feedbackEpoch: number | undefined;
  const pendingFeedback: Array<{ epoch: number; run: () => void }> = [];
  let feedbackQueued: boolean | undefined, feedbackSending = false;
  const feedbackVisible = (visible: boolean): Promise<void> => {
    if (destroyed) return Promise.resolve();
    feedbackQueued=visible;
    if(feedbackSending || feedbackEpoch === undefined)return Promise.resolve();
    feedbackSending=true;
    return (async()=>{while(!destroyed && feedbackQueued!==undefined){const latest=feedbackQueued;feedbackQueued=undefined;try{await call('pill_feedback_visible',{visible:latest,epoch:feedbackEpoch});}catch{ /* A later state change can retry. */ }}feedbackSending=false;})();
  };
  let streamUnlisten: (() => void) | undefined, streamPending = false, streamEnabled = false;
  const action = (name: 'dictate' | 'stop' | 'cancel' | 'copy' | 'retry' | 'discard' | 'open_accessibility') => {
    void call('record_observability_event', { name: 'island_action', properties: { action: name, source: 'island' } }).catch(() => {});
  };
  const island = createIsland(root, {
    mac: isMacOS,
    actions: {
      start: () => { action('dictate'); return call('start_recording', { source: 'pointer' }); }, stop: () => { action('stop'); return call('stop_recording'); },
      cancel: () => { action('cancel'); return call('cancel_recording'); }, dismiss: () => call('hide_pill_widget'),
      original: () => { action('copy'); return call('copy_last_original'); },
      card: c => { if(c.command !== 'island_action') action(c.command === 'discard_kept_dictation' ? 'discard' : 'retry'); return call(c.command, c.command === 'island_action' ? {action:c.action} : c.command === 'retry_kept_dictation' ? {id:c.id,engine:c.engine} : {id:c.id}); },
      feedbackVisible,
      quickSet: async (kind, id) => { await call('island_quick_set', { kind, id }); await readQuick(); },
      openSettings: () => call('island_action', { action: 'open_settings' }),
      openPrivacySettings: () => { action('open_accessibility'); return call('open_accessibility_settings'); },
    },
    icon: key => call<string | null>('pill_app_icon', { iconKey: key }),
    hitRegions: sendRegions,
    setTimeout: ((handler: () => void, delay: number) => (deps.setTimeout ?? window.setTimeout.bind(window))(handler, delay)) as typeof window.setTimeout,
    clearTimeout: handle => (deps.clearTimeout ?? window.clearTimeout.bind(window))(handle),
  });
  // Debug-only measurement: one startup query, then nothing unless the backend opts in.
  void Promise.resolve().then(() => call<boolean>('perf_enabled')).then(enabled => {
    if (destroyed || enabled !== true) return;
    island.attachPerf(true, {
      visible: unixMs => call('perf_island_visible', { unixMs }),
      summary: summary => call('perf_pill_summary', { summary }),
    });
  }).catch(() => {});
  const subscribe = <T,>(event: string, handler: (payload: T) => void) => {
    subscriptions.push(on<T>(event, ({ payload }) => { if (!destroyed) handler(payload); }).then(stop => {
      if (destroyed) stop(); else unlisteners.push(stop);
    }).catch(() => {}));
  };
  const subscribeFeedback = <T extends object>(event: string, handler: (payload: T) => void) => {
    subscribe<T & { renderer_epoch?: number }>(event, value => {
      // Untagged local/test events keep their existing immediate semantics.
      // Native delivery is tagged even across the close/check/emit race.
      if (value.renderer_epoch === undefined) { handler(value); return; }
      if (feedbackEpoch === undefined) pendingFeedback.push({ epoch: value.renderer_epoch, run: () => handler(value) });
      else if (value.renderer_epoch === feedbackEpoch) handler(value);
    });
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
      streamEnabled = islandShowsLiveWords(settings);
      island.settings(settings); syncStream();
      // Geometry events are authoritative; this fallback also supports old settings fixtures.
      const anchor = isMacOS ? settings.pill_indicator_position ?? 'bottom-center' : 'bottom-right';
      if (!geometryReceived) island.geometry({ anchor, anchorX: anchor.endsWith('-left') ? 11 : anchor.endsWith('-right') ? 451 : 231, anchorY: anchor.startsWith('top-') ? 17 : 425 });
      const geometry = await call<PillGeometry>('pill_get_geometry');
      if (!destroyed && revision === settingsRevision && geometry?.anchor) { geometryReceived = true; island.geometry(geometry); }
    } catch { /* Subsequent settings/geometry events recover without sensitive logs. */ }
  };
  // True while the window was shown for a Starting that has not reached Recording yet.
  let shownForStart = false;
  const recordingState = (payload: RecordingStatePayload) => {
    // Starting shows the island at once (mic still opening); Recording makes it live.
    if (payload.generation !== undefined && !island.observeRecordingGeneration(payload.generation)) return;
    if (payload.state === 'starting') { shownForStart = true; island.starting(payload.generation); }
    else if (payload.state === 'recording') { shownForStart = false; island.start(payload.generation); }
    else if (payload.state === 'stopping' || payload.state === 'transcribing') {
      if (island.machine.state !== 'polishing') island.work('transcribing');
    } else if (!island.machine.terminal) {
      // A start that failed before Recording shows no flash: the window folds away as it did before the early show.
      if (payload.state === 'error' && !island.feedbackActive && !island.machine.pending) island.flash('error', 'Recording failed');
      else if (!['error', 'too_short', 'nospeech'].includes(island.machine.state)) island.rest();
      // A start that ended before Recording leaves nothing to show: let the backend hide the window it opened early.
      if (payload.state === 'idle' && shownForStart) {
        shownForStart = false;
        if (!island.feedbackActive) void feedbackVisible(false);
      }
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
  subscribeFeedback<RecordingTooShort>('recording-too-short', value => island.tooShort(value));
  subscribeFeedback<DictationBlocked>('dictation-blocked', value => island.blocked(value));
  subscribeFeedback<DictationRecovery>('dictation-recovery', value => island.recovery(value));
  subscribeFeedback<DictationNote>('dictation-note', value => island.note(value));
  subscribeFeedback<{generation:number;kind:'mic_silent_live'}>('dictation-note-withdraw', value => island.withdrawNote(value));
  subscribeFeedback<EscapeHint>('escape-hint', value => { if(island.machine.observeGeneration(value.generation)) island.escapeHint(); });
  subscribeFeedback<IslandNotice>('island-notice', value => island.notice(value));
  subscribeFeedback<{id:string}>('kept-dictation-expired', value => island.expired(value.id));
  subscribeFeedback<{kind: IslandNotice['kind']}>('island-notice-clear', value => island.clearNotice(value.kind));
  subscribeFeedback<PasteOutcomePayload>('paste-outcome', value => island.outcome(value));
  subscribe('settings-changed', () => { void readSettings(); void readQuick(); });
  for (const event of ['model-changed', 'audio-device-changed', 'polish-options-changed', 'ai-enabled-changed', 'sharing-status-changed', 'shortcut-settings-changed']) subscribe(event, () => { void readQuick(); });
  subscribe('island-escape', () => island.closeQuick());
  void Promise.resolve().then(async () => {
    await Promise.all(subscriptions);
    const epoch = await call<number>('pill_feedback_ready').catch(() => 0);
    if (destroyed) return;
    // Bind the drain request before any worker can outlive this renderer.
    const requestedEpoch = typeof epoch === 'number' ? epoch : 0;
    await call('pill_feedback_ready', { epoch: requestedEpoch }).catch(() => {});
    if (destroyed) return;
    feedbackEpoch = requestedEpoch;
    for (const queued of pendingFeedback.splice(0)) if (queued.epoch === feedbackEpoch) queued.run();
    if (feedbackQueued !== undefined) void feedbackVisible(feedbackQueued);
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
