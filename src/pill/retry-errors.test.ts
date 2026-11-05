import { afterEach, expect, it } from 'vitest';
import { pillHarness } from '@/pill/test-harness';

let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());

it('keeps the failed card and visibly reports a rejected Delete without private errors', async () => {
  h = pillHarness(); await h.settle(0);
  const base = h.invoke.getMockImplementation()!;
  h.invoke.mockImplementation(async command => {
    if (command === 'discard_kept_dictation') throw '/private/unwritable';
    return base(command);
  });
  h.emit('dictation-recovery', {generation:1,id:'clip',kind:'cloud_failed',engine_short:'Soniox',alt_engine_short:null,alt_engine_id:null,expires_in_ms:600000,retry_ready:true,retry_failed:true});
  await h.settle(1500);
  h.get('[data-key="recovery:clip"] .act:nth-child(2)').click();
  await h.settle(1000);
  expect(h.get('[data-key="delete-error"]').textContent).toContain("Couldn't delete audio");
  expect(h.get('[data-key="recovery:clip"] .act:nth-child(2)')).not.toBeDisabled();
  expect(h.root.textContent).not.toContain('/private/unwritable');
});

it.each([
  ['Recording expired or discarded', 'That recording is no longer available.'],
  ['Recording is still finalizing', 'Audio is still finishing. Try again shortly.'],
  ['Dictation is busy', 'Finish the current dictation, then retry.'],
  ['Requested local engine is not ready', 'That engine is no longer ready. Check your engine in Home.'],
  ['Retry failed; recording remains available until expiry', 'Retry failed. Audio is still kept temporarily in Home → Needs you.'],
  ['Retry failed; recording is no longer available', 'Retry failed. The temporary recording is no longer available.'],
  ['/private/provider-response?token=secret', 'Open Home → Needs you to check the recording and retry.'],
])('visibly explains a rejected Retry (%s), without rendering private errors', async (error, copy) => {
  h = pillHarness(); await h.settle(0);
  const base = h.invoke.getMockImplementation()!;
  h.invoke.mockImplementation(async command => {
    if (command === 'retry_kept_dictation') {
      // Cleanup can retire the id before the rejected invocation reaches JS.
      if (error === 'Recording expired or discarded') h.emit('kept-dictation-expired', { id:'clip' });
      throw error;
    }
    return base(command);
  });
  h.emit('dictation-recovery', {generation:1,id:'clip',kind:'integrity',engine_short:'Parakeet v3',alt_engine_short:'Parakeet v3',alt_engine_id:'parakeet-tdt-0.6b-v3',expires_in_ms:600000,retry_ready:true});
  await h.settle(1500);
  h.get('[data-key="recovery:clip"] .act').click();
  await h.settle(1000);
  const message = h.get('[data-key="retry-error"]');
  expect(message.hidden).toBe(false);
  expect(message.inert).toBe(false);
  expect(message.textContent).toContain("Couldn't retry");
  expect(message.textContent).toContain(copy);
  expect(h.root.textContent).not.toContain('token=secret');
  expect(h.invoke).not.toHaveBeenCalledWith('discard_kept_dictation', expect.anything());
});
