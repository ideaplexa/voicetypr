# Plan 031 — Pill micro-bundle + Handy-style status UX

> One pass, display-layer only: rebuild the pill entry as a self-contained micro-bundle
> (308KB → target ≤ ~20KB JS) AND give it an honest status contract (recording timer +
> cancel ×, "Transcribing…", "Polishing…", error flash). No recording/transcription/paste
> logic changes. UX reference: Handy's pill (bottom-center, compact, never steals focus).
> All findings below verified against main @ af63ab1 with file:line.

## Verified current state

- `src/pill.tsx` wraps `RecordingPill` in `SettingsProvider`; `usePillController` pulls
  `useRecording` + `useSetting("pill_indicator_mode")`. Cost: pill 118KB + globals 190KB
  ≈ 308KB JS for a passive 80×40 indicator (toast.tsx = 1.1KB proves the floor).
- Pill state inputs are ALL Tauri events, already emitted to the pill window:
  `recording-state-changed` (payload `{ state: "idle"|"starting"|"recording"|"stopping"|
  "transcribing"|"error", error: string|null }`, app_state.rs:251), `recording-started`
  (unit, audio.rs:4369), `transcription-started` (unit, audio.rs:5101),
  `enhancing-started/completed/failed` (audio.rs:5362/5408/5429), `audio-level`
  (bare f64, 10fps throttled, audio.rs:4333), `recording-too-short` (string,
  audio.rs:5013 — emitted to pill, currently UNHANDLED by any frontend listener).
- Cancel: `cancel_recording` command exists (audio.rs:6934, registered lib.rs:1338) —
  aborts + deletes temp audio + returns to Idle. NO frontend invoke site exists today;
  only ESC double-tap (escape_handler.rs) and shortcut action use it.
- Window: fixed 80×40, duplicated builders (window_manager.rs:245-259 AND
  lib.rs:1198-1211): resizable(false), always_on_top, transparent(true), shadow(false),
  skip_taskbar, focused(false), content_protected(true). macOS: `to_panel()`
  non-activating NSPanel (window_manager.rs:277, lib.rs:1224). Windows:
  WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE (window_manager.rs:284-326). No
  set_ignore_cursor_events anywhere → clicks DO reach the webview; there are simply no
  click handlers today. `accept_first_mouse` is NOT set.
- No elapsed-time tracking exists anywhere (frontend or backend).
- Positioning: `calculate_pill_position` (window_manager.rs:15-40) uses hardcoded
  pill_width=80/pill_height=40; user-configurable position, default bottom-center.

## Part A — micro-bundle

1. Rewrite `src/pill.tsx` as a self-contained entry: NO `SettingsProvider`, NO
   `useRecording`, NO `globals.css`. Direct `listen()` on the events above + local
   state machine (same 4 states as today's `usePillController`: idle/listening/
   transcribing/formatting — keep the type). `audio-level` subscription only while
   listening (as today, usePillController.ts:40-63).
2. `pill_indicator_mode` ("always" | "when_recording"): read once at boot via the
   existing settings invoke (verify the exact command name in src — SettingsContext
   uses it) and re-read on the `settings-changed` event. Do NOT import the settings
   context.
3. Styling: small dedicated `pill.css` (plain CSS, no Tailwind/globals import).
   Re-implement the level bars visual as a tiny local component (current `AudioBars`
   depends on Tailwind classes from globals.css). Keep the same dark rounded look
   (bg #14171c, white/10 border, rounded-full).
4. Old `RecordingPill`/`PillShell`/`usePillController` become unused by the pill entry:
   remove them and migrate `RecordingPill.test.tsx` to test the new pill (user-visible
   behavior per state; tests must pass, not be deleted — rewrite them against the new
   component).
5. Measure: `pnpm build`, report `dist/assets/pill-*.js` (+ its css) size before/after
   in your summary. Target ≤ ~20KB JS.

## Part B — status states (Handy contract, non-streaming)

Window change (both duplicated builders, window_manager.rs:256 + lib.rs:1208):
- inner_size 80×40 → **260×64**, still transparent/undecorated; content anchored
  bottom-center via CSS (container `align-items: end; justify-content: center` +
  small bottom padding) so the visible pill keeps its distance from screen bottom.
- Update `pill_width`/`pill_height` constants (window_manager.rs:21-22) to 260/64 so
  positioning math still centers correctly.
- Add `.accept_first_mouse(true)` to both builders (macOS: click × without a prior
  activating click). Keep focused(false), NSPanel conversion, and Windows NOACTIVATE
  exactly as-is — the pill must NEVER take focus (release criterion).

States (root container stays `pointer-events: none`; ONLY the × button gets
`pointer-events: auto`):
- **idle** (only visible when pill_indicator_mode = "always"): current tiny 3-dot look.
- **listening**: level bars + elapsed timer (m:ss, local setInterval started on
  entering listening, cleared on exit — frontend only) + a small `×` cancel button.
  `×` onClick → `invoke("cancel_recording")` (command exists, audio.rs:6934). Guard
  against double-fire (disable after first click until state changes).
- **transcribing**: small spinner + "Transcribing…" label (covers stopping+transcribing
  states, as today).
- **formatting**: spinner + "Polishing…" label.
- **error flash**: on `recording-too-short` (string payload) or
  `recording-state-changed` with `state === "error"`, show a brief red-tinted pill with
  a short message (~1.5s) then return to idle visibility rules. This makes the pill
  finally consume `recording-too-short`.
- Transitions: keep it calm — one container that changes width smoothly (CSS
  transition on width/padding), no remounting flicker between states.

## Acceptance

- `pnpm typecheck`, `pnpm lint`, `pnpm test` pass (pill tests rewritten, not removed).
- `cargo test` in src-tauri passes (window builder + constants touched).
- `pnpm build` succeeds; report pill chunk size delta.
- No new focus-stealing: builders keep focused(false)/NSPanel/NOACTIVATE.
- No backend behavior changes beyond window size/accept_first_mouse.
- Do not commit.

## Explicitly out of scope

Live transcript preview text (streaming committed/tentative — separate track),
onboarding changes, any recording/paste logic, Soniox/Whisper/Parakeet code.
