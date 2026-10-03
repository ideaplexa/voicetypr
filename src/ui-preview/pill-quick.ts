import type { QuickOption, QuickOptions } from '@/pill/quick-settings';
export function quickFixture(windows = false, noMic = false): QuickOptions {
  const rows = (values: Array<[string, string]>, current: string): QuickOption[] => values.map(([id, label]) => ({ id, label, checked: label === current }));
  return {
    polish: { current: 'Message', options: rows(['Off','Clean','Writing','Notes','Message','Code'].map(label => [`style_${label}`, label]), 'Message') },
    engine: { current: 'Parakeet', groups: [
      { label: windows ? 'On this PC' : 'On this Mac', options: rows([['model_parakeet','Parakeet'],['model_turbo','Whisper Turbo']], 'Parakeet') },
      { label: 'Cloud', options: rows([['model_soniox','Soniox'],['model_deepgram','Deepgram']], '') },
      { label: 'Network', options: rows([['model_remote_studio','Studio · Parakeet']], '') },
    ] },
    mic: { current: 'Shure MV7', options: rows([['microphone_default',windows ? 'System default' : 'System Default'],['microphone_Shure MV7','Shure MV7'],['microphone_MacBook Pro Microphone','MacBook Pro']], 'Shure MV7') },
    language: { current: 'English', options: rows([['language_auto','Auto'],['language_en','English'],['language_es','Spanish'],['language_fr','French'],['language_de','German']], 'English') },
    shortcut_caps: { mode: 'hold', keys: windows ? ['Ctrl','Alt','Space'] : ['⌥','Space'] },
    mic_ok: !noMic,
  };
}
