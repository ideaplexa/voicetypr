import type { TranscriptionStreamEvent } from '@/types/streaming';
import type { PasteOutcomePayload } from '@/types/paste-outcome';
import { emptyContext, type DictationContext } from '@/pill/contracts';
export type IslandState = 'rest' | 'peek' | 'start' | 'listening' | 'live' | 'transcribing' | 'polishing' | 'pasted' | 'copied' | 'no_permission' | 'too_short' | 'nospeech' | 'error';
export type LayerName = 'start' | 'words' | 'row' | 'hint' | 'work' | 'done' | 'notice' | 'note' | 'peek' | 'pick';
export class IslandMachine {
  state: IslandState = 'rest';
  context = emptyContext();
  generation = 0;
  private consumedContext = 0;
  private observedGeneration = 0;
  committed = '';
  tentative = '';
  session = -1;
  lastSession = -1;
  revision = -1;
  hint = false;
  words = 0;
  error = '';
  startedAt = 0;
  frozenSeconds = 0;
  get recording() { return this.state === 'start' || this.state === 'listening' || this.state === 'live'; }
  get processing() { return this.state === 'transcribing' || this.state === 'polishing'; }
  get terminal() { return this.state === 'pasted' || this.state === 'copied' || this.state === 'no_permission'; }
  get layers(): readonly LayerName[] {
    switch (this.state) {
      case 'rest': return [];
      case 'peek': return ['peek'];
      case 'start': return ['start'];
      case 'live': return this.hint ? ['words', 'hint'] : ['words', 'row'];
      case 'listening': return [this.hint ? 'hint' : 'row'];
      case 'transcribing': case 'polishing': return [this.hint ? 'hint' : 'work'];
      case 'pasted': return ['done'];
      case 'copied': case 'no_permission': case 'error': return ['notice'];
      case 'nospeech': case 'too_short': return ['note'];
    }
  }
  /** 236 px holds from listening through work; hover controls grow outside that row. */
  get width() { return this.state === 'rest' ? 12 : this.state === 'start' || this.state === 'peek' || this.state === 'live' ? 300 : this.state === 'copied' || this.state === 'no_permission' ? 300 : 236; }
  get height() { return this.state === 'rest' ? 12 : this.state === 'start' ? 85 : this.state === 'peek' ? 64 : this.state === 'copied' || this.state === 'no_permission' ? 70 : this.state === 'live' ? 64 : 34; }
  contextEvent(context: DictationContext) {
    if (context.generation <= this.generation || context.generation < this.observedGeneration) return false;
    this.context = context;
    this.generation = context.generation;
    // Context resolution can finish after recording-started. Never reopen a stopped take.
    if (this.recording) {
      this.consumedContext = context.generation;
      if (context.show_start_card) this.state = 'start';
    }
    return true;
  }
  observeGeneration(generation: number) {
    if (generation < this.generation || generation < this.observedGeneration) return false;
    this.observedGeneration = generation; return true;
  }
  start(now: number) {
    if (this.recording) return false;
    this.clearStream(); this.hint = false; this.words = 0;
    this.startedAt = now; this.frozenSeconds = 0;
    this.state = this.context.generation > this.consumedContext && this.context.show_start_card ? 'start' : 'listening';
    this.consumedContext = this.context.generation;
    return true;
  }
  foldStart() { if (this.state === 'start') this.state = this.committed || this.tentative ? 'live' : 'listening'; }
  work(state: 'transcribing' | 'polishing', now: number) {
    if (this.recording) this.frozenSeconds = Math.floor((now - this.startedAt) / 1000);
    this.clearStream(); this.state = state;
  }
  outcome(payload: PasteOutcomePayload) {
    if (this.recording) return false;
    this.clearStream(); this.hint = false; this.words = payload.words; this.state = payload.outcome;
    return true;
  }
  rest() { this.state = 'rest'; this.hint = false; this.clearStream(); }
  clearStream() { this.committed = this.tentative = ''; this.session = -1; this.revision = -1; }
  stream(event: TranscriptionStreamEvent) {
    if (!this.recording) return false;
    if (event.type === 'started') {
      if (event.session_id <= this.lastSession) return false;
      this.clearStream(); this.session = this.lastSession = event.session_id; this.revision = event.revision;
      return true;
    }
    if (this.session < 0 && event.type === 'partial' && event.session_id > this.lastSession) {
      this.session = this.lastSession = event.session_id;
    }
    if (event.session_id !== this.session || event.revision <= this.revision) return false;
    this.revision = event.revision;
    if (event.type === 'partial') {
      // Reject rewrites; committed ink may only grow (invariant 10).
      if (!event.committed.startsWith(this.committed)) return false;
      this.committed = event.committed; this.tentative = event.tentative;
      if (this.state !== 'start') this.state = this.committed || this.tentative ? 'live' : 'listening';
    } else {
      this.clearStream(); if (this.state === 'live') this.state = 'listening';
    }
    return true;
  }
}
