# Parakeet TDT v3 ONNX spike

Standalone CPU benchmark for `parakeet-rs` 0.3.8 and the local int8 TDT v3 model. It is separate from the Voicetypr app.

```sh
cd tools/parakeet-onnx-spike
cargo build --release --offline
MODEL=../../.tmp/models/parakeet-tdt-0.6b-v3-onnx
CLIPS=../../.tmp/clips
target/release/parakeet-onnx-spike --model "$MODEL" --wav "$CLIPS/jfk.wav" --lang en --threads 4
target/release/parakeet-onnx-spike --model "$MODEL" --bench "$CLIPS" --threads 4
```

`--wav` prints one JSON line with text, word start/end times in seconds, model load time, transcription time, audio duration, and RTF. `--bench` loads once, prints a table and a JSON summary with per-clip WER/timing and averages. The manifest supplies references; if `jfk.wav` is present but absent from the manifest, the bench includes it using the JFK reference. WER lowercases and strips punctuation, then computes word edit distance divided by reference words. The model auto-detects language; `--lang` is accepted for CLI parity but `parakeet-rs` TDT has no language-hint input. WAV input is downmixed to mono and linearly resampled to 16 kHz. Audio longer than 30 seconds is split at sustained quiet gaps, with a 15-second fallback for spans without gaps; word timestamps are offset to the original file.

## Mac CPU results (2026-09-28)

Apple Silicon (`aarch64-apple-darwin`), default four intra-op threads, CPU execution provider, int8 model. One completed release-build run follows. Timings varied across runs on this Mac: aggregate RTF was 0.025 in an earlier run and 0.073 here. RTF is transcription time divided by audio duration, excluding model load and WAV decoding.

| Clip | ONNX WER % | CoreML batch WER % | Transcribe ms | RTF |
| --- | ---: | ---: | ---: | ---: |
| jfk | 0.0 | 0.0 | 922 | 0.084 |
| ls-0 | 6.7 | 6.7 | 459 | 0.082 |
| ls-1 | 2.9 | 8.8 | 1121 | 0.074 |
| ls-2 | 0.0 | 0.0 | 395 | 0.074 |
| ls-3 | 0.0 | 0.0 | 539 | 0.076 |
| mls-de-0 | 25.0 | 15.6 | 1135 | 0.063 |
| mls-de-1 | 3.7 | 7.4 | 928 | 0.065 |
| mls-es-0 | 0.0 | 0.0 | 1176 | 0.072 |
| mls-es-1 | 2.4 | 4.9 | 1289 | 0.069 |
| long-en | 3.1 | 3.1 | 3872 | 0.075 |

Average clip WER: **4.38%** (unweighted); aggregate RTF: **0.073**; model load: **8.26 s**; peak process RSS: **1,733,672,960 bytes** (1.61 GiB). `mls-de-0` misses the plan's approximately one-point CoreML accuracy target by 9.4 points, so accuracy needs investigation before integration. Long audio returned empty text in one pass with this export; quiet-gap splitting restored 3.1% WER. No Windows runtime/runner measurement was made in this offline Mac spike.

## Size and Windows packaging

The release executable is **27,194,272 bytes** (25.93 MiB) on this Mac. The cached ONNX Runtime `libonnxruntime.a` used to link it is **80,425,592 bytes** (76.70 MiB). `ort`/`ort-sys` use **static linking by default** here: `otool -L` shows no ONNX Runtime dylib. On Windows the default build likewise links the ONNX Runtime static library into the `.exe`, so packaging does not require a separate `onnxruntime.dll`; executable size must be measured on a Windows runner and cannot be inferred from the Mac size. The int8 model files are additional assets (about 639 MiB for encoder and decoder alone).

For future GPU experiments, `parakeet-rs` exposes the `directml` feature (`ort/directml`) and `ExecutionProvider::DirectML`; selecting the feature alone does not change this CLI's explicit CPU provider. Its `load-dynamic` feature changes ORT to runtime library loading and would require packaging the compatible DLL. DirectML execution also needs a Windows-specific runtime check, including sequential execution settings; it was not tested here.
