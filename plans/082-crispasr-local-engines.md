# Plan 082 — CrispASR local engines and Auto spoken language

Status: IMPLEMENTED — WINDOWS CHECKS PASSED / NEEDS-SMOKE — OpenCode
(OpenAI gpt-6-astra), 2026-10-05. Packaged desktop, macOS builds/runtime and
broader real-speech quality remain unverified; see 082-S1–S5 in `SMOKE.md` and
`../docs/reports/2026-10-05-crispasr.md`. Specification approved 2026-10-04. Baseline:
`70c904a1`. Branch: `feat/crispasr-auto-language`; worktree:
`.worktrees/crispasr-auto`.

## Goal

Add Moondream Parakeet Ultra Q8_0 and NetEase Youdao Confucius4-R2T2 Q4_K
and Q8_0 through CrispASR on the app's supported macOS and Windows platforms.
Start recognition while the user speaks, keep the microphone start path fast,
and support automatic spoken-language selection. Follow the existing author's
module, protocol, UI, test and commit conventions.

The founder approved a persistent library-backed sidecar and both R2T2
quantizations, with Q4_K as the smaller default. Integration proceeds in small,
buildable conventional commits. No release or push is authorized.

## Evidence from onboarding

- `audio/recorder.rs` captures at the device's native rate. Preallocated chunk
  pools and bounded `try_send` queues keep writer and stream-tap allocation
  outside the real-time callback. Sink creation waits for capture to start.
- `audio/stream_tap.rs` carries the recording generation and reports dropped
  frames at finalization. Incomplete streams cannot own the final transcript.
- `whisper/cache.rs` retains one loaded model. Preview peeks at that cache
  rather than blocking microphone start on a cold model load.
- `whisper/decode_stream.rs` performs background preview decoding, but its
  finalization deliberately avoids another decode: batch remains authoritative.
- Plan 070 and `sidecar/parakeet-swift/Sources/main.swift` document and fix
  premature commits, boundary duplication, lost language context, and dropped
  preview audio. FluidAudio's fixed input padding is specific to that runtime;
  its window constants are not transferable to CrispASR without measurement.
- Soniox's generation-keyed final-result handoff in `commands/audio.rs` is the
  existing pattern for using a completed stream result with complete-WAV
  fallback. Its validity checks matter independently of its network transport.
- The stop path retains 250 ms post-roll for user stops, then drains capture.
  Cancellation and errors stop immediately. `StopInFlightGuard` and the 8 s
  recorder teardown deadline remain contracts.
- `commands/settings.rs`, `whisper/languages.rs`, both Whisper decode paths,
  and the Vulkan sidecar currently turn Auto into English. A dropdown-only
  change cannot implement this feature.
- The writing pipeline currently falls back to English when transcript
  language is absent. Auto must not accidentally become a translation request
  or be reported as a real language in history.

## Architecture

### Native runtime boundary

Add a persistent `sidecar/crispasr/` executable using a source-pinned CrispASR
v0.8.41 revision. Use the upstream C++ `CrispasrBackend` interface and
`CrispasrRealtimeSession` for R2T2. That interface exposes the realtime recipe;
the public session C ABI's generic streaming functions are not a substitute
for verifying the native R2T2 path.

Keep CrispASR in its own process: its exported Whisper and ggml symbols overlap
with the app's existing `whisper-rs` runtime. A subprocess also permits bounded
cancellation/restart when native inference cannot abort cooperatively.

Use newline-delimited JSON stdin/stdout, matching the existing sidecars:
request IDs, generation/session IDs, explicit error codes, and protocol-only
stdout. Commands cover load/warm, batch transcribe, stream start/audio/finalize/
cancel, status and shutdown. Encode PCM payloads with the existing base64
dependency; audio and transcript payloads are never logged.

The Rust `crispasr/` module owns the catalog, model lifecycle, process client
and stream adapter. One loaded model serves serialized inference work; new
sessions cannot race batch requests or warmup on the same backend. Warmup runs
after model selection/startup, outside capture, and yields to real work.

### Model management and platform integration

Add three explicit catalog entries:

| Model | Artifact | Role |
|---|---|---|
| Parakeet Ultra Q8 | `cstr/parakeet-ultra-GGUF/parakeet-ultra-q8_0.gguf` | Requested multilingual TDT model |
| Confucius4-R2T2 Q4_K | `cstr/confucius4-r2t2-GGUF/confucius4-r2t2-q4_k.gguf` | Smaller R2T2 default |
| Confucius4-R2T2 Q8 | `cstr/confucius4-r2t2-GGUF/confucius4-r2t2-q8_0.gguf` | Higher-precision R2T2 option |

Use immutable model revisions, expected sizes and SHA-256 hashes. Downloads
use temporary files, incremental hashing, atomic completion and cancellation.
Register the entries through the existing local-model list and lifecycle
commands, with model-specific language sets. Keep model licenses and
attribution with the artifacts. R2T2's weights use the NetEase Youdao model
license, distinct from CrispASR's MIT inference code.

Integrate selection, readiness, preload, download/delete/repair, menus, uploads,
CLI and the existing transcription executor. Advertise only implemented
capabilities; neither new model supports Whisper's translate-to-English task.
Existing downstream vocabulary/library rules and Polish operate on the shared
transcription result. Native vocabulary hints require a tested adapter rather
than inheriting FluidAudio's CTC vocabulary capability by model name.

Build/package a Metal-capable sidecar for Apple Silicon and CPU-capable paths
for supported desktop targets. Windows acceleration uses Vulkan with a usable
CPU fallback even when the GPU runtime cannot initialize. Runtime selection
and reporting honor the existing acceleration preference. Build tooling should
use Node/CMake rather than introduce new shell-specific scripts.

### Capture and background recognition

Reuse `StreamTapSink`, `StreamingResampler` and `StreamSessionGate`. Convert
device audio to mono 16 kHz outside the callback, and flush the resampler tail
before finalization. New-engine background processing is independent of the
live-preview display switch: hiding the pill text does not defer all work
until stop.

Keep ingress and inference separate with bounded buffers. Coalesce redundant
preview scheduling, never silently discard audio and still trust the result.
Queue overflow, resampling failure, process failure, cancellation, or missing
audio invalidates final authority and uses the complete recording when valid.
Inference completion after cancellation or a generation change cannot emit or
paste stale text.

Initial resource limits are 120 s of retained stream PCM (7.68 MB at 16 kHz
mono f32), 5 s of unprocessed ingress audio, and one active inference request.
Crossing either audio bound disables that recording's stream authority and
releases its stream state; the complete on-disk capture remains available for
final transcription. These bounds are failure recovery, not permission to
truncate a transcript. A stalled sidecar is killed on the bounded request
deadline and restarted for a subsequent valid request. Finalization runs
outside the recorder's join budget, through a generation-keyed result handoff.

Parakeet Ultra uses context-preserving decode-ahead with a revisable tail and
conservative committed prefixes. Do not copy FluidAudio's 15 s padding
assumption or Whisper-specific beam/audio-context parameters. Use a single
final transcription of complete audio (bounded long-form processing where
required), and hand that result to the executor instead of repeating a preview
final and a second batch final. Background previews are not automatically
treated as batch-equivalent. No additional neural VAD model is required.

R2T2 uses the upstream prefix-rollback realtime recipe. Feed audio in order,
emit its stable text through the append-only event contract, and flush after
the last post-roll/resampler samples. Reuse the completed stream as the final
result only when its model, language, generation, sample coverage and terminal
status match the stopped recording. No mandatory second offline decode follows
a valid stream final. Keep complete-file fallback for failed/incomplete streams.

R2T2's implementation re-encodes accumulated audio for each step; it does not
reuse an encoder cache. Bound long-session work and buffer growth, and measure
step cadence on real hardware. Any turn/window boundary must preserve all
audio and avoid duplicated/missing words. Do not claim constant-cost streaming
or a measured speed improvement without benchmark evidence.

Use the same new-engine audio preparation policy for stream final and file
fallback. In particular, do not apply whole-recording peak gain, dither or
Whisper-tail trimming only to one of the paths and then claim equivalent
results. Keep the established capture-integrity and conservative no-input
checks; do not introduce a new speech-rejection threshold for these models.

### Auto spoken language

Store the explicit choice as `speech_language: "auto"`. Expose Auto only for
models/providers whose adapters support it; English-only models remain English
and explicit-language-only providers do not advertise autodetection. Existing
saved explicit language choices and missing-setting defaults retain their
meaning. Auto is a spoken-language choice, not an output/translation language.

Normalize the choice at request boundaries and keep absent CLI overrides
distinct from an explicit Auto selection. Whisper's native decoder receives
an empty string for Auto, for in-process batch, preview, and the Windows Vulkan
sidecar. Read the decoder's detected language for result metadata rather than
labelling the transcript "auto" or English by default. Translation tasks still
report English output.

For CrispASR, leave the model language unconstrained in Auto mode. Avoid an
extra language-ID model or inference pass for Parakeet, which can transcribe
its supported languages directly. If a backend does not report detected
language, preserve unknown metadata and instruct Polish to keep the source
language instead of inventing one. Apply consistent selection and labels in
React, the tray and the island quick-settings menu.

## Verification and acceptance

Automated checks cover:

- Auto settings round-trip, English-only/model capability filtering, CLI
  override semantics, native Whisper parameter mapping, transcript-language
  metadata and downstream source-language preservation.
- Catalog identities, integrity verification and incomplete/cancelled download
  behavior; selection/preload/delete and UI readiness.
- Typed sidecar messages, load reuse, request/session identity, process failure,
  cancellation and timeout; no audio/transcript payloads in diagnostics.
- Ordered streaming, bounded queues, dropped-frame invalidation, stale events,
  terminal final authority, post-roll and resampler-tail coverage, and fallback.
- Stable-prefix behavior, short takes, repeated words and chunk boundaries.

Run focused Vitest and native tests first, then TypeScript/oxlint, workspace
clippy/format checks and relevant sidecar build/tests. Windows backend tests
use `src-tauri/run-tests.ps1` so the test executables receive their manifest.

Runtime checks use real speech, never synthetic TTS for engine-quality claims:
short and long English/non-English clips, quiet speech, pause/tail cases, and
back-to-back cancel/restart. Record cold/warm load, first partial, stop-to-text,
real-time factor, memory behavior and transcript/WER comparisons. Test R2T2
Q4_K and Q8_0 separately. The current checkout has no checked-in real-speech
corpus; use documented redistributable fixtures and keep the harness in git.

macOS/Windows hardware behavior stays `NEEDS-SMOKE` until actually exercised.
Keep all development tests/reproduction harnesses in the repository and commit
them with their owning feature slice.

## Upstream references inspected

- [CrispASR v0.8.41](https://github.com/CrispStrobe/CrispASR/releases/tag/v0.8.41)
- [Backend and realtime interface](https://github.com/CrispStrobe/CrispASR/blob/v0.8.41/examples/cli/crispasr_backend.h)
- [R2T2 recipe implementation](https://github.com/CrispStrobe/CrispASR/blob/v0.8.41/src/core/qwen3_stream.h)
- [CrispASR bindings](https://github.com/CrispStrobe/CrispASR/blob/v0.8.41/docs/bindings.md)
- [Parakeet Ultra GGUF](https://huggingface.co/cstr/parakeet-ultra-GGUF)
- [Confucius4-R2T2 GGUF](https://huggingface.co/cstr/confucius4-r2t2-GGUF)
