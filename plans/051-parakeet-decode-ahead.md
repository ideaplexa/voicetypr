# Plan 051 — Parakeet decode-ahead live preview (bypass broken upstream EOU)

> Executable spec, Phase 1 (engine + bench gate). Task #19. Evidence trail (2026-07-10):
> - EOU is STILL broken on FluidAudio 0.15.5 (fresh model, rebuilt ModelHub): empty
>   transcripts. Track A stays dormant; capability flip waits on upstream.
> - The sidecar's SlidingWindow engine with a tuned small geometry (1s chunk/0.5s
>   right) hits the latency bar (first partial 1.5s, ~1s cadence) but the TEXT IS
>   GARBAGE ("careful pants mark", "o windowo window"): FluidAudio's loop PERMANENTLY
>   bakes each chunk's tokens at decode time, so boundary-cut words can never be
>   corrected. Structural, not tunable.
> - Fix: port the PROVEN plan-032 Whisper decode-ahead semantics
>   (src-tauri/src/whisper/decode_ahead.rs — read it FIRST, it is the reference
>   implementation with 25 tests) into the sidecar as a third stream engine
>   `decode_ahead`: fresh coherent decode of the whole window every ~1s, commit
>   only by timestamp, tentative tail stays revisable. Preview-only: the pasted
>   text stays the batch decode at stop (same stance as Whisper).
>
> Phase 2 (capability flip + activation + frontend) happens ONLY after the Phase 1
> bench gate passes — it is NOT in this slice.

## Grounded APIs (FluidAudio 0.15.5, verified in .build/checkouts)

- `AsrManager.transcribe(_ samples: [Float], decoderState: inout TdtDecoderState,
  language: Language? = nil) async throws -> ASRResult` — 16kHz mono f32 in;
  `ASRResult { text, confidence, duration, tokenTimings: [TokenTiming]? }`.
- `TokenTiming { token: String, tokenId: Int, startTime: TimeInterval,
  endTime: TimeInterval, confidence: Float }` — token is a SentencePiece piece
  (leading `▁` marks a word start).
- Fresh state per decode: `TdtDecoderState.make(decoderLayers: await
  manager.decoderLayerCount)` (see SlidingWindowAsrManager.swift:704 for the idiom).
- `AudioConverter().resampleBuffer(_ buffer: AVAudioPCMBuffer) throws -> [Float]`
  — arbitrary rate/channels → 16k mono f32.
- The sidecar ALREADY holds a loaded batch `AsrManager` for the selected model —
  find how the `transcribe` command handler reaches it in main.swift and reuse
  THAT instance (do NOT load a second copy).

## Protocol contract (must match the Rust fold, audio.rs ~247)

- `stream_partial { text, is_confirmed: true }` → `text` is the FULL cumulative
  committed string; every emission must be a byte-prefix extension of the previous
  one (Rust asserts monotonicity and DROPS violators).
- `stream_partial { text, is_confirmed: false }` → `text` is ONLY the tentative
  tail (replaced wholesale each time; committed unchanged).
- `finalize_stream` → responds with the final text (committed + final tentative).

## Changes

### 1. Sidecar `sidecar/parakeet-swift/Sources/main.swift` — the engine

- `start_stream` accepts `engine: "decode_ahead"`; `ActiveStreamSession.engine`
  gains `.decodeAhead(DecodeAheadAsrSession)`.
- New `actor DecodeAheadAsrSession` (in main.swift, near the session plumbing),
  a faithful port of `DecodeAheadBuffer` (decode_ahead.rs) fused with its driver:
  - Config (16k samples): `minSamples = 16_000` (1s), `incrSamples = 16_000` (1s),
    `maxWindowSamples = 224_000` (14s — the model input is fixed 15s; leave 1s
    slack), `tailMarginSeconds = 1.5`.
  - State: `samples: [Float]`, `head: Int` (start of the un-committed window,
    ABSOLUTE index minus `droppedSamples` after compaction — mirror the Rust
    field structure exactly), `committed: String`, `nextInferAtLen: Int`,
    `noProgressRuns: Int`, one `AudioConverter`, a weak/unowned ref to the shared
    `AsrManager`.
  - `appendChunk(buffer: AVAudioPCMBuffer)`: resample → append; while
    `should_decode` (port the Rust rule: window length ≥ max(minSamples,
    nextInferAtLen), plus the eos variant) run one decode pass.
  - Decode pass: `window = samples[head...]` capped at `maxWindowSamples`
    (if longer, the compaction below has failed — assert/log); fresh
    `TdtDecoderState`; `let result = try await manager.transcribe(window, ...)`.
    Map `result.tokenTimings ?? []` to the Rust `DecodedSegment` shape:
    each token = a segment with `text = token piece`, `end = endTime`.
  - `ingest` (PORT THE RUST RULE EXACTLY, including no-progress handling):
    commit every token whose `endTime <= windowSeconds - tailMarginSeconds`
    (on eos/finalize: commit ALL tokens); the committed pieces are appended to
    `committed` via SentencePiece detok — replace `▁` with a space, then when
    appending to a non-empty `committed`, ensure exactly the spacing the pieces
    dictate (a piece starting with `▁` starts a new word; pieces without it glue
    to the previous word). Head advances past the last committed token's
    `endTime * 16_000` samples (relative to the window start), exactly like the
    Rust `ingest`. `tentative` = detok of the remaining tokens.
  - `maybeCompact` (port): when `head ≥ 16_000`, drop `samples[..<head]` and
    rebase indices; MUST NOT touch `nextInferAtLen` (that was the GLM-caught bug
    in the Rust original — keep the regression comment).
  - Emissions after each pass: if `committed` grew → send
    `{text: committed, is_confirmed: true}`; always send
    `{text: tentative, is_confirmed: false}` (even when empty — it clears the
    pill's stale tail).
  - `finalize()`: run a final decode with eos=true (commit everything), return
    `committed` as the stream final. `cancel()`: drop state, no emission.
- `audio_chunk` and `finalize_stream`/`cancel_stream` handlers: add the
  `.decodeAhead` arms mirroring the `.slidingWindow` ones (including
  `withLibraryStdoutRedirected`).
- Decode passes must be SERIALIZED per session (actor gives this) and must not
  block the stdin read loop — follow how the sliding-window path structures
  its awaits.

### 2. Rust plumbing (bench only — NO capability flip in this phase)

- `src-tauri/src/parakeet/messages.rs`: `ParakeetStreamEngine::DecodeAhead`
  (serde snake_case → "decode_ahead" on the wire).
- `src-tauri/src/cli.rs`: stream-bench accepts `--engine decode_ahead` (find the
  engine-string parse and extend it); config knobs are ignored by this engine.

## Gate (Claude runs, not GLM)

- `swift build -c release` clean; `cargo clippy --lib` + `cargo test --lib` clean.
- `stream-bench --engine decode_ahead` on en-2s/en-5s/en-15s:
  - first_partial_ms ≤ ~2500 (Whisper-preview bar, not the dead 700ms EOU oracle)
  - partials ≥ duration_seconds − 2 (real cadence)
  - final/committed text READABLE and ≈ the known corpus sentences (no
    "pants mark"-grade garbage, no duplicated fragments) — this is the whole
    point of the port; compare against the batch `transcribe` output.
- de/es clips optionally with the v3 model later; v2 is English — fine for gate.

## Phase 2 (separate, after gate): capability flip

PARAKEET → streaming (no endpointing) like WHISPER; factory sends
`ParakeetStreamEngine::DecodeAhead` + no config; `activate_live_preview` drops the
EOU download for Parakeet (decode-ahead needs no extra model — better activation
UX than EOU would have had); ModelsSection `livePreviewNeedsDownload` false for
parakeet; truth-table + frontend tests. EOU code stays for upstream-fixed future.

## Outcome (2026-07-14)

Implemented both phases. Parakeet live preview now uses decode-ahead immediately
with no EOU download; regular mode remains batch-only. The native EOU path stays
dormant after direct FluidAudio 0.15.5 testing produced zero RNNT tokens on
`all`, `cpuAndGPU`, and `cpuOnly` compute settings.

Final adversarial review fixed four additional correctness defects:

- recognize FluidAudio-normalized leading spaces as word boundaries, not only
  raw SentencePiece `▁`;
- move blocking stdin reads off `MainActor`, with bounded FIFO backpressure, so
  decode work cannot starve terminal commands;
- bound Rust and Swift preview queues while keeping cancel immediate and draining
  accepted audio before finalize;
- preserve nonempty decode text when token timings are unavailable, and always
  make bounded progress at the 14-second window cap.

Exact-source runtime evidence with the 1.5-second tail margin:

- en-5s: first partial 989 ms, first confirmed 2000 ms, 13 partials / 4 confirmed;
- en-15s: first partial 988 ms, first confirmed 1996 ms, 22 / 10;
- continuous >30s: first partial 982 ms, first confirmed 1993 ms, 49 / 19;
- cancel during decode: immediate `stream_cancelled`, zero stale partials;
- 16 seconds silence then speech, quiet ending, and immediate stop completed
  without starvation or authoritative batch truncation.

Final gates: Swift release build; 1,276 Rust library tests; clippy with warnings
denied; TypeScript typecheck; ESLint; 616 frontend tests. Final review: SOUND.
