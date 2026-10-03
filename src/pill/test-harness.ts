import { vi } from 'vitest';
import { createRecordingPill } from '@/pill';
import { emptyContext } from '@/pill/contracts';
export function pillHarness(settings: Record<string, unknown> = {}, initial = { state: 'idle', error: null as string | null }) {
  vi.useFakeTimers();
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
  const root = document.createElement('div'); document.body.append(root);
  const handlers = new Map<string, (event: { payload: unknown }) => void>();
  const invoke = vi.fn(async (cmd: string) => cmd === 'get_settings' ? { pill_indicator_mode: 'always', streaming_preview_enabled: true, ...settings } : cmd === 'get_current_recording_state' ? initial : null);
  const controller = createRecordingPill(root, { invoke: invoke as never, listen: async (name, handler) => { handlers.set(name, handler as never); return () => { handlers.delete(name); }; } });
  const emit = (name: string, payload?: unknown) => handlers.get(name)?.({ payload });
  const settle = async (ms = 350) => { await vi.advanceTimersByTimeAsync(ms); };
  const context = (changes: Partial<ReturnType<typeof emptyContext>> = {}) => { const c = { ...emptyContext(), generation: 1, ...changes }; emit('dictation-context', c); return c; };
  const get = <T extends HTMLElement = HTMLElement>(selector: string) => root.querySelector<T>(selector)!;
  return { root, invoke, controller, emit, settle, context, get, handlers,
    destroy() { controller.destroy(); root.remove(); vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); } };
}
