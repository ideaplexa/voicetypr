import type { PillGeometry } from '@/pill-geometry';
export type QuickKind = 'polish' | 'engine' | 'mic' | 'language';
export interface QuickOption { id: string; label: string; checked: boolean; disabled?: boolean }
export interface QuickChoices { current: string; options: QuickOption[] }
export interface QuickOptions {
  polish: QuickChoices;
  engine: { current: string; groups: Array<{ label: string; options: QuickOption[] }> };
  mic: QuickChoices;
  language: QuickChoices;
  shortcut_caps: { mode: 'hold' | 'toggle'; keys: string[] };
  mic_ok: boolean;
  polish_context?: { generation: number; app_name: string; style: string | null; will_run: boolean; overridden: boolean } | null;
}
export const quickKinds: readonly QuickKind[] = ['polish', 'engine', 'mic', 'language'];
export const quickTitles: Record<QuickKind, string> = { polish: 'Default style', engine: 'Voice engine', mic: 'Microphone', language: 'Language' };
export function quickRows(options: QuickOptions, kind: QuickKind): QuickOption[] {
  return kind === 'engine' ? options.engine.groups.flatMap((group, index) => [
    { id: `header:${index}`, label: group.label, checked: false, disabled: true }, ...group.options,
  ]) : options[kind].options;
}
/** Popup selected row overlays the source chip, constrained to the webview's edges. */
export function pickPlacement(chip: DOMRect, rows: QuickOption[], top: boolean, canvasHeight: number) {
  const selected = Math.max(0, rows.findIndex(row => row.checked));
  const height = Math.min(canvasHeight - 4, 46 + rows.length * 30 + 8);
  const desired = chip.top + chip.height / 2 - 46 - selected * 30 - 15;
  const y = Math.max(2, Math.min(canvasHeight - height - 2, desired));
  const scrollTop = Math.max(0, Math.min(rows.length * 30 - (height - 54), y + 46 + selected * 30 + 15 - (chip.top + chip.height / 2)));
  return { y, height, selected, top, scrollTop };
}
/** The visible dot centre (not the panel bounding edge) is the record's fixed anchor. */
export function dotCentre(g: PillGeometry) {
  return { x: g.anchorX + (g.anchor.endsWith('-left') ? 6 : g.anchor.endsWith('-right') ? -6 : 0), y: g.anchorY + (g.anchor.startsWith('top-') ? 6 : -6) };
}
export class PickGuard {
  private opened = 0;
  private x = 0;
  private y = 0;
  private moved = false;
  open(now: number, x: number, y: number) { this.opened = now; this.x = x; this.y = y; this.moved = false; }
  pointer(x: number, y: number) { if (Math.hypot(x - this.x, y - this.y) >= 4) this.moved = true; }
  allows(now: number) { return this.moved || now - this.opened >= 250; }
}
