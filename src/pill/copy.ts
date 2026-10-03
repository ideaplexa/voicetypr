import type { DictationContext } from '@/pill/contracts';
export const elapsed = (seconds: number) => `${Math.floor(seconds / 60)}:${Math.floor(seconds % 60).toString().padStart(2, '0')}`;
export const styleName = (context: DictationContext) => context.polish.style ? context.polish.style[0].toUpperCase() + context.polish.style.slice(1) : '· not polished';
export const micName = (name: string) => name.replace(/^(?:Headset |Internal )?Microphone(?: Array)?\s*\((.+)\)$/, '$1').replace(/ Microphone$/, '');
export const copiedTitle = (mac: boolean) => `Copied — press ${mac ? '⌘V' : 'Ctrl+V'}`;
export const shortCopy = (context: DictationContext) => `Too short — ${context.mode === 'hold' ? 'hold' : 'talk'} a bit longer`;

export const contextMicName = (context: DictationContext) => micName(/^(?:(?:Headset |Internal )?Microphone(?: Array)?)$/.test(context.mic.name) ? context.mic.tooltip : context.mic.name);
