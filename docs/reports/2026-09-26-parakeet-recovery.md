# Parakeet recovery checkpoint — 2026-09-26

Two reproduced integration defects are fixed locally. A fresh debug build and
real saved-audio checks are complete. **The selected Parakeet preview/native
paths do not pass transcription-quality acceptance on these fixtures.** This
checkpoint is not a packaged-app, insertion, CI, or release pass.

## Source and environment

- Worktree: `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-integration`.
- Branch: `feat/049-pure-rust-audio`; starting HEAD:
  `c286411603e4e3fe283f22bd4bd728e206e19fa6`.
- Tested source includes the uncommitted startup guard and build-path fixes below.
  The HEAD alone does not identify the tested source; hashes are recorded in
  `.tmp/recovery-20260926/source-hashes.json` and the result JSON files.
- Apple Silicon, macOS 27.0 build `26A428`; Swift debug configuration.
- FluidAudio remains pinned to **0.15.5**,
  `19600a485baa4998812e4654b70d2bab8f2c9949`.
- Tested distributed sidecar SHA-256:
  `8ae0bf5d35554fbd1df90d443c6a7008c6ab629a8c75feefb7ede716448f005f`.
- No main integration, push, application preference changes, microphone capture,
  paid-provider calls, or replacement of the installed app occurred.

## Fixed and verified

1. **Successful Swift builds could package an old executable.** On this toolchain,
   SwiftPM writes to `.build/out/Products/Debug`, while the old script copied a
   July binary from `.build/debug`. `build.sh` now obtains the output directory
   with `swift build -c "$BUILD_CONFIG" --show-bin-path`. The canonical debug
   build succeeded and the distributed binary hash equals the fresh build hash.
2. **Loading a model as the first command could divert JSON replies to stderr.**
   Swift initializes the global duplicated stdout descriptor lazily. A native
   library redirect could run before the first access, causing that descriptor
   to capture stderr. Startup now initializes/checks it before any native call.
   Fresh first-command cached loads and both native downloads delivered progress
   and status JSON on stdout, with no non-JSON stdout lines in the final runs.
3. README protocol examples now supply `model_version` and show the actual
   camelCase status-response fields.

Verification: canonical debug build, distributed-binary hash comparison,
`bash -n`, `git diff --check`, the existing five token-normalization checks, and
the real sidecar checks below. The full frontend/Rust quality gate and release
build were not run; these edits remain uncommitted.

## Real inference results

Inputs are existing synthetic speech fixtures from `perf-corpus/synthetic`.
Names are historical labels; durations below come from WAV headers. Word error
rate is Levenshtein distance over case-folded Unicode word tokens, so punctuation
and capitalization do not count. A matching batch/preview string is not a
quality pass. Three synthetic English fixtures and one Italian fixture are a
regression check, not a general model benchmark.

The final rows were run outside the shell sandbox. Some model compilation and
checks overlapped, so timing fields in the raw files are diagnostic only, not
performance comparisons or p50/p95 measurements.

| Engine | Fixture | Actual duration | Batch WER | Preview-final WER | Preview last 3 reference words preserved |
|---|---|---:|---:|---:|---|
| tdt | `en-2s.wav` | 3.05 s | 0.0% | 0.0% | Yes |
| tdt | `en-5s.wav` | 9.13 s | 6.2% | 31.2% | No |
| tdt | `en-15s.wav` | 13.22 s | 7.1% | 57.1% | Yes |
| unified | `en-2s.wav` | 3.05 s | 100.0% | 100.0% | No |
| unified | `en-5s.wav` | 9.13 s | 81.2% | 81.2% | No |
| nemotron | `en-2s.wav` | 3.05 s | 42.9% | 42.9% | No |
| nemotron | `en-5s.wav` | 9.13 s | 93.8% | 87.5% | No |
| nemotron | `it-5s.wav` | 4.61 s | 25.0% | 25.0% | Yes |

- **TDT:** batch is substantially better than the longer decode-ahead preview.
  For the 9.13-second fixture, batch has one substitution (`chain` for `change`);
  preview adds/drops several words. The 13.22-second preview has 57.1% WER.
  Early permanently committed partials contain errors that later context cannot
  repair. The cause of the acoustic/context errors has not been isolated.
  Local preview remains cosmetic; these results do not establish that incorrect
  preview text was inserted into another application.
- **Unified English 640 ms:** download and load succeed. The 3.05-second fixture
  and restart return empty text. On 9.13 seconds both batch and preview return
  “The voice transcription are this record's latency atturs.”, omitting much of
  the utterance. An independent CPU-only program linked to the same FluidAudio
  build reproduces both outputs, without Voicetypr protocol/session code. Thus
  switching to CPU or relaxing shell permissions does not resolve this failure.
  A separate encoder probe confirms Float32 output and correct valid-frame
  lengths (64→8, 112→14, 300→38, 624→78); a simple output-shape mismatch was not
  reproduced. SDK, model, and preprocessing correctness remain to be isolated.
- **Full Nemotron Multilingual 1120 ms:** all 22 model files downloaded and loaded;
  English and Italian batch/preview calls return text. Accuracy is insufficient
  on these inputs: the longer English batch has 93.8% WER and preview 87.5%.
  Italian has 25% WER and preserves the final three reference words. Streaming
  uses automatic language detection; batch uses the fixture's language hint.

## Lifecycle and download checks

All three engines reject model switching during an active stream with
`stream_busy`, acknowledge cancellation, emit no late partial/final during the
0.3-second observation after the cancellation acknowledgement, and accept a
new stream. TDT and Nemotron return nonempty restart finals. Unified accepts
restart but returns empty text again; its inference check exits unsuccessfully.
This bounded observation is not a proof against every possible lifecycle race.

Unified downloaded 15 files (~608.5 MB); full Nemotron downloaded 22 files
(~664.1 MB). Exact file hashes, paths, and sizes are saved in
`.tmp/recovery-20260926/native-models-manifest.json`. Existing TDT caches and old
benchmark artifacts were preserved. The first sandboxed Unified failure was
retained separately, then reproduced with normal system access.

## Reproduction and retained evidence

Run from the worktree above, with normal Core ML compiler/model-cache access:

```sh
bash sidecar/parakeet-swift/build.sh debug
python3 .tmp/recovery-20260926/smoke.py --model tdt --binary sidecar/parakeet-swift/dist/parakeet-sidecar-aarch64-apple-darwin --files en-2s en-5s en-15s
python3 .tmp/recovery-20260926/smoke.py --model unified --binary sidecar/parakeet-swift/dist/parakeet-sidecar-aarch64-apple-darwin --files en-2s en-5s
python3 .tmp/recovery-20260926/smoke.py --model nemotron --binary sidecar/parakeet-swift/dist/parakeet-sidecar-aarch64-apple-darwin --files en-2s en-5s it-5s
```

Do not add `--download` to cached reruns: that invokes the app's force-download
path. The harness reports `protocol_status` separately from transcript quality;
zero exit status alone does not mean acceptable accuracy.

Local evidence in `.tmp/recovery-20260926/` includes `smoke.py`, per-model
`*-results.json`, `*-events.json`, `*-stderr.log`, `*-run.log`, source/model
manifests, preserved pre-fix/sandbox failures, and the isolated Unified CPU and
encoder-shape probe sources, binaries, and logs. These scratch artifacts are
untracked; the dated report captures the portable conclusion.

## Remaining acceptance work

1. Isolate and repair Unified/Nemotron accuracy using these same inputs plus a
   small real-speech control. Any SDK/model change needs exact version/hash and
   before/after evidence; do not infer a fix from a successful load.
2. Repair TDT preview context/commit behavior, then verify short, continuous
   >30-second, silence-tail, and cancel/restart fixtures. Do not hide errors by
   weakening the transcript comparison or changing final-text authority.
3. After sidecar accuracy passes, verify actual Tauri model selection,
   microphone capture, pill rendering, focus, clipboard restoration, and one
   final insertion. Then perform the affected Whisper/cloud regressions,
   package/quality gates, and release acceptance separately.
4. Main reconciliation remains on hold under the user's existing instruction;
   this report does not authorize a merge, rebase, port, or push.
