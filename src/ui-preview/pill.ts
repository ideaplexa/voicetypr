import { installPreviewPlatform } from '@/ui-preview/platform';
import type { TranscriptionStreamEvent } from '@/types/streaming';
import { pillIconPng } from '@/ui-preview/pill-icon';
import { emptyContext } from '@/pill/contracts';

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Handler[]>();
const params = new URLSearchParams(location.search);
const state = params.get('state') ?? 'rest';
const windows = params.get('platform') === 'windows';
const position = params.get('position') ?? (windows ? 'bottom-right' : state === 'top-anchor-live' ? 'top-center' : 'bottom-center');
const geometry = { anchor: position, anchorX: position.endsWith('-left') ? 0 : position.endsWith('-right') ? 440 : 220, anchorY: position.startsWith('top-') ? 6 : 414 };
const background = params.get('theme') === 'dark' ? '#141415' : '#e9e9e5';
document.documentElement.style.setProperty('background', background, 'important');
document.body.style.setProperty('background', background, 'important');
installPreviewPlatform(windows ? 'windows' : 'macos');
const { createRecordingPill } = await import('@/pill');
const root = document.getElementById('preview-root');
if (root) {
  createRecordingPill(root, {
    invoke: async <T>(command: string): Promise<T> => {
      const value: unknown = command === 'get_settings' ? {pill_indicator_mode:'always',transcription_mode:'live_preview',pill_indicator_position:position} : command === 'pill_get_geometry' ? geometry : command === 'pill_app_icon' ? (['start','listening','live','top-anchor-live','copied'].includes(state) ? pillIconPng : null) : command === 'get_current_recording_state' ? {state:'idle',error:null} : null;
      return value as T;
    },
    listen: async <T>(event: string, handler: (event: {payload:T}) => void) => {
      const wrapped: Handler = ({payload}) => handler({payload:payload as T});
      handlers.set(event,[...(handlers.get(event) ?? []),wrapped]);
      return () => handlers.set(event,(handlers.get(event) ?? []).filter(entry => entry !== wrapped));
    },
    // Freeze the 800 ms start-card and terminal deadlines for export fixtures.
    setTimeout: () => 0,
    clearTimeout: () => {},
  });
  const emit = (name: string,payload?: unknown) => handlers.get(name)?.forEach(handler => handler({payload}));
  await new Promise<void>(resolve => window.setTimeout(resolve,50));
  const context = emptyContext();
  context.generation=1; context.app={name:'Editor',icon_key:'fixture'};
  context.polish={will_run:!['listening-no-badge','listening-amber'].includes(state),style:state === 'listening-no-badge' ? null : 'message',key_ok:state !== 'listening-amber',keep_words:false};
  context.mic={name:'Shure MV7',tooltip:'Shure MV7 USB Microphone',ok:true};
  context.engine={short_name:'Parakeet',kind:'local'};context.language={code:'en',label:'English'};
  context.show_start_card=state==='start';
  emit('dictation-context',context);
  const active=['start','listening','listening-amber','listening-no-badge','live','preview','esc-hint','top-anchor-live'].includes(state);
  if(active) emit('recording-started');
  if(['live','preview','top-anchor-live'].includes(state)) {
    const partial: TranscriptionStreamEvent={type:'partial',session_id:1,revision:1,committed:"…and I'll send the notes before the call ",tentative:'tomorrow'};
    emit('transcription-stream',partial);
  }
  if(state==='transcribing') emit('transcription-started');
  if(state==='polishing'||state==='formatting') emit('enhancing-started');
  if(state==='error') emit('recording-state-changed',{state:'error',error:'Recording failed'});
  if(state==='too_short') emit('recording-too-short');
  if(state==='nospeech') emit('toast',{message:'No speech detected',duration_ms:1500});
  if(state==='esc-hint') emit('toast',{message:'Press ESC again to cancel',duration_ms:2000});
  if(['pasted','copied','no_permission'].includes(state)) emit('paste-outcome',{outcome:state,words:38});
  // Fixture audio, elapsed only; none of this simulator enters the real pill bundle.
  if(active) for(let i=0;i<10;i++) { emit('audio-level',{generation:1,level:[.1,.4,.8,.5,.2,0,.6,.9,.3,0][i]}); await new Promise<void>(resolve => window.setTimeout(resolve,70)); }
  else await new Promise<void>(resolve => window.setTimeout(resolve,900));
  document.documentElement.dataset.pillPreviewReady='true';
}
