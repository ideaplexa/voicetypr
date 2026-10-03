# Plan 080 Phase A / P5 — Island quick settings

Implemented locally on `feat/2.1-beta4`, **uncommitted**. Native acceptance and screenshot acceptance remain **NEEDS-SMOKE**.

## Behavior and wiring

- `menu/quick.rs` exposes `island_quick_options` and `island_quick_set`. Options project the tray's shared snapshot/model; setting IDs go through `menu/actions.rs::run`, exactly as tray actions do. Quick hydration skips transcript history. Invalid kinds, stale/unavailable IDs and group headers cannot dispatch. Errors shown in the island contain no backend error text.
- Polish, grouped engines, microphones and languages hydrate before the first take. Chips refresh on settings/model/audio-device/Polish/AI/sharing/shortcut events. Async reads reject stale revisions and destruction; native geometry stays authoritative during settings reloads.
- Real shortcut caps come from P6's effective shortcut presentation, including a configured separate PTT key. Hold/toggle wording uses the effective primary mode. The gear calls `island_action(open_settings)` and the shared main-navigate Settings route.
- Native `pill-pointer` samples drive 450 ms hover intent, a 50 px/s speed threshold and the 120 ms entry click guard. Fast movement resets dwell; because P1 emits movement-only samples, no further sample means the pointer has stopped. The 20×20 rest target extends upward only. Peek/list exit has a 400 ms grace period.
- Dot, record button and empty peek clicks invoke the existing `start_recording`. The renderer waits for real recording events; it does not invent a recording generation or turn a false start return into a take.
- Four chips span the peek; the 26 px record button stays on the visible dot centre for all six positions. No mic uses a red chip, amber record icon and the requested “No mic — pick one below” wording.
- Lists grow with the existing spring/layer loop. The checked row aligns over its source chip where canvas bounds allow, with scrolling for long lists. Engine headers are disabled. A row click requires 250 ms or 4 px of movement; picks await shared persistence, move the check, then return after 350 ms. Chip labels cross-fade. Failed picks stay open and permit retry.
- The start card's style chip uses the same Polish choices and pauses its fold timer while the list is open. Recording, audio generation and stream ownership remain with the existing machine/backend.
- Escape is observed through the existing pass-through `Chord` trigger and a content-free `island-escape` event. No panel focus or consuming idle key binding is introduced. Existing recording cancellation semantics remain.
- Settled rest/peek have no rAF loop; animations and hit regions use the existing demand-driven/coalesced paths.

## Scope details and differences to review

- Native canvas is now **462×442**, adding 11 px transparent margins. Without the margin a 26 px button centred on the old 12 px dot would clip at canvas edges. `canvas_origin` preserves the old screen anchor; native focus configuration is untouched. Preview fixtures use the padded geometry too.
- Languages share the same complete model; the tray retains eight common entries plus More, while the island can scroll every supported language.
- The no-mic hint deliberately follows the P5 request, rather than the export's “choose one above”.
- Start-card selection uses the required **P6 global Polish handler** and persists beyond the current take. No per-take-only override protocol was added. Existing per-app rules still take precedence in the writing pipeline; the lab's “Style for this dictation” label is not proof of a backend per-take override. Review that behavior in native acceptance, particularly with a per-app rule.
- No usage count was added: v3.2's requested peek has four chips and the record/shortcut/gear row.

## Validation

- `pnpm typecheck`, `pnpm lint`, full Vitest and production build passed. **1,225 tests across 100 files**, including **28 new P5 cases**; no existing tests removed (baseline 1,197). Existing Vite chunk-size warning remains.
- Tests cover dwell/speed/entry guards, dot/record/empty-panel start, all four IPC pick flows, 250 ms and 4 px guards, failure/retry, disabled groups, start list, sync and native geometry authority, no-mic copy, both platforms' hold/toggle caps, every anchor within 1 px by layout coordinates, hit targets and idle rAF cessation.
- Rust formatting passed. Five new quick-model/dispatch/caps tests, a canvas-fit test and a pass-through Escape test are written. **App Rust tests have not executed:** both required `cargo test` and `cargo clippy --workspace --all-targets -- -D warnings` stop in `build.rs` because SwiftPM's nested `sandbox-exec` reports `sandbox_apply: Operation not permitted`. No build bypass was made.
- Independent `cargo test -p keytrigger` passed **50 tests**. This does not compile the new app commands.
- `pnpm ui:preview --only pill` failed before opening a page: Chrome exited with **SIGABRT**, producing **zero screenshots**. Reference PNGs were inspected, but rendered pixel matching has not been performed.
- Preview matrix adds peek, peek-nomic, pick-polish, pick-engine, pick-mic, pick-language and start-pick, plus top-anchor versions, for macOS/Windows and light/dark.
- Logs: `.tmp/p5-{typecheck,lint,vitest,build,cargo-test,clippy,fmt,keytrigger,targeted,preview}.log`.

**NEEDS-SMOKE:** Claude must compile/test/clippy the app Rust, capture/compare the preview matrix, and exercise the real non-key panel on macOS/Windows: pointer dwell and early clicks; chip/list hit testing and scrolling; Escape pass-through; 26 px button/dot alignment throughout morphs; all anchors and Windows work area/DPI; settings/tray/main-window synchronization; no mic/device changes; start-style semantics with per-app rules; click-to-dictate focus/paste; back-to-back real-speech takes. No commit, push or release was made.
