# Plan 048 — Wave 3a: route uploads through the executor (#16-full)

> From arch-roadmap Wave 3 / T1-C "full slice". Wave 0 already shipped the DISCRETE
> #16 fixes (timeout wrappers, effective duration, cloud guard) via
> `run_upload_local_engine_with_timeout` / `run_upload_cloud_with_timeout` in the
> upload match. This slice SUPERSEDES those by routing `transcribe_audio_file_impl`
> through `transcribe_with_app`, deleting the ~220-line parallel dispatch so uploads
> and the desktop recording path share ONE engine dispatch. Higher blast radius (the
> executor is shared with live dictation) → the whole test suite must pass and the CLI
> JSON contract must be byte-identical. Line refs are current-tree anchors; re-verify
> by semantics.

## Grounded shapes (current)

- `TranscriptionRequest` (request.rs:141): `source, audio, engine, spoken_language,
  task, context, timeout, cancellation, initial_prompt` — NO `audio_ctx`, NO
  `speed_mode_override`, NO diarize.
- `EngineSelection` (request.rs:36), `TimeoutPolicy` (:46), `CleanupPolicy` (:29).
- `transcribe_with_app` (executor.rs:50) → `route_once` (:176) → `run_with_policy` (:307).
  route_once's Whisper arm calls `transcribe_whisper_with_acceleration(..., None, None, …)`
  — passing `None` for audio_ctx and speed_mode_override today (adaptive default + settings).
- Upload: `transcribe_audio_file_impl` (audio.rs:6485), match at :6588 with Whisper/
  Parakeet/Cloud/Remote arms using the Wave-0 timeout wrappers. It threads the CLI's
  `audio_ctx: Option<i32>` and `speed_mode_override: Option<bool>` into the whisper arm.
- Diarized cloud: the upload path uses `transcribe_typed_diarized`; the executor cloud
  arm may not diarize — verify.

## Scope

1. **Extend `TranscriptionRequest`** with `audio_ctx: Option<i32>` and
   `speed_mode_override: Option<bool>` (or a nested `WhisperOverrides { audio_ctx,
   speed_mode }` to keep the struct tidy). Default to `None` everywhere they're not set
   (the desktop recording path builds the request WITHOUT them → both `None` → today's
   adaptive-default + settings-read behavior UNCHANGED).
2. **route_once Whisper arm**: pass `request.audio_ctx` and `request.speed_mode_override`
   (via engines::transcribe_whisper_with_acceleration) instead of the hardcoded `None,
   None`. Desktop path passes None → identical behavior; upload/CLI passes its overrides.
3. **Rewrite `transcribe_audio_file_impl`** to build a `TranscriptionRequest`
   (`source: Upload`, `audio: TranscriptionAudio::File(path)`, resolved engine/language/
   task/initial_prompt, `audio_ctx`, `speed_mode_override`, `timeout: TimeoutPolicy::Upload`,
   `cleanup: CleanupPolicy::CallerOwns`) and call `transcribe_with_app`, then map the
   `TranscriptionResult` → `UploadTranscription` preserving EVERY current JSON field
   (text, words, metadata incl. audio_duration_ms, processing_duration_ms, timings_ms,
   model, engine). DELETE the 4-arm match AND the Wave-0 `run_upload_*_with_timeout`
   helpers (now redundant — the executor's `run_with_policy` provides the timeout).
4. **Diarized cloud**: if the executor cloud arm cannot diarize, KEEP the diarized-cloud
   case as a pre-dispatch early-return branch in `transcribe_audio_file_impl` (it returns
   a different shape — `UploadDiarizationSegment`s). Do not force it through the executor.
5. **Remote arm**: LEAVE the upload Remote arm inline (documented Stage 5 deferral — that's
   #19, a later slice). Only Whisper/Parakeet/Cloud route through the executor here.
6. **Span timings**: ensure the executor result carries `span_timings_ms` for the CLI JSON
   (executor Parakeet branch reads `manager.latest_timing_snapshot()`; Whisper carries its
   own). Verify the CLI `timings_ms` field is still populated.

## Acceptance
- **CLI JSON golden**: `voicetypr transcribe --file <wav> --engine whisper --model
  large-v3-turbo --language en --json` returns the SAME field set as before (I will run
  this A/B vs the pre-change binary). `--audio-ctx` and `--speed-mode=true` still take
  effect through the new path.
- **Hung-engine → Timeout** (not a hang) for uploads; Parakeet upload duration-0 → header-
  derived (the Wave-0 behaviors preserved, now via the executor).
- `cargo test` + `cargo clippy --workspace --all-targets -- -D warnings` + `pnpm
  typecheck/lint/test` green — the DESKTOP recording path tests must pass UNCHANGED (proof
  the shared executor change didn't perturb live dictation).
- Do not commit.
