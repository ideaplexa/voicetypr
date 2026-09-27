# Plan 076 — Parakeet on Windows (ONNX, in-process)

Status: SPIKE — Claude 2026-09-28. Founder wants Windows parity and more local
models on Windows. Windows local = Whisper only; Parakeet is macOS-only
(Swift/FluidAudio/CoreML sidecar).

## Research (2026-09-28, verified on crates.io / Hugging Face)

- `parakeet-rs` 0.3.8 (updated 2026-09-23, ~100k downloads, MIT/Apache-2.0):
  pure Rust on `ort` (ONNX Runtime); implements Parakeet TDT (greedy), Parakeet
  Unified EN (offline + streaming) and Nemotron cache-aware streaming — the
  same model family we ship on macOS. Word/token timestamps for TDT/CTC.
- `transcribe-rs` 0.3.11 (Handy's engine, ~180k downloads, MIT): `ort` engine
  for Parakeet/Moonshine/Canary/…; Handy ships Parakeet v3 on Windows CPU.
- Models: `istupakov/parakeet-tdt-0.6b-v3-onnx` int8 ≈ 670 MB (encoder +
  decoder_joint + nemo128 preprocessor + vocab); Unified EN int8 ≈ 663 MB;
  Nemotron multilingual streaming has no int8 yet (fp32 ≈ 2.6 GB).
- GPU: `ort` DirectML EP (opt-in, needs sequential execution), CUDA; NPU: `ort`
  `qnn` feature (Snapdragon) — later.
- Risks: young crates (maintenance), ONNX Runtime binary size on Windows,
  streaming timestamps unconfirmed.

## Step 1 — spike (this plan)

A standalone CLI in `tools/parakeet-onnx-spike/` (own Cargo project, not linked
into the app): load TDT v3 int8 via `parakeet-rs` (and, if cheap, the same via
`transcribe-rs` for comparison), transcribe a WAV with a language hint, print
text + word timestamps + load time + real-time factor as JSON. Measure on the
real-speech set (EN/DE/ES/long) against the macOS sidecar numbers: WER must be
within ~1 point of the CoreML batch results; CPU real-time factor recorded on
this Mac and on a GitHub Windows runner; binary size of the ONNX Runtime
dependency on Windows.

## Step 2 — integrate (separate plan after the spike's numbers)

In-process `ParakeetOnnx` backend behind the existing Parakeet engine on
Windows (same model ids, same `transcription/` contract), model download +
checksum, int8 by default, DirectML opt-in; live preview via the Rust
decode-ahead framework (Whisper's) or Unified/Nemotron streaming decoders.

## STOP conditions

- WER clearly worse than the macOS CoreML path on the same clips.
- CPU speed slower than real time for 0.6B int8 on a typical laptop runner.
