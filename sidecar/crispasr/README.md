# CrispASR sidecar

Persistent native inference for Parakeet Ultra and Confucius4-R2T2. Built from
CrispASR v0.8.41 (`340d7085eaa53c40a46dcb73a6d3d0448a480006`) and its
ggml revision (`2f5a80d258c46e6ac8eee95f1328c0f58376d7ee`). The sidecar links
the upstream Parakeet and Qwen3 adapters and their native libraries, rather
than embedding CrispASR's entire ASR/TTS catalog into the desktop process.

```console
node scripts/build-crispasr-sidecar.mjs --variant cpu
node scripts/build-crispasr-sidecar.mjs --variant gpu
```

The GPU variant uses Metal on Apple Silicon and Vulkan on Windows. Build both
Windows variants so a missing/incompatible Vulkan runtime can fall back to the
independent CPU executable. `--source`, `--build-dir` and `--target` allow an
existing pinned source cache, a separate build volume, and cross-target builds.
Native build tools/CMake are required; Vulkan builds also need the Vulkan SDK.
The Windows x64 build uses AVX2, matching the upstream standard CPU build.
C++20 designated initialization avoids invoking Whisper-only parameter defaults
while using the upstream backend interface; upstream model libraries keep their
own language/compiler settings.

Protocol: newline JSON with `id`, `type` and generation-scoped `session_id`.
`load_model` retains and warms a model; `start_stream` opens a recording or
offline batch; `audio_chunk` takes base64 little-endian mono 16 kHz float32;
`finalize_stream` reports the complete sample count and result. `cancel_stream`,
`status`, and `shutdown` close the lifecycle. Runtime stdio from the native
libraries is silenced; only structured protocol messages reach stdout.

`mode: "batch"` buffers normalized input and runs complete-file recognition.
Parakeet owns its long-form handling; offline R2T2 uses the upstream 30 s
energy-minimum chunker. Recording R2T2 uses upstream's realtime prefix rollback.
The recording R2T2 schedule is 2 s on CPU and 640 ms when GPU is requested; the CPU
schedule avoids repeatedly re-encoding at a cadence the processor cannot meet.
Offline retries use 2 s steps on either backend because they need no live updates.
An empty offline Auto result retries once through the native R2T2 recipe on
nonzero PCM. This preserves successful offline results and never forces English
as an automatic-language fallback. `stream_retry` reports that fallback.
Parakeet recording previews stay tentative, with full context, and its final
is one complete decode. Recorded streams are capped at 120 s, offline input at
one hour; limits return an error rather than truncate input. Rust owns backlog
limits, cancellation/timeouts, and complete-file fallback.

The build runs the native PCM transport test. After building, set
`CRISPASR_SIDECAR` to the executable and run:

```console
node --test sidecar/crispasr/tests/protocol.test.mjs
```

Model accuracy, latency and platform GPU behavior require real-speech smoke
tests. Passing the protocol tests alone is not evidence of inference quality.
Local measurements and remaining platform smoke checks are recorded in
`docs/reports/2026-10-05-crispasr.md`.

The CTest fault-injection harness runs the actual pinned Qwen3 adapter with
encoder, prefill and decoder failures. `src/qwen3_adapter.cpp` checks native C API
results at the C++ boundary, including decoder function pointers: an allocation
or graph failure must produce an error, not an authoritative partial final.
Upstream source remains unmodified and revision/clean-source checks still apply.

For Rust-to-native real-speech verification, set `CRISPASR_TEST_BINARY`,
`CRISPASR_TEST_MODEL`, `CRISPASR_TEST_BACKEND` (`parakeet` or `qwen3`),
`CRISPASR_TEST_AUDIO` (PCM16 WAV) and optionally `CRISPASR_TEST_GPU=1`, then run
`crispasr::sidecar::tests::real_model_protocol_round_trip` with `--ignored --exact`.
On Windows, use the manifest-embedded library test executable as described by
`src-tauri/run-tests.ps1`. The test checks both batch and recording sample coverage,
nonempty results and monotonic committed text; it does not print transcripts.
