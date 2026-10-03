import { afterEach, describe, expect, it, vi } from 'vitest';
import { pillHarness } from '@/pill/test-harness';
import type { TranscriptionStreamEvent } from '@/types/streaming';
let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
async function mount(settings: Record<string, unknown> = {}) { h = pillHarness(settings); await h.settle(0); h.emit('recording-started'); }
const stream = (payload: TranscriptionStreamEvent) => h.emit('transcription-stream', payload);
const partial = (revision: number, committed: string, tentative = '', session_id = 1) => stream({ type:'partial',revision,committed,tentative,session_id });
const committed = () => h.get('.pill-committed');

describe('RecordingPill streaming preview', () => {
  it('keeps one ltr isolate with committed ink before tentative ink', async () => { await mount(); expect(Array.from(h.get('.pill-preview-line').children)).toEqual([committed(),h.get('.pill-tentative')]); });
  it('does not subscribe when live preview is off', async () => { await mount({streaming_preview_enabled:false}); expect(h.handlers.has('transcription-stream')).toBe(false); });
  it('subscribes exactly once across repeated settings reads', async () => { await mount(); const original = h.handlers.get('transcription-stream'); h.emit('settings-changed'); h.emit('settings-changed'); await h.settle(0); expect(h.handlers.get('transcription-stream')).toBe(original); });
  it('keeps committed span mounted across appended partials', async () => { await mount(); const node = committed(); partial(1,'hello',' world'); partial(2,'hello world'); await h.settle(); expect(committed()).toBe(node); expect(node.textContent).toBe('hello world'); expect(h.get('[data-layer="words"]').hidden).toBe(false); });
  it('allows tentative rewrites but rejects non-monotonic committed ink without logging content', async () => { const warn = vi.spyOn(console,'warn'); await mount(); partial(1,'the quick',' borwn'); partial(2,'the quick',' brown'); partial(3,'rewrite',' tail'); expect(committed().textContent).toBe('the quick'); expect(h.get('.pill-tentative').textContent).toBe(' brown'); expect(warn).not.toHaveBeenCalled(); });
  it('ignores stale sessions and duplicate revisions', async () => { await mount(); stream({type:'started',session_id:1,revision:0,engine:'test'}); partial(1,'one'); partial(2,'wrong','',2); partial(1,'duplicate'); partial(3,'one two'); expect(committed().textContent).toBe('one two'); });
  it('hides on final, cancellation, error, and state exit', async () => {
    await mount();
    for (const [index,type] of ['final','cancelled','error'].entries()) {
      const session_id = index + 1; stream({type:'started',session_id,revision:0,engine:'test'}); partial(1,'visible','',session_id); await h.settle();
      stream(type === 'final' ? {type,session_id,revision:2,text:'visible'} : type === 'error' ? {type,session_id,revision:2,error:'failed'} : {type:'cancelled',session_id,revision:2}); await h.settle(); expect(h.get('[data-layer="words"]').hidden).toBe(true);
    }
    partial(1,'visible','',4); h.emit('transcription-started'); await h.settle(); expect(h.get('[data-layer="words"]').hidden).toBe(true);
  });
  it('rejects an old session after a new take without synthetic production demos', async () => { await mount(); partial(1,'first'); h.emit('transcription-started'); h.emit('recording-started'); partial(2,'stale'); partial(1,'second','',2); expect(committed().textContent).toBe('second'); });
});
