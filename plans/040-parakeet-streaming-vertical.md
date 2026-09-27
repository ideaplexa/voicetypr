# Plan 040 — Streaming slice 4: Parakeet streaming vertical (measured prototype)

> Branch feat/040-parakeet-streaming, stacked on committed 037 (contract) + 038
> (substrate). Goal: a WORKING, MEASURED SlidingWindow streaming prototype behind
> default-off flags — NOT a shipped default. The output of this slice is (a) real
> partials flowing sidecar→Rust→pill and (b) the numbers that decide
> SlidingWindow-vs-EOU per the oracle gate (06-oracle-decision.md): first usable
> partial p50 ≤700ms, first stable prefix p50 ≤1800ms on 2-8s utterances, final
> WER delta ≤ +1.5 abs. If SlidingWindow misses, this slice still stands — the
> session protocol is engine-agnostic and EOU slots in behind it (needs a new
> ~120M CoreML download; separate slice).

## Verified grounding (2026-07-02 Explore; FluidAudio v0.15.2 @ 7f963cdc)

- Sidecar loop main.swift:216 `runEventLoop`: one stdin JSON line → await handler
  → next line. Strictly request/response EXCEPT `progress` events emitted during
  load/download — the template for unsolicited events. No message ids anywhere.
- Rust sidecar.rs:150 `request_with_progress_and_cancel`: writes command, loops on
  rx; `Progress` → callback + keep looping (sidecar.rs:227-231); first other
  parsed response returns. `&mut self` = one in-flight command.
- SlidingWindowAsrManager (actor, SlidingWindowAsrManager.swift:10): loadModels
  takes existing TDT AsrModels (no download, :140); startStreaming (:156);
  streamAudio(AVAudioPCMBuffer) (:212); transcriptionUpdates
  AsyncStream<SlidingWindowTranscriptionUpdate{text,isConfirmed,confidence,…}>
  (:217, :803); finish()->String (:231); cancel() (:303). Configs (:678-711):
  .default chunk 15s/minConfirm 10s; .streaming chunk 11s/hyp 1s/minConfirm 10s —
  expect poor short-clip confirmation; hypothesis (volatile) updates ~1s cadence.
  Custom config allowed — prototype should try smaller hypothesisChunkSeconds
  (e.g. 0.5s) and record what actually helps.
- AVAudioPCMBuffer transcribe path resamples internally → Rust sends NATIVE-rate
  i16 chunks + {sample_rate, channels} in start_stream; Swift wraps into
  AVAudioPCMBuffer at native rate. NO Rust-side resampling.
- 038 substrate: stream_tap worker currently no-op; its on_frame observer is the
  feed point. StreamSessionGate (037) provides session/revision discipline.
  Pill preview (039, other branch) consumes "transcription-stream" events —
  emitting those events app-side is IN scope here (behind flags), the pill code
  is NOT (it merges separately; use the locked wire format from
  src-tauri/src/transcription/stream.rs).

## Scope

### A. Sidecar session protocol (main.swift)
New commands (same JSON-line transport):
- `start_stream {model_id, model_version, sample_rate, channels, config?}` —
  requires model loaded (reuse loadModel state); creates a SlidingWindowAsrManager
  session, `startStreaming()`, spawns a forwarder Task that consumes
  transcriptionUpdates and writes unsolicited
  `{"type":"stream_partial","text":…,"is_confirmed":…}` lines (serialize stdout
  writes with the existing response-writing path — verify how progress avoids
  interleaving and reuse it). Replies `{"type":"stream_started"}`.
- `audio_chunk {pcm_b64}` — base64 of little-endian i16 interleaved native-rate
  samples; handler decodes, wraps AVAudioPCMBuffer, `streamAudio()`, replies
  NOTHING (fire-and-forget; must return fast so the loop can read the next line).
- `finalize_stream {}` — `finish()`, emits `{"type":"stream_final","text":…}`,
  tears down session.
- `cancel_stream {}` — `cancel()` + teardown, replies `{"type":"stream_cancelled"}`.
- While a session is active, reject other heavy commands (transcribe/load) with a
  clear busy error; `status`/`shutdown`/`cancel_stream` allowed. One session max.

### B. Rust streaming client (parakeet/sidecar.rs + manager.rs)
- New session API, separate from `request()`: `open_stream(...) -> StreamHandle`.
  StreamHandle: `send_chunk(&[i16])` (write-only, no response wait),
  `finalize() -> final text`, `cancel()`. A dedicated receive loop task consumes
  rx: `stream_partial` → callback; `stream_final`/`error` → resolve finalize;
  reuse the progress-branch pattern. Timeouts: overall session inactivity 30s;
  finalize 30s.
- Wire into the 038 worker: when a NEW settings key `streaming_engine_enabled`
  (default false) AND streaming_tap_enabled are on AND active engine is parakeet
  with a loaded model: worker's on_frame → send_chunk (chunks are already
  Vec<i16> native rate); Finalize → finalize(); Cancel → cancel().
- Emit contract events (037 types) via tauri emit to the pill window on the
  "transcription-stream" channel: Started on session open; Partial with
  committed=confirmed text, tentative=volatile text (map
  SlidingWindowTranscriptionUpdate.isConfirmed accordingly — committed must be
  monotonic: only append confirmed text, never regress; guard with the 037 helper
  and drop violations with a warn log); Final on finalize. Session id = captured
  recording generation; revision = monotonic counter. IMPORTANT: the batch
  transcription path stays UNTOUCHED and remains the source of the pasted text —
  the stream result is preview-only in this slice (oracle: final insert once at
  stop via the existing path).

### C. Measurement (the decision data)
- Spans via log_performance: PARAKEET_STREAM_FIRST_PARTIAL (session open → first
  stream_partial), PARAKEET_STREAM_FIRST_CONFIRMED, PARAKEET_STREAM_FINAL.
- A test/dev harness path: extend scripts/perf-harness.mjs? NO — keep this slice
  focused; instead add a hidden CLI subcommand `stream-bench --file <wav>` that
  replays a wav through the session at real-time pace (chunk cadence = chunk
  duration) and prints the three spans + final text as JSON. Run it on the 032
  corpus 2s/5s/15s en clips (harness worktree corpus can be copied or
  regenerated with scripts/gen-corpus.mjs if absent on this branch — it IS
  present, 032 is merged into this lineage? VERIFY: this branch stacks on 037
  which branched from main BEFORE 032 — gen-corpus.mjs is NOT here. Regenerate
  a small en-only corpus inline or accept any wav path; do not port the harness).
- Report in your summary: p50-ish numbers (3 reps per bucket) for first partial /
  first confirmed / final, with .streaming config and with a tuned config
  (hypothesisChunkSeconds 0.5).

## Acceptance
- All flags off (default): zero behavior change; cargo test (1201+) passes;
  pnpm typecheck/lint pass (TS untouched or only types reused).
- Flags on, manual/CLI proof: stream-bench on a 5s clip shows partials flowing
  and a final; spans logged; no stdout protocol corruption (partials during an
  in-flight finalize handled).
- Batch path: recording→transcription→paste identical with flags on (stream is
  preview-only; if the stream session errors, recording is unaffected).
- Report the gate numbers + your read: does SlidingWindow pass the oracle gate
  or do we proceed to EOU?
- Do not commit.
