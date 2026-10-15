import { afterEach, expect, it, vi } from 'vitest';
import { waitFor } from '@testing-library/react';

const controllers = vi.hoisted(() => [] as Array<{destroy():void}>);
vi.mock('@/pill', async importOriginal => {
  const original = await importOriginal<typeof import('@/pill')>();
  return {...original, createRecordingPill: (...args: Parameters<typeof original.createRecordingPill>) => {
    const controller = original.createRecordingPill(...args); controllers.push(controller); return controller;
  }};
});
vi.mock('@tauri-apps/plugin-os', () => ({type: () => window.__TAURI_OS_PLUGIN_INTERNALS__.os_type}));
afterEach(() => {
  for (const controller of controllers.splice(0)) controller.destroy();
  document.body.replaceChildren(); delete document.documentElement.dataset.pillPreviewReady;
  vi.restoreAllMocks(); vi.resetModules();
});
it.each([
  ['esc-hint', 'cancel', 'row', 'bottom-center'],
  ['esc-hint-live', 'cancel', 'words', 'bottom-center'],
  ['esc-hint-discard', 'discard', 'work', 'bottom-center'],
  ['top-anchor-esc-hint', 'cancel', 'row', 'top-center'],
])('renders preview %s with a live Escape card and retained island content', async (state, copy, layer, anchor) => {
  window.history.replaceState(null, '', `/pill-preview.html?state=${state}`);
  document.body.innerHTML='<div id="preview-root"></div>';
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
  await import('@/ui-preview/pill');
  await waitFor(() => expect(document.documentElement.dataset.pillPreviewReady).toBe('true'), {timeout:3000});
  const card=document.querySelector<HTMLElement>('.escape-hint-card')!;
  expect(card.hidden).toBe(false);
  expect(card.querySelector('.card-title')?.textContent).toBe(`Press Esc again to ${copy}`);
  expect(card.querySelector('.outline-clock path')).not.toBeNull();
  expect(document.querySelector('.pill-surface .outline-clock path')).toBeNull(); // no countdown runs inside the island
  expect(document.querySelector(`[data-layer="${layer}"]`)?.getAttribute('aria-hidden')).toBe('false');
  const stack=document.querySelector<HTMLElement>('.feedback-stack')!;
  expect(stack.dataset.anchor).toBe(anchor);
  const anchorY=Number.parseFloat(document.querySelector<HTMLElement>('.pill-root')!.style.getPropertyValue('--pill-anchor-y'));
  expect(Number.parseFloat(stack.style.top) < anchorY).toBe(anchor.startsWith('bottom'));
});
