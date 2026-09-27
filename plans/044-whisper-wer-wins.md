# Plan 044 — Whisper WER-gated wins (adaptive audio_ctx + speed mode)

> Authored from `docs/review/grounding-whisper-wins.md` (ultracode 2026-07-03).
> ⚠️ The file:line refs in this plan are APPROXIMATE ANCHORS captured before slice
> 045; the actual sites drift ~15-25 lines. Codex MUST locate each site by the
> DESCRIBED SEMANTICS (function name + what the code does), not the exact line, and
> only stop-and-report if the described code genuinely isn't there.
>
> Two INDEPENDENTLY-GATED perf sub-changes. Each ships through the perf-harness WER
> gate SEPARATELY (own A/B run). Do them in order; do not bundle.

## PRODUCT DECISION — language default is English, EVERYWHERE (auto NOT offered)

Owner decision (2026-07-03): **the Whisper speech-language default stays English
everywhere, and auto-detect is intentionally NOT offered.** Rationale: whisper.cpp
auto-detect is unreliable on SHORT dictation clips (2-8s gives the detector too little
signal), so for a dictation app it degrades quality. European users transcribe in their
language by EXPLICITLY selecting it (already works); the default is English.

Therefore the previously-drafted "fix auto→en + add Auto-detect UI option" work is
**DROPPED** — the existing `auto → en` squash (languages.rs `validate_language`,
settings.rs `normalize_speech_language_for_model`, transcriber.rs, the Vulkan sidecar)
is a QUALITY GUARDRAIL and must be **left exactly as-is**. Do NOT add an "Auto-detect"
option to `LanguageSelection.tsx`/`ModelsSection.tsx`. Do NOT change any default. The
two sub-changes below are language-INDEPENDENT (duration- and flag-based) and must not
perturb language handling at all.

## The gate (applies to every sub-change)

Harness: `scripts/perf-harness.mjs` (runs `<bin> transcribe --file … --engine whisper
--model … --language <lang> --json`; per-language WER + `last_word_present`; emits
`perf-report.{json,md}`). Corpus: `scripts/gen-corpus.mjs` (en/fr/de/es/it/pt/nl +
sv/pl, 2s/5s/15s). **Synthetic corpus = relative deltas only, never absolute WER.**
Reps ≥ 5. `large-v3-turbo` (multilingual) is already downloaded in the app models dir.

```
node scripts/gen-corpus.mjs --out perf-corpus/synthetic          # one-time
cd src-tauri && cargo build --release && cd ..
cp src-tauri/target/release/voicetypr /tmp/voicetypr-baseline    # before the change
# after each change: rebuild, then A/B baseline-bin vs candidate-bin, diff the reports
```

Because the changes are language-independent, gating on English (`--models
large-v3-turbo`, plus `base.en` if downloaded) plus a couple of European buckets is
sufficient — the WER-neutrality claim is per-language but the win is duration-based.

---

## Sub-change A — length-adaptive `audio_ctx` default (short-clip speed, plan-015 gated)

**Plumbing today:** `audio_ctx: Option<i32>` exists end-to-end (plan 032) but is only ever
`Some` from the CLI. Product paths pass `None` (executor route, the upload/bytes
`build_transcription_job` calls, remote/transcription). Applied at `transcriber.rs` via
`params.set_audio_ctx()` (the FullParams block, ~:586).

**Design — the SHARED layer** (`transcriber.rs`, after `resampled_audio` + duration known):
move the duration computation above the params block, then:
`let effective_audio_ctx = audio_ctx.or_else(|| adaptive_audio_ctx(resampled_audio.len()));`
(explicit CLI override always wins; log the branch: `custom`/`adaptive`/`full`).

New pure, unit-testable `fn adaptive_audio_ctx(samples: usize) -> Option<i32>`:
```
positions_per_sec = 50            // 30s = 3000 mel frames @10ms → conv stride 2 → 1500 pos
needed  = ceil(dur_s * 50) + 50   // +50 = 1.0s tail pad (plan-015 never-lose-speech)
rounded = round_up_to_multiple_of_64(needed)
ctx     = max(rounded, 256)       // floor 256 (~5.1s): tiny ctx = known hallucination zone
if ctx >= 1500 → None             // long/multi-window clip → stock full-window behavior
```
The `>=1500 → None` guard is why it only affects short clips; multi-window (>30s) clips are
never trimmed (`exp_n_audio_ctx` applies to every window — whisper.cpp:6951).

**Windows GPU sidecar caveat (scope note):** when `audio_ctx.is_some()` the GPU sidecar is
bypassed; the sidecar's `SidecarRequest::Transcribe` has no audio_ctx field and never calls
`set_audio_ctx`. Putting the default in transcriber.rs covers macOS Metal, Windows CPU
fallback, and remote host; **the Vulkan sidecar keeps stock full-ctx behavior and is OUT OF
SCOPE** (do NOT thread it through the protocol here).

**Gate A (plan-015 never-lose-speech gate):** per-language WER delta ≈ 0 for every language
and bucket; `last_word_pass_rate == 1.000` on ALL rows (the 2s bucket is the tail-loss
canary); `inference` p50 improvement on 2s/5s buckets is the win purchased.

---

## Sub-change B — speed mode opt-in (flash-attn; turbo as UI steering)

**flash_attn (verified):** whisper-rs 0.16 `WhisperContextParameters.flash_attn: bool`
(context-level, model-load time — not per-run FullParams). Disables DTW; repo has zero DTW
usage (safe). CPU no-op-to-marginal → gate to the Metal (Apple-Silicon) branch.

**Wiring:**
- `Transcriber::new(model_path)` → `Transcriber::new(model_path, speed_mode: bool)`; set
  `ctx_params.flash_attn(true)` next to `ctx_params.use_gpu(...)` in the constructor's Metal
  branch, gated to `is_apple_silicon`. Windows main-binary path is CPU-only.
- `TranscriberCache::get_or_create` carries the flag; cache key → `format!("{path}|fa={flag}")`
  (MAX_CACHE_SIZE=1 → toggle flip evicts old ctx, correct). Update all call sites (dispatch,
  startup preload, preload command, remote host, and the test cache constructor — note the
  test uses `TranscriberCache::new()`, adapt as needed).
- Dispatch reads the setting in `transcribe_whisper_with_acceleration`, same store-read pattern
  as `transcription_acceleration_mode`.
- **Setting:** `whisper_speed_mode: bool` (serde default false) — add to `Settings`, `Default`,
  mirror in `src/types.ts`. UI: "Speed mode" toggle in the whisper block of `ModelsSection.tsx`,
  copy: "Faster transcription (flash attention); pairs best with Large v3 Turbo".
- **Turbo:** `large-v3-turbo` already recommended. Speed mode must NOT silently swap the user's
  model — wire turbo as UI steering only (badge/recommend when speed mode on).
- **Harness hook:** add `transcribe --speed-mode` to `TranscribeArgs` (next to `--audio-ctx`)
  threaded through `transcribe_audio_file_for_cli` so the gate doesn't mutate the settings store.
  Vulkan sidecar out of scope.

**Gate B:** single-binary A/B (candidate without flag = baseline, with flag = candidate). WER
delta per language within noise on `large-v3-turbo`, last-word 1.000, inference p50/p95 down;
report `base.en` too if downloaded to prove the toggle is safe on small models.

## Acceptance (whole plan)
- Each sub-change: its gate passes, `cargo test` + `cargo clippy --workspace --all-targets -D
  warnings` + `pnpm typecheck/lint/test` green.
- `adaptive_audio_ctx` has pure unit tests (boundaries: 1s, 2s, 5s, 27s, 30s, 60s).
- ZERO change to language handling — English default everywhere, no Auto option, auto→en squash
  untouched. Default paths unchanged when the new speed-mode setting is off.
- Do not commit (Claude commits after gate review).

## Outcome (2026-07-03, gated on this machine, large-v3-turbo)

Both sub-changes gated via direct CLI A/B on the synthetic corpus (adaptive default vs
`--audio-ctx 1500` forced-full; speed-mode with/without).

**Sub-change A — adaptive audio_ctx:** the gate CAUGHT a regression — floor 256 landed
in whisper.cpp's repetition-hallucination zone (de-2s duplicated the whole sentence). An
audio_ctx sweep showed ctx≥384 clean; **floor raised 256→512** (margin), re-verified clean
across en/de/fr/es/it/pt. Final: last word preserved on every clip (plan-015 gate), no
hallucination, WER-neutral (only normal whisper non-determinism), and **~2.7-3.1× faster
on short clips** (en-2s 226ms vs 701ms full; de-2s 233 vs 686; fr-2s 258 vs 700; en-5s
251 vs 686). Vulkan sidecar out of scope (keeps stock full-ctx).

**Sub-change B — speed mode (flash-attn, Metal opt-in):** WER-neutral (text IDENTICAL
with/without on en-5s, en-15s); faster on longer clips (en-15s 425→383ms); marginal on
short clips as expected (decode-time win). Opt-in toggle, default off. Turbo = UI steering.
