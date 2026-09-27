# Plan 044 grounding — Whisper WER-gated wins (worktree: /Volumes/1tb-drive/developer/oss/worktrees/voicetypr-integration)

## 0. Key discovery that reshapes the slice

`auto` is squashed to `en` in TWO layers, and the primary GUI path never even reaches the transcriber's squash:

1. `src-tauri/src/whisper/languages.rs:438-457` — `validate_language()` returns `"en"` for any unsupported code, and `"auto"` is not in `SUPPORTED_LANGUAGES` (test asserts this at languages.rs:475).
2. `src-tauri/src/commands/settings.rs:299-351` — `normalize_speech_language_for_model()` calls `validate_language(Some(speech_language))` at settings.rs:304, so every dispatch path (`audio.rs:5328` desktop recording, `audio.rs:6685` upload/CLI, `audio.rs:7051` audio-bytes) converts `auto`→`en` **before** the transcriber is called. The CLI `--language auto` (cli.rs:441 → `transcribe_audio_file_for_cli` audio.rs:6664 → normalize at :6685) is squashed too — the harness cannot exercise auto today.
3. `src-tauri/src/whisper/transcriber.rs:504-517` — the transcriber's own `auto`→`Some("en")` (comment "30-second requirement" at :506-507) is dead code for GUI flows and live only for direct callers.
4. The Windows Vulkan sidecar has a third copy: `sidecar/whisper-vulkan/src/main.rs:288-291` (`Some("auto") | None => Some("en")`).
5. There is currently **no "Auto" option in the speech-language UI** — `src/components/LanguageSelection.tsx:22` `languages` array has explicit codes only (`ModelsSection.tsx:176,191-201,783` wires it). So "auto" only arrives via legacy stored settings or CLI.

## 1. Fixing auto→en: exact design

**What whisper.cpp does with auto (verified in the vendored source actually compiled into the app):** project pins `whisper-rs = 0.16.0` (src-tauri/Cargo.toml:34,83,91,94,97) → `whisper-rs-sys 0.15.0` (Cargo.lock) → vendored whisper.cpp **v1.8.3** (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/whisper-rs-sys-0.15.0/whisper.cpp/CMakeLists.txt:3`). In `whisper_full_with_state` at `whisper.cpp:6812`: `if (params.language == nullptr || strlen(...)==0 || strcmp(params.language,"auto")==0 || params.detect_language)` → runs `whisper_lang_auto_detect_with_state` (one extra encoder+1-token-decode pass, whisper.cpp:4021-4060), stores `state->lang_id` (:6820) and continues decoding in the detected language. **There is no 30-second requirement** — the mel is zero-padded; detection works on short clips (accuracy degrades gracefully). The "30-second" comment at transcriber.rs:506 is stale folklore.

**What `set_language(None)` does in whisper-rs 0.16:** `whisper_params.rs:289-296` — `None` sets `fp.language = std::ptr::null()`, which hits the nullptr arm of whisper.cpp:6812 → real auto-detect. Doc comment at whisper_params.rs:285 confirms: "For auto-detection, set this to either 'auto' or None". `set_detect_language(true)` (whisper_params.rs:303-305) is NOT what we want — whisper.cpp:6824-6826 **returns early after detection** without transcribing.

**Design:**
- `transcriber.rs:504-522`: for `Some("auto")`, set `final_lang = None` and call `params.set_language(Some("auto"))` — or equivalently pass `None`; prefer `params.set_language(None)` (null ptr) to avoid the CString allocation. Delete the "no longer supported" branch.
- `languages.rs`: `validate_language` must pass `"auto"` through (add explicit arm returning `"auto"` before the `is_language_supported` check), and update the test at languages.rs:475. Alternatively add a `validate_language_or_auto` wrapper to avoid changing all callers' semantics — but the two GUI callers (`settings.rs:304`, `settings.rs:176` in `normalize_final_text_language`) both want auto-pass-through only for *speech* language, so the explicit-arm-in-`normalize_speech_language_for_model` approach is tighter: at settings.rs:303-306, short-circuit `if speech_language == "auto" && engine == "whisper" && !model_requires_english_speech(...) { return "auto".to_string(); }` (`.en` models at settings.rs:291-297 must stay forced-en; parakeet/soniox/cohere arms unchanged).
- **Detected-language reporting**: `transcriber.rs:781-785` currently fabricates `transcript_language` from `final_lang`; with auto it would be `None`. After `state.full(...)` (transcriber.rs:640), read the real one: `state.full_lang_id_from_state()` (whisper-rs `whisper_state/mod.rs:333`, backed by `state->lang_id` set during auto-detect at whisper.cpp:6820/7904) + `whisper_rs::get_lang_str(id)` (`standalone.rs:46`). Use it whenever `final_lang.is_none() && !translate`.
- **Vulkan sidecar**: `sidecar/whisper-vulkan/src/main.rs:288-291` → change `Some("auto") | None => None` (pass through to null); its `resolve_transcript_language` (main.rs:250-266) already handles auto via `state.lang_detect(0, threads)` — switch it to `full_lang_id_from_state` too, since a second `lang_detect` pass is a redundant encode.
- **Cost note**: with `language=auto`, whisper.cpp runs the detection encoder pass at **full** audio_ctx (auto-detect at :6812 happens before `state->exp_n_audio_ctx = params.audio_ctx` at :6947-6951), then re-encodes for decoding. Auto costs ~1 extra encoder pass; document in the plan, don't fight it.
- **UI (optional, same slice)**: add `{ value: "auto", label: "Auto-detect" }` gated to whisper engine in `LanguageSelection.tsx:22` / `ModelsSection.tsx:783`; `isEnglishOnlyModel` reset at `ModelsSection.tsx:510-513` and `ModelsTab.tsx:96` already force `en` for `.en` models.

## 2. Length-adaptive audio_ctx default: exact sites

**Plumbing today:** `audio_ctx: Option<i32>` exists end-to-end from plan 032 but is only ever `Some` from the CLI: cli.rs:103 (`--audio-ctx`) → cli.rs:442 → `transcribe_audio_file_for_cli` audio.rs:6591 → `transcribe_audio_file_impl` audio.rs:6728 → `transcribe_whisper_with_acceleration` audio.rs:1596-1603 → `transcribe_with_metadata_with_prompt` transcriber.rs:298 → `params.set_audio_ctx()` transcriber.rs:586-589 (binding verified: whisper-rs `whisper_params.rs:252`). The three product paths pass `None`: executor.rs:197 (desktop recording), audio.rs:7082 (audio bytes), remote/transcription.rs:398 (remote host).

**Primary site — the SHARED layer** (same reasoning as plan 032: CLI, GUI executor, upload, and remote host all funnel through the transcriber): `transcriber.rs`, immediately after `resampled_audio` exists and duration is known. Concretely:
- Move the duration computation (currently transcriber.rs:608-609) above the params block, then replace transcriber.rs:586-589 with:
  - `let effective_audio_ctx = audio_ctx.or_else(|| adaptive_audio_ctx(resampled_audio.len()));`
  - explicit CLI override always wins; log which branch fired (`custom` vs `adaptive` vs `full`).
- New `fn adaptive_audio_ctx(samples: usize) -> Option<i32>` in transcriber.rs (unit-testable, no state):
  ```
  encoder positions per second = 50   // 30s window = 3000 mel frames @10ms → conv stride 2 → 1500 positions
  needed  = ceil(dur_s * 50) + 50     // +50 = 1.0s tail pad — never-lose-speech (plan 015): last word must have encoder slack
  rounded = round_up_to_multiple_of_64(needed)
  ctx     = max(rounded, 256)         // floor 256 (≈5.1s): tiny ctx values are a known whisper.cpp hallucination zone
  if ctx >= 1500 → None               // long clip: full window, exact stock behavior (also covers multi-window >30s clips)
  ```
  "Applied only when clip is short" falls out of the `>=1500 → None` guard: any clip ≥ ~27.7s ((1500−64… conservatively) gets stock behavior; multi-window clips (>30s) are never trimmed, since `exp_n_audio_ctx` applies to *every* window (whisper.cpp:6951) and would degrade them.
- **Windows GPU sidecar caveat (must be in the plan):** audio.rs:1616 — when `audio_ctx.is_some()` the GPU sidecar is *bypassed* (CLI sweep semantics); when `None`, the sidecar path runs and `SidecarRequest::Transcribe` (gpu_sidecar.rs:75-82) has **no audio_ctx field**, and the sidecar's own params block (sidecar/whisper-vulkan/src/main.rs:282-317) never calls `set_audio_ctx`. Putting the adaptive default in transcriber.rs covers macOS Metal, Windows CPU fallback, and the remote host — the Vulkan sidecar keeps stock full-ctx behavior. Either duplicate `adaptive_audio_ctx` in main.rs:282ff (it computes `duration_seconds` at main.rs:277) as a separately-gated follow-up, or explicitly declare the sidecar out of scope in plan 044. Do NOT thread it through the protocol in the same change as the CPU/Metal default — separate gate runs.

## 3. Speed-mode opt-in surface (turbo + flash_attn)

**flash_attn verified:** whisper-rs 0.16 `WhisperContextParameters.flash_attn: bool` — field at `whisper_ctx.rs:465` (default `false` at :477), builder `flash_attn(&mut self, bool)` at :491-492, passed into C params at :584. It is a **context-level** (model load) parameter, not a per-run `FullParams`. Warning at whisper_ctx.rs:464: disables DTW — repo has zero DTW usage (`grep set_dtw` → nothing), safe.

**Wiring sites:**
- `Transcriber::new(model_path)` → `Transcriber::new(model_path, speed_mode: bool)` (or a small `TranscriberOptions`); set `ctx_params.flash_attn(true)` next to `ctx_params.use_gpu(...)` at transcriber.rs:66-88 (macOS) — Windows main-binary path is CPU-only (transcriber.rs:173-177); flash_attn on CPU is a no-op-to-marginal, gate it to the Metal branch (`is_apple_silicon`).
- `TranscriberCache::get_or_create` (cache.rs:48) must carry the flag: cache key at cache.rs:63 becomes `format!("{path}|fa={flag}")` (MAX_CACHE_SIZE=1 at cache.rs:10 means a toggle flip evicts the old context — correct behavior). Call sites to update: audio.rs:1668 (dispatch), lib.rs:1146 (startup preload), commands/model.rs:1150 (preload command), remote/transcription.rs:385 (remote host), tests/mod.rs:104.
- Dispatch reads the setting in `transcribe_whisper_with_acceleration` (audio.rs:1665-1669), same store-read pattern as `transcription_acceleration_mode` (audio.rs:1610).
- **Settings key:** `whisper_speed_mode: bool` (serde default false) — add to `Settings` struct commands/settings.rs:37-81, `Default` at :83-117, mirror in `src/types.ts:84`-ish `Settings` interface. UI: a "Speed mode" toggle in the whisper block of `ModelsSection.tsx` (near LanguageSelection at :783), copy: "Faster transcription (flash attention); pairs best with Large v3 Turbo".
- **Turbo:** `large-v3-turbo` is already in the catalog and already `recommended: true` (manager.rs:136-149; auto-select prefers recommended+fastest at manager.rs:564). Speed mode should NOT silently swap the user's model — wire turbo as UI steering only (recommend/badge turbo when speed mode is on). If the slice wants harness numbers for "turbo+flash_attn", that's just `--models large-v3-turbo` + the flag.
- **Harness hook:** add `transcribe --speed-mode` flag to `TranscribeArgs` (cli.rs:88-116, next to `--audio-ctx` at :101-103) → thread through `transcribe_audio_file_for_cli` (audio.rs:6585) so the gate doesn't depend on mutating the settings store. Vulkan sidecar: out of scope (its context init at main.rs:~229 would need its own flag; Windows GPU users gate separately).

## 4. Exact harness gate commands

Harness: `scripts/perf-harness.mjs` (invokes `<bin> transcribe --file … --engine … --model … --language <manifest lang> --json` at perf-harness.mjs:147-163; computes per-language WER at :285-305 and `last_word_present` (last ref word in final 10 hyp words) at :226-231; emits `perf-report.{json,md}` at :86-88). Corpus: `scripts/gen-corpus.mjs` (en/fr/de/es/it/pt/nl + sv/pl, 2s/5s/15s buckets, macOS `say`).

```bash
# one-time corpus
node scripts/gen-corpus.mjs --out perf-corpus/synthetic

# baseline binary (integration HEAD before the change)
cd src-tauri && cargo build --release && cd ..
cp src-tauri/target/release/voicetypr /tmp/voicetypr-baseline

# after each change, rebuild, then A/B:
node scripts/perf-harness.mjs --corpus perf-corpus/synthetic --engines whisper \
  --models base,large-v3-turbo --bin /tmp/voicetypr-baseline --reps 5 --out perf-out/baseline
node scripts/perf-harness.mjs --corpus perf-corpus/synthetic --engines whisper \
  --models base,large-v3-turbo --bin src-tauri/target/release/voicetypr --reps 5 --out perf-out/candidate
diff perf-out/baseline/perf-report.md perf-out/candidate/perf-report.md
```

Per-change gates (all read `wer_by_language` + `last_word_pass_rate` + `timing_summary` p50/p95 `inference`):
1. **auto→en fix** — harness passes `--language <item.lang>` today (perf-harness.mjs:158-159), so auto is untestable without a small harness addition: add `--language-mode manifest|auto` (default `manifest`) that substitutes `"auto"` in `runCli`. Gate: `auto` per-language WER within noise of forced-language WER for every corpus language; `last_word_pass_rate` unchanged; forced-language run byte-identical to baseline (the fix must not perturb the non-auto path).
2. **adaptive audio_ctx** — pre-work sweep via existing CLI flag (needs a harness `--audio-ctx <n>` passthrough appended in `runCli`, or manual: `src-tauri/target/release/voicetypr transcribe --file perf-corpus/synthetic/fr-2s.wav --model base --language fr --audio-ctx 256 --json`) to pick the floor; then the default lands in Rust and the gate is binary-vs-binary with the plain commands above. Gate: per-language WER delta ≈ 0 for **every** language and bucket, `last_word_pass_rate == 1.000` on all rows (the 2s bucket is the tail-loss canary — this is the plan-015 never-lose-speech gate), `inference` p50 improvement on 2s/5s buckets is the win being purchased.
3. **speed mode (turbo + flash_attn)** — after adding CLI `--speed-mode` and a harness passthrough flag: baseline = candidate binary *without* the flag, candidate = same binary *with* it (single-binary A/B, cleaner than cross-binary). Gate: WER delta per language within noise on `large-v3-turbo`, last-word 1.000, inference p50/p95 down; report `base` too to prove the toggle is safe on small models.

Reports carry the "synthetic corpus = relative deltas only" disclaimer (gen-corpus.mjs:133, perf-harness header) — gates are deltas, never absolute WER claims. Reps ≥5; harness skips uninstalled models with SKIPPED rows (perf-harness.mjs:166-168), so both `base` and `large-v3-turbo` must be downloaded in the app first.