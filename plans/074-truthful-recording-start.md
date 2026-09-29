# Plan 074 — First word not clipped: truthful "recording" cue + faster start

Status: SPEC — Claude 2026-09-27. Target: 2.1.0-beta.2. Baseline: after plan 073.

## Why

Users lose the first word when they start talking right after the hotkey.
Root cause (verified in code, 2026-09-27): the app says "recording" before the
microphone is actually capturing.

- `AudioRecorder::start_recording` (`audio/recorder.rs:576`) spawns the recorder
  thread and stores the handle immediately (`recorder.rs:~1105`); it returns Ok
  before the device is opened. `is_recording()` is true from that moment.
- `commands/audio.rs::start_recording` then sets `Recording`, plays the start
  sound and shows the pill — while the thread is still opening the device,
  building the stream (slow on Bluetooth) and calling `stream.play()`
  (`recorder.rs:~994`). Users react to the cue; their first syllables arrive
  before capture exists.
- The live-preview sink factory runs on the recorder thread **before** the WAV
  writer and `build_input_stream` (`recorder.rs:653`). Parakeet's factory can
  `block_on(load_model)` (seconds on a cold start), delaying capture itself.
- `commands/audio.rs` (~5234-5252) runs a second CPAL host/device enumeration
  only to log the device name (tens of ms, sometimes ~100 ms).
- Timing logs stop at "recorder.start_recording returned Ok", so the real device
  latency is invisible today.

Handy (reference): always-on mic is a default-off debug setting; on-demand mode
keeps the stream open 30 s after a recording; caches the resolved device. We do
NOT add an always-open mic here (it keeps the macOS mic indicator lit) — that is
a founder decision, only if the numbers after this plan still show clipping.

## Change

1. **Readiness signal.** The recorder thread reports readiness after
   `stream.play()` succeeded **and** the first audio callback ran. The callback
   only sets an `AtomicBool` (invariant 1: no allocation or blocking in the
   callback); the recorder thread polls it (e.g. every 2 ms) and sends
   `Ready { first_audio_ms }` or `Failed(String)` on a `tokio::sync::oneshot`.
   If the thread fails before ready (device open, unsupported format, play),
   it sends `Failed` so the error surfaces at start, not later.
2. **`start_recording` waits for readiness before the cue.** Return the
   receiver from `AudioRecorder::start_recording`; `commands/audio.rs` awaits it
   **after releasing the recorder mutex**, with a timeout (2 s). Only then:
   state `Recording`, start sound, pill recording state. On `Failed`: clean up
   exactly like today's start failure path (state, pill, generation). On
   timeout: proceed as today and log a warning (don't fail the recording).
   While waiting, the state stays `Starting` (the pill may show its existing
   starting look).
3. **Keep invariant 3 and hold-to-talk intact.** `start_recording` still
   returns `true` only if this call started the recording. A stop/release that
   arrives while waiting must use the existing pending-stop path
   (`pending_stop_after_start`, `recording_started_cue_eligible`) — the longer
   `Starting` window must not drop or double-fire stops; the start sound must
   not play when a pending stop was consumed.
4. **Sink creation never delays capture.** Create the stream sink so that a
   slow factory (Parakeet cold `load_model`) cannot delay `build_input_stream`
   / `stream.play()`: build and start the stream first, then create the sink;
   frames captured before the sink exists still reach the WAV (the WAV stays
   the batch source of truth). If the preview misses the first frames, that is
   acceptable; document it. Soniox's factory (plan 073) must not block either.
5. **Remove the duplicate CPAL probe** (log-only enumeration in
   `commands/audio.rs`); log the device name from the recorder thread instead.
6. **Measure.** Add `start_to_first_audio_ms` (hotkey start call → first
   callback) to capture metrics and a `⏱️ [REC TIMING]` log line for
   build_input_stream, play and first callback. No content.

## Review outcomes (gpt-6-astra medium, four rounds, 2026-09-28)

The longer Starting window exposed old races; all P1s fixed with scripted
interleaving tests:
- stale readiness / pill-await continuations return `Ok(false)` without side
  effects (generation checked after every await);
- stops during Starting queue before `StopInFlightGuard`, re-read the state and
  reclaim the flag if start already moved on — used by `stop_recording`, native
  PTT release and toggle;
- the in-app toggle sends its stop during `starting`; Escape-cancel is armed in
  Starting; the start cue/event fire only if this generation is still Recording;
- bindings are rebuilt on every `start_recording` exit (a bound Escape is
  swallowed system-wide, so it must never stay armed in Idle).

Accepted P2: cancellation landing between the final cancellation check and the
Recording transition (sub-millisecond) can leave cancel's Starting cleanup
rejected by the state machine. The clean-core state machine (0.4a) replaces
these hand-rolled handoffs.

## Tests (must exist before merge)

- Recorder: readiness sent only after the first callback flag is set
  (test the poll/report helper with a fake flag); `Failed` on a forced error.
- Commands: pure decision helpers for the ordering — cue only after `Ready`;
  `Failed` → no cue, state cleaned; timeout → cue + warning.
- Pending stop during `Starting` (the wait window): stop is queued and applied
  once, no start sound, `start_recording` return value unchanged
  (extend the existing `recording_started_cue_requires_no_pending_stop` style).
- Sink ordering: a factory that blocks does not delay the `play()` call
  (inject a slow factory; assert the stream started first).
- Real check (Claude): BlackHole E2E — play a clip at the same instant as the
  hotkey; compare captured audio onset before/after; log
  `start_to_first_audio_ms` for built-in mic, BlackHole and a Bluetooth mic.

## Acceptance

- The start cue never fires before the first audio callback.
- `start_to_first_audio_ms` visible in logs/metrics; first word kept in the
  E2E clip test on Mac; Windows fast check green.
- `pnpm check` green.

## STOP conditions

- If waiting for readiness adds a visible delay on the built-in mic
  (> ~150 ms p50 cue delay vs today), stop and report numbers first.
- If moving sink creation after `play()` breaks the preview contract
  (committed-prefix monotonicity, stale-session gate), stop.
