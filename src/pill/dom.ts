import { icon } from '@/pill/icons';
export function createIslandDom(host: HTMLElement) {
  const root = document.createElement('div'); root.className = 'pill-root';
  root.innerHTML = `<div class="pill-surface" hidden>
    <i class="pill-rest-dot" data-testid="pill-rest-dot" aria-label="Recording idle"><i class="core"></i></i>
    <div class="appico" aria-hidden="true"><span class="letter"></span><img hidden alt="" /></div>
    <i class="aibadge" aria-hidden="true" hidden></i>
    <canvas class="glyph" data-testid="pill-bars" aria-hidden="true"></canvas>
    <div class="layer start" data-layer="start"><div class="start-row"><span></span><span class="mono start-time">0:00</span></div><div class="chips"><button class="chip style-chip" type="button"><span class="style-icon">${icon("star",10)}</span><span class="style-name"></span><span class="style-chevron">${icon("chevron",9)}</span></button><span class="chip mic-chip">${icon("mic",10)}<span class="mic-name"></span><i class="mic-dot"></i></span><span class="chip engine-chip"></span></div></div>
    <div class="layer fill words-layer" data-layer="words"><div class="pill-preview" data-testid="pill-preview"><span class="pill-preview-line" data-testid="pill-preview-line"><span class="pill-committed" data-testid="pill-committed"></span><span class="pill-tentative" data-testid="pill-tentative"></span></span></div></div>
    <div class="layer fill" data-layer="row"><div class="row listening-row"><span class="wslot"></span><span class="mono pill-timer" aria-label="Recording elapsed time">0:00</span><span class="ctl"><button class="round stop" aria-label="Stop recording" title="Stop" type="button"><svg width="8" height="8" viewBox="0 0 8 8" aria-hidden="true"><rect width="8" height="8" rx="1.5" fill="currentColor"/></svg></button><button class="round cancel" aria-label="Cancel recording" title="Cancel" type="button">${icon("close",12)}</button></span></div></div>
    <div class="layer fill" data-layer="hint"><div class="row hint-row"><span>Press Esc again to cancel</span><span class="mono hint-time">0:00</span></div><span class="drain"><i></i></span></div>
    <div class="layer fill" data-layer="work"><div class="row work-row"><span class="work-label">Transcribing</span><span class="mono work-time">0:00</span><button class="act skip" type="button" hidden>Skip</button></div></div>
    <div class="layer fill" data-layer="done"><div class="row result-row"><span class="done-label">Pasted</span><span class="dim done-count"></span></div><span class="hold"><i></i></span></div>
    <div class="layer fill notice" data-layer="notice"><div class="row result-row"><span class="notice-title"></span><span class="dim notice-count"></span></div><div class="acts"><span class="notice-sub dim"></span><button class="act privacy-settings" type="button" hidden>Open System Settings</button><button class="act dismiss" aria-label="Dismiss copied card" type="button">${icon("close",12)}</button></div></div>
    <div class="layer fill" data-layer="note"><div class="row note-row"><span class="note-clock dim">${icon("clock",14)}</span><span class="note-text dim"></span><button class="round note-dismiss" type="button" aria-label="Dismiss no speech note" hidden>${icon("close",12)}</button></div></div>
    <div class="layer fill peek" data-layer="peek"><div class="chips peek-chips"><button class="chip peek-style" type="button"></button><button class="chip peek-engine" type="button"></button><button class="chip peek-mic" type="button"></button><button class="chip peek-language" type="button"></button></div><div class="peek-row"><span class="peek-hint dim"></span><button class="round record" aria-label="Start dictation" type="button">${icon("mic",16)}</button></div></div>
  </div><div class="pill-announcement" role="status" aria-live="polite" aria-atomic="true"></div>`;
  host.replaceChildren(root);
  const get = <T extends HTMLElement>(selector: string) => root.querySelector<T>(selector)!;
  return { root, surface: get<HTMLDivElement>('.pill-surface'), get,
    canvas: get<HTMLCanvasElement>('.glyph'), icon: get<HTMLDivElement>('.appico'), badge: get<HTMLElement>('.aibadge'),
    committed: get<HTMLSpanElement>('.pill-committed'), tentative: get<HTMLSpanElement>('.pill-tentative'),
    announcement: get<HTMLDivElement>('.pill-announcement') };
}
export type IslandDom = ReturnType<typeof createIslandDom>;
