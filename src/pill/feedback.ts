import type { DictationBlocked, DictationNote, DictationRecovery, IslandAction, IslandNotice, RecordingTooShort } from '@/types/island-events';
import type { IslandIcon } from '@/pill/icons';
import { elapsed } from '@/pill/copy';
export type CardCommand = { command: 'island_action'; action: IslandAction } | { command: 'retry_kept_dictation'; id: string; engine: string | null } | { command: 'transcribe_anyway' | 'discard_kept_dictation'; id: string };
export interface CardAction { label: string; call: CardCommand }
export interface FeedbackCard { key: string; title: string; sub: string; tone: 'neutral' | 'amber' | 'red'; duration: number | null; expiresAt?: number; liveGeneration?: number; recoveryGeneration?: number; retryFailed?: boolean; micStatus?: { generation: number; resolves: 'new_take' | 'recording' }; glyph?: IslandIcon; closeLabel?: string; actions: CardAction[] }
const action = (label: string, value: IslandAction): CardAction => ({label, call: {command:'island_action',action:value}});
const card = (key: string, title: string, sub = '', duration: number | null = 3500, tone: FeedbackCard['tone'] = 'neutral', actions: CardAction[] = []): FeedbackCard => ({key,title,sub,duration,tone,actions});
export function blockedCard(e: DictationBlocked, mac: boolean): FeedbackCard {
  const settings = mac ? 'Open System Settings' : 'Open Privacy settings';
  const table = {
    trial_ended: ['Trial ended','Activate Voicetypr to keep dictating','Activate'],
    license_check_failed: ["Couldn't verify your license",'Connect to the internet, then recheck','Recheck'],
    license_verify_required: ["Couldn't verify your license",'Connect to the internet, then recheck','Recheck'],
    no_engine: ['No voice engine set up','Download a model or add a cloud key','Set up'],
    cloud_key_missing: ['Cloud key missing','Add a key to keep dictating','Fix key'],
    cloud_key_rejected: ['Cloud key rejected','Update the key in Settings','Fix key'],
    soniox_storage_full: ['Soniox storage is full','Delete old recordings to keep dictating','Clean up'],
    mic_permission_denied: ['Microphone access is off',mac ? 'Allow Voicetypr in Privacy & Security' : 'Allow Voicetypr in Privacy settings',settings],
    mic_missing: ['No microphone','Plug one in, or choose another','Choose mic'],
    mic_busy: ['Mic in use by another app','Close the call, or choose another mic','Choose mic'],
    accessibility_off: ['Allow Accessibility to paste automatically',`Your words are copied — press ${mac ? '⌘V' : 'Ctrl+V'} for now`,settings],
    starting_up: ['Getting ready…','',''],
  } satisfies Record<DictationBlocked['kind'], [string,string,string]>;
  const [title,sub,label] = table[e.kind];
  return {...card(`blocked:${e.kind}`,title,sub,e.kind === 'starting_up' ? 3200 : null,e.kind === 'mic_busy' ? 'red' : 'amber',label ? [action(label,e.action)] : []),micStatus:e.kind.startsWith('mic_') ? {generation:e.generation,resolves:'recording'} : undefined,glyph:e.kind.startsWith('mic_') ? 'mic-off' : e.kind==='accessibility_off' ? 'clipboard' : 'bang'};
}
export function recoveryCard(e: DictationRecovery, now: number): FeedbackCard {
  if (e.kind === 'mic_dropped_empty') return {...card(`recovery:${e.id}`,'Mic disconnected · no audio captured'),micStatus:{generation:e.generation,resolves:'new_take'},glyph:'mic-off',expiresAt:now+e.expires_in_ms};
  if (e.retry_failed) {
    const actions: CardAction[] = [];
    if (e.retry_ready !== false) {
      // A null engine asks the backend to reuse this clip's last failed selection.
      actions.push({label:'Retry again',call:e.kind === 'no_speech' ? {command:'transcribe_anyway',id:e.id} : {command:'retry_kept_dictation',id:e.id,engine:null}});
      if (e.alt_engine_short && e.alt_engine_id) actions.push({label:'Try another model',call:{command:'retry_kept_dictation',id:e.id,engine:e.alt_engine_id}});
    }
    actions.push({label:'Delete audio',call:{command:'discard_kept_dictation',id:e.id}});
    return {...card(`recovery:${e.id}`,`${e.engine_short} couldn't transcribe this`,'Recording kept',null,'amber',actions),recoveryGeneration:e.generation,retryFailed:true,closeLabel:'Close, recording kept',glyph:'bang',expiresAt:now+e.expires_in_ms};
  }
  const titles = {
    cloud_failed: `${e.engine_short} unreachable`, network_offline: 'Network offline',
    model_missing: `${e.engine_short} isn't downloaded`, remote_offline: `${e.engine_short} is offline`,
    unknown: 'Recording recovered', integrity: 'Recording interrupted', no_speech: 'No speech heard',
  };
  const actions: CardAction[] = e.retry_ready === false ? [] : e.kind === 'unknown' ? [{label:'Retry',call:{command:'retry_kept_dictation',id:e.id,engine:null}}] : e.kind === 'no_speech' ? [{label:'Transcribe anyway',call:{command:'transcribe_anyway',id:e.id}}] : e.alt_engine_short && e.alt_engine_id ? [{label:`Retry with ${e.alt_engine_short}`,call:{command:'retry_kept_dictation',id:e.id,engine:e.alt_engine_id}}] : [];
  const sub = e.retry_ready === false ? 'Audio is still finishing…' : e.expires_in_ms > 600_000 ? 'Audio kept up to 7 days · Manage in Home' : e.kind === 'no_speech' ? 'Audio kept for 30 s' : 'Temporary audio · Manage in Home';
  return {...card(`recovery:${e.id}`,e.kind==='no_speech' ? titles[e.kind] : `${titles[e.kind]} · Recording kept`,sub,null,e.kind === 'no_speech' ? 'neutral' : 'amber',actions),recoveryGeneration:e.generation,retryFailed:false,closeLabel:'Close, recording kept',glyph:e.kind==='no_speech' ? 'quiet' : 'bang',expiresAt:now+e.expires_in_ms};
}
/** Never surface native/provider errors: they may contain paths or other private data. */
export function retryErrorCard(error: unknown): FeedbackCard {
  const messages: Record<string,string> = {
    'Recording expired or discarded':'That recording is no longer available.',
    'Recording is still finalizing':'Audio is still finishing. Try again shortly.',
    'Dictation is busy':'Finish the current dictation, then retry.',
    'Recording is already being retried':'This recording is already being retried.',
    'Requested engine is not ready':'That engine is no longer ready. Check your engine in Home.',
    'Requested local engine is not ready':'That engine is no longer ready. Check your engine in Home.',
    'Retry failed; recording remains available until expiry':'Retry failed. Audio is still kept temporarily in Home → Needs you.',
    'Retry failed; recording is no longer available':'Retry failed. The temporary recording is no longer available.',
    'Retry discarded':'Retry was cancelled.',
  };
  return card('retry-error',"Couldn't retry",typeof error === 'string' && Object.prototype.hasOwnProperty.call(messages,error) ? messages[error] : 'Open Home → Needs you to check the recording and retry.',null,'amber');
}
export function deleteErrorCard(): FeedbackCard {
  return card('delete-error',"Couldn't delete audio",'Recording is still kept. Try Delete audio again in Home.',6000,'amber');
}
export function noteCard(e: DictationNote): FeedbackCard {
  const reasons = {timeout:'Polish timed out',rate_limited:'Polish rate limited',network:'Polish unavailable offline',guard:'Polish kept the original',error:'Polish failed'};
  switch(e.kind) {
    case 'mic_dropped': return {...card(e.kind,`Mic disconnected · using the ${elapsed(Math.floor(e.captured_ms/1000))} we got`),micStatus:{generation:e.generation,resolves:'new_take'}};
    case 'storage_failed': return card(e.kind,"Couldn't keep the recording",'',5000,'amber');
    case 'mic_silent_live': return {...card(`mic_silent_live:${e.generation}`,'Your mic sent silence — is it muted?','',null,'amber',[action('Choose mic','choose_mic')]),liveGeneration:e.generation};
    case 'mic_silent': return {...card(e.kind,'Your mic sent silence — is it muted?','',5000,'amber',[action('Choose mic','choose_mic')]),micStatus:{generation:e.generation,resolves:'new_take'}};
    case 'translate_failed': return card(e.kind,'Pasted untranslated · translation failed');
    case 'model_fallback': return card(e.kind,`Using ${e.alt_engine_short} · ${e.engine_short} isn't downloaded`);
    case 'gpu_fallback': return card(e.kind,'GPU unavailable · using the CPU');
    case 'polish_skipped': return card(e.kind,`Pasted unpolished · ${reasons[e.reason]}`);
  }
}
export function shortCard(e: RecordingTooShort) { return card('too_short',`Too short — ${e.mode === 'hold' ? 'hold' : 'talk'} a bit longer`,'',2200); }
export const noticeCopy = {
  finishing:'Finishing…', polish_on:'Polish on', polish_off:'· not polished', polish_setup:'Set up an AI model to use Polish',
  shortcuts_retired:'Formatting-mode shortcuts were retired', long_silence:'Still listening…',
  silence_stopped:'Stopped after 5 min of silence · using the audio we got', silence_discarded:'Stopped after 5 min of silence · no audio captured',
  recording_failed:'Recording failed', copy_failed:'Copy failed',
  transcription_failed:'Transcription failed · try again', history_retry:'Transcription failed · retry from History',
  shortcut_throttled:'Hold on…', no_speech:'No speech heard',
  secure_input:'Secure Input is on',
} satisfies Record<IslandNotice['kind'],string>;
export function noticeCard(e: IslandNotice) { return card(`notice:${e.kind}`,noticeCopy[e.kind],e.kind === 'secure_input' ? 'If your shortcut is unavailable, start from the tray or mic. Use the island to stop or cancel.' : '',e.kind === 'finishing' ? 600 : e.kind === 'long_silence' ? null : 3500); }
