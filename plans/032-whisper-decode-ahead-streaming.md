# Plan 032 — Whisper decode-ahead streaming (local live preview)

> From plan 028 Phase 3. Makes **local Whisper** show live committed/tentative preview
> in the pill — via SLIDING-WINDOW DECODE-AHEAD, not native token streaming (whisper.cpp
> has NO KV/encoder-state reuse; `whisper_full` takes a full slice each call). Reference
> algorithm: `itsmontoya/scribble` `BufferedSegmentTranscriber` (deepwiki-verified). Same
> capability-driven `StreamTapSink` framework as Parakeet/Soniox — Whisper registers a
> decode-ahead sink; NO rewrite. Branch/worktree: feat/049 (same, phased commits).
>
> ⚠️ Final pasted text STAYS the batch `whisper_full`-at-stop result (accuracy unchanged);
> the decode-ahead layer is preview-only. (Result-authority stays batch — unlike Soniox,
> no double-billing concern since it's all local compute.)
>
> ⚠️ COST: decode-ahead re-decodes overlapping audio repeatedly → CPU-heavy. Gate it to
> live_preview mode + Whisper engine; it wants Metal/Vulkan. Best for long-form; on short
> 2–8s dictation confirmed text barely promotes before stop (acceptable — the batch final
> still lands). Do NOT enable by default on CPU-only.

## The algorithm (scribble, verified via deepwiki 2026-07)
Growing `samples: Vec<f32>` + `head` index; `window() = samples[head..]`.
- **Decide-to-decode** (`should_decode`): skip if `!eos && window < min_window` (~1s); else
  only re-run once window grew ≥ `incr_step` (~1s) since last attempt; FORCE at eos or when
  window ≥ `max_window` (30s * 16 000).
- **Run** `whisper_full` on the window → segments (each: text + `end_timestamp` centiseconds).
- **Emit rule**: if ≥2 segments → commit all-but-last (last is tentative); at eos/max-cap →
  commit everything; else emit nothing.
- **Advance head** past the last COMMITTED segment's end (`end_cs/100 * 16000` samples), so
  finalized audio is never re-decoded. `maybe_compact()` drains consumed samples when ≥1s
  consumed or head past halfway (bounds memory).
- **Backoff**: when ≤1 segment (no progress), wait longer before retry (up to 16× min_window)
  to avoid thrashing on ambiguous boundaries.
- committed grows append-only (satisfies `assert_committed_monotonic`); tentative = the last
  (uncommitted) segment's text, replaced each decode.

## Hybrid split
**GLM (xhigh) — pure, unit-testable buffer (no whisper, no threads):** module
`whisper/decode_ahead.rs`:
```
struct DecodedSegment { text: String, end_cs: i64 }   // abstract whisper output
struct DecodeAheadBuffer { samples: Vec<f32>, head, committed: String, next_infer_at,
                           no_progress_runs, cfg }
  push(&mut, &[f32])
  window(&self) -> &[f32]
  should_decode(&self, eos: bool) -> bool
  ingest(&mut, segs: &[DecodedSegment], eos: bool) -> DecodeAheadPartial{committed,tentative}
     // commit-all-but-last / advance head via end_cs->samples / backoff / compact
```
Tests: window grows; min-window gate; ≥2 segs commits all-but-last + advances head; eos
commits all; head-advance means committed audio isn't in window next round; backoff on ≤1
seg; committed append-only monotonic; compaction bounds len. (16 kHz mono f32 assumed —
the tap resamples? see below.)

**Claude — the whisper integration + threading + sink + routing:**
- `WhisperStreamSink` (impl `StreamTapSink`): `send_frame(&[i16])` → convert to f32
  (`convert_integer_to_float_audio`) + mono (`convert_stereo_to_mono_audio` if channels>1) +
  RESAMPLE to 16 kHz (whisper needs 16k; the tap is at device rate) → enqueue to a **dedicated
  std::thread** (whisper_full is sync+blocking — must NOT run on the recorder worker thread).
- Decode thread: owns a `WhisperContext` state loop — `push` samples, `should_decode`?, then
  `ctx.create_state()` + `state.full(params, buffer.window())` → collect `full_n_segments` /
  `get_segment(i)` (.to_string() + end timestamp) into `DecodedSegment`s → `buffer.ingest` →
  emit `Partial{committed,tentative}` via the shared gate. On finalize: force eos decode, emit
  `Final`, return committed. On cancel: stop + `Cancelled`.
- Factory `build_whisper_stream_sink_factory` (mirror parakeet) — guard whisper + live_preview
  + model-loaded; get the `WhisperContext` from `WhisperManager`; greedy fast params (best_of 1,
  no timestamps-print) for preview speed. FP note: preview uses a SEPARATE state so it never
  disturbs the authoritative batch decode at stop.
- Dispatch (`audio.rs:4002-4015`): engine-match adds `"whisper" => build_whisper_stream_sink_factory`.
  KEEP the CRITICAL trap fix (Parakeet routed by its own factory, byte-identical).
- Capability: flip `EngineStreamCapabilities::WHISPER` to streaming (with the truth-table test);
  reconcile with `commands/model.rs:76/164` which read capabilities for frontend metadata.
- `emit_parakeet_stream_event` → engine-neutral rename (shared).

## Verification
Automated: GLM buffer unit tests; sink threading (send_frame non-block; finalize returns
committed; cancel); build + clippy -D warnings + all tests + windows-check. Decode-ahead
QUALITY (does the preview read well, latency) = manual smoke — but unlike Soniox, NO key
needed, so smoke is just "record + watch the pill" on any Whisper model. Whisper-Metal is
broken on THIS dev box ([[tahoe-metal-breaks-local-whisper-harness]]) → decode-thread logic
gated by unit tests + CPU-model smoke on another machine / CI.

## Sequencing
Independent of Soniox (3be80e6 WIP). Both are `StreamTapSink` factories behind the same
engine-dispatch; do Whisper fully, then re-wire Soniox into the same dispatch. The dispatch
generalization + capability flip + emit rename are shared plumbing — land them once here.
