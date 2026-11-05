# Silero VAD v6.2

MIT, Copyright (c) 2020-present Silero Team. See bundled LICENSE.
Model is unmodified `ggml-silero-v6.2.0.bin` (885,098 bytes).
Source: https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/ggml-silero-v6.2.0.bin
SHA-256: `2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987`.
The ggml-org repository's model card declares MIT. It converts Silero models
for whisper.cpp; the original Silero licence is retained here.

Embedded with include_bytes, hash-checked and written to a private, automatically
deleted temporary file because WhisperVadContext accepts a filename only.
No user download or network access at inference. PCM16 is downmixed/resampled
to 16 kHz without normalization or trimming. Fresh context for every recording.
512-sample probabilities use the advisory 0.5 threshold, counting only actual
samples in the padded final frame. No segment filtering or chunk changes.

Runtime: existing whisper-rs 0.16.0 / whisper-rs-sys 0.15.0 whisper.cpp VAD,
CPU only (`use_gpu=false`), one thread. No ONNX Runtime dependency on macOS.
Intel VAD requires OS-enabled AVX, AVX2, FMA and F16C; otherwise the content-free
reason is `runtime_unsupported`. Release/CI pin non-native ggml x86 flags,
including AVX512 off. Those flags also pin the in-process Whisper CPU build.

The 250 ms result budget is cooperative. Decode, initialization and inference
are checked before/after native work; whisper.cpp's VAD API processes the
whole bounded take without a per-frame cancellation callback. Over-budget
results are unknown, never silence. One worker retains its slot until native
work and context destruction finish. Quit closes admission/reports and waits at
most 250 ms at the VAD drain stage. If still active, ordered teardown continues,
then the process exits without C++ static destruction. An 8 s watchdog from
exit entry bounds earlier/later teardown while admitted VAD work exists.
Bootstrap installs a content-discarding native logger once, before workers.
Caller-thread native errors make the result unavailable, including the pinned
backend's otherwise-successful partial-compute path. No native text is emitted.
Per-frame ONNX/ggml comparison and hardware gaps are recorded in plan 087.
That plan also records retained upstream native-OOM abort and filename-allocation
limitations; short-run RSS is not a leak-free lifetime guarantee.
