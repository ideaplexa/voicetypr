# Recording Overlay / Streaming Pill — Handy teardown

Handy's overlay is a single reused Tauri window that morphs between a **compact pill** (waveform dot + bars during recording; spinner + label while transcribing/processing) and a **Live panel** (the pill sculpted open to reveal streaming live text). It feels "lightweight and smooth" because of three disciplined engineering choices: (1) a strict **`committed`/`tentative` split** that makes the live text append-only and flicker-free; (2) a **unified card** (`.scard`) whose state changes are pure, transitionable CSS *width/border-radius* mutations rather than mount/unmount pops; and (3) a **non-activating, focus-stealing-free window** whose only high-frequency backend→frontend channel (`mic-level`) is delivered via `emit_to` on a single window label to halve the per-audio-callback WebKit dispatch cost.

This doc is a line-level teardown of that surface, scoped to the overlay itself. The Rust engine that *produces* the stream (slice 1) and the first-run onboarding screens (slice 3) are out of scope here.

---

## File map

| File | Lines | Purpose |
|---|---|---|
| `src/overlay/RecordingOverlay.tsx` | 286 | The overlay's entire React app: `OverlayState` machine, the five Tauri event listeners, the `committed`/`tentative` render path, the shared `listeningRow`/`workingRow` builders, the scroll-pin `useLayoutEffect`. |
| `src/overlay/RecordingOverlay.css` | 370 | All animation/transition: pop-in keyframes, `width`/`border-radius` morph transitions, the `grid-template-rows 0fr→1fr` open trick, caret blink, dot pulse, spinner, waveform bar easing, top-edge mask fade. |
| `src/overlay/main.tsx` | 10 | Webview entrypoint — mounts `<RecordingOverlay />` in `React.StrictMode`. |
| `src/overlay/index.html` | 27 | Transparent `<body>`/`#root` shell; loads `main.tsx`. |
| `src-tauri/src/overlay.rs` | 480 | Backend: one reused window, per-state sizing, `show_overlay_state`, the cached `OVERLAY_ENABLED` atomic, `emit_to` single-label level dispatch, fade-out timing, per-platform non-activating/focus flags. |
| `src/bindings.ts` (cited `840-989`) | — | tauri-specta-generated event/type shapes: `StreamTextEvent`, `StreamPhaseEvent`, `StreamPhase`, `StreamWorkKind`, and the `events` map. |

Supporting entry points (read for data flow, owned by other slices): `src-tauri/src/actions.rs:486-488,592` calls the `show_*_overlay` helpers; `src-tauri/src/managers/transcription.rs:1078-1092` emits the stream events.

---

## Mechanism walkthrough

### 1. Event wiring — every subscription and the state machine

The overlay mounts once and registers **five** listeners inside a single `useEffect` (`RecordingOverlay.tsx:51-117`). Two are string-keyed `"show-overlay"` / `"hide-overlay"` (custom events emitted by the Rust `show_overlay_state`/`hide_recording_overlay`); three are the specta-generated `events.*` accessors:

```tsx
// RecordingOverlay.tsx:53-79  — "show-overlay" drives the entire state machine
const unlistenShow = await listen("show-overlay", async (event) => {
  await syncLanguageFromSettings();
  try {
    const settings = await commands.getAppSettings();
    if (settings.status === "ok") {
      setPosition(settings.data.overlay_position === "top" ? "top" : "bottom");
    }
  } catch { /* keep previous placement */ }
  const overlayState = event.payload as OverlayState;
  setState(overlayState);
  if (overlayState === "recording" || overlayState === "streaming") {
    setStreamText({ committed: "", tentative: "" });
  }
  if (overlayState === "streaming") {
    setPhase("listening");
    setWorkKind("transcribing");
    setElapsed(0);
    setSession((s) => s + 1); // remount the card fresh for this session
  }
  setIsVisible(true);
});
```

The **payload of `"show-overlay"` *is* the target state** (`OverlayState` string), so the backend's `show_overlay_state` (`overlay.rs:384` `emit("show-overlay", state)`) both *shows the window* and *commands the UI state* in one event. The remaining four listeners:

```tsx
// RecordingOverlay.tsx:81-83
const unlistenHide = await listen("hide-overlay", () => { setIsVisible(false); });

// RecordingOverlay.tsx:85-95  — high-rate audio levels, ~per-audio-callback
const unlistenLevel = await listen<number[]>("mic-level", (event) => {
  const newLevels = event.payload as number[];
  const smoothed = smoothedLevelsRef.current.map((prev, i) => {
    const target = newLevels[i] || 0;
    return prev * 0.7 + target * 0.3;            // exponential smoothing, 16 buckets
  });
  smoothedLevelsRef.current = smoothed;
  setLevels(smoothed.slice(0, WAVE_BARS));         // take first 9 bars
});

// RecordingOverlay.tsx:97-105  — the streaming live text + phase
const unlistenStream = await events.streamTextEvent.listen((event) => {
  setStreamText(event.payload);
});
const unlistenPhase = await events.streamPhaseEvent.listen((event) => {
  const payload: StreamPhaseEvent = event.payload;
  setPhase(payload.phase);
  if (payload.kind) setWorkKind(payload.kind);
});
```

That is the **complete** subscription set — grep for `listen(` / `events.` / `commands.` in `RecordingOverlay.tsx` returns only these five (`53, 81, 85, 97, 101`) plus the cancel button's `commands.cancelOperation()` (`:170`). The cleanup function (`:107-113`) unhooks all five.

**The `OverlayState` machine** is `type OverlayState = "recording" | "streaming" | "transcribing" | "processing"` (`:15`). It is driven almost entirely by the *payload* of `"show-overlay"`, i.e. by *which* backend `show_*_overlay()` helper fired. The backend entry points (`actions.rs:487-488,592`):

```rust
// actions.rs:486-489  — start of recording
OverlayStyle::Live if model_supports_streaming => utils::show_streaming_overlay(app),
OverlayStyle::Live | OverlayStyle::Minimal => show_recording_overlay(app),
OverlayStyle::None => {}
// actions.rs:592  — after recording stops, before transcription result
_ => show_transcribing_overlay(app),
```

Each maps to `show_overlay_state(app_handle, "<state>")` (`overlay.rs:389-406`). Transitions are therefore **commanded, not inferred**: the overlay does not detect "recording finished" from a stream gap; `actions.rs` explicitly tells it `transcribing`, then later `processing` (`actions.rs:705`), and finally `hide-overlay`.

The **two render branches** keyed off `state`:

- `state === "streaming"` → the **Live panel** branch (`:211-259`): pill sculpted open, committed/tentative text, caret, working spinner overlayed while `phase === "working"`.
- otherwise → the **compact pill** branch (`:271-282`): exactly one row at a time — `listeningRow` for `"recording"`, `workingRow` for `"transcribing"`/`"processing"`. `const working = state === "transcribing" || state === "processing"` (`:265`).

A subtle but important invariant: while in the Live branch the **`phase`** ("listening" vs "working") is a *sub-state layered on top of `state`*, not an `OverlayState` value. `phase` comes only from `StreamPhaseEvent` (`:101-105`), so mid-stream the panel swaps its bottom control row between `listeningRow` and `workingRow` *without* the backend re-emitting `"show-overlay"` — the card stays mounted and only its footer row swaps (`:249-256`). This is what keeps the transcript "staying put under a working spinner instead of collapsing" (see the comment at `:216-218`).

### 2. Anti-flicker rendering — `committed` vs `tentative` (THE CORE)

This is the headline mechanism. The live text is rendered as **two sibling `<span>`s inside one `<p>`**, and the flicker-free property is **structural + backend-semantic, not visual**.

**The JSX** (`RecordingOverlay.tsx:237-245`):

```tsx
<p>
  <span className="committed">
    {streamText.committed ? streamText.committed + " " : ""}
  </span>
  <span className="tentative">{streamText.tentative}</span>
  {/* Drop the blinking caret once finalizing */}
  {!working && <span className="scaret" />}
</p>
```

**The contract** these spans honor is defined on the *backend* type (`bindings.ts:980-985`):

```ts
/**
 * Live transcription snapshot emitted to the overlay during a streaming run.
 * `committed` is the append-only, flicker-free prefix; `tentative` is the
 * volatile suffix the model may still rewrite.
 */
export type StreamTextEvent = { committed: string; tentative: string }
```

So `committed` is **monotonic append-only** — it only ever grows; `tentative` is fully volatile and may be rewritten wholesale on the next event. The emit helper is a plain struct construction (`transcription.rs:1086-1092`), but the *guarantee* is upstream in the streaming core (slice 1): each `StreamTextEvent` carries a committed prefix that is a strict superset of the previous one.

**Why this produces zero flicker of the committed prefix, concretely:**

1. **DOM isolation by sibling position.** The two spans are *static JSX children of `<p>`* — not a `.map()` array, so React reconciles them **by position**, not by key, and there is no key warning. Because both `<span className="committed">` and `<span className="tentative">` are *always present* on every render (committed renders `""` or text+space; tentative renders the string, possibly `""`), **neither element is ever unmounted or remounted**. On each `StreamTextEvent`, React only mutates the existing text node inside each span. The committed glyphs are repainted in place; their measured layout box never changes because the string only grows at its right edge (LTR) — so already-rendered committed characters never shift.

2. **Backend monotonicity is what makes (1) flicker-free.** If `committed` could shrink, the in-place text-node update would still work but you'd see characters *disappear* from the middle/end — a flicker. The append-only contract is the load-bearing half of the trick; the two-span split is what lets the volatile `tentative` be rewritten arbitrarily *without* touching the committed text node's layout.

3. **No visual differentiation — by design.** There is **no `.committed` CSS rule at all** (confirmed by grep: the only `.stext-cap` child rules are `.stext-cap p`, `.stext-cap .tentative`, and `.ov-stage.top .stext-cap p`). The `.tentative` rule is simply `color: inherit` (`RecordingOverlay.css:223-225`), which is effectively a no-op since it already inherits from `.stext-cap`. So committed and tentative render **identically** — same italic, same `color-mix(in srgb, var(--color-text) 90%, transparent)` (`:208`), same `font-size: 15px` (`:204`). The text reads as one continuous flow; the structural seam is invisible. [INFERENCE] This is a deliberate choice to avoid the "two-tone tentative text" look some dictation apps use; Handy bets that the append-only prefix already eliminates perceived flicker, so dimming the tentative suffix is unnecessary visual noise.

4. **The committed span pads a trailing space** (`committed + " "`) so word boundaries are preserved as text crosses the committed→tentative boundary; when `committed` is empty it renders the empty string (the span still exists, keeping reconciliation stable).

**Scroll-pin keeps the newest line in view without reflowing the prefix.** A `useLayoutEffect` keyed on `streamText` (`:128-134`) pins the scroll container to the bottom *only while pinned*:

```tsx
useLayoutEffect(() => {
  const el = capRef.current;
  if (!el) return;
  setOverflowing(el.scrollHeight > el.clientHeight + 1);
  if (pinnedRef.current) el.scrollTop = el.scrollHeight;
}, [streamText]);
```

`pinnedRef` is released when the user scrolls up (`handleStreamScroll`, `:143-147`) and re-asserted to `true` at the start of each fresh session (`:137-140`). Note the scroll-pin only ever sets `scrollTop` — it never re-renders or relayouts the committed text; it is purely viewport motion inside a fixed `max-height: 64px` cap (`--ov-cap-max-h`, `:35`). Older lines fade off the top edge via a CSS mask (see §3).

**`key={session}` remounts the card fresh per recording session** (`:225`), so a new recording never animates in from the previous session's open-panel size — `setSession((s) => s + 1)` on `"streaming"` (`:76`) mints a new key and replays the pop-in. This isolates sessions without tearing down the listener effect.

### 3. Animations & transitions — what reads as "smooth"

Everything animates off **three CSS custom easing/duration primitives** reused across the card: `cubic-bezier(0.22, 1, 0.36, 1)` (a strong decel, used for the 460 ms pop and morph) and shorter `ease`s for fades. There are deliberately **two distinct classes of motion**:

**(a) The compact pill — fade the *container*, never the pill itself.** The pill never pops or morphs; only its opacity does (`RecordingOverlay.css:77-83`):

```css
.ov-fade { opacity: 0; transition: opacity 200ms ease-out; }
.ov-fade.show { opacity: 1; }
```

This is applied only on the compact branch (`RecordingOverlay.tsx:274`: `className={... ov-fade ${isVisible ? "show" : ""}}`). Recording→transcribing swaps *content* (waveform↔spinner) and the pill's *width* animates, but the container just fades. The width morph is a real `transition` because the pill has **two fixed widths** rather than `max-content` (intrinsic sizes can't be transitioned — the comment at `:164-168` explains this):

```css
.scard.compact { animation: none; width: var(--ov-rest-w); }   /* 172px */
.scard.compact.cworking { width: var(--ov-work-w); }           /* 216px */
.scard { transition: width 460ms cubic-bezier(0.22,1,0.36,1),
                    border-radius 460ms cubic-bezier(0.22,1,0.36,1); }  /* :132-134 */
```

So the compact pill grows 172→216 px and softens radius as it goes from "recording" to "working", and the 200 ms opacity fade-in runs concurrently. No mount/unmount of the card → no pop.

**(b) The Live panel — pop on mount, then morph open.** The Live card *does* pop, once per session, via `key={session}` remount triggering the `scard-pop` keyframes (`RecordingOverlay.css:137-147`):

```css
.scard { animation: scard-pop 460ms cubic-bezier(0.22,1,0.36,1);
         transform-origin: bottom center; }   /* :126,131 */
@keyframes scard-pop {
  from { opacity: 0; transform: scale(0.92); }   /* scales from anchored edge */
  to   { opacity: 1; transform: none; }
}
```

`transform-origin: bottom center` (`:126`) means the scale grows *upward from the screen edge* — "the card is never drawn past it" (comment `:60-61`), so the pop never spills over the dock/taskbar. Top placement flips this to `transform-origin: top center` (`:93`).

Then the panel *opens* (text region revealed) via the **`grid-template-rows 0fr → 1fr` trick** — the canonical CSS way to animate `height: auto`. The text region is a grid row that collapses to zero and expands to content:

```css
.stext { display: grid; grid-template-rows: 0fr; opacity: 0;
  transition: grid-template-rows 440ms cubic-bezier(0.22,1,0.36,1),
              opacity 260ms ease; }                                   /* :178-185 */
.scard.open .stext { grid-template-rows: 1fr; opacity: 1; }           /* :186-189 */
.stext-clip { overflow: hidden; min-height: 0; }                      /* :190-193 */
```

`open` is set whenever there's text (`RecordingOverlay.tsx:219` `const open = hasText`), so the panel sculpt-open is driven purely by the *arrival of the first `StreamTextEvent`*, and the open size is a fixed width (`--ov-open-w: 392px`, `.scard.open { width: var(--ov-open-w); border-radius: 16px }` `:155-158`). Crucially the panel **stays open while finalizing** — `open = hasText` keeps it true even when `phase === "working"`, so the transcript doesn't collapse mid-stream (comment `:215-218`); only when there was never any text does it collapse to a `working` pill (`const collapsed = working && !hasText`, `:220`, → `.scard.working { width: var(--ov-work-w); border-radius: 18px }` `:159-162`).

**(c) Micro-interactions.** Each is a short, GPU-cheap loop:
- **Waveform bars**: height set inline from smoothed levels (`RecordingOverlay.tsx:159` `height: ${Math.max(3, Math.min(18, 3 + Math.pow(v, 0.7) * 15))}px`), with `transition: height 80ms linear` (`:309`) — fast enough to track speech, slow enough to smooth. `Math.pow(v, 0.7)` is a gamma curve that keeps quiet speech visible.
- **Recording dot**: `animation: sdot-pulse 1.9s infinite` expanding a `box-shadow` ring (`:266-281`) — subtle "alive" signal.
- **Caret**: `animation: scaret-blink 1.05s steps(1) infinite` hard-stepping opacity (`:226-240`) — classic terminal blink, dropped once `working` (`:244`).
- **Spinner**: `border-top-color` accent ring, `animation: sspin 0.7s linear infinite` (`:346-359`).
- **Cancel button**: `:hover` scales 1.05, `:active` scales 0.95 (`:336-343`) — the only input affordance.

**(d) Leave animation.** `hide-overlay` flips `isVisible` false → the compact pill's `.ov-fade` opacity drops (200 ms); the Live card gets `.leaving` (`:148-154`):

```css
.scard.leaving { opacity: 0; transform: scale(0.96);
  transition: opacity 240ms ease, transform 300ms ease; }
```

The *backend* waits 300 ms before actually hiding the native window (`overlay.rs:434-439`):

```rust
let _ = overlay_window.emit("hide-overlay", ());
let window_clone = overlay_window.clone();
std::thread::spawn(move || {
    std::thread::sleep(std::time::Duration::from_millis(300));
    let _ = window_clone.hide();
});
```

So the frontend CSS fade (~240-300 ms) and the backend hide (300 ms) are hand-tuned to the same window — the window vanishes exactly as the fade completes.

**Why it reads as smooth vs janky:** no element is unmounted/remounted during a run (sibling-position reconciliation + `key` only on session boundary); all morphs are `transition`s on *transitionable* properties (`width`, `border-radius`, `grid-template-rows`, `opacity`, `transform`) — never on `max-content` or `height: auto`; `will-change: transform, width, opacity` (`:135`) promotes the card to its own compositor layer; and `transform-origin` anchoring means pops never visibly cross the screen edge.

### 4. Window mechanics (backend) — sizing, positioning, the `emit_to` optimization

**One window, resized per state.** A single window labeled `"recording_overlay"` is created at boot hidden (`overlay.rs:326,349`) and reused for every state. `overlay_dimensions` picks one of two sizes (`overlay.rs:53-59`):

```rust
const OVERLAY_WIDTH: f64 = 256.0;        // compact slack past the 216px working pill
const OVERLAY_HEIGHT: f64 = 46.0;
const OVERLAY_STREAM_WIDTH: f64 = 400.0; // slack past the 392px open panel
const OVERLAY_STREAM_HEIGHT: f64 = 120.0;
fn overlay_dimensions(state: &str) -> (f64, f64) {
    if state == "streaming" { (OVERLAY_STREAM_WIDTH, OVERLAY_STREAM_HEIGHT) }
    else { (OVERLAY_WIDTH, OVERLAY_HEIGHT) }
}
```

The native window is deliberately a bit larger than the CSS card (`--ov-work-w: 216` / `--ov-open-w: 392`), because **the card is CSS-anchored flush to the screen edge and centered horizontally** — window height doesn't move where the card sits, only `OVERLAY_*_OFFSET` does (comment `:35-44`). This decouples native resize from visual position: when the window grows from compact to streaming, the card stays pinned to the bottom edge and just gets more room above it.

`show_overlay_state` (`overlay.rs:358-386`) is the single show path — size, position, show, then emit the state payload:

```rust
fn show_overlay_state(app_handle: &AppHandle, state: &str) {
    if settings.overlay_style == OverlayStyle::None { return; }          // respect user pref
    let (width, height) = overlay_dimensions(state);
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        let _ = overlay_window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
        if let Some((x, y)) = calculate_overlay_position(app_handle, width, height) {
            let _ = overlay_window.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }
        let _ = overlay_window.show();
        #[cfg(target_os = "windows")] { force_overlay_topmost(&overlay_window); }
        let _ = overlay_window.emit("show-overlay", state);              // payload IS the state
    }
}
```

**Positioning follows the cursor's monitor**, centered horizontally, offset from top or bottom edge by per-platform constants (`overlay.rs:217-248`):

```rust
fn calculate_overlay_position(app_handle, width, height) -> Option<(f64,f64)> {
    let monitor = get_monitor_with_cursor(app_handle)?;        // multi-monitor aware
    ...
    let x = monitor_x + (monitor_width - width) / 2.0;         // horizontally centered
    let y = match settings.overlay_position {
        OverlayPosition::Top    => monitor_y + OVERLAY_TOP_OFFSET,            // 46 macOS / 4 win+linux
        OverlayPosition::Bottom => monitor_y + monitor_height - height - OVERLAY_BOTTOM_OFFSET, // 15 macOS / 40 win+linux
    };
    Some((x, y))
}
```

It uses raw monitor geometry (not `work_area()`, which is buggy on macOS negative-position monitors — `:218-226`) and **LogicalPosition** (not Physical, which Tauri would scale by the *current* monitor's factor and break cross-monitor moves — `:224-226`). `get_monitor_with_cursor` (`:166-194`) finds which monitor the mouse is on and falls back to the primary.

**The `emit_to` single-label optimization (§4 headline).** This applies specifically to the **high-frequency** `mic-level` path, which fires roughly per audio callback (~24 Hz). `emit_levels` (`overlay.rs:456-479`):

```rust
static OVERLAY_ENABLED: AtomicBool = AtomicBool::new(false);   // cached, avoids store read per callback

pub fn emit_levels(app_handle: &AppHandle, levels: &[f32]) {
    // Skip entirely when overlay disabled — a hidden overlay's WebKit subprocess
    // would otherwise process every event (WebKit C++ alloc accumulation; issue #1279)
    if !OVERLAY_ENABLED.load(Ordering::Relaxed) { return; }

    // In Tauri 2 both AppHandle::emit and WebviewWindow::emit broadcast to ALL
    // webviews; Tauri's filter then skips webviews with no listener, so the
    // settings webview never received mic-level. But the previous dual-call
    // pattern still produced TWO eval_script calls to the overlay per callback
    // (one from each .emit()). emit_to with the overlay's label produces a
    // SINGLE eval_script call per callback — cutting per-callback WebKit
    // dispatch in half.
    let _ = app_handle.emit_to("recording_overlay", "mic-level", levels);
}
```

Two coupled optimizations here:
1. **`OVERLAY_ENABLED` atomic gate** — avoids even constructing/emitting the event when the user has `overlay_style: none` (the Linux default). The comment ties this to a real WebKit memory-accumulation bug (#1279): a hidden overlay's subprocess still processed every event, each driving C++ allocation that accumulated. The atomic is populated once at startup (`lib.rs`) and on setting change, never read from the store on the audio hot path.
2. **`emit_to(label)` instead of broadcast `.emit()`** — Tauri 2's `.emit()` fans out to *every* webview's listener registry; even though webviews without a matching listener no-op, the *previous* code emitted twice (once via `AppHandle::emit`, once via `WebviewWindow::emit`), yielding **two `eval_script` dispatches into the overlay per callback**. `emit_to("recording_overlay", ...)` collapses that to one. At ~24 Hz that halves the per-second WebKit IPC dispatch load.

> **Important nuance (verified):** the *stream text/phase* events do **not** use `emit_to`. They go through the specta `Event::emit` trait broadcast (`transcription.rs:1083` `.emit(&self.app_handle)` and `:1091`). The single-label optimization is reserved for the **high-rate** level path; the stream events fire far less often (per decoded chunk), so broadcast is acceptable and Tauri's listener filter skips the settings webview anyway.

### 5. Focus / click-through — non-activating panel behavior

The overlay must never steal focus from the text field the user is dictating into. This is handled at the **window-creation** layer with per-platform primitives.

**macOS — a real `NSPanel`, non-activating.** Handy does *not* use a plain Tauri window on macOS; it uses `tauri-nspanel` to build an `NSPanel` with explicit non-activating configuration (`overlay.rs:25-33, 320-356`):

```rust
tauri_panel! {
    panel!(RecordingOverlayPanel {
        config: { can_become_key_window: false, is_floating_panel: true }
    })
}
...
match PanelBuilder::<_, RecordingOverlayPanel>::new(app_handle, "recording_overlay")
    .level(PanelLevel::Status)
    .no_activate(true)
    .style_mask(StyleMask::empty().borderless().nonactivating_panel())
    .with_window(|w| w.decorations(false).transparent(true).focusable(false))
    .collection_behavior(
        CollectionBehavior::new().can_join_all_spaces().full_screen_auxiliary(),
    )
    .build()
```

`can_become_key_window: false` + `nonactivating_panel()` + `no_activate(true)` + `focusable(false)` together mean: the panel floats above normal windows at status level, shows on **all Spaces** (and as an auxiliary full-screen layer over macOS fullscreen apps), but **never becomes the key window** — the user's editor keeps keyboard focus the entire time. `PanelLevel::Status` puts it above normal app windows but below things like screen-saver.

**Windows & Linux — a borderless, non-focusable, topmost window.** The non-macOS builder (`overlay.rs:274-293`):

```rust
WebviewWindowBuilder::new(app_handle, "recording_overlay", ...)
    .resizable(false).shadow(false).maximizable(false).minimizable(false).closable(false)
    .accept_first_mouse(true)       // clicks register without needing foreground first
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focusable(false)
    .focused(false)
    .visible(false)
```

`focusable(false)` + `focused(false)` is the focus-steal guard; `skip_taskbar(true)` keeps it out of the taskbar/alt-tab; `accept_first_mouse(true)` means the cancel button works on the first click without a foregrounding click (macOS exposes the same concept via NSPanel; here it's a Tauri/tao flag). On Windows, Z-order is *re-asserted* after every `show()` via a direct Win32 `SetWindowPos(HWND_TOPMOST)` with `SWP_NOACTIVATE` so topmost is enforced **without stealing focus** (`overlay.rs:136-164`):

```rust
SetWindowPos(hwnd, Some(HWND_TOPMOST), 0,0,0,0,
    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
```

On Linux it prefers the **GTK Layer Shell** at `Layer::Overlay` with `KeyboardMode::None` and `exclusive_zone(0)` (`overlay.rs:111-134`), which is the Wayland-native "always-on-top, no keyboard focus, doesn't reserve space" surface — with a `HANDY_NO_GTK_LAYER_SHELL` escape hatch for compositors that lack it (`:112`). The overlay is **not** click-through (the cancel button must be clickable), but it is **non-activating** on all three platforms.

---

## Why it feels lightweight / smooth / instant

- **Flicker-free live text is structural, not cosmetic.** The append-only `committed` prefix (`bindings.ts:982-985`) + two always-present sibling spans (`RecordingOverlay.tsx:237-245`) mean React only ever *grows* the committed text node in place. The user never sees a committed word rewritten, because the backend promises it never will be, and the DOM split means even a wholesale `tentative` rewrite cannot relayout the committed glyphs. This single decision is the largest contributor to "streaming feels smooth."
- **The card never tears down mid-run.** The compact branch fades its container (`ov-fade`, 200 ms) and morphs `width` (460 ms); the Live branch pops once per session (`key={session}`) then *transitions* open via the `grid-template-rows 0fr→1fr` trick. No `height: auto` or `max-content` is ever transitioned — only animatable properties — and `will-change` promotes the layer (`:135`). State changes read as a continuous morph, not a flash.
- **The native window is decoupled from the visual card.** CSS anchors the card flush to the screen edge and centers it, so the backend can resize the window (compact↔streaming) without the visible card jumping position — only the offsets move it (`overlay.rs:35-44`). Pops scale from the anchored edge so they never overshoot the dock.
- **The high-rate channel is cheap.** `mic-level` at ~24 Hz is gated by a cached atomic (no store read on the audio thread) and delivered via `emit_to` on one label — one `eval_script` per callback, and skipped entirely when the overlay is off (`overlay.rs:456-478`). The waveform smoothing happens in the listener (`prev*0.7 + target*0.3`, `:89-92`), so the backend ships raw buckets and the frontend does the EMA.
- **It never steals focus.** A real non-activating `NSPanel` (macOS) / `focusable(false)+skip_taskbar+always_on_top` window (Win/Linux) means dictating into your editor is uninterrupted even as the pill pops and streams. The cancel button still works on first click via `accept_first_mouse(true)`.

---

## VoiceTypr takeaways

| Priority | Takeaway | Rationale |
|---|---|---|
| **High** | **Split live text into `committed` + `tentative` spans as two always-present siblings** (no keys, reconcile-by-position) and enforce an **append-only `committed` contract** in the Rust streaming core. | This is the single highest-leverage fix for perceived flicker. Even if VoiceTypr's model rewrites the trailing word every chunk, the committed prefix repaints in place and never jumps. The DOM split is ~3 lines of JSX; the value is the backend monotonicity guarantee. |
| **High** | **Animate the panel open with `grid-template-rows 0fr → 1fr`**, not `height: auto` / `max-content`. | Lets you transition "reveal text region" smoothly with pure CSS; no JS height measurement, no jank. Handy pairs it with a 440 ms decel easing and `opacity 260 ms`. |
| **High** | **Deliver the per-audio-callback level stream via `emit_to("<overlay-label>", ...)`**, gated by a cached `AtomicBool` "overlay enabled" flag — never read the settings store on the audio thread, and skip emission entirely when the overlay is off. | Halves the WebKit `eval_script` dispatch per callback vs broadcast `.emit()`, and prevents the hidden-overlay memory accumulation Handy hit (issue #1279). |
| **High** | **Use a real non-activating `NSPanel` on macOS** (`can_become_key_window:false`, `nonactivating_panel()`, `no_activate(true)`, `can_join_all_spaces`, `full_screen_auxiliary`). | A plain Tauri window will steal focus from the field the user is dictating into on every show. The NSPanel is what makes "pops while I type" acceptable. |
| **Med** | **Keep the panel *open while finalizing*** (`open = hasText`, ignore `phase==="working"` for collapse) and only collapse to a working pill when there was *no* text. | Prevents the transcript from collapsing-and-squishing mid-stream under the spinner — a visible glitch Handy explicitly avoids (`RecordingOverlay.tsx:215-220`). |
| **Med** | **One reused window resized per state**, with the visual card CSS-anchored flush to the screen edge so native resize doesn't move it. | Simpler lifecycle than create/destroy per state, and decouples native geometry from perceived position. Mirror the `--ov-*` CSS vars as the source of truth for native window sizes (`overlay.rs:35-44`). |
| **Med** | **Pin scroll to newest only while `pinned`**; release on scroll-up, re-pin on new session; fade older lines via a CSS `mask-image` gradient gated on an `.overflowing` class. | Lets users scroll back to read history without being yanked to the bottom by the next chunk, and the mask signals scroll-back without a visible scrollbar (`stext-cap` hides the scrollbar, `:215-217`). |
| **Med** | **`key={session}` remount the card per recording session** so a new recording pops in fresh and never animates from the previous session's open size. | One-line fix that isolates sessions cleanly without tearing down the listener effect. |
| **Low** | **EMA-smooth the waveform in the listener** (`prev*0.7 + target*0.3`), take the first N of 16 FFT buckets, and apply a `pow(v,0.7)` gamma + clamp `[3,18]px` with an `80ms linear` height transition. | Produces a calm, speech-tracking waveform without backend smoothing; ships raw buckets, smooths in JS. |
| **Low** | **Top vs bottom placement as a pure flexbox flip** (`align-items: flex-end` ↔ `flex-start`, `flex-direction: column-reverse`, mirrored mask/padding) driven by a settings read on `show-overlay`. | One geometry supports both placements; the `transform-origin` flip keeps pops anchored to whichever edge. |
| **Low** | **On Windows, re-assert topmost via Win32 `SetWindowPos(HWND_TOPMOST)` with `SWP_NOACTIVATE`** after each `show()`, rather than trusting `set_always_on_top`. | Handy's comment notes Tauri's flag "can be overridden"; the direct Win32 call is more reliable for always-above behavior without focus theft (`overlay.rs:136-164`). |

---

## Open questions / risks

- **The append-only `committed` guarantee is not enforced at the type level.** `StreamTextEvent` is just `{ committed: string; tentative: string }` (`bindings.ts:985`); nothing in the type system stops a future Rust change from *shrinking* `committed`, which would silently reintroduce flicker (committed characters would vanish mid-prefix). [INFERENCE] The guarantee lives implicitly in the streaming-core diff logic (slice 1's territory). A VoiceTypr fork should add a debug-assert or property test that each emitted `committed` is a prefix of the next.
- **`.tentative { color: inherit }` is a no-op** (`RecordingOverlay.css:223-225`) — committed and tentative are visually identical. If VoiceTypr's model rewrites tentative *aggressively* (large trailing rewrites), users may still perceive churn at the cursor even though the prefix is stable. Consider an optional low-opacity on `.tentative` as a tuning knob; Handy chose not to.
- **`hide-overlay` hides the window on a fixed 300 ms `thread::sleep`** (`overlay.rs:434-439`) tuned to the CSS fade. If VoiceTypr changes the leave easing duration, this constant must move with it or the window will vanish mid-fade / linger after.
- **`get_monitor_with_cursor` falls back to the primary monitor** if cursor position can't be read (`overlay.rs:193`); on a multi-monitor setup where the cursor is on a non-primary display and `input::get_cursor_position` fails, the overlay would jump to the primary screen. [INFERENCE] The enigo/scaling normalization logic (`:170-177`) is documented as correct, but the fallback path is unverified for edge monitors.
- **Linux depends on GTK Layer Shell** (`overlay.rs:111-134`); compositors without it fall back to a regular borderless window that may not be truly always-on-top or may reserve space differently. The `HANDY_NO_GTK_LAYER_SHELL` escape hatch exists precisely because this path is fragile.
- **The stream text/phase events use broadcast `.emit()`** (`transcription.rs:1083,1091`), not `emit_to`. This is fine at chunk-rate, but if VoiceTypr ever increases stream-event frequency (e.g. per-token streaming of a fast model), the broadcast fan-out to the settings webview's listener registry (even though filtered) could become measurable and would warrant the same single-label treatment as `mic-level`.
