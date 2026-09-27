# Plan 033 — Parakeet eager warm prediction (T1.2)

> Implemented on the perf/032-t10-harness branch/worktree, on top of the (uncommitted)
> 032 harness changes — it uses the new PARAKEET_* spans to prove the win.
> Kills the first-dictation ANE first-predict cost. Grounding verified 2026-07-02
> (Explore). HARNESS EVIDENCE (2026-07-02, parakeet-tdt-0.6b-v2, this machine):
> steady-state inference p50 78-90ms; model_load p50 ~154ms; first-ever-compile
> outlier 34.8s — the cold cliff this plan moves off the user's first dictation.
> NOTE: manager.rs/sidecar.rs were just touched by 032 (spans, AtomicU64 snapshot) —
> integrate with those, don't duplicate timing.

## Verified current behavior

- Startup autoload (lib.rs:1886-1906, inside perform_startup_checks spawned at :749):
  IF engine=="parakeet" AND model downloaded (set at lib.rs:1796-1806) → 
  parakeet_manager.load_model → sidecar spawn + AsrModels.loadFromCache +
  manager.loadModels (main.swift:369-389). Loads/compiles, NEVER runs an inference.
- Settings-change hook: settings.rs:760-770 (same load_model, background spawn).
- First real transcribe (main.swift:489-492 manager.transcribe) pays first-predict.
- Sidecar resident (ParakeetClient::ensure, sidecar.rs:289-299); no idle shutdown;
  respawn-once on Terminated; killed on Timeout/cancel (sidecar.rs:389-397) —
  NOTE: after a timeout/cancel kill, next dictation re-pays spawn+load+first-predict;
  re-warm after respawn is in scope via the same hook.
- No warmup command exists in the sidecar (main.swift:238-304 switch).
- Whisper analog exists: gpu_sidecar warm_on_preload (gpu_sidecar.rs:334-361).

## Scope

1. **Swift sidecar `warmup` command** (main.swift): after model is loaded, synthesize
   ~1s of silence in-memory (16kHz mono f32; write temp wav only if
   manager.transcribe requires a file URL — it does: transcribe(fileURL) — use a
   temp file in NSTemporaryDirectory, delete after), run one transcribe with fresh
   TdtDecoderState (pattern main.swift:489), discard result, respond
   {type:"warmed", ms:<elapsed>}. Errors are non-fatal (respond warmed:false).
2. **Rust: ParakeetManager::warmup()** (manager.rs): send Warmup command,
   SHORT_REQUEST_TIMEOUT... no — first warm IS the ANE compile+predict, can take
   seconds: give it its own WARMUP_TIMEOUT_SECS=60. log_performance PARAKEET_WARMUP.
3. **Call sites**: (a) lib.rs:1889 right after startup load_model succeeds;
   (b) settings.rs:764-769 after model-switch load; (c) after respawn-once retry
   path? NO — that's mid-real-request, the real request warms it. Keep (a)+(b).
   Both already run in background spawns — warm must not block or delay a real
   transcription: skip warmup if a transcription is in-flight (check existing
   busy/cancellation state in manager) and let a queued real transcribe win.
4. **Do NOT add a user setting** (none exists for preload today; keep parity).

## Measurement (through 032 harness/spans)

Before/after via app logs: cold app start → first dictation; compare
PARAKEET_INFERENCE of dictation #1 with/without warm (expect first-dictation
inference to drop to steady-state ~decode time). Also report PARAKEET_WARMUP ms
itself (the cost moved off the user's first dictation into startup background).

## Acceptance
- cargo test + typecheck/lint/test pass; sidecar builds (build.rs compiles Swift).
- Manual/scripted proof: logs show SPAWN→LOAD→WARMUP at startup, and first real
  dictation's PARAKEET_INFERENCE ≈ subsequent ones.
- No warm when engine != parakeet or model not downloaded (existing conditionals).
- A real transcription arriving during warmup is not delayed/broken (sidecar is
  single-request FIFO — verify ordering; if the sidecar serializes on main actor,
  warm simply finishes first — measure that worst case is acceptable vs today).
- Do not commit.
