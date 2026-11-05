# Silero VAD v6.2

MIT, Copyright (c) 2020-present Silero Team. See bundled LICENSE.
Model is unmodified upstream `silero_vad_16k_op15.onnx` (1,289,603 bytes).
Source commit: `be95df9152c0d7618fa1edfeb296fc3dae32376f` (v6.2).
Source: https://github.com/snakers4/silero-vad/tree/be95df9152c0d7618fa1edfeb296fc3dae32376f
SHA-256: `7ed98ddbad84ccac4cd0aeb3099049280713df825c610a8ed34543318f1b2c49`.

Embedded with include_bytes, no user download or network access at inference.
16 kHz mono, 512-sample windows, 64-sample context, zeroed recurrent state per
recording. The 0.5 probability threshold is advisory, not an enforcement gate.
No segment filtering or chunk-boundary changes are enabled.

Runtime: ort =2.0.0-rc.13, static ONNX Runtime 1.28.0, CPU sessions with one
thread. Matches Windows Parakeet's runtime. Intel macOS has no upstream rc.13
binary and reports `runtime_unsupported`; existing dictation is unchanged.
Windows bundles the prebuilt's DirectML DLL dependency app-local even though
the VAD never selects a GPU provider. Runtime licences are in ../onnxruntime.
