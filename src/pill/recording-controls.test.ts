import { afterEach, expect, it } from 'vitest';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());
const ready = async (settings: Record<string, unknown> = {}) => {
  h = pillHarness(settings); await h.settle(0);
  h.emit('recording-state-changed', { state: 'starting', generation: 1 });
  expect(h.get('.ctl').hidden).toBe(true);
  h.emit('recording-state-changed', { state: 'recording', generation: 1 });
  await h.settle(1000);
};

it('keeps Stop visible without hover and reserves the same listening width on hover', async () => {
  await ready();
  expect(h.get('.ctl').hidden).toBe(false);
  expect(h.get('.ctl').inert).toBe(false);
  expect(h.get('.stop').style.opacity).not.toBe('0');
  expect(h.get('.cancel').style.opacity).toBe('0');
  const width = h.get('.pill-content').style.width;
  h.emit('pill-pointer', { inside: true });
  await h.settle(350);
  expect(h.get('.cancel').style.opacity).toBe('');
  expect(h.get('.pill-content').style.width).toBe(width);
  h.get<HTMLButtonElement>('.stop').click();
  expect(h.invoke).toHaveBeenCalledWith('stop_recording');
  expect(h.invoke).not.toHaveBeenCalledWith('cancel_recording');
  expect(h.get<HTMLButtonElement>('.stop').disabled).toBe(true);
});

it('keeps Cancel separate and does not stop from a capsule click', async () => {
  await ready();
  h.get('.pill-surface').click();
  expect(h.invoke).not.toHaveBeenCalledWith('stop_recording');
  h.emit('pill-pointer', { inside: true });
  h.get<HTMLButtonElement>('.cancel').click();
  expect(h.invoke).toHaveBeenCalledWith('cancel_recording');
  expect(h.invoke).not.toHaveBeenCalledWith('stop_recording');
});

it('prevents mouse focus on Stop while retaining its keyboard button semantics', async () => {
  await ready();
  const stop = h.get<HTMLButtonElement>('.stop');
  const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
  stop.dispatchEvent(down);
  expect(down.defaultPrevented).toBe(true);
  expect(stop.type).toBe('button');
  expect(stop.getAttribute('aria-label')).toBe('Stop recording');
});

it('keeps Stop available in start details and live preview as well as listening', async () => {
  await ready({ transcription_mode: 'live_preview' });
  h.context({ generation: 1, show_start_card: true });
  expect(h.get('.pill-root').dataset.state).toBe('start');
  expect(h.get('.ctl').hidden).toBe(false);
  h.emit('transcription-stream', { type: 'partial', session_id: 1, revision: 1, committed: 'Words', tentative: '' });
  await h.settle(800);
  expect(h.get('.pill-root').dataset.state).toBe('live');
  expect(h.get('.ctl').hidden).toBe(false);
  h.emit('transcription-started');
  expect(h.get('.ctl').hidden).toBe(true);
});
