import { dotCentre, pickPlacement, PickGuard, quickKinds, quickRows, quickTitles, type QuickKind, type QuickOptions } from '@/pill/quick-settings';
import { icon } from '@/pill/icons';
import { StackView } from '@/pill/stack-view';
import { blockedCard, recoveryCard, noteCard, shortCard, noticeCard, type FeedbackCard } from '@/pill/feedback';
import type { DictationBlocked, DictationRecovery, DictationNote, RecordingTooShort, IslandNotice } from '@/types/island-events';
import { formatKeyForDisplay } from '@/lib/keyboard-normalizer';
import { createIslandDom } from '@/pill/dom';
import { Crossfade } from '@/pill/crossfade';
import { Words } from '@/pill/words';
import { Layers } from '@/pill/layers';
import { Spring, clamp } from '@/pill/spring';
import { AppIcon } from '@/pill/app-icon';
import { Badge } from '@/pill/badge';
import { Glyph } from '@/pill/glyph';
import { IslandMachine } from '@/pill/island';
import { copiedTitle, elapsed, contextMicName, shortCopy, styleName } from '@/pill/copy';
import type { DictationContext, IslandActions, PillPointer, PillSettings } from '@/pill/contracts';
import type { PillAudioLevel } from '@/types';
import type { PasteOutcomePayload } from '@/types/paste-outcome';
import type { TranscriptionStreamEvent } from '@/types/streaming';
import { applyPillGeometry, type PillGeometry } from '@/pill-geometry';

export interface RendererDeps {
  actions: IslandActions;
  mac: boolean;
  icon: (key: string) => Promise<unknown>;
  hitRegions: (rects: Array<{ x: number; y: number; width: number; height: number }>) => void;
  setTimeout: typeof window.setTimeout;
  clearTimeout: (id: number) => void;
}
export function createIsland(host: HTMLElement, deps: RendererDeps) {
  const dom = createIslandDom(host), machine = new IslandMachine();
  let quick: QuickOptions | undefined;
  let geometry: PillGeometry = { anchor: deps.mac ? 'bottom-center' : 'bottom-right', anchorX: deps.mac ? 231 : 451, anchorY: 425 };
  let pick: { kind: QuickKind; from: 'peek' | 'start'; y: number; height: number; selected: number; width: number } | undefined;
  let quickPending = false, clickPending = false;
  const pickGuard = new PickGuard();
  let feedbackOwned = false, polished = false, pastedLeft = 2400, pastedSince = 0;
  const stack = new StackView(dom.root, {
    wake: () => wake(), changed: () => sync(),
    call: c => deps.actions.card?.(c) ?? Promise.reject(new Error('Unavailable')),
    announce: s => announce(s), setTimeout: deps.setTimeout, clearTimeout: deps.clearTimeout,
  });
  const appIcon = new AppIcon(dom.icon);
  const farRow = (state: typeof machine.state) => state === 'start' || state === 'copied' || state === 'no_permission' || state === 'pasted' && !top;
  const words = new Words(dom.get('.pill-preview-line'), dom.committed, dom.tentative);
  const crossfade = new Crossfade(dom.surface);
  const layers = new Layers(dom.surface), glyph = new Glyph(dom.canvas), badge = new Badge(dom.badge);
  const W = new Spring(12), H = new Spring(12), R = new Spring(6), IX = new Spring(5), IY = new Spring(5), IS = new Spring(0), GX = new Spring(34), GY = new Spring(4), GW = new Spring(44), HC = new Spring(0, .25, 0);
  const ITOP = new Spring(26), GTOP = new Spring(38);
  const POFF = new Spring(0);
  const springs = [W, H, R, IX, IY, IS, GX, GY, GW, HC, ITOP, GTOP, POFF];
  const media = window.matchMedia('(prefers-reduced-motion: reduce)');
  let reduced = media.matches, mode: PillSettings['pill_indicator_mode'] = 'when_recording';
  let destroyed = false, frame: number | undefined, last = 0, hovering = false, top = false;
  let finishing = false, finishingTimer: number | undefined;
  let copiedTimer: number | undefined, pickTimer: number | undefined;
  let lastState = machine.state, startTimer: number | undefined, feedbackTimer: number | undefined, hintTimer: number | undefined, hoverTimer: number | undefined;
  let slowSince = 0;
  let entered = 0, pointerX = 0, pointerY = 0, pointerTime = 0;
  let closeHold = 0;
  let waveSeconds = 0, cancelPending = false, stopPending = false;
  let enabledStream = false, displayedSeconds = -1;
  let badgeFromX = 25, badgeFromY = 56, shortcut = deps.mac ? 'Alt+Space' : 'Ctrl+Alt+Space';
  const timerEl = dom.get<HTMLElement>('.pill-timer'), startTimeEl = dom.get<HTMLElement>('.start-time'), workTimeEl = dom.get<HTMLElement>('.work-time'), hintTimeEl = dom.get<HTMLElement>('.hint-time');
  const timerElements = [timerEl, startTimeEl, workTimeEl, hintTimeEl];
  const rowEl = dom.get<HTMLElement>('.listening-row'), ctlEl = dom.get<HTMLElement>('.ctl');
  const rowLayers = ['row', 'hint', 'work', 'note'].map(name => layers.entries[name as 'row' | 'hint' | 'work' | 'note'].el);
  const timers = () => { for (const id of [startTimer, feedbackTimer, hintTimer, hoverTimer, copiedTimer, finishingTimer, pickTimer]) if (id !== undefined) deps.clearTimeout(id); startTimer = feedbackTimer = hintTimer = hoverTimer = copiedTimer = finishingTimer = pickTimer = undefined; finishing = false; };
  const text = (selector: string, value: string) => { const el = dom.get<HTMLElement>(selector); if (el.textContent !== value) el.textContent = value; if (el.matches('.done-label,.note-text')) el.title = value; };
  const inlineWidth = (selector: string) => {
    const range = document.createRange(); range.selectNodeContents(dom.get(selector));
    const width = typeof range.getBoundingClientRect === 'function' ? range.getBoundingClientRect().width : 0;
    range.detach(); return width;
  };
  let regionSignature = '';
  const hit = () => {
    const b = dom.surface.getBoundingClientRect();
    let x=b.x,y=b.y,width=b.width,height=b.height;
    if(machine.state==='rest') {x-=4;width+=8;y-=8;height=20;}
    const rects = [...(dom.surface.hidden ? [] : [{x,y,width,height}]),...stack.rects()];
    const signature=JSON.stringify(rects);
    if(signature!==regionSignature){regionSignature=signature;deps.hitRegions(rects);}
  };
  const wake = () => { if (!destroyed && frame === undefined) { last = performance.now(); frame = requestAnimationFrame(tick); } };
  const syncTime = (now: number) => {
    if (machine.recording) waveSeconds = Math.max(0, (now - machine.startedAt) / 1000);
    const seconds = Math.floor(machine.recording ? waveSeconds : machine.frozenSeconds);
    if (seconds === displayedSeconds) return; displayedSeconds = seconds;
    const t = elapsed(seconds);
    for (const el of timerElements) if (el.textContent !== t) el.textContent = t;
  };
  const place = () => {
    dom.surface.style.width = `${Math.max(12, W.x)}px`; dom.surface.style.height = `${Math.max(12, H.x)}px`; dom.surface.style.borderRadius = `${Math.max(6, R.x)}px`;
    const panel = machine.state === 'peek' || !!pick;
    const dot = dotCentre(geometry);
    if (panel) {
      const insetX = Math.min(17, W.x / 2), insetY = Math.min(17, H.x / 2);
      const left = geometry.anchor.endsWith('-left') ? dot.x - insetX : geometry.anchor.endsWith('-right') ? dot.x - W.x + insetX : dot.x - W.x / 2;
      const y = (top ? dot.y - insetY : dot.y - H.x + insetY) + POFF.x;
      dom.surface.style.transform = 'none';
      dom.surface.style.left = `${left}px`; dom.surface.style.top = `${y}px`;
      const record = dom.get<HTMLElement>('.record');
      record.style.left = `${dot.x - left - 14}px`;
      record.style.top = `${dot.y - y - 14}px`;
      record.style.transform = 'none';
    } else {
      dom.surface.style.transform = ''; dom.surface.style.left = ''; dom.surface.style.top = '';
    }
    const size = Math.max(0, IS.x);
    dom.icon.style.width = dom.icon.style.height = `${size}px`;
    dom.icon.style.left = `${IX.x - size / 2}px`;
    // IY tracks distance from the anchored edge; card icons follow their title row.
    const cy = top ? IY.x : H.x - 2 - IY.x;
    dom.icon.style.top = `${cy - size / 2}px`; dom.icon.style.opacity = String(clamp(size / 18));
    dom.icon.style.borderRadius = `${Math.max(3, size * (deps.mac ? .28 : .22))}px`;
    dom.icon.hidden = size < .1 || panel;
    dom.canvas.style.left = `${GX.x}px`;
    const gy = top ? GY.x : H.x - 2 - GY.x - 24;
    dom.canvas.style.top = `${gy}px`; dom.canvas.style.width = '260px'; dom.canvas.style.height = '24px';
    rowEl.style.width = `${Math.max(0, W.x - 2)}px`;
    ctlEl.style.width = `${Math.max(0, HC.x)}px`; ctlEl.style.opacity = String(clamp(HC.x / 62)); ctlEl.inert = HC.to === 0;
    const edge = Math.max(0, IY.x - 16);
    for (const el of rowLayers) { el.style.left = `${IX.x - 21}px`; el.style[top ? 'top' : 'bottom'] = `${edge}px`; el.style[top ? 'bottom' : 'top'] = ''; }
  };
  function tick(now: number) {
    frame = undefined;
    if (destroyed) return;
    const dt = Math.min(.05, Math.max(0, (now - last) / 1000)); last = now;
    let moving = false, shapeMoving = false;
    if (closeHold > 0 && !reduced) { closeHold = Math.max(0, closeHold - dt); moving = true; }
    else for (const spring of springs) { const before = spring.x; shapeMoving = spring.step(dt, reduced) || spring.x !== before || shapeMoving; }
    moving = shapeMoving || moving;
    if (farRow(machine.state)) { IY.x = H.x - 2 - ITOP.x; IY.v = H.v - ITOP.v; GY.x = H.x - 2 - GTOP.x; GY.v = H.v - GTOP.v; }
    moving = layers.step(dt, reduced, W.x, W.to, H.x, H.to) || moving;
    moving = words.step(dt, reduced) || moving;
    moving = crossfade.step(dt) || moving;
    if (!machine.recording && layers.entries.words.alpha === 0) words.clear();
    if (machine.recording) { syncTime(now); glyph.wave.advance(waveSeconds, dt); }
    moving = glyph.step(dt, reduced) || moving;
    const size = Math.max(0, IS.x), badgeY = Math.max(IY.x - size / 2 - 2, 8);
    moving = badge.step(dt, IX.x + size / 2 - 4, top ? badgeY - 6 : H.x - 2 - badgeY - 6, reduced, badgeFromX, badgeFromY) || moving;
    if (shapeMoving) place(); glyph.draw(waveSeconds, Math.max(0, GW.x), reduced);
    const landed = machine.state === 'rest' && W.x < 12.5 && H.x < 12.5;
    stack.tuck(mode === 'never' || !landed);
    moving = stack.step(dt,reduced,landed) || moving;
    hit();
    if (moving || (!dom.surface.hidden && machine.recording && !reduced)) frame = requestAnimationFrame(tick);
    else dom.surface.style.willChange = 'auto';
  }
  const sync = () => {
    const state = machine.state, fromStart = lastState === 'start' && state !== 'start';
    if (state !== lastState && reduced) crossfade.capture();
    if (state !== lastState && !reduced && (machine.width < W.to || machine.height < H.to)) closeHold = .08;
    if (farRow(state) && !farRow(lastState)) { ITOP.x = H.x - 2 - IY.x; ITOP.v = H.v - IY.v; GTOP.x = H.x - 2 - GY.x; GTOP.v = H.v - GY.v; }
    if (fromStart) {
      const chip = dom.get<HTMLElement>('.style-chip').getBoundingClientRect(), surface = dom.surface.getBoundingClientRect();
      badgeFromX = chip.width ? chip.left + 10 - surface.left - 6 : 18;
      badgeFromY = chip.height ? chip.top + chip.height / 2 - surface.top - 6 : top ? 16 : 54;
    }
    lastState = state;
    const rest = state === 'rest';
    dom.root.dataset.state = pick ? 'pick' : rest ? 'idle' : state;
    dom.root.dataset.preview = String(state === 'live');
    dom.root.dataset.platform = deps.mac ? 'macos' : 'windows';
    dom.root.dataset.reducedMotion = String(reduced);
    dom.surface.hidden = mode === 'never' || (rest && mode !== 'always' && !stack.present);
    const owns = mode !== 'never' && (stack.present || machine.terminal || ['too_short','nospeech','error'].includes(state));
    if (owns !== feedbackOwned) { feedbackOwned=owns; void deps.actions.feedbackVisible?.(owns).catch(() => {}); }
    dom.root.dataset.visible = String(!dom.surface.hidden);
    dom.get<HTMLElement>('.pill-rest-dot').hidden = !rest;
    dom.get<HTMLElement>('.core').classList.toggle('bad', !machine.context.mic.ok);
    const ctl = hovering && state === 'listening' && !machine.hint && !stopPending;
    HC.to = ctl ? 62 : 0;
    W.to = machine.width + (ctl ? 62 : 0);
    const showDoneActions = state === 'pasted' && polished && hovering && !!deps.actions.original;
    H.to = showDoneActions ? 70 : machine.height;
    dom.get<HTMLElement>('.done-actions').hidden = !showDoneActions;
    dom.get<HTMLButtonElement>('.original').hidden = !polished; R.to = rest ? 6 : H.to > 34 ? 20 : 17;
    IS.to = rest ? 0 : state === 'start' ? 28 : 18;
    IX.to = rest ? 5 : state === 'start' ? 28 : 21;
    IY.to = rest ? 5 : 16;
    GX.to = state === 'start' ? 52 : machine.recording ? 42 : 34; GY.to = 4;
    GW.to = machine.recording ? machine.width - (state === 'start' ? 105 : 93) : 22;
    layers.target(pick ? ['pick'] : machine.layers); glyph.target(state, !machine.hint);
    badge.target(machine.context, machine.recording && state !== 'start' || machine.processing, fromStart);
    if (machine.committed || machine.tentative || layers.entries.words.alpha === 0) words.update(machine.committed, machine.tentative, reduced);
    dom.canvas.dataset.state = state; dom.canvas.hidden = !!pick || state === 'peek';
    if (pick) dom.badge.hidden = true;
    const label = finishing && machine.processing ? 'Finishing…' : state === 'polishing' ? `Polishing${machine.context.polish.style ? ` · ${styleName(machine.context)}` : ''}` : 'Transcribing';
    if (machine.processing) layers.swapText(dom.get('.work-label'), label);
    else if (!machine.processing) text('.work-label', label);
    dom.get<HTMLButtonElement>('.skip').hidden = state !== 'polishing' || !deps.actions.skip;
    text('.done-count', `· ${machine.words} ${machine.words === 1 ? 'word' : 'words'}`);
    text('.notice-title', state === 'error' ? machine.error : state === 'no_permission' ? 'Allow Accessibility to paste automatically' : copiedTitle(deps.mac));
    dom.get('.notice-title').title = dom.get('.notice-title').textContent ?? '';
    text('.notice-count', state === 'error' ? '' : `${machine.words} words`);
    text('.notice-sub', state === 'no_permission' ? `Your words are copied — press ${deps.mac ? '⌘V' : 'Ctrl+V'} for now` : state === 'copied' ? `${machine.context.app.name || 'The app'} didn't accept the paste` : '');
    dom.get<HTMLButtonElement>('.privacy-settings').hidden = state !== 'no_permission' || !deps.actions.openPrivacySettings;
    text('.privacy-settings', deps.mac ? 'Open System Settings' : 'Open Privacy settings');
    dom.surface.classList.toggle('amber', state === 'copied' || state === 'no_permission');
    dom.surface.classList.toggle('red', state === 'error');
    dom.get<HTMLElement>('.note-clock').hidden = state !== 'too_short';
    dom.get<HTMLButtonElement>('.note-dismiss').hidden = state !== 'nospeech';
    dom.get<HTMLButtonElement>('.dismiss').hidden = state === 'error';
    text('.note-text', state === 'nospeech' ? 'No speech heard' : shortCopy(machine.context));
    text('.hint-row > span:first-child', `Press Esc again to ${machine.processing ? 'discard' : 'cancel'}`);
    dom.get<HTMLButtonElement>('.cancel').disabled = cancelPending;
    dom.get<HTMLButtonElement>('.stop').disabled = stopPending;
    if (state === 'too_short' || state === 'nospeech') {
      const width = inlineWidth('.note-text');
      if (width) W.to = Math.min(360,width + (state === 'too_short' ? 38 + 14 + 7 + 14 + 2 : 60 + 14 + 8 + 26 + 2));
    } else if (state === 'error') {
      const width = inlineWidth('.notice-title'); if (width) W.to = Math.min(360,width + 76);
    } else if (state === 'start') {
      const styleWidth = inlineWidth('.style-name'), micWidth = inlineWidth('.mic-name'), engineWidth = inlineWidth('.engine-chip');
      if (styleWidth && micWidth && engineWidth) W.to = Math.min(340,Math.max(180,styleWidth + (machine.context.polish.style ? 39 : 12) + micWidth + 35 + engineWidth + 38));
      H.to = layers.entries.start.el.scrollHeight ? layers.entries.start.el.scrollHeight + 2 : machine.height;
      GW.to = Math.max(0,W.to - 105);
    } else if (state === 'copied' || state === 'no_permission') {
      const titleWidth = inlineWidth('.notice-title'), countWidth = inlineWidth('.notice-count');
      if (titleWidth) {
        const rowWidth = titleWidth + countWidth + 96;
        const actionWidth = inlineWidth('.notice-sub') + (state === 'no_permission' ? dom.get('.privacy-settings').offsetWidth + 5 : 0) + 26 + 5 + 24;
        W.to = Math.min(360,Math.max(236,rowWidth,actionWidth));
      } else if (state === 'no_permission') W.to = 360;
    }
    if (state === 'copied' || state === 'no_permission' || state === 'error') {
      const notice = layers.entries.notice.el;
      notice.style.width = `${Math.min(360,W.to)}px`;
      dom.get('.acts').hidden = state === 'error';
      // Measure after constraining width so wrapped subtext and actions grow the card.
      H.to = notice.scrollHeight ? notice.scrollHeight + 2 : state === 'no_permission' ? 102 : machine.height;
    }
    if (state === 'peek') { W.to = Math.min(438, Math.max(378, dom.get<HTMLElement>('.peek-chips').scrollWidth + 22)); H.to = 70; }
    if (pick) { W.to = pick.width; H.to = pick.height; }
    POFF.to = pick ? pick.y - (top ? dotCentre(geometry).y - 17 : dotCentre(geometry).y - pick.height + 17) : 0;
    ITOP.to = state === 'start' ? 26 : 16; GTOP.to = state === 'start' ? 38 : 28;
    if (farRow(state)) { IY.to = H.to - 2 - ITOP.to; GY.to = H.to - 2 - GTOP.to; }
    syncTime(performance.now());
    if (dom.surface.hidden && !stack.present) {
      if (frame !== undefined) cancelAnimationFrame(frame); frame = undefined; words.clear(); crossfade.clear();
      for (const spring of springs) spring.snap(); layers.step(1, true); glyph.step(1, true); place(); hit();
    }
    else { if (reduced) { for (const spring of springs) spring.snap(); if (farRow(state)) { IY.x = H.x - 2 - ITOP.x; GY.x = H.x - 2 - GTOP.x; } place(); } dom.surface.style.willChange = 'width,height'; wake(); }
  };
  const announce = (value: string) => { dom.announcement.textContent = value; };
  const contextText = () => {
    const c = machine.context, style = c.polish.style ? `✦ ${styleName(c)}` : '· not polished';
    layers.swapText(dom.get('.style-name'), c.polish.style ? styleName(c) : '· not polished'); text('.mic-name', contextMicName(c));
    dom.get<HTMLElement>('.style-icon').hidden = dom.get<HTMLElement>('.style-chevron').hidden = !c.polish.style;
    dom.get<HTMLElement>('.mic-dot').classList.toggle('bad', !c.mic.ok); text('.engine-chip', c.engine.short_name);
    dom.get<HTMLElement>('.mic-chip').title = c.mic.tooltip;
    dom.get<HTMLElement>('.style-chip').classList.toggle('raw', !c.polish.style);
    if (pick) return; // Defer chip ink until its panel returns, then cross-fade it.
    const labels = quick ? [quick.polish.current, quick.engine.current, quick.mic_ok ? quick.mic.current : 'No mic', quick.language.current] : [style.replace(/^✦ /, ''), c.engine.short_name, c.mic.ok ? contextMicName(c) : 'No mic', c.language.label];
    for (const [index, selector] of ['.peek-style', '.peek-engine', '.peek-mic', '.peek-language'].entries()) layers.swapText(dom.get(`${selector} .quick-label`), labels[index]);
    const micOk = quick?.mic_ok ?? c.mic.ok;
    dom.get<HTMLElement>('.peek-mic').classList.toggle('bad', !micOk);
    const hint = dom.get<HTMLElement>('.peek-hint'); hint.replaceChildren();
    if (!micOk) hint.textContent = 'No mic — pick one below';
    else {
      hint.append((quick?.shortcut_caps.mode ?? c.mode) === 'hold' ? 'Hold ' : 'Press ');
      for (const key of quick?.shortcut_caps.keys ?? shortcut.split('+')) { const cap = document.createElement('kbd'); cap.className = 'kc'; cap.textContent = key === 'Space' ? 'Space' : formatKeyForDisplay(key, deps.mac); hint.append(cap); }
      hint.append(' to talk');
    }
    const record = dom.get<HTMLButtonElement>('.record');
    record.classList.toggle('bad', !micOk);
    record.innerHTML = icon(micOk ? 'mic' : 'mic-off', 16);

  };
  const foldStart = () => { if (startTimer !== undefined) deps.clearTimeout(startTimer); startTimer = deps.setTimeout(() => { startTimer = undefined; if (!pick) machine.foldStart(); sync(); }, 800); };
  const rest = () => { pick = undefined; timers(); machine.rest(); cancelPending = stopPending = false; sync(); };
  const flash = (kind: 'too_short' | 'nospeech' | 'error', message = '') => {
    if (machine.state === kind && machine.error === message) return;
    timers(); machine.clearStream(); machine.hint = false; machine.state = kind; machine.error = message;
    announce(kind === 'too_short' ? shortCopy(machine.context) : kind === 'nospeech' ? 'No speech heard' : message);
    feedbackTimer = deps.setTimeout(() => { feedbackTimer = undefined; rest(); }, kind === 'error' ? 1500 : kind === 'too_short' ? 1400 : 30000); sync();
  };
  const start = () => {
    if (machine.state === 'copied' || machine.state === 'no_permission') {
      const permission=machine.state==='no_permission';
      stack.push({key:machine.state,title:permission ? 'Allow Accessibility to paste automatically' : copiedTitle(deps.mac),sub:permission ? `Your words are copied — press ${deps.mac ? '⌘V' : 'Ctrl+V'} for now` : `${machine.context.app.name || 'The app'} didn't accept the paste`,tone:'amber',glyph:'clipboard',duration:null,actions:permission ? [{label:deps.mac ? 'Open System Settings' : 'Open Privacy settings',call:{command:'island_action',action:'open_accessibility'}}] : []},undefined,false);
    }
    pick = undefined;
    stack.tuck(true);
    if (!machine.start(performance.now())) return;
    timers(); cancelPending = stopPending = false; glyph.wave.reset(); waveSeconds = 0; announce('');
    if (machine.state === 'start') foldStart(); sync();
  };
  const context = (value: DictationContext) => {
    if (!machine.contextEvent(value)) return;
    contextText();
    appIcon.set(value.app.name, value.app.icon_key, deps.icon);
    if (machine.state === 'start') foldStart(); sync();
  };
  const pointer = (p: PillPointer) => {
    const now = performance.now();
    if (dom.surface.hidden && !stack.present) { hovering = false; if (hoverTimer !== undefined) deps.clearTimeout(hoverTimer); hoverTimer = undefined; return; }
    const bounds = dom.surface.getBoundingClientRect();
    // P1 coordinates are logical webview coordinates. Never depend on DOM mousemove.
    const inside = p.inside && (p.x === undefined || p.y === undefined || (p.x >= bounds.left - (machine.state === 'rest' ? 4 : 0) && p.x <= bounds.right + (machine.state === 'rest' ? 4 : 0) && p.y >= bounds.top - (machine.state === 'rest' ? 8 : 0) && p.y <= bounds.bottom));
    const speed = pointerTime && p.x !== undefined && p.y !== undefined ? Math.hypot(p.x - pointerX, p.y - pointerY) / Math.max(4, now - pointerTime) * 1000 : 0;
    if (pick && p.x !== undefined && p.y !== undefined) pickGuard.pointer(p.x, p.y);
    pointerX = p.x ?? pointerX; pointerY = p.y ?? pointerY; pointerTime = now;
    if (inside && !hovering) entered = now;
    hovering = inside; if (!inside) slowSince = 0; dom.surface.classList.toggle('hovered', inside);
    if (hoverTimer !== undefined) deps.clearTimeout(hoverTimer); hoverTimer = undefined;
    if (machine.state === 'rest' && inside && !pick) {
      if (speed >= 50 || !slowSince) slowSince = now;
      // Native events are movement-only. A fast sample resets dwell; no later sample means the pointer stopped.
      hoverTimer = deps.setTimeout(() => { hoverTimer = undefined; if (hovering && machine.state === 'rest' && !pick) { machine.state = 'peek'; sync(); } }, Math.max(0, 450 - (now - slowSince)));
    }
    else if ((machine.state === 'peek' || pick) && !inside) hoverTimer = deps.setTimeout(() => { hoverTimer = undefined; if (!hovering) { if (pick?.from === 'start') closePick(); else rest(); } }, 400);
    stack.pointer(p.x,p.y);
    if (machine.state === 'pasted') {
      if (feedbackTimer !== undefined) { deps.clearTimeout(feedbackTimer);feedbackTimer=undefined;pastedLeft=Math.max(0,pastedLeft-(now-pastedSince)); }
      if (!inside) { pastedSince=now;feedbackTimer=deps.setTimeout(rest,pastedLeft); }
      dom.get<HTMLElement>('.hold i').style.animationPlayState=inside ? 'paused' : 'running';
      sync();
    }
    if (machine.recording) sync();
    const buttons = dom.surface.querySelectorAll<HTMLButtonElement>('button');
    for (const button of buttons) { const b = button.getBoundingClientRect(); button.classList.toggle('hov', inside && p.x !== undefined && p.y !== undefined && p.x >= b.left && p.x <= b.right && p.y >= b.top && p.y <= b.bottom); }
  };
  const closePick = () => {
    if (pickTimer !== undefined) deps.clearTimeout(pickTimer); pickTimer = undefined;
    const from = pick?.from; pick = undefined;
    if (from === 'start' && machine.state === 'start') foldStart();
    sync(); contextText(); wake();
  };
  const renderPick = () => {
    if (!pick || !quick) return;
    const list = dom.get<HTMLElement>('.pick-list'); list.replaceChildren();
    text('.pick-title', quickTitles[pick.kind]);
    const kind = pick.kind;
    const rows = [...quickRows(quick, kind)];
    if (kind === 'polish' && pick.from === 'start' && quick.polish_context?.overridden) rows.push({id:'header:app-style',label:`${quick.polish_context.app_name} uses ${styleName(machine.context)}`,checked:false,disabled:true});
    for (const row of rows) {
      const button = document.createElement('button'); button.type = 'button'; button.className = 'pick-option';
      button.disabled = row.disabled === true || quickPending;
      button.setAttribute('role', 'option'); button.setAttribute('aria-selected', String(row.checked));
      button.dataset.id = row.id;
      const label = document.createElement('span'); label.textContent = row.label;
      const check = document.createElement('span'); check.className = 'pick-check'; check.textContent = '✓'; check.hidden = !row.checked;
      button.append(label, check); list.append(button);
      button.onclick = e => {
        e.stopPropagation();
        if (!pick || quickPending || !pickGuard.allows(performance.now()) || !deps.actions.quickSet) return;
        const active = pick; quickPending = true; renderPick(); dom.get<HTMLElement>('.pick-error').hidden = true;
        void deps.actions.quickSet(kind, row.id).then(() => {
          if (destroyed || pick !== active || !quick) return;
          const confirmed = quickRows(quick, kind).some(item => item.id === row.id && item.checked);
          for (const item of quickRows(quick, kind)) if (!item.id.startsWith('header:')) item.checked = item.id === row.id;
          if (!confirmed) quick[kind].current = row.label;
          if (kind === 'mic' && !confirmed) quick.mic_ok = true;
          renderPick();
          if (pickTimer !== undefined) deps.clearTimeout(pickTimer);
          pickTimer = deps.setTimeout(() => { pickTimer = undefined; if (pick === active) closePick(); }, 350);
        }).catch(() => { if (!destroyed && pick === active) dom.get<HTMLElement>('.pick-error').hidden = false; })
          .finally(() => { quickPending = false; if (!destroyed && pick === active) renderPick(); });
      };
    }
  };
  const openPick = (kind: QuickKind, chip: HTMLElement) => {
    if (!quick || !deps.actions.quickSet || !['peek', 'start'].includes(machine.state)) return;
    if (machine.state === 'start' && (kind !== 'polish' || !machine.context.polish.style)) return;
    if (startTimer !== undefined) { deps.clearTimeout(startTimer); startTimer = undefined; }
    if (hoverTimer !== undefined) { deps.clearTimeout(hoverTimer); hoverTimer = undefined; }
    const rows = quickRows(quick, kind);
    const placement = pickPlacement(chip.getBoundingClientRect(), rows, top, dom.root.clientHeight || 420);
    pick = { kind, from: machine.state === 'start' ? 'start' : 'peek', width: Math.max(300, W.to), ...placement };
    pickGuard.open(performance.now(), pointerX, pointerY);
    dom.get<HTMLElement>('.pick-error').hidden = true;
    renderPick(); sync();
    // Scroll large lists so the selected row remains visible.
    dom.get<HTMLElement>('.pick-list').scrollTop = placement.scrollTop;
  };
  for (const [i, selector] of ['.peek-style', '.peek-engine', '.peek-mic', '.peek-language'].entries()) {
    const chip = dom.get<HTMLButtonElement>(selector); chip.onclick = e => { e.stopPropagation(); openPick(quickKinds[i], chip); };
  }
  dom.get<HTMLButtonElement>('.style-chip').onclick = e => { e.stopPropagation(); openPick('polish', dom.get('.style-chip')); };
  dom.get<HTMLButtonElement>('.pick-back').onclick = e => { e.stopPropagation(); closePick(); };
  dom.get<HTMLButtonElement>('.peek-gear').onclick = e => { e.stopPropagation(); void deps.actions.openSettings?.().catch(() => {}); };
  const keydown = (e: KeyboardEvent) => { if (e.key === 'Escape') { if (pick) closePick(); else if (machine.state === 'peek') rest(); } };
  window.addEventListener('keydown', keydown, { passive: true });
  dom.get<HTMLButtonElement>('.cancel').onclick = () => {
    if (cancelPending) return; cancelPending = true; sync();
    void deps.actions.cancel().catch(() => { if (!destroyed) { cancelPending = false; sync(); } });
  };
  dom.get<HTMLButtonElement>('.stop').onclick = () => {
    if (stopPending) return; stopPending = true; sync();
    void deps.actions.stop().catch(() => { if (!destroyed) { stopPending = false; sync(); } });
  };
  dom.get<HTMLButtonElement>('.note-dismiss').onclick = rest;
  dom.get<HTMLButtonElement>('.privacy-settings').onclick = () => { void deps.actions.openPrivacySettings?.().catch(() => {}); };
  dom.get<HTMLButtonElement>('.original').onclick = () => {
    const generation=machine.generation;
    void deps.actions.original?.().then(result => {
      if (!destroyed && machine.state === 'pasted' && machine.generation === generation && result === 'copied') {text('.done-label','Original copied');announce('Original copied');}
    }).catch(() => {});
  };
  dom.get<HTMLButtonElement>('.skip').onclick = () => { void deps.actions.skip?.(); };
  dom.get<HTMLButtonElement>('.dismiss').onclick = () => { rest(); if (mode === 'when_recording') void deps.actions.dismiss(); };
  dom.surface.onclick = e => {
    const target = e.target as HTMLElement;
    if (pick || target.closest('.chip')) return;
    if (machine.recording) {
      if (!target.closest('button') && !stopPending) {
        stopPending = true; sync();
        void deps.actions.stop().catch(() => { if (!destroyed) { stopPending = false; sync(); } });
      }
      return;
    }
    if (clickPending || machine.state !== 'rest' && machine.state !== 'peek') return;
    if (performance.now() - entered < 120) return;
    clickPending = true;
    // Backend owns generation and returns whether this call started a take. Real events drive rendering.
    void deps.actions.start().catch(() => { if (!destroyed) stack.push(noticeCard({kind:'recording_failed'})); sync(); }).finally(() => { clickPending = false; });
  };
  const motionChanged = () => { reduced = media.matches; sync(); };
  media.addEventListener('change', motionChanged);
  applyPillGeometry(dom.root, geometry); contextText(); sync();
  return {
    machine,
    quickOptions(value: QuickOptions) { if (!value?.polish || !value.engine || !value.shortcut_caps) return; quick = value;
      if (machine.recording && value.polish_context?.generation === machine.context.generation) {
        machine.context.polish.style = value.polish_context.style;
        machine.context.polish.will_run = value.polish_context.will_run;
      }
      contextText(); if (pick) renderPick(); sync(); },
    closeQuick() { if (pick) closePick(); else if (machine.state === 'peek') rest(); },
    get feedbackActive() { return stack.active; },
    settings(value: PillSettings) { shortcut = value.recording_mode === 'push_to_talk' ? value.ptt_hotkey ?? shortcut : value.hotkey ?? shortcut; contextText(); mode = value.pill_indicator_mode ?? 'when_recording'; enabledStream = value.transcription_mode === 'live_preview' || value.streaming_preview_enabled === true; if (!enabledStream) machine.clearStream(); sync(); },
    feedback(card: FeedbackCard, handoff = true, announce = true) {
      const origin = handoff && !machine.recording && machine.state !== 'rest' ? dom.surface.getBoundingClientRect() : undefined;
      stack.push(card, origin, announce);
      if (handoff && !machine.recording) rest();
    },
    blocked(value: DictationBlocked) { if (machine.observeGeneration(value.generation)) this.feedback(blockedCard(value,deps.mac),value.kind !== 'starting_up',!(value.kind==='accessibility_off' && machine.state==='no_permission')); },
    recovery(value: DictationRecovery) { if (machine.observeGeneration(value.generation)) this.feedback(recoveryCard(value,performance.now())); },
    note(value: DictationNote) { if (machine.observeGeneration(value.generation)) this.feedback(noteCard(value),false); },
    tooShort(value: RecordingTooShort) { if (machine.observeGeneration(value.generation)) this.feedback(shortCard(value)); },
    notice(value: IslandNotice) {
      if(value.kind==='finishing' && machine.processing) {
        finishing=true;if(finishingTimer!==undefined)deps.clearTimeout(finishingTimer);
        finishingTimer=deps.setTimeout(()=>{finishingTimer=undefined;finishing=false;sync();},600);sync();
      } else this.feedback(noticeCard(value),false);
    },
    expired(id: string) { stack.remove(`recovery:${id}`); },
    clearNotice(kind: IslandNotice['kind']) { stack.remove(`notice:${kind}`); },
    geometry(value: PillGeometry) { geometry = value; stack.setGeometry(value); top = value.anchor.startsWith('top-'); applyPillGeometry(dom.root, value); sync(); },
    context, pointer, start, rest, flash,
    level(value: PillAudioLevel) {
      if (!machine.recording || !machine.observeGeneration(value.generation)) return;
      glyph.wave.sample(value.level);
      if (reduced) { glyph.wave.advance((performance.now() - machine.startedAt) / 1000, 1 / 60); syncTime(performance.now()); glyph.draw(waveSeconds, GW.x, true); }
    },
    stream(value: TranscriptionStreamEvent) { if (enabledStream && machine.stream(value)) sync(); },
    work(state: 'transcribing' | 'polishing') { pick = undefined; if (startTimer !== undefined) deps.clearTimeout(startTimer); startTimer = undefined; machine.work(state, performance.now()); sync(); },
    outcome(value: PasteOutcomePayload) {
      pick = undefined;
      if (machine.terminal && machine.state === value.outcome && machine.words === value.words) return;
      if (!machine.outcome(value)) return;
      polished = value.polished === true; pastedLeft=2400;pastedSince=performance.now();text('.done-label','Pasted');
      timers(); announce(value.outcome === 'pasted' ? `Pasted · ${value.words} words` : value.outcome === 'no_permission' ? 'Copied — allow Accessibility to paste automatically' : copiedTitle(deps.mac));
      if (value.outcome === 'pasted') dom.get<HTMLElement>('.hold').replaceChildren(document.createElement('i'));
      if (value.outcome !== 'pasted') copiedTimer=deps.setTimeout(() => {
        copiedTimer=undefined;
        stack.push({key:value.outcome,title: value.outcome === 'no_permission' ? 'Allow Accessibility to paste automatically' : copiedTitle(deps.mac),sub:value.outcome === 'no_permission' ? `Your words are copied — press ${deps.mac ? '⌘V' : 'Ctrl+V'} for now` : `${machine.context.app.name || 'The app'} didn't accept the paste`,tone:'amber',glyph:'clipboard',duration:null,actions:value.outcome === 'no_permission' ? [{label:deps.mac ? 'Open System Settings' : 'Open Privacy settings',call:{command:'island_action',action:'open_accessibility'}}] : []},dom.surface.getBoundingClientRect(),false);
        rest();
      },2400);
      if (value.outcome === 'pasted') feedbackTimer = deps.setTimeout(() => { feedbackTimer = undefined; rest(); }, 2400);
      sync();
    },
    escapeHint() {
      if (!machine.recording && !machine.processing) return;
      machine.hint = true; dom.get<HTMLElement>('.drain').replaceChildren(document.createElement('i'));
      if (hintTimer !== undefined) deps.clearTimeout(hintTimer);
      hintTimer = deps.setTimeout(() => { hintTimer = undefined; machine.hint = false; sync(); }, 2000); sync();
    },
    destroy() { destroyed = true; window.removeEventListener('keydown', keydown); stack.destroy(); appIcon.destroy(); timers(); crossfade.clear(); if (frame !== undefined) cancelAnimationFrame(frame); media.removeEventListener('change', motionChanged); deps.hitRegions([]); host.replaceChildren(); },
  };
}
