# Plan 042 — EOU streaming engine + user-facing "Live preview" mode

> Branch feat/042-eou-streaming off integrate/perf-ux-2026-07 (contains ALL prior
> slices: 037 contract, 038 tap, 039 pill preview, 040 session protocol + 033
> warmup — read them). This slice makes live preview REAL and USER-FACING for
> local Parakeet: the EOU streaming engine behind the existing session protocol,
> a rollback-safe model-activation flow, and a capability-driven settings toggle.
> Final pasted text STAYS the batch v2/v3 result (accuracy unchanged); the live
> preview is the streaming layer on top.

## Verified groundwork (in this branch already)

- Sidecar session protocol (main.swift): start_stream/audio_chunk/finalize_stream/
  cancel_stream + unsolicited stream_partial, one session max, SlidingWindow-backed
  ActiveStreamSession. Gate data: SlidingWindow useless for live dictation (partials
  only at finalize) — hence this slice.
- FluidAudio v0.15.2 (pinned): `StreamingEouAsrManager` actor —
  init(configuration:chunkSize:), chunk sizes .ms160/.ms320/.ms1280 (each a separate
  CoreML encoder export, HF repos parakeetEou160/320/1280, model
  nvidia/parakeet_realtime_eou_120m-v1, ~120M params, ENGLISH-ONLY);
  loadModels(...) downloads via DownloadUtils.downloadRepo; appendAudio(_:),
  process(audioBuffer:), finish()->String, reset(); setPartialCallback((String)->Void)
  (accumulated partial transcript), setEouCallback, eouDetected. Also
  StreamingModelVariant.createManager(...). Paths in
  .build/checkouts/FluidAudio/Sources/FluidAudio/ASR/Parakeet/Streaming/.
- Rust: ParakeetClient::open_stream + ParakeetStreamHandle (sidecar.rs), stream sink
  wiring in stream_tap/recorder/audio.rs behind streaming_tap_enabled +
  streaming_engine_enabled; 037 contract events to pill; pill renders
  committed/tentative behind streaming_preview_enabled (039).
- EngineStreamCapabilities (transcription/stream.rs) — all engines final_only today.

## Scope

### A. Sidecar: EOU engine + model management (main.swift)
1. `start_stream` gains `engine: "sliding_window" | "eou"` (default sliding_window
   for back-compat) and for eou a `chunk_ms: 160|320|1280` (default 320 — measured
   best latency/efficiency balance candidate; bench decides).
2. EOU session path: StreamingEouAsrManager; audio_chunk → appendAudio/process;
   partial callback → committed/tentative mapping: maintain `committedPrefix` that
   grows ONLY at EoU boundaries (on eouDetected/eou callback, fold the current
   partial into committed); tentative = partial text beyond committedPrefix. Emit
   stream_partial {text (tentative), is_confirmed:false} for partial updates and
   {text (new committed total), is_confirmed:true} at EoU folds — SAME event shape
   040 already defined (Rust maps is_confirmed to committed/tentative today; verify
   the Rust mapping accumulates committed correctly and monotonic guard holds).
   finalize_stream → finish() → stream_final.
3. Model management: new commands `eou_model_status {chunk_ms}` → {downloaded:bool,
   path?} (check FluidAudio cache dir presence WITHOUT triggering download) and
   `download_eou_model {chunk_ms}` → drives loadModels with progress events
   (existing progress mechanism), responds ok/error. Keep the downloaded manager
   cached for the session start. Failure → clean error, no partial state.

### B. Rust: activation flow + capability truth
1. ParakeetManager: `eou_model_status`, `download_eou_model` (progress → existing
   pattern), stream open passes engine+chunk_ms.
2. Capability: EngineStreamCapabilities::PARAKEET becomes supports_streaming=true
   (macOS only — cfg or runtime platform check; Windows keeps final_only). Add a
   command `get_active_stream_capabilities` returning the ACTIVE engine's
   capabilities + whether the EOU model is downloaded — the single source the
   settings UI reads. Cloud providers keep final_only (slice 043 flips them).
3. New setting `transcription_mode: "regular" | "live_preview"` (default regular).
   ROLLBACK-SAFE ACTIVATION: a command `activate_live_preview` that (a) checks
   capability, (b) if EOU model missing → download with progress events,
   (c) verifies with a tiny in-sidecar EOU warm session (~1s silence, like 033
   warmup), (d) ONLY THEN persists transcription_mode=live_preview. Any failure/
   cancel/restart mid-way → mode stays regular, clean error surfaced, partial
   downloads either resumed or cleaned by FluidAudio's downloader (verify which).
   Deactivation = set regular, nothing to undo.
4. Recording path: when transcription_mode==live_preview AND capability holds →
   enable the existing tap+stream+preview path (the three internal flags become
   driven by this ONE user setting; keep the internal flags as dev overrides but
   the setting is the product switch). Batch paste path untouched.

### C. Frontend: the mode control + activation UX
1. Settings (models/transcription section — follow existing sections style):
   "Transcription mode" radio/segmented: Regular | Live preview (English), visible
   ONLY when get_active_stream_capabilities says supports_streaming; disabled state
   with tooltip on Windows/unsupported. Choosing Live preview when model missing →
   inline download progress (reuse existing model-download progress UI patterns) →
   success activates, failure reverts with error toast. Label the English-only
   nature honestly ("Live preview is English-only for now; your final text still
   uses your selected model").
2. Pill: streaming_preview_enabled behavior now follows the user mode (no UI change
   needed beyond what 039 shipped — verify the gating chain).
3. Tests: settings control visibility per capability; activation happy path +
   download-failure revert (mocked invokes); mode persistence.

### D. Measurement (proof)
stream-bench: `--engine eou --chunk-ms 160|320` paths; run on the synthetic en
corpus 2s/5s/15s, 3 reps: report first_partial/first_confirmed(EoU)/final vs the
oracle gate (≤700ms p50 first partial, ≤1800ms first committed). Compare 160 vs
320 and recommend the default.

## Acceptance
- cargo test + clippy -D warnings + pnpm typecheck/lint/test ALL pass.
- Default (regular mode): zero behavior change.
- stream-bench EOU numbers reported vs gate (the EOU model download ~250MB is
  authorized — disk has 25GB free).
- Manual GUI proof deferred to user smoke; everything else automated.
- Activation flow: simulated failure (e.g. status check offline) leaves mode
  regular — test at the Rust level where feasible.
- Do not commit.

## Outcome (2026-07-02)

Shipped-but-dormant. The session protocol, sidecar EOU engine, activation flow,
`transcription_mode` setting, settings UI, and `stream-bench` support remain in
tree, but `EngineStreamCapabilities::PARAKEET` is final-only so the user-facing
mode control stays hidden until upstream EOU is fixed.

Empirical verdict:
- SlidingWindow is functional but fails the live-preview latency gate by design.
- EOU 160ms and 320ms are broken upstream on this machine: FluidAudio's own
  reference CLI decodes real speech to an empty transcript with both the pinned
  v0.15.2 build and latest v0.15.4, including after a fresh model download.
- FluidAudio's canonical fresh CLI model layout is flat:
  `~/Library/Application Support/FluidAudio/Models/parakeet-eou-streaming/320ms`.
  The sidecar resolver keeps both canonical and legacy fallbacks so existing
  caches are tolerated, but this slice does not expose the feature while upstream
  EOU returns empty transcripts.
- Unified (`parakeet-unified-en-0.6b`, FluidAudio v0.15.3+) follows a different
  path and works differently, but its 2080ms chunks fail the <=700ms first-partial
  gate for local live preview.

Local live preview therefore waits on upstream FluidAudio EOU. The multilingual
live-preview path remains the cloud WebSocket track, starting with Soniox.
