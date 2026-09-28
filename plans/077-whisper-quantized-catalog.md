# Plan 077 — Smaller Whisper models (quantized catalog) + model bench in CI

Status: BENCH RUN 1 DONE — Claude 2026-09-28. Founder asked for optimized/distilled Whisper
options (smaller, same accuracy). Whisper is Windows' only local engine today.

## Facts (byte-verified on huggingface.co/ggerganov/whisper.cpp, 2026-09-28)

We ship only f16: base.en 148 MB, small.en 488 MB, large-v3-turbo 1625 MB,
large-v3 3095 MB. Official quantized files:

| File | Size |
|---|---|
| ggml-large-v3-turbo-q8_0.bin | 874 MB (−46%) |
| ggml-large-v3-turbo-q5_0.bin | 574 MB (−65%) |
| ggml-large-v3-q5_0.bin | 1081 MB (−65%) |
| ggml-small.en-q8_0.bin | 264 MB (−46%) |

No published WER deltas for these; q8_0 is near-lossless by construction.
Distil-Whisper large-v3.5 (English, 1.52 GB f16 only) beats turbo on short-form
in its own benchmarks, but whisper.cpp documents distil support as initial
(no chunking) — benchmark before offering. Avoid: CrisperWhisper (non-commercial
license), lite-whisper (no ggml), community fine-tune re-uploads (unvetted).
Watch: Kyutai STT (Rust-native, CC-BY-4.0), Moonshine (MIT, ONNX).

## Change

1. **Model bench workflow** (`.github/workflows/model-bench.yml`,
   workflow_dispatch): on `macos-14` (Metal) and `windows-2022` (CPU; Vulkan
   if the runner allows), build the CLI, download the candidate models and the
   public real-speech clips (LibriSpeech / MLS, CC BY 4.0, fetched by script,
   never committed), run `scripts/perf-harness.mjs`-style WER + latency per
   model, upload a JSON/markdown report. Candidates: base.en, small.en,
   small.en-q8_0, large-v3-turbo (f16, q8_0, q5_0), large-v3-q5_0,
   distil-large-v3.5.
2. **Catalog update after the numbers** (Whisper model list in
   `src-tauri/src/whisper/`): add the winners with exact URL + sha256 + size;
   make turbo q5_0 or q8_0 the recommended default if WER is within noise of
   f16; drop large-v3 f16 (3.1 GB) if large-v3-q5_0 matches it. Existing
   downloaded models keep working (no forced re-download).

## Acceptance

- Bench report attached to the PR with WER + stop→text per model per platform.
- New catalog entries only where WER ≤ f16 + 0.5 points on the real clips.

## Bench run 1 (run 36355859435, 2026-09-28; 8 clips EN/DE/ES, 2 reps)

Mean WER (lower is better) and mean transcribe time per clip:

| Model | Size | macOS-14 Metal | Windows CPU (4 vCPU) |
|---|---:|---|---|
| base.en | 141 MiB | 9.2% · 2.4 s | 8.0% · 4.0 s |
| small.en | 465 MiB | 4.8% · 5.6 s | 3.6% · 14.0 s |
| small.en-q8_0 | 252 MiB | 4.8% · 6.1 s | 3.6% · 15.5 s |
| turbo f16 | 1549 MiB | **empty output** | 2.2% · 91 s |
| turbo-q8_0 | 834 MiB | 2.2% · 11.4 s | 2.6% · 104 s |
| turbo-q5_0 | 547 MiB | 2.2% · 8.8 s | 2.3% · 128 s (5/18 load errors) |
| large-v3-q5_0 | 1031 MiB | **empty output** | 3.1% (10/18 load errors) |
| distil-large-v3.5 | 1449 MiB | **empty output** | 11.5% · 70 s |

Reads:
- turbo-q5_0 matches turbo f16 accuracy at a third of the size and is the
  fastest large model on Metal → candidate default. small.en-q8_0 is identical
  to small.en at half the size → replace.
- distil-large-v3.5 is poor in whisper.cpp (11.5%), as its docs warn → reject.
- On a 4-vCPU Windows CPU, turbo is ~7× slower than real time; quantized is
  even slower on CPU. Windows users without a GPU need small models or
  Parakeet ONNX (plan 076) — strong support for 076.
- Must investigate before any catalog change: (1) empty transcripts for turbo
  f16 / large-v3-q5_0 / distil on the macOS runner (runner GPU memory? if the
  app can return empty text instead of an error, that is a product bug);
  (2) intermittent `whisper_init_from_file` load failures for q5_0 models on
  Windows (capture full stderr; memory?).
