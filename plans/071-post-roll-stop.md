# Plan 071 — Post-roll on stop (last words cut off)

Status: READY FOR IMPLEMENTATION (2026-09-27). Target: small 2.0.7 on `main`.
Branch `fix/postroll-stop`, worktree `worktrees/voicetypr-postroll`.

## Problem (founder report)

"When I finish talking and immediately stop, some of my last words get missed."

## Root cause (verified in code on `main` @ c47e1465)

Nothing keeps capturing after the user stops:

- `src-tauri/src/recording/hotkeys.rs:162-183` — push-to-talk `KeyPhase::Released`
  immediately spawns `stop_recording(...)`.
- `src-tauri/src/commands/audio.rs:4794` `stop_recording` (Tauri command) calls
  `recorder.stop_recording()` at ~4865 with no delay.
- `src-tauri/src/audio/recorder.rs:1022-1046` `AudioRecorder::stop_recording`
  sends `RecorderCommand::Stop`; the worker (`stop_rx.recv()` ~907, then the
  drain barrier ~910-925) stops the stream at once.

The existing drain barrier only rescues samples already delivered by the OS;
it adds no capture time. People release the key while the last syllable is
still decaying, so that audio is never recorded. Trimming, resampling and
engine paths were checked and are not the cause.

## Change

1. **Post-roll for user-initiated stops only.**
   - Add `const STOP_POST_ROLL: Duration = Duration::from_millis(250);` in
     `recorder.rs` with a doc comment explaining why.
   - Change `RecorderCommand` to carry the mode, e.g.
     `enum RecorderCommand { Stop { post_roll: Duration } }` (or `Stop` +
     `StopAfterPostRoll(Duration)` — your choice, keep it explicit).
   - Add `pub fn stop_recording_with_post_roll(&mut self, post_roll: Duration)`
     on `AudioRecorder`; keep `stop_recording()` as the immediate stop
     (post_roll = zero) so every existing caller keeps its behaviour.
   - In the worker, after receiving the stop command and BEFORE
     `stop_requested.store(true, …)` (the drain barrier), sleep for `post_roll`
     while the cpal stream keeps running, so audio keeps flowing into the WAV
     writer and silence/level detection exactly as during recording.
   - Only the Tauri command `stop_recording` in `commands/audio.rs` (the path
     used by push-to-talk release, toggle stop, tray and UI stop) calls
     `stop_recording_with_post_roll(STOP_POST_ROLL)`.
   - Stay immediate (no post-roll): `cancel_recording` (~7507/7568), the
     start-abort paths (~4598, ~4641), `commands/settings.rs:1501`, `Drop`,
     the size-limit and device-error internal stops (~689, ~738), tests.
   - `STOP_JOIN_TIMEOUT` (8 s) already covers +250 ms; update its doc comment
     to list the post-roll in its sub-budgets.
   - The UI already switches to `Stopping` before the recorder stop, so users
     get instant feedback; the post-roll adds ~250 ms to stop-to-text.

2. **Measure whether post-roll saved speech** (no audio or text leaves the app).
   - Add `post_roll_speech_detected: bool` (and the post-roll length in ms) to
     `CaptureAudioMetrics` (~234): true when the level/silence detector saw
     speech during the post-roll window. Log it with the existing stop logs.
     This is the number that later tells us, per engine and version, how often
     words would have been cut and whether 250 ms is enough. Do NOT add a new
     PostHog event in this change (separate observability slice); just make
     sure the value is available on the metrics snapshot.

## Constraints

- No behaviour change for cancel, errors, size limits or app shutdown.
- No new threads; the sleep happens on the existing recording worker thread.
- Keep the realtime audio callback allocation-free as it is today.
- Don't touch the transcription engines, trimming or resampling.
- Match surrounding code style and comment density.
- Do not commit or push.

## Tests (must pass)

- Unit test: immediate `stop_recording()` still stops without waiting for any
  post-roll (use the existing test seams around the drain barrier ~1400-1432,
  or factor the post-roll decision into a small pure function and test it).
- Unit test: a stop with post-roll delays setting `stop_requested` by at least
  the post-roll duration (mock/injected clock or measured elapsed time with a
  generous margin — no flaky tight timing).
- Unit test: `CaptureAudioMetrics` snapshot carries the post-roll fields.
- Existing suites: `pnpm test:backend` (or `cd src-tauri && cargo test`),
  `cargo clippy --all-targets -- -D warnings`, `pnpm typecheck`, `pnpm test`
  if any TS type mirrors `CaptureAudioMetrics`.

## Acceptance (Claude verifies)

- All gates green.
- Manual: dictate a sentence and release the key on the last word — the last
  word appears; logs show `post_roll_speech_detected=true` for such stops.
- Cancel (Escape) is still instant and deletes the recording.
