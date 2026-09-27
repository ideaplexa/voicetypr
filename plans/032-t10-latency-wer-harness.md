# Plan 032 — T1.0 latency + WER measurement harness

> The gate for every transcript-affecting perf win (audio_ctx, flash-attn, turbo,
> Parakeet warm measurement). Nothing WER-affecting ships without a before/after from
> this harness. All grounding verified against main @ af63ab1 (Explore run 2026-07-02).

## Verified foundation

- Headless CLI exists: `voicetypr transcribe --file <p> [--model m]
  [--engine whisper|parakeet] --json` (cli.rs:41,83,371,397 →
  commands/audio.rs `transcribe_audio_file_for_cli` → `transcribe_audio_file_impl`).
  CORRECTION (verified 2026-07-02): the CLI does NOT route through
  executor::route_once — `transcribe_audio_file_impl` matches ActiveEngineSelection
  directly, calling `transcribe_whisper_with_acceleration` (whisper) and
  `parakeet_manager.load_model` + `transcribe_with_custom_vocabulary` (parakeet)
  itself (~audio.rs:6412+). Decode goes through the same transcriber/manager layers
  as the GUI, so decode timings are representative; span sites must live in those
  SHARED layers, not the executor. JSON output: {text, words, metadata, model, engine}.
- Whisper spans already logged via `log_performance`: TRANSCRIBER_INIT
  (transcriber.rs:28-196), AUDIO_PREPROCESSING (:366-449), WHISPER_INFERENCE
  (:604-637), TEXT_EXTRACTION (:670-702). Output carries audio_duration_ms +
  processing_duration_ms (consumed executor.rs:205-206).
- Parakeet has NO Rust-side timing: executor branch executor.rs:208-273 (load :212,
  decode :238), sidecar spawn sidecar.rs:121-128, manager load manager.rs:294. Only
  timeout bounds exist (messages.rs:77-81).
- FullParams site: transcriber.rs:480-581. NO set_audio_ctx anywhere.
- package.json `"benchmark": "node scripts/benchmark-runner.js"` — target file MISSING.
- Fixture: tests/fixtures/audio-files/test-audio.wav (orphaned, usable for smoke).

## ⚠️ Pre-flight question to answer FIRST (do not skip)

transcriber.rs:493-509 resolves language; line ~496 forces "auto" → "en". Investigate
and report (do not change behavior in this plan): does a user with language=auto
speaking French get set_language("en")? If yes, say so explicitly in your report —
it changes how the WER baseline must be read, and we will fix it as its own gated
change. Check where the language setting comes from (settings store → executor →
transcriber) and what values reach :496.

## Scope

### 1. Parakeet spans (Rust, log-only) — in the SHARED layer
`Instant::now()` + `log_performance` INSIDE the ParakeetManager/sidecar methods so
both the GUI executor path and the direct CLI path emit them:
- sidecar spawn (sidecar.rs:121 `ParakeetSidecar::spawn`) → `PARAKEET_SPAWN`
- model load (parakeet/manager.rs `load_model` / `load_model_with_cancel` body)
  → `PARAKEET_MODEL_LOAD`
- decode (parakeet/manager.rs `transcribe_with_custom_vocabulary` body, ~:409)
  → `PARAKEET_INFERENCE`
Match the existing log_performance format used by the whisper spans. For timings_ms
threading (part 2), have these methods return/record elapsed so the CLI can surface
them without log scraping.

### 2. Timing in CLI JSON output (both engines)
Extend the `--json` metadata of `transcribe` with a `timings_ms` object:
- whisper: preprocessing, inference, extraction, total (already measured — thread the
  values through instead of re-measuring; extend WhisperTranscriptionOutput as needed)
- parakeet: model_load, inference, total
No log scraping in the harness — JSON only.

### 3. Harness-only CLI flags (behavior-neutral: default path MUST be unchanged)
- `transcribe --language <code>`: overrides the settings-store language for this run
  (thread to set_language; document interaction with the auto→en finding).
- `transcribe --audio-ctx <n>`: calls params.set_audio_ctx(n) ONLY when present
  (verify whisper-rs 0.16 exposes set_audio_ctx; if the binding is missing, report
  instead of hacking). This is the sweep hook for the future audio_ctx work.
Both flags: plumb through TranscribeArgs (cli.rs:83) → transcribe_audio_file_for_cli →
executor path with Option<>s defaulting to today's behavior.

### 4. `scripts/perf-harness.mjs` (+ fix package.json benchmark slot)
- Input: `--corpus <dir>` containing manifest.jsonl, lines:
  `{"file":"...","lang":"en","reference":"...","bucket":"2s"|"5s"|"15s"}`
- Matrix: `--engines whisper,parakeet` × `--models <csv>` × corpus × `--reps N` (default 5)
- Runs the built CLI binary (accept `--bin <path>`; document how to build:
  `cargo build --release` in src-tauri and point at target/release binary; also accept
  a debug binary path for smoke)
- Collects timings_ms + text per run
- Computes: p50/p95 per span per engine/model/bucket; WER vs reference (word-level
  Levenshtein, lowercase, strip punctuation — implement in the script, no deps);
  last-word-present check (last reference word appears in output tail); per-language
  WER table
- Output: `perf-report.json` + `perf-report.md` (tables, deterministic ordering so
  two reports diff cleanly). Header must state corpus type (synthetic vs real).
- First failure mode handled: model not installed → skip with a clear SKIPPED row,
  never a crash.

### 5. `scripts/gen-corpus.mjs` — bootstrap corpus (synthetic)
- macOS `say` voices → wav (say outputs AIFF; convert via `afconvert` to 16k wav or
  let the app normalize — prefer plain wav out of the generator)
- Languages: en fr de es it pt nl + sv/pl if a voice is installed (probe `say -v '?'`,
  skip missing with a note)
- 3 buckets per language: ~2s / ~5s / ~15s scripted sentences (fixed text in the
  script so references are exact); write manifest.jsonl
- Print the standard disclaimer: synthetic TTS is valid for RELATIVE regression
  deltas, not absolute WER claims.

## Acceptance
- `pnpm benchmark -- --corpus <generated> --engines whisper --models <one installed> --reps 2`
  produces both reports end-to-end on this machine (use whatever whisper model is
  installed; if none, run the smoke on tests/fixtures/audio-files/test-audio.wav and
  say so).
- Default CLI behavior byte-identical when new flags absent.
- cargo test, pnpm typecheck/lint/test all pass.
- Report the auto→en pre-flight finding explicitly.
- Do not commit.

## Out of scope
Changing audio_ctx defaults, flash-attn, turbo, model steering (all ship LATER through
this gate); real-speech corpus acquisition; GUI stop→paste tail spans.
