import { recordEvent } from "@/lib/observability";
import { dotCentre, pickPlacement, PickGuard, quickKinds, quickRows, quickTitles, type QuickKind, type QuickOptions } from '@/pill/quick-settings';
import { icon } from '@/pill/icons';
import { StackView } from '@/pill/stack-view';
import { blockedCard, recoveryCard, noteCard, shortCard, noticeCard, type FeedbackCard } from '@/pill/feedback';
import type { DictationBlocked, DictationRecovery, DictationNote, RecordingTooShort, IslandNotice } from '@/types/island-events';
import { formatKeyForDisplay } from '@/lib/keyboard-normalizer';
import { createIslandDom } from '@/pill/dom';
import { Crossfade } from '@/pill/crossfade';
import { Words } from '@/pill/words';
import { Layers, layerNames } from '@/pill/layers';
import { boxFor, newBox, sameRects, type Box, type Rect } from '@/pill/shape';
import { Spring, clamp } from '@/pill/spring';
import { AppIcon } from '@/pill/app-icon';
import { Badge } from '@/pill/badge';
import { Glyph } from '@/pill/glyph';
import { OutlineClock } from '@/pill/outline-clock';
import { groupLeft, waveZone, ZONE } from '@/pill/zones';
import { IslandMachine } from '@/pill/island';
import { createPerfProbe, type PerfProbe, type PerfSink } from '@/pill/perf';
import { copiedTitle, elapsed, contextMicName, shortCopy, styleName } from '@/pill/copy';
import { islandShowsLiveWords, type DictationContext, type IslandActions, type PillPointer, type PillSettings } from '@/pill/contracts';
import type { PillAudioLevel } from '@/types';
import type { PasteOutcomePayload } from '@/types/paste-outcome';
import type { TranscriptionStreamEvent } from '@/types/streaming';
import { applyPillGeometry, type PillGeometry } from '@/pill-geometry';

export interface RendererDeps {
  staticPreview?: boolean;
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
  let reduced = deps.staticPreview || media.matches, mode: PillSettings['pill_indicator_mode'] = 'when_recording';
  let destroyed = false, frame: number | undefined, last = 0, hovering = false, top = false;
  let finishing = false, finishingTimer: number | undefined;
  let copiedTimer: number | undefined, pickTimer: number | undefined;
  let lastState = machine.state, startTimer: number | undefined, feedbackTimer: number | undefined, hoverTimer: number | undefined;
  let slowSince = 0;
  let perf: PerfProbe | undefined; // debug-only measurement (VOICETYPR_PERF=1); undefined otherwise
  // A start that never went live (cancelled or failed while pending) reports no frames.
  const endTake = () => { if (machine.pending) perf?.abort(); else if (machine.recording) perf?.end(machine.generation); };
  let entered = 0, pointerX = 0, pointerY = 0, pointerTime = 0;
  let buttonRects: Array<{ button: HTMLButtonElement; rect: DOMRect }> | undefined, lastLayoutKey = '';
  let closeHold = 0;
  let waveSeconds = 0, cancelPending = false, stopPending = false;
  let enabledStream = false, displayedSeconds = -1, timerText = '0:00';
  let badgeFromX = 25, badgeFromY = 56, shortcut = deps.mac ? 'Alt+Space' : 'Ctrl+Space';
  const timerEl = dom.get<HTMLElement>('.pill-timer'), startTimeEl = dom.get<HTMLElement>('.start-time'), workTimeEl = dom.get<HTMLElement>('.work-time');
  const timerElements = [timerEl, startTimeEl, workTimeEl];
  const rowEl = dom.get<HTMLElement>('.listening-row'), ctlEl = dom.get<HTMLElement>('.ctl');
  // The Pasted card's own outline is its 2.4 s countdown (it pauses while hovered).
  const pastedClock = new OutlineClock('sage'); dom.get('.done').append(pastedClock.el);
  const rowLayers = ['row', 'work', 'note'].map(name => layers.entries[name as 'row' | 'work' | 'note'].el);
  const timers = () => { for (const id of [startTimer, feedbackTimer, hoverTimer, copiedTimer, finishingTimer, pickTimer]) if (id !== undefined) deps.clearTimeout(id); startTimer = feedbackTimer = hoverTimer = copiedTimer = finishingTimer = pickTimer = undefined; finishing = false; };
  const text = (selector: string, value: string) => { const el = dom.get<HTMLElement>(selector); if (el.textContent !== value) el.textContent = value; if (el.matches('.done-label,.note-text')) el.title = value; };
  const inlineWidth = (selector: string) => {
    const range = document.createRange(); range.selectNodeContents(dom.get(selector));
    const width = typeof range.getBoundingClientRect === 'function' ? range.getBoundingClientRect().width : 0;
    range.detach(); return width;
  };
  // Hit regions come from the spring values, never from the DOM, and are sent only when they change.
  let sentRects: Rect[] = [];
  const hit = (settled = true) => {
    const rects: Rect[] = [];
    if (!dom.surface.hidden) {
      const b = shapeNow();
      if (machine.state === 'rest') rects.push({ x: b.left - 4, y: b.top - 8, width: b.width + 8, height: 20 });
      else rects.push({ x: b.left, y: b.top, width: b.width, height: b.height });
    }
    for (const rect of stack.rects()) rects.push(rect);
    // Sub-pixel jitter is not worth an IPC while moving; the settled shape is always sent exactly.
    if (sameRects(rects, sentRects, settled ? 0 : .01)) return;
    sentRects = rects; deps.hitRegions(rects);
  };
  const wake = () => { if (!deps.staticPreview && !destroyed && frame === undefined) { last = performance.now(); frame = requestAnimationFrame(tick); } };
  const syncTime = (now: number) => {
    if (machine.recording) waveSeconds = Math.max(0, (now - machine.startedAt) / 1000);
    // While starting the clock waits at 0:00; the bars idle on the same time base.
    const seconds = machine.pending ? 0 : Math.floor(machine.recording ? waveSeconds : machine.frozenSeconds);
    if (seconds === displayedSeconds) return; displayedSeconds = seconds;
    const t = elapsed(seconds); timerText = t;
    for (const el of timerElements) if (el.textContent !== t) el.textContent = t;
    // A longer time (10:00) narrows the wave's zone so the gap to the timer stays the same.
    if (machine.recording && machine.state !== 'start') { const w = waveZone(machine.width, t).width; if (GW.to !== w) { GW.to = w; if (reduced) GW.snap(); } }
  };
  // ---- Geometry (plan 082 F6) ----
  // `layout()` runs on state change only: it sizes the fixed target box that holds the content and places
  // the shadow. `paint()` runs per spring frame: it resizes the surface (the clip, which has no
  // size-dependent descendants) and moves content by translate/scale only. Nothing here measures the DOM.
  const target = newBox(), cur = newBox();
  let boxPanel = false, dotAt = dotCentre(geometry), iconSide = 18, peekShift = 0;
  interface Pose { w: number; h: number; poff: number; ix: number; iy: number; is: number; gx: number; gy: number }
  const targetPose: Pose = { w: 12, h: 12, poff: 0, ix: 0, iy: 0, is: 0, gx: 0, gy: 0 }, nowPose: Pose = { ...targetPose };
  const fillPose = (p: Pose, goal: boolean) => {
    const v = (spring: Spring) => goal ? spring.to : spring.x;
    p.w = Math.max(12, v(W)); p.h = Math.max(12, v(H)); p.poff = v(POFF); p.ix = v(IX); p.iy = v(IY); p.is = Math.max(0, v(IS)); p.gx = v(GX); p.gy = v(GY);
  };
  const iconCentreY = (p: Pose) => top ? p.iy : p.h - 2 - p.iy;
  const glyphTop = (p: Pose) => top ? p.gy : p.h - 2 - p.gy - 24;
  const rowEdge = (p: Pose) => Math.max(0, p.iy - 16);
  /** Top-hugging layers keep their distance from the shape's top edge; the rest keep it from the bottom edge (see pill.css). */
  const huggingTop = (name: string) => top ? name !== 'start' && name !== 'notice' : name === 'start' || name === 'notice' || name === 'done';
  const put = (el: HTMLElement, name: string, value: string) => { if (el.style.getPropertyValue(name) !== value) el.style.setProperty(name, value); };
  const nudge = (el: HTMLElement, x: number, y: number) => put(el, 'translate', `${x}px ${y}px`);
  /** The live shape in viewport px, from the current spring values. Shared by paint, hit regions and hover. */
  const shapeNow = (): Box => {
    fillPose(nowPose, false);
    return boxFor(geometry, boxPanel, dotAt, top, nowPose.w, nowPose.h, nowPose.poff, cur);
  };
  const layout = () => {
    const panel = machine.state === 'peek' || !!pick;
    boxPanel = panel; dotAt = dotCentre(geometry);
    fillPose(targetPose, true);
    boxFor(geometry, panel, dotAt, top, targetPose.w, targetPose.h, targetPose.poff, target);
    // Content is laid out at the target padding box (surface minus its 1px border) and never resized per frame.
    dom.content.style.width = `${Math.max(0, targetPose.w - 2)}px`; dom.content.style.height = `${Math.max(0, targetPose.h - 2)}px`;
    const shadow = dom.shadow.style;
    shadow.left = `${target.left}px`; shadow.top = `${target.top}px`; shadow.width = `${target.width}px`; shadow.height = `${target.height}px`;
    shadow.borderRadius = `${Math.max(6, R.to)}px`;
    if (IS.to > .5) iconSide = IS.to;
    const icon = dom.icon.style;
    icon.width = icon.height = `${iconSide}px`;
    icon.left = `${targetPose.ix - iconSide / 2}px`; icon.top = `${iconCentreY(targetPose) - iconSide / 2}px`;
    icon.borderRadius = `${Math.max(3, iconSide * (deps.mac ? .28 : .22))}px`;
    dom.canvas.style.left = `${targetPose.gx}px`; dom.canvas.style.top = `${glyphTop(targetPose)}px`;
    dom.canvas.style.width = '260px'; dom.canvas.style.height = '24px';
    pastedClock.size(targetPose.w, targetPose.h, R.to);
    rowEl.style.width = `${Math.max(0, W.to - 2)}px`; rowEl.style.paddingRight = `${12 + HC.to}px`;
    ctlEl.style.left = `${Math.max(0, W.to - HC.to - 14)}px`; ctlEl.inert = HC.to === 0;
    const edge = rowEdge(targetPose);
    for (const el of rowLayers) { el.style.left = `${targetPose.ix - 21}px`; el.style[top ? 'top' : 'bottom'] = `${edge}px`; el.style[top ? 'bottom' : 'top'] = ''; }
    if (panel) {
      const record = dom.get<HTMLElement>('.record');
      record.style.left = `${dotAt.x - target.left - 14}px`; record.style.top = `${dotAt.y - target.top - 14}px`; record.style.transform = 'none';
    }
  };
  const paint = () => {
    shapeNow();
    const surface = dom.surface;
    put(surface, 'width', `${nowPose.w}px`); put(surface, 'height', `${nowPose.h}px`); put(surface, 'border-radius', `${Math.max(6, R.x)}px`);
    if (boxPanel) {
      put(surface, 'transform', 'none'); put(surface, 'left', `${cur.left}px`); put(surface, 'top', `${cur.top}px`);
    } else { put(surface, 'transform', ''); put(surface, 'left', ''); put(surface, 'top', ''); }
    // The shadow keeps its target-sized box and follows the shape by transform.
    dom.shadow.style.transform = `translate(${cur.left - target.left}px,${cur.top - target.top}px) scale(${cur.width / target.width},${cur.height / target.height})`;
    const grow = nowPose.h - targetPose.h, edgeShift = rowEdge(nowPose) - rowEdge(targetPose), ixShift = nowPose.ix - targetPose.ix;
    for (const name of layerNames) {
      const el = layers.entries[name].el;
      if (name === 'row' || name === 'work' || name === 'note') nudge(el, ixShift, top ? edgeShift : grow - edgeShift);
      else nudge(el, 0, huggingTop(name) ? 0 : grow);
    }
    peekShift = huggingTop('peek') ? 0 : grow;
    // The record button stays on the native dot while its panel morphs.
    if (boxPanel) nudge(dom.get<HTMLElement>('.record'), target.left - cur.left, target.top - cur.top - peekShift);
    const size = nowPose.is;
    nudge(dom.icon, ixShift, iconCentreY(nowPose) - iconCentreY(targetPose));
    put(dom.icon, 'scale', String(size / iconSide)); dom.icon.style.opacity = String(clamp(size / 18));
    dom.icon.hidden = size < .1 || boxPanel;
    nudge(dom.canvas, nowPose.gx - targetPose.gx, glyphTop(nowPose) - glyphTop(targetPose));
    ctlEl.style.opacity = String(clamp(HC.x / 62));
  };
  function tick(now: number) {
    frame = undefined;
    if (destroyed) return;
    const dt = Math.min(.05, Math.max(0, (now - last) / 1000)); last = now;
    if (perf && machine.recording && !machine.pending && !reduced) perf.frame(now);
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
    if (shapeMoving) paint();
    if (moving) buttonRects = undefined;
    glyph.draw(waveSeconds, Math.max(0, GW.x), reduced);
    const landed = machine.state === 'rest' && W.x < 12.5 && H.x < 12.5;
    stack.tuck(mode === 'never' || !landed);
    stack.setGeometry(machine.recording || machine.processing ? {...geometry,anchorY:geometry.anchorY+(top ? 1 : -1)*Math.max(0,H.x-12)} : geometry);
    moving = stack.step(dt,reduced,landed,mode !== "never") || moving;
    hit(!moving);
    if (!deps.staticPreview && (moving || (!dom.surface.hidden && machine.recording && !reduced))) frame = requestAnimationFrame(tick);
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
    if (state === 'peek' && dom.root.dataset.state !== 'peek') recordEvent({ name: 'island_peek_opened', properties: {} });
    const rest = state === 'rest';
    dom.root.dataset.state = pick ? 'pick' : rest ? 'idle' : state;
    dom.root.dataset.preview = String(state === 'live');
    dom.root.dataset.starting = String(machine.pending);
    dom.root.dataset.platform = deps.mac ? 'macos' : 'windows';
    dom.root.dataset.reducedMotion = String(reduced);
    dom.surface.hidden = mode === 'never' || (rest && mode !== 'always' && !stack.present);
    const owns = mode !== 'never' && (stack.present || machine.terminal || ['too_short','nospeech','error'].includes(state));
    if (owns !== feedbackOwned) { feedbackOwned=owns; void deps.actions.feedbackVisible?.(owns).catch(() => {}); }
    dom.root.dataset.visible = String(!dom.surface.hidden);
    dom.get<HTMLElement>('.pill-rest-dot').hidden = !rest;
    dom.get<HTMLElement>('.core').classList.toggle('bad', !machine.context.mic.ok);
    const ctl = hovering && state === 'listening' && !stopPending;
    HC.to = ctl ? 62 : 0;
    W.to = machine.width + (ctl ? 62 : 0);
    const showDoneActions = state === 'pasted' && polished && hovering && !!deps.actions.original;
    H.to = showDoneActions ? 70 : machine.height;
    dom.get<HTMLElement>('.done-actions').hidden = !showDoneActions;
    dom.get<HTMLButtonElement>('.original').hidden = !polished; R.to = rest ? 6 : H.to > 34 ? 20 : 17;
    IS.to = rest ? 0 : state === 'start' ? 28 : 18;
    IX.to = rest ? 5 : state === 'start' ? 28 : 21;
    IY.to = rest ? 5 : 16;
    const zone = waveZone(machine.width, timerText);
    GX.to = state === 'start' ? 52 : machine.recording ? zone.x : ZONE.glyphMin; GY.to = 4;
    GW.to = machine.recording ? state === 'start' ? machine.width - 105 : zone.width : 22;
    layers.target(pick ? ['pick'] : machine.layers); glyph.target(state, true);
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
      // Incoming details must be measured at their own target width, not the previous listening box.
      const start = layers.entries.start.el, oldWidth = start.style.width;
      start.style.width = `${Math.max(0,W.to - 2)}px`;
      H.to = start.scrollHeight ? start.scrollHeight + 2 : machine.height;
      start.style.width = oldWidth;
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
    // Terminal cards: glyph, title and count sit together, centred between the app icon and the right edge.
    if (state === 'pasted' || state === 'copied' || state === 'no_permission' || state === 'error') {
      const pasted = state === 'pasted', label = inlineWidth(pasted ? '.done-label' : '.notice-title'), count = inlineWidth(pasted ? '.done-count' : '.notice-count');
      if (label) {
        const left = groupLeft(W.to - ZONE.border, ZONE.glyph + label + (count ? (pasted ? 5 : 20) + count : 0));
        GX.to = left; put(layers.entries[pasted ? 'done' : 'notice'].el, '--grp-x', `${left + ZONE.glyph}px`);
      }
    }
    if (state === 'peek') { W.to = Math.min(438, Math.max(378, dom.get<HTMLElement>('.peek-chips').scrollWidth + 22)); H.to = 70; }
    if (pick) { W.to = pick.width; H.to = pick.height; }
    POFF.to = pick ? pick.y - (top ? dotCentre(geometry).y - 17 : dotCentre(geometry).y - pick.height + 17) : 0;
    ITOP.to = state === 'start' ? 26 : 16; GTOP.to = state === 'start' ? 38 : 28;
    if (farRow(state)) { IY.to = H.to - 2 - ITOP.to; GY.to = H.to - 2 - GTOP.to; }
    syncTime(performance.now());
    const layoutKey = `${state}|${pick ? 1 : 0}|${W.to}|${H.to}|${HC.to}`;
    if (layoutKey !== lastLayoutKey) { lastLayoutKey = layoutKey; buttonRects = undefined; }
    dom.shadow.hidden = dom.surface.hidden;
    if (dom.surface.hidden && !stack.present) {
      if (frame !== undefined) cancelAnimationFrame(frame); frame = undefined; words.clear(); crossfade.clear();
      for (const spring of springs) spring.snap(); layers.step(1, true); glyph.step(1, true); layout(); paint(); hit();
    }
    else {
      if (reduced) { for (const spring of springs) spring.snap(); if (farRow(state)) { IY.x = H.x - 2 - ITOP.x; GY.x = H.x - 2 - GTOP.x; } }
      // New targets, same live springs: content is re-expressed against the new box so nothing jumps.
      layout(); paint(); dom.surface.style.willChange = 'width,height'; wake();
    }
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
  const rest = () => { endTake(); stack.dropLive(); pick = undefined; timers(); machine.rest(); cancelPending = stopPending = false; sync(); };
  const flash = (kind: 'too_short' | 'nospeech' | 'error', message = '') => {
    endTake(); machine.pending = false; stack.dropLive();
    if (machine.state === kind && machine.error === message) return;
    timers(); machine.clearStream(); machine.state = kind; machine.error = message;
    announce(kind === 'too_short' ? shortCopy(machine.context) : kind === 'nospeech' ? 'No speech heard' : message);
    feedbackTimer = deps.setTimeout(() => { feedbackTimer = undefined; rest(); }, kind === 'error' ? 1500 : kind === 'too_short' ? 1400 : 30000); sync();
  };
  /** Keypress: Starting was published, the mic is not open yet. Show the idle listening layout at 0:00. */
  const starting = (generation?: number) => {
    if (machine.recording) return;
    if (generation !== undefined && !machine.observeGeneration(generation)) return;
    stack.resolveMicStatus('new_take', generation);
    stack.dropLive();
    if (machine.state === 'copied' || machine.state === 'no_permission') {
      const permission=machine.state==='no_permission';
      stack.push({key:machine.state,title:permission ? 'Allow Accessibility to paste automatically' : copiedTitle(deps.mac),sub:permission ? `Your words are copied — press ${deps.mac ? '⌘V' : 'Ctrl+V'} for now` : `${machine.context.app.name || 'The app'} didn't accept the paste`,tone:'amber',glyph:'clipboard',duration:null,actions:permission ? [{label:deps.mac ? 'Open System Settings' : 'Open Privacy settings',call:{command:'island_action',action:'open_accessibility'}}] : []},undefined,false);
    }
    pick = undefined;
    stack.tuck(true);
    if (!machine.arm(performance.now(), generation)) return;
    timers(); cancelPending = stopPending = false; glyph.wave.reset(); waveSeconds = 0; displayedSeconds = -1; announce('');
    sync();
    perf?.begin();
  };
  const start = (generation?: number) => {
    if (machine.pending) {
      // Recording began. Clock and bars go live from here; a context that already arrived keeps its start card.
      if (!machine.start(performance.now(), generation)) return;
      stack.resolveMicStatus('recording', generation);
      glyph.wave.reset(); waveSeconds = 0; displayedSeconds = -1; announce('');
      if (machine.state === 'start') foldStart();
      sync();
      return;
    }
    if (machine.recording) return;
    if (generation !== undefined && !machine.observeGeneration(generation)) return;
    stack.resolveMicStatus('new_take', generation);
    stack.resolveMicStatus('recording', generation);
    stack.dropLive();
    if (machine.state === 'copied' || machine.state === 'no_permission') {
      const permission=machine.state==='no_permission';
      stack.push({key:machine.state,title:permission ? 'Allow Accessibility to paste automatically' : copiedTitle(deps.mac),sub:permission ? `Your words are copied — press ${deps.mac ? '⌘V' : 'Ctrl+V'} for now` : `${machine.context.app.name || 'The app'} didn't accept the paste`,tone:'amber',glyph:'clipboard',duration:null,actions:permission ? [{label:deps.mac ? 'Open System Settings' : 'Open Privacy settings',call:{command:'island_action',action:'open_accessibility'}}] : []},undefined,false);
    }
    pick = undefined;
    stack.tuck(true);
    if (!machine.start(performance.now(), generation)) return;
    timers(); cancelPending = stopPending = false; glyph.wave.reset(); waveSeconds = 0; announce('');
    if (machine.state === 'start') foldStart(); sync();
    perf?.begin();
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
    // P1 coordinates are logical webview coordinates. Never depend on DOM mousemove or measure the DOM.
    const bounds = shapeNow(), slack = machine.state === 'rest';
    const inside = p.inside && (p.x === undefined || p.y === undefined || (p.x >= bounds.left - (slack ? 4 : 0) && p.x <= bounds.left + bounds.width + (slack ? 4 : 0) && p.y >= bounds.top - (slack ? 8 : 0) && p.y <= bounds.top + bounds.height));
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
      pastedClock.pause(inside);
      sync();
    }
    if (machine.recording) sync();
    // Button rects are cached and re-measured only after a layout or motion change.
    buttonRects ??= [...dom.surface.querySelectorAll<HTMLButtonElement>('button')].map(button => ({ button, rect: button.getBoundingClientRect() }));
    for (const { button, rect: b } of buttonRects) button.classList.toggle('hov', inside && p.x !== undefined && p.y !== undefined && p.x >= b.left && p.x <= b.right && p.y >= b.top && p.y <= b.bottom);
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
  if (!deps.staticPreview) window.addEventListener('keydown', keydown, { passive: true });
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
  if (!deps.staticPreview) media.addEventListener('change', motionChanged);
  applyPillGeometry(dom.root, geometry); contextText(); sync();
  return {
    machine,
    renderStatic() {
      if (!deps.staticPreview) return;
      const now = performance.now();
      for (let i = 0; i < 20; i++) tick(now + i * 50);
      paint();
    },
    quickOptions(value: QuickOptions) { if (!value?.polish || !value.engine || !value.shortcut_caps) return; quick = value;
      if (machine.recording && value.polish_context?.generation === machine.context.generation) {
        machine.context.polish.style = value.polish_context.style;
        machine.context.polish.will_run = value.polish_context.will_run;
      }
      contextText(); if (pick) renderPick(); sync(); },
    closeQuick() { if (pick) closePick(); else if (machine.state === 'peek') rest(); },
    get feedbackActive() { return stack.active; },
    settings(value: PillSettings) { shortcut = value.recording_mode === 'push_to_talk' ? value.ptt_hotkey ?? shortcut : value.hotkey ?? shortcut; contextText(); mode = value.pill_indicator_mode ?? 'when_recording'; enabledStream = islandShowsLiveWords(value); if (!enabledStream) machine.clearStream(); sync(); },
    feedback(card: FeedbackCard, handoff = true, announce = true) {
      const origin = handoff && !machine.recording && machine.state !== 'rest' ? dom.surface.getBoundingClientRect() : undefined;
      stack.push(card, origin, announce);
      if (handoff && !machine.recording) rest();
    },
    blocked(value: DictationBlocked) { if (machine.observeGeneration(value.generation)) this.feedback(blockedCard(value,deps.mac),value.kind !== 'starting_up',!(value.kind==='accessibility_off' && machine.state==='no_permission')); },
    recovery(value: DictationRecovery) {
      // Retained clips outlive their take. Their id/lease, not the active-take watermark, owns the action.
      if (!value.id || !(value.expires_in_ms > 0)) return;
      if (value.kind === 'mic_dropped_empty' && value.generation < machine.takeGeneration) return;
      this.feedback(recoveryCard(value,performance.now()),value.generation >= machine.takeGeneration);
    },
    withdrawNote(value: { generation: number; kind: "mic_silent_live" }) { stack.remove(`${value.kind}:${value.generation}`); },
    note(value: DictationNote) { if (value.kind === "mic_silent_live" && (!machine.recording || value.generation !== machine.takeGeneration)) return; if (machine.observeGeneration(value.generation)) this.feedback(noteCard(value),false); },
    tooShort(value: RecordingTooShort) { if (machine.observeGeneration(value.generation)) this.feedback(shortCard(value)); },
    notice(value: IslandNotice) {
      if (value.kind === "long_silence" && !machine.recording) return;
      if(value.kind==='finishing' && machine.processing) {
        finishing=true;if(finishingTimer!==undefined)deps.clearTimeout(finishingTimer);
        finishingTimer=deps.setTimeout(()=>{finishingTimer=undefined;finishing=false;sync();},600);sync();
      } else this.feedback(value.kind === "long_silence" && machine.recording ? {...noticeCard(value),liveGeneration:machine.takeGeneration} : noticeCard(value),false);
    },
    expired(id: string) { stack.remove(`recovery:${id}`); },
    clearNotice(kind: IslandNotice['kind']) { stack.remove(`notice:${kind}`); },
    geometry(value: PillGeometry) { geometry = value; stack.setGeometry(value); top = value.anchor.startsWith('top-'); applyPillGeometry(dom.root, value); sync(); },
    context, pointer, start, starting, rest, flash,
    /** Debug-only: the backend reports VOICETYPR_PERF=1 on a debug build. */
    attachPerf(enabled: boolean, sink: PerfSink) {
      perf?.destroy();
      perf = createPerfProbe(enabled, {
        sink, now: () => performance.now(), timeOrigin: () => performance.timeOrigin,
        raf: callback => requestAnimationFrame(callback), caf: id => cancelAnimationFrame(id),
        visible: () => !dom.surface.hidden && machine.recording,
        domNodes: () => host.querySelectorAll('*').length, stackItems: () => stack.stack.items.length,
      });
    },
    level(value: PillAudioLevel) {
      // No audio can belong to a take whose mic has not opened.
      if (!machine.recording || machine.pending || !machine.observeGeneration(value.generation)) return;
      perf?.level(performance.now(), value.generation);
      glyph.wave.sample(value.level);
      if (reduced) { glyph.wave.advance((performance.now() - machine.startedAt) / 1000, 1 / 60); syncTime(performance.now()); glyph.draw(waveSeconds, GW.x, true); }
    },
    stream(value: TranscriptionStreamEvent) { if (enabledStream && machine.stream(value)) sync(); },
    work(state: 'transcribing' | 'polishing') {
      endTake();
      const hint=stack.stack.items.find(item=>item.key===`escape_hint:${machine.generation}`);
      stack.dropLive(); pick = undefined;
      if (startTimer !== undefined) deps.clearTimeout(startTimer); startTimer = undefined;
      machine.work(state, performance.now());
      if(hint && (hint.expiresAt ?? 0)>performance.now()) stack.push({...hint,title:'Press Esc again to discard'},undefined,false);
      sync();
    },
    outcome(value: PasteOutcomePayload) {
      if (machine.terminal && machine.state === value.outcome && machine.words === value.words) return;
      if (!machine.outcome(value)) return;
      stack.dropLive(value.generation ?? machine.generation);
      pick = undefined;
      polished = value.polished === true; pastedLeft=2400;pastedSince=performance.now();text('.done-label','Pasted');
      timers(); announce(value.outcome === 'pasted' ? `Pasted · ${value.words} words` : value.outcome === 'no_permission' ? 'Copied — allow Accessibility to paste automatically' : copiedTitle(deps.mac));
      // A new outcome never inherits the previous take's pause: the clock and the dismissal timer both follow the pointer as it is now.
      pastedClock.pause(value.outcome === 'pasted' && hovering);
      if (value.outcome === 'pasted') pastedClock.start(2400);
      if (value.outcome !== 'pasted') copiedTimer=deps.setTimeout(() => {
        copiedTimer=undefined;
        stack.push({key:value.outcome,title: value.outcome === 'no_permission' ? 'Allow Accessibility to paste automatically' : copiedTitle(deps.mac),sub:value.outcome === 'no_permission' ? `Your words are copied — press ${deps.mac ? '⌘V' : 'Ctrl+V'} for now` : `${machine.context.app.name || 'The app'} didn't accept the paste`,tone:'amber',glyph:'clipboard',duration:null,actions:value.outcome === 'no_permission' ? [{label:deps.mac ? 'Open System Settings' : 'Open Privacy settings',call:{command:'island_action',action:'open_accessibility'}}] : []},dom.surface.getBoundingClientRect(),false);
        rest();
      },2400);
      if (value.outcome === 'pasted' && !hovering) feedbackTimer = deps.setTimeout(() => { feedbackTimer = undefined; rest(); }, 2400);
      sync();
    },
    escapeHint() {
      if (!machine.recording && !machine.processing) return;
      stack.push({key:`escape_hint:${machine.generation}`,title:`Press Esc again to ${machine.processing ? 'discard' : 'cancel'}`,sub:'',tone:'neutral',glyph:'back',duration:null,liveGeneration:machine.generation,expiresAt:performance.now()+2000,actions:[]});
      sync();
    },
    destroy() { destroyed = true; perf?.destroy(); perf = undefined; window.removeEventListener('keydown', keydown); stack.destroy(); appIcon.destroy(); timers(); crossfade.clear(); if (frame !== undefined) cancelAnimationFrame(frame); media.removeEventListener('change', motionChanged); deps.hitRegions([]); host.replaceChildren(); },
  };
}
