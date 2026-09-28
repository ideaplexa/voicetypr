# Plan 076b — Integrate ONNX Parakeet on Windows (batch first)

Status: SPEC v2 — Claude 2026-09-28, revised after the gpt-6-astra xhigh second opinion (below).
Evidence: plan 076 (spike). Code map verified 2026-09-28 on `feat/2.1-beta2`.

## Goal

Windows users can pick **Parakeet TDT v3** (25 European languages) as a local
model, download it in-app, and dictate with it — same model id, same settings,
same history/UI as on Mac. CPU by default. Live preview and the streaming
models (Unified / Nemotron) come in 076c.

## How Parakeet is wired today (facts)

- Catalog `parakeet/models.rs::AVAILABLE_MODELS` (CoreML `.mlmodelc` files,
  downloaded by the Swift sidecar via FluidAudio); `manager.rs::list_models`
  returns `vec![]` on non-macOS at compile time (manager.rs:196-228), so
  Windows never sees Parakeet anywhere (UI, CLI, auto-select, engine
  resolution, pre-recording validation).
- Downloads/deletes go through `commands/model.rs::download_model` /
  `delete_model` with `download-progress` etc. events carrying `engine`.
- Transcription: `transcription/executor.rs` Parakeet arm → `manager.load_model`
  + `transcribe_with_custom_vocabulary` (CTC vocabulary only for
  `parakeet-tdt-*`, via the sidecar). Shared WAV normalization, watchdog and
  timeouts for all local engines.
- Capabilities: `provider_capabilities.rs` — Parakeet `supports_vocabulary_terms:
  true` (CTC, macOS sidecar only).
- Whisper's in-process pattern: `whisper/cache.rs::TranscriberCache`
  (LRU size 1, `get_loaded` / `get_or_create` / `clear`), preload on selection
  and at startup, cleared first in `RunEvent::Exit` (AGENTS.md invariant 5).

## Design

1. **Backend split inside `parakeet/`, enum dispatch (no trait):**
   `ParakeetManager` keeps its public API; internally it holds
   `Backend::Sidecar` (macOS, unchanged) or `Backend::Onnx` (Windows x86_64).
   Selected at compile time with `cfg`, so macOS behaviour and binaries are
   byte-identical.
2. **Catalog per platform:** `models.rs` gets an ONNX definition for
   `parakeet-tdt-0.6b-v3` (same id, languages, `kind: TdtV3`, recommended)
   with the file list from `istupakov/parakeet-tdt-0.6b-v3-onnx` pinned to a
   commit revision: `encoder-model.int8.onnx`, `decoder_joint-model.int8.onnx`,
   `nemo128.onnx`, `vocab.txt`, `config.json`, each with sha256 and size
   (≈ 670 MB total). `list_models` returns it on Windows x86_64 only. The other
   three models stay macOS-only for now.
3. **Download in Rust:** reuse the Whisper downloader path in
   `commands/model.rs` (streamed HTTP with progress, cancel, sha256 verify,
   atomic rename) for a multi-file model into
   `<app_data>/models/parakeet-onnx/<model_id>/`; "downloaded" = all files
   present with matching size and a verified marker file. Same events, so the
   UI works unchanged (ModelCard's coarse-progress special case becomes
   macOS-only or goes away if progress is fine-grained).
4. **Inference:** `parakeet-rs` (0.3.8, `ort` static, CPU EP) inside
   `spawn_blocking`; an `OnnxParakeetCache` (size 1, `get_loaded` /
   `get_or_create` / `clear`) mirroring `TranscriberCache`; preload on model
   selection and at startup like the sidecar autoload; audio longer than 30 s
   split at quiet gaps (15 s fallback) exactly as the spike does — a single
   pass on a 52 s clip returned empty text. Cancellation checked between
   chunks; the existing local-engine watchdog applies. Language: no hint input
   in parakeet-rs (auto-detect); existing `speech_language` normalization for
   Parakeet stays.
5. **Capabilities per platform:** Parakeet on Windows reports
   `supports_vocabulary_terms: false` (no CTC boosting yet — the Polish
   pipeline's vocabulary rules still apply) and no live preview
   (`parakeet_preview_sink_eligible` false off macOS). UI hides the CTC
   vocabulary download off macOS.
6. **Exit teardown:** clear `OnnxParakeetCache` in `RunEvent::Exit` right after
   `TranscriberCache`; update AGENTS.md invariant 5.
7. **Build/CI:** `parakeet-rs` under
   `[target.'cfg(all(target_os = "windows", target_arch = "x86_64"))'.dependencies]`;
   cache the downloaded ONNX Runtime in CI; add an assertion that the release
   exe does not dynamically import `onnxruntime.dll` (static link, like the
   no-Vulkan-import check). Windows aarch64 unchanged (no Parakeet).

## Tests

- Catalog: Windows build lists exactly the ONNX v3 entry; macOS unchanged.
- Download: multi-file verify (size/sha mismatch → error, partial → not
  downloaded), cancel cleans temp files.
- Chunking: pure splitter (quiet gaps, 15 s fallback, timestamp offsets).
- Engine resolution + pre-recording validation accept Parakeet on Windows
  when downloaded; reject when not.
- Capabilities: vocabulary/preview flags per platform.
- Windows CI job runs `voicetypr transcribe --engine parakeet --model
  parakeet-tdt-0.6b-v3` on the 36-clip MLS set (cached model) and asserts WER
  within 1 point of the spike and RTF < 0.2.

## Acceptance

WER ≈ spike on Windows runner, RTF < 0.2, peak memory recorded (target
< 2.5 GB), exe grows ≈ 23 MB, macOS build unchanged (binary + tests),
`pnpm check` + Windows fast check green.

## Open questions for the second opinion

- Enum backend inside `ParakeetManager` vs a separate `OnnxParakeetManager`
  selected at the `ActiveEngineSelection` level.
- Reuse Whisper's downloader for multi-file models vs a small new one.
- Memory (2.1 GB peak on Mac CPU): session options (arena, threads) worth
  setting from day one?

## Second opinion (gpt-6-astra xhigh) — decisions

1. `ParakeetManager` stays the façade with compile-time backend dispatch;
   `ActiveEngineSelection::Parakeet` unchanged (remote transcription calls the
   manager directly, `remote/transcription.rs:429`). ONNX code in its own
   files; one serialized model slot, no LRU.
2. A small, dedicated multi-file downloader (staged beside the destination,
   HTTP status checks, stall cancellation, exact bytes + sha256, cleanup on
   every failure, aggregate progress, revision-bound marker written last,
   download/delete/load serialized); reuse only the command/event plumbing.
   Only encoder, decoder_joint and vocab are loaded by parakeet-rs
   (`nemo128.onnx`/`config.json` unused); pin the HF revision.
3. CPU EP, intra-op threads = min(4, available), inter-op 1 from day one;
   measure Windows memory before arena tuning.

Additional requirements:
- **Cancellation/ownership:** the hard-timeout wrapper covers Whisper only
  (`executor.rs:400`) and sidecar cancel kills a process; started
  `spawn_blocking` work cannot be aborted. Check cancel before/after load and
  between every chunk, keep worker ownership, reject late results, never
  start overlapping workers.
- **Unload contract:** close admission, cancel, wait for the worker, then
  drop the session — before delete and at exit. Loading ONNX Parakeet unloads
  Whisper and vice versa (only one heavy local model resident); stale preload
  completions are discarded.
- **Capabilities:** `EngineStreamCapabilities` final-only for Parakeet off
  macOS; no EOU sidecar calls from capability queries; gate vocabulary
  compilation, CTC commands and automatic upload diarization
  (`executor.rs:240`, `upload.ts:98`); persisted live-preview setting must
  degrade cleanly.
- **Selected-but-missing model:** validate the *selected* model before
  recording (today any available engine passes, `audio.rs:4857`), keep it
  repairable, invalidate the warm session on delete.
- **UX:** show the ≈ 670 MB download and temporary disk need, a loading state,
  "language detected automatically", Whisper-only GPU controls; actionable
  errors (missing/corrupt/download/memory/unsupported) instead of the macOS
  quarantine/reinstall text; no paths or raw ORT errors in logs.
- **Chunking:** overlap-free split at quiet gaps with tests for continuous
  speech, boundary words, empty chunks and timestamp offsets; bounded memory
  for long uploads; never report the requested language as detected.
- **CLI:** own init/teardown for the ONNX backend; bench separates cold load
  from warm RTF.
- **Packaging:** import checks (no dynamic onnxruntime.dll) in native CI,
  release and Store workflows; verify NSIS and the MSIX (static CRT, staged
  runtime DLLs) on a clean install; `get_model_definition` must be
  platform-filtered too; Windows ARM64 explicitly excluded (assert).
- **CI time:** perf/WER evaluation stays out of fast PR checks; cache ORT,
  pinned models and normalized clips; resolve the French discrepancy before
  enforcing WER thresholds.

## Slices

1. Dependency, link and packaging proof (no user-visible change).
2. Hidden ONNX backend with serialized lifecycle, cancellation and unload.
3. Transactional download/delete.
4. Catalog, settings, capabilities and UX exposure on Windows x64.
5. Windows evaluation job + installed NSIS/MSIX check (NEEDS-SMOKE on hardware).

## Known gap

Clearing the Whisper cache does not stop an active live-preview thread that
holds its own `Arc<Transcriber>` (`commands/audio.rs`). Windows live preview
with CPU Whisper is rare, and the preview ends when the recording ends.
