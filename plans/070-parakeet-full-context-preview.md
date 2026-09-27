# Plan 070 — Parakeet full-context live preview (decode-ahead v2)

Status: DRAFT (2026-09-27). Continues [plan 069](069-handy-informed-audio-streaming-recovery.md)
stage "Repair TDT preview context/commit behavior" and the
[2026-09-26 checkpoint](../docs/reports/2026-09-26-parakeet-recovery.md).

Scope: `sidecar/parakeet-swift/Sources/main.swift` only (`DecodeAheadAsrSession` and
`ActiveStreamSession` decode-ahead ingress). The sidecar protocol (`stream_partial`
committed/tentative, `stream_final`, `stream_cancelled`) is unchanged, so no Rust or
TypeScript change is required. Batch stays the authoritative pasted text
(`src-tauri/src/commands/audio.rs:361`).

## Problem

Parakeet TDT live preview is unusable on anything longer than a short phrase, while
batch on the same audio is fine (2026-09-26 checkpoint, realtime-paced sidecar):

| Fixture | Batch WER | Current preview WER | Current preview text (excerpt) |
|---|---:|---:|---|
| en-5s (9.1 s) | 6.2% | 31.2% | "The voice Transcription R Miss records Latency and Accuracy…" |
| en-15s (13.2 s) | 7.1% | 57.1% | "A careful Oh and And shortly, sure at me Diamond long Examples…" |

## Root causes (verified)

1. **No left context after a commit.** `ingest` advances `head` past committed audio
   (`main.swift:703-707`) and the next decode starts exactly there
   (`currentWindow`, `main.swift:622-629`). Parakeet then hears a mid-sentence
   fragment as a new utterance: sentence-start capitalization on every window
   ("Transcription", "Latency", "Examples") and mid-word misrecognition
   ("harness" → "R Miss"). Event log: `.tmp/recovery-20260926/tdt-events.json`.
2. **Commits after one hypothesis.** A token is committed permanently as soon as one
   decode puts it ≥1.5 s before the tail with a ≥0.15 s token gap
   (`main.swift:675-696`). TDT timings are emission frames (80 ms), so the gap test
   passes almost everywhere and unstable words get baked in.
3. **Audio dropped under load.** Chunks queue behind a running decode; the queue drops
   the oldest buffer at 32 (`main.swift:338-341`), punching holes into the preview
   audio on slower machines.

Cost fact that makes the fix free: FluidAudio pads every ≤15 s input to exactly 15 s
(`FluidAudio/.../TDT/AsrManager+Transcription.swift:15-17`). Decoding 14 s of context
costs the same as decoding 1 s.

## Evidence for the fix

Growing-prefix batch re-decode (= full-context preview) with the real sidecar,
1 s steps, debug build, M4 Pro (`scratchpad prefix_experiment.py`):

- en-5s: every intermediate hypothesis is correct except the last 1–2 words; final
  6.2% (= batch). Per decode 219–364 ms.
- en-15s: early hypotheses garble the not-yet-finished phrase ("pants is sure to eat
  him"), then self-correct at 6 s ("benchmark eats short medium and long samples").
  Final = batch (7.1%). Per decode 244–478 ms.
- de-15s (8.0 s): converges to batch; 144–239 ms per decode.

Lesson from en-15s: a word agreed by two consecutive hypotheses near the tail can still
be wrong, so confirmation needs **three** agreeing hypotheses **and** a tail margin.

## Design

Keep the actor, generation gating, cancel semantics, detokenizer, and emission
routing. Replace the window/commit logic.

State (16 kHz mono f32):
- `samples: [Float]`, `baseOffset: Int` (absolute index of `samples[0]`, for compaction).
- `windowStart: Int` (absolute), `committedEnd: Int` (absolute; audio before it is
  already committed text), `committed: String`.
- `history: [[Word]]`: the last two post-filter hypotheses. `Word = (text, normKey,
  startAbs, endAbs, pieces)`; `normKey` = casefolded letters/digits only.

Constants: `minSamples = 16_000` (1 s before the first decode), `stepSamples = 8_000`
(decode again after ≥0.5 s new audio), `maxWindow = 224_000` (14 s),
`slideTrigger = 192_000` (12 s), `leftContext = 48_000` (3 s), `tailMargin = 32_000`
(2 s), `forceCommitAge = 96_000` (6 s), `timeTolerance = 2_560` (160 ms = 2 frames).

Ingress: `audio_chunk` resamples and appends immediately (no pending-buffer queue,
nothing dropped). A single decode loop task runs while the stream is open: when
`end - lastDecodeEnd ≥ stepSamples` (and `end - windowStart ≥ minSamples`), decode
`samples[windowStart ..< end]` capped at `maxWindow`. Decodes never overlap; the loop
always takes the newest audio (natural throttling on slower Macs).

Per decode:
1. Fresh `TdtDecoderState`, `manager.transcribe(window)` (unchanged helper).
2. Absolute token times: `windowStart + t * 16_000`.
3. Drop tokens with `startAbs < committedEnd - timeTolerance` (left-context region).
4. Group tokens into words (`▁`/leading-space starts a word).
5. Confirm: longest prefix of words where each word ends ≤ `end - tailMargin` AND a
   word with the same `normKey` and `|startAbs Δ| ≤ timeTolerance` exists in both
   `history` entries. Append their detokenized text to `committed`; set
   `committedEnd` to the last confirmed word's end plus half the gap to the next word
   (or its end if none).
6. Slide: if `end - windowStart > slideTrigger`, set
   `windowStart = max(windowStart, committedEnd - leftContext)`. If the window still
   exceeds `maxWindow`, force-confirm words ending ≤ `end - forceCommitAge` (bypass
   agreement), then slide again. Compact `samples` below `windowStart`.
7. Emit: committed partial if it grew (full cumulative, monotonic), then tentative =
   detok of the unconfirmed words (always sent, even empty).
8. `history` = last two filtered hypotheses (rebase nothing: times are absolute).

Finalize: stop the loop, await any in-flight decode, then decode the remaining window
(slide with force as needed until the whole tail fits) and commit every word after
`committedEnd`. Return `committed`. Cancel: bump generation, clear state, stop the loop.

Fallback when the model returns text without timings (`modelReturnedTimings: false`):
treat the whole text as tentative; commit it only at finalize or forced slide.

## Tests

1. Pure planner (`DecodeAheadPlanner` struct, no FluidAudio) with a deterministic
   harness flag `--decode-ahead-v2-harness`, like the existing token harness
   (`main.swift:864-935`): left-context tokens dropped; agreement needs 3 hypotheses;
   tail margin respected; committed text monotonic; slide keeps 3 s context; forced
   commit at 14 s; finalize commits everything once; no duplicate/missing word across a
   slide; timing-less fallback.
2. Existing token-normalization harness still passes.
3. Real sidecar, realtime-paced (`.tmp/recovery-20260926/smoke.py --model tdt`):
   en/de/es/it 2s/5s/15s fixtures, a ≥30 s concatenation (forces slides), and a
   clip with a ≥3 s silence tail. Lifecycle checks as before (stream_busy, cancel, no
   late events, restart).

## Acceptance

- Preview-final WER ≤ batch WER + 3 points on every fixture; ≥30 s clip ≤ batch + 5.
- Committed text word-diff vs batch ≤ 5% (preview may be revised, committed may not).
- First nonempty partial ≤ 1.5 s after audio start; tentative updates ≥ 1/s during speech.
- No "dropped" log lines; no sentence-start capitalization in the middle of committed
  text on the English fixtures.
- Lifecycle results unchanged. Release build measured for per-decode latency.

## Non-goals / follow-ups

- Making the Parakeet stream final authoritative (skip the post-stop batch decode for
  ≤14 s utterances → faster stop-to-paste). Needs its own proof; not in this plan.
- Whisper decode-ahead has the same no-left-context shape but commits at segment
  boundaries; measure it separately before changing anything.
- FluidAudio upgrade / Parakeet Ultra / vocabulary boosting: separate slices.

## FluidAudio 0.17.4 probe (2026-09-27, isolated copy `.tmp/fa-upgrade`, release build)

Current sidecar source compiles unchanged against 0.17.4 (`21493f8d`). Same fixtures:

| Engine | 0.15.5 batch / preview WER | 0.17.4 batch / preview WER |
|---|---|---|
| TDT en-2s | 0% / 0% | 0% / **85.7%** |
| TDT en-5s | 6.2% / 31.2% | 6.2% / 50% |
| TDT en-15s | 7.1% / 57.1% | 7.1% / 53.6% |
| Unified en-2s / en-5s | 100% / 81.2% | identical (empty; truncated) |
| Nemotron en-2s / en-5s / it-5s | 42.9% / 93.8% / 25% | 42.9% / 87.5% / 25% |

The upgrade fixes neither native engine, and the current commit rule gets *worse*
with faster/different timings — more evidence the preview algorithm, not the SDK, is
the defect. Upgrade after this plan lands, re-running the same acceptance set.
Worth it then for #909 (whole-window blank decodes on 11–13 s spans, fixed 0.15.8)
and the drop-in Parakeet Ultra model (v0.17.3, lower WER than v3 at the same speed).
