# Plan 080 / P2b — Morphing Island renderer

Implemented locally on `feat/2.1-beta4`, uncommitted. Source checks and real native acceptance remain separate; the latter is **NEEDS-SMOKE**.

## Modules

`src/pill.tsx` is event transport, subscription cleanup and startup hydration only. No React is imported into the pill entry or its renderer dependencies.

| Module under `src/pill/` | Responsibility |
| --- | --- |
| `contracts.ts` | P2a payloads and optional P3/P4/P5 action capabilities |
| `island.ts` | State/layer decisions, generation and stream revision guards, start-card decision, elapsed-time freeze |
| `renderer.ts` | Controller, native pointer hover, deadlines, shared-element positioning and one demand-driven frame loop |
| `spring.ts` | Lab response 0.34 / bounce 0.22, <=6 ms integration, interruptible targets, cubic-out easing |
| `layers.ts` | 60 ms entrance delay, 200 ms entrance, 80 ms exit, fit gates, delayed second rows and label cross-fades |
| `wave.ts` | Fixed 96-slot peak ring, 70 ms elapsed bars, 4 px pitch, 24 px fade, 30/180 ms envelope, stationary reduced-motion meter |
| `glyph.ts` | Shared wave → arc → check/clipboard/quiet/error canvas; white/sage spinner colour transition |
| `app-icon.ts` | Generation-safe PNG loading; letter tile until decode succeeds, deterministic app-name palette, load-error fallback |
| `badge.ts`, `icons.ts` | 12 px badge, 7 px lab SVG, amber key warning, 420 ms chip-to-icon flight, shared SVG set |
| `words.ts` | Persistent committed node, tentative ink at .52, line FLIP and new ink colour/slant transition |
| `crossfade.ts` | 150 ms whole-shape reduced-motion cross-fade with a canvas snapshot |
| `dom.ts`, `copy.ts` | Static DOM and user-facing copy/short display names |

The `[hidden]` guard in `pill.css` remains authoritative. All six P1 anchors are supported; top anchors reverse the card rows and grow downward. The native P1 geometry event remains authoritative, including a user's configured Windows anchor. Windows preview fixtures default to bottom-right; this slice does not change persisted native positioning settings.

## Event wiring

- `dictation-context`: stale/duplicate generations rejected; real app identity, icon key, style, mic tooltip and engine. `pill_app_icon` responses are guarded against late generations and destruction; failure uses a letter tile.
- `recording-started` / `recording-state-changed`: start/listen, stop/work, rest/error. Duplicate starts do not restart elapsed time. Startup hydration cannot overwrite a recording event received while its command is pending.
- `audio-level`: content-free generation + real peak level; older generations rejected. No synthetic voice model ships in the pill.
- `transcription-stream`: optional setting-controlled subscription, mounted committed node, stale sessions/revisions rejected, committed rewrites rejected without logging text.
- `pill-pointer {inside,x,y}`: native coordinates reveal Stop/Cancel and apply button hover styles; clicks remain DOM clicks. Rest hover has 450 ms slow-pointer intent, 400 ms close delay and a 120 ms click guard.
- `transcription-started`, `enhancing-*`: frozen timer and width hold; polishing uses its real style label.
- `paste-outcome`: pasted for 2.4 s; copied/no-permission sticky cards, word counts, dismiss and the existing `open_accessibility_settings` action.
- Existing `toast`: Esc confirmation goes inline for 2 s; too-short and no-speech messages map to the lab notes. Too-short holds 1.4 s; no-speech has a 30 s frontend deadline and a local dismiss button; neither claims that audio was retained.
- `pill-geometry` / `pill_get_geometry`: P1 anchoring. `pill_set_hit_regions`: current animated shape, expanded rest hover target, changed bounds only, one report per frame, latest-value coalescing while IPC is in flight.

`commands/pill_feedback.rs` now holds native pasted visibility for 2400 ms and keeps copied outcomes pending without a hide timer. Keeping the pending generation also prevents the no-permission error path from immediately hiding its copied card.

## Performance and accessibility

There is one rAF loop, active for moving springs/layers/ink, a scrolling recording wave, the spinning work arc or a reduced-motion cross-fade. Settled rest has no rAF or timer; breathing is CSS opacity on the core. Reduced-motion recording updates its stationary meter from level events and has no continuous rAF. State deadlines and hover-intent timers are bounded and cleaned up on interruption/destruction.

The wave preallocates timestamps, levels and perceptual heights. Its gradient is cached until width changes. Frame-time DOM references are cached; canvas/state-transition snapshots allocate only during transitions. Canvas backing dimensions are **2 × devicePixelRatio**. No transcript/audio/clipboard/secret/path/window-title logging was added. A single polite live region announces each outcome once, including duplicate-outcome suppression.

## Deferred capabilities and deliberate differences

- P3 owns the feedback stack, problem-state mapping and native feedback-window ownership. The old separate toast can still accompany the inline hint. Existing short/no-speech backend paths can hide the native pill immediately in `when_recording`; the renderer and preview alone do not prove those notes are visible in that mode.
- P4 owns Undo/Original/Retry and Transcribe anyway. Those actions are absent, with typed capabilities retained. Existing no-speech handling can delete the clip, so there is no “Audio kept” promise.
- P5 owns initial quick-settings loading, gear/today's count, pick lists and chip interactions. The peek shape and context chips render; chip clicks are no-ops in this slice. Before the first context event it uses neutral fallback labels.
- No backend Skip or finishing-press event exists. Skip stays hidden; no synthetic “Finishing…” event was added.
- The legacy compact/full options both use the Island layout; this slice specifies one renderer. Recording settings selector cleanup is separate.
- Live grows to **300 px**, matching the lab; slim listening/transcribing/polishing rows hold **236 px**. Start/notice/note widths measure their real text, bounded by the P1 canvas; long permission copy is capped at 438 px.
- Invalid selected Polish shows amber even when `will_run` is false: P2a computes availability into `will_run`, so using it alone would hide the requested invalid-key warning. Disabled/raw Polish with no selected style has no badge.
- The real timer starts at 0:00; the lab's free-play listening timer artificially clamps to at least 0:01. The preview uses an app-icon fixture; it does not port fake app scenes or the lab rail/pointer/scenario simulator.
- Geometry/motion pixel acceptance against `v32-*.png` has not been performed. Native focus, hit testing, top anchors, DPI, back-to-back dictation and real speech remain **NEEDS-SMOKE**.

## Validation

- Frontend gates: typecheck, oxlint, full Vitest and production build passed (991 tests); targeted renderer suite: 79 tests. The existing RecordingPill test family grew from 37 to 58 cases; its cancel, Esc, stream and outcome assertions remain.
- One full rerun exposed an intermittent existing `EnhancementSettings` portal-option lookup; its isolated run passed. The test now waits with `findByRole` for the option before clicking, preserving the same behavior assertion and test count.
- Rust formatting passed. `cargo test` and `cargo clippy --workspace --all-targets -- -D warnings` were attempted and stopped in `build.rs` before Rust tests/checking, because SwiftPM's nested `sandbox-exec` was denied. Temporary CLI-only retries with local caches / disabled SwiftPM subprocess sandbox / native build system also failed with Swift service/file access errors. No build-script or sidecar source was changed to bypass the gate.
- `pnpm ui:preview --only pill` exited because installed Chrome **SIGABRTed before opening a page**; zero screenshots were produced. Claude must capture and compare the expanded preview matrix against the v32 exports.
- Preview matrix: rest, start, listening, amber badge, no badge, live, transcribing, polishing, pasted, copied, no_permission, error, too_short, nospeech, esc-hint, top-anchor-live; macOS/Windows and light/dark.

Gate logs are under `.tmp/p2b-*.log`. No commit, push or release was made.

## Claude screenshot review corrections

- App icons accept the backend PNG data URL contract only and remain hidden until successful decoding. Null, malformed responses, command failures and image load errors preserve the same-sized/radius letter tile. Its colour comes from a fixed six-colour app-name palette. Generation changes and destruction invalidate pending icon responses. Preview start/listening/live/copied fixtures use an embedded valid 64×64 PNG; other fixtures return null.
- Copied cards put the icon and amber clipboard disc on the first row beside the platform-specific paste instruction and right-aligned word count. The second row shows the app-specific failure reason (or permission wording), retained privacy action when available, and a 26 px dismiss button. The amber outline remains. Shared icon/glyph springs follow the title row and settle without an idle animation loop.
- Live words and row zones grow to 300 px; transcribing and polishing return to 236 px. Top macOS anchors retain reversed growth/rows.
- Reviewed the supplied Windows light copied/live/start shots: right and bottom edges were already fixed correctly. The Windows top-anchor-live fixture now also defaults to bottom-right; macOS retains its top-anchor coverage. Native geometry ownership is unchanged.
- Correction gates passed: typecheck, lint, **1004 tests across 89 files**, production build. Added 13 tests covering malformed/null/failed/late icon loading, successful decode and load-error fallback, fixed palette, copied row positioning on both platforms, and live-to-processing width.
- Fresh screenshot attempt (`pnpm ui:preview --only pill`) again failed: installed Chrome **SIGABRTed before opening a page**, producing zero new screenshots. Existing shots were inspected against the v32 copied/live references; fresh pixel acceptance remains with Claude and native behavior remains **NEEDS-SMOKE**. No Rust source was changed for these corrections. No commit was made.

Correction logs: `.tmp/p2b-review-{typecheck,lint,vitest,build,preview,targeted}.log`.
