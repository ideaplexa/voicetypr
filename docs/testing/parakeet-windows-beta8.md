# Windows Parakeet beta.8 review and evaluation checklist

This is a handoff, not a claim of Windows runtime acceptance. Worktree:
`/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-parakeet-win-b8`,
branch `feat/parakeet-windows-v3`, base `release/2.1.0-beta.8` (`29e7717e`).
Source: cumulative `3975a232..6711e881` from `feat/parakeet-windows-spike`.
No push, merge, CI dispatch, app launch, microphone use or model download was
performed for this handoff. Windows and installed-app checks below are
**NEEDS-SMOKE**. Use only operator-approved, real speech.

**Commit status:** `bc5010f2` contains the code port; `bdfb06a7` contains the
separate lifecycle/cancellation review fixes. The release-critical CI group is
committed as `bf0a64b2`. The founder renamed the model cache key that the secret
scanner had falsely flagged, then committed that group. No CI commit or
cache-key authorization remains outstanding.
Terminal shutdown admission is fixed in `cc686d0a`, separately from reusable
model-switch unload.

## Local gate evidence

On macOS Apple Silicon, with no app launch or model download. All three Rust
gates were rerun in full after the terminal shutdown fix and passed:

| Gate | Result |
| --- | --- |
| `pnpm install --frozen-lockfile` | Passed once, using the existing cache |
| `cargo test --lib` | 2,239 passed, 25 ignored, zero failures |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed, zero warnings |
| `cargo fmt --check` | Passed |
| `./node_modules/.bin/tsc --noEmit` | Passed |
| `./node_modules/.bin/oxlint src --deny-warnings` | Passed, zero warnings |
| `./node_modules/.bin/vitest run` | 140 files, 1,606 tests passed |
| Workflow helper Node tests | 19 passed |
| Changed workflow YAML parse and staged whitespace check | Passed |

Cargo ran from `src-tauri`, serialized with
`lockf -k /tmp/voicetypr-cargo.lock`, `CARGO_BUILD_JOBS=4`,
`CARGO_INCREMENTAL=0`, `nice -n 10`, using the existing external shared target
cache. Cargo resolved the merged lockfile, including the ONNX build-time
dependency closure. These are host gates, not a Windows compile or hardware
substitute.

## Review boundaries and fixes

The port preserves beta.8 TLS initialization, model-load failure telemetry,
recording generations, GPU fallback, model progress store, onboarding settings
design, Nemotron 3.5 catalog changes, Vulkan 1.3.296.0 and installer smoke.
The cumulative apply needed manual resolution in `.gitignore`, the native and
release workflows, Cargo.lock, lib.rs, Parakeet manager, transcription engines,
ReadinessLocalPanel and ModelManagementContext. The aborted cherry-pick attempt
left no commits; this is a cumulative port, not the original commit sequence.

Self-review covered native worker ownership, cancelled/stale loads and results,
shared Whisper/ONNX admission, remote-cache lock order, transaction publication,
startup recovery, deletion and exit order, platform capabilities, CLI teardown,
then-static packaging and frontend compatibility (superseded below). Fixes beyond the spike:

- Cancel queued Whisper work before it can evict Parakeet; cancel a queued
  download while waiting for the operation lock. Regression tests hold the lock
  and require cancellation to finish without waiting for its owner.
- Recheck the unload epoch around native model construction and close admission
  again when clearing the slot.
- Permanently close ONNX admission at desktop and CLI exit. A queued preload
  rechecks shutdown after its gates, while model-switch unload stays reusable.
- Unload ONNX in CLI teardown as well as desktop teardown.
- Report a missing selected Parakeet model through the existing blocked-island
  and visible-main error paths, without stealing focus or silently using Whisper.
- Extract startup/selection preloads into `parakeet/preload.rs` and keep the GPU
  warming helper with engine dispatch, rather than growing bootstrap/settings/audio.
- Preserve current onboarding/tests and expose CPU-only, final-only limitations
  with an explicit choice of Whisper.
- Pin and hash the legacy research workflow's three model files. Add a separate
  NSIS artifact to native CI for installed-app hardware checks.

The Silero branch rebased onto `fad6b8a5` replaces the static-ORT/DirectML
packaging blockers with the official lazy Microsoft CPU DLL. Startup does not
preload Windows Parakeet, even if selected. Explicit selection/download or first
transcription may initialize it, after the existing CPU gate and SHA256 check.
The only runtime path is absolute `current_exe().parent()/onnxruntime.dll`.
Missing, altered or wrong-version bytes produce a content-free unavailable error.
There is no PATH/System32 fallback. No ORT exists in either Mac dependency graph.
The original handoff's terminal shutdown admission fix remains intact. Native Windows code has not been compiled
locally: the Rust MSVC target is installed, but a usable MSVC compiler/linker and
Windows SDK are not available on this Mac. Do not install them for this slice.

## Release-critical workflow changes

These changes require release/packaging review, even though no workflow was run.

| Workflow | Change and reason |
| --- | --- |
| `.github/workflows/native-ci.yml` | Stage the pinned official Microsoft CPU DLL and notices; reject ORT/DirectML native imports and static ORT definitions using the main linker MAP; run pinned Ivy Bridge SDE before uploading preview/NSIS artifacts. Preserve Vulkan 1.3.296.0 and existing install/dependency/version/uninstall smoke. |
| `.github/workflows/release.yml` | Stage the official CPU DLL, assert no native/static ORT linkage and run pinned Intel SDE Ivy Bridge CLI checks before upload, so a runtime-linkage change cannot silently produce a broken release package. Existing publishing behavior is unchanged; do not dispatch it for branch validation. |
| `.github/workflows/store-msix.yml` | Stage and verify the official DLL/notices in both layout and final MSIX; assert no native/static ORT linkage and run pinned Ivy Bridge SDE before upload. Preserve the immutable-SHA and reviewed-main ancestry trust gates. |
| `.github/workflows/windows-check.yml` | Run clippy/tests plus main-exe build and pinned SDE `-ivb` version/VAD checks; assert that `parakeet-rs` is absent from the Windows ARM64 dependency graph. Bundle placeholders remain code-check-only, not packaging evidence. |
| `.github/workflows/parakeet-onnx-spike.yml` | Add the legacy standalone research workflow as manual-only, pin the three model files to the app's exact revision and SHA256 values, and use a locked build. This is optional export research, not integrated-app or Windows RSS acceptance. Its cache-key false positive was resolved before commit `bf0a64b2`. |

## Exact CI entry points, only after approval and a push

Run these later from a clone of this repository. They were not run here.

```sh
# Full selected-ref frontend/native checks, including Windows NSIS install smoke.
gh workflow run ci.yml --ref feat/silero-vad-shadow -f include_intel=false

# Optional quicker Windows clippy + Rust tests + SDE CLI checks, no installer.
gh workflow run windows-check.yml --ref feat/silero-vad-shadow
```

`ci.yml` has exactly one input, `include_intel` (boolean, default false).
`native-ci.yml` is reusable (`workflow_call`), not directly dispatchable.
The full Windows lane uses windows-2022, real Vulkan resources, static CRT,
Rust clippy/tests, an NSIS build, install/dependency/`--version`/uninstall smoke
and the native/static-ORT import/MAP assertion. Download the
`ci-windows-x64-nsis` artifact for hardware installation; the
`ci-windows-x64-preview` artifact contains only the app executable, not its
Parakeet DLL. Use the installer for Parakeet hardware evaluation.
Match the run's head SHA to the reviewed full commit SHA. Retain run URL,
artifact hashes and results; never infer runtime quality from CI being green.

The fast lane uses resource placeholders. It proves neither packaging nor GPU
resources. No `model-bench.yml` exists. `parakeet-onnx-spike.yml` is an optional
standalone research benchmark, not the integrated app, cancellation or installed
app evaluation. It has no inputs; if available on the default branch it can be
dispatched at the feature ref. Its Windows RSS field is unavailable, so do not
use it to accept the RAM target.

Store MSIX is a separate workflow with the sole required input `commit_sha`
(full, lowercase, 40-character SHA). Its trust gate requires that commit to be an
ancestor of `origin/main`. It cannot package this unmerged feature branch.
Do not weaken that gate. After reviewed integration and separate approval:

```sh
gh workflow run store-msix.yml --ref main -f commit_sha=FULL_REVIEWED_MAIN_SHA
```

Replace the placeholder with the exact integrated SHA. Never dispatch
`release.yml` merely to check a branch: it publishes releases.

## Private Windows CLI accuracy checks

Use a dedicated Windows test account/profile, not founder data. Install the
approved NSIS artifact. Download models only with the operator's approval.
Set Polish off, remove/disable library rules and custom vocabulary, and record
these settings before WER measurement. The CLI reaches the shared writing
pipeline and has no raw-STT bypass flag. Keep JSON, stderr, references, WAVs,
screenshots and perf reports private; the harness contains transcript text and
paths. Do not upload these as public CI artifacts or copy text into logs.

PowerShell, using operator-chosen paths and an already-installed Whisper model:

```powershell
$Bin = 'C:\Program Files\Voicetypr\voicetypr.exe' # confirm actual installation path
$Clip = 'C:\ParakeetEval\private\matched-16k-mono.wav'
$Out = 'C:\ParakeetEval\private\results'
$WhisperModel = 'small' # replace with the installed comparator's ID
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Get-FileHash -Algorithm SHA256 -LiteralPath $Bin
Get-FileHash -Algorithm SHA256 -LiteralPath $Clip
& $Bin models --json > "$Out\models.json"
if ($LASTEXITCODE -ne 0) { throw 'Model inventory failed' }
& $Bin transcribe --file $Clip --engine parakeet --model parakeet-tdt-0.6b-v3 --json > "$Out\parakeet.json" 2> "$Out\parakeet.stderr"
if ($LASTEXITCODE -ne 0) { throw 'Parakeet CLI failed' }
& $Bin transcribe --file $Clip --engine whisper --model $WhisperModel --json > "$Out\whisper.json" 2> "$Out\whisper.stderr"
if ($LASTEXITCODE -ne 0) { throw 'Whisper CLI failed' }
```

Use the same explicit language policy on both platforms. Parakeet TDT v3 has no
language-hint input; Whisper language, audio context and speed mode must be
recorded, not silently varied.

Prepare the matched PCM **once**, then copy that exact file to both machines
and compare SHA256. Do not independently re-encode on each OS:

```powershell
ffmpeg -nostdin -i 'C:\ParakeetEval\private\source.wav' -map 0:a:0 -ac 1 -ar 16000 -c:a pcm_s16le $Clip
if ($LASTEXITCODE -ne 0) { throw 'Normalization failed' }
```

Run the same Parakeet command with the existing Mac app/sidecar against that
identical normalized PCM. Also compare original-input decoding/downmix/resampling
separately. The 39% versus 6% report refers to **one French clip**, not aggregate
French WER. Investigate PCM differences before changing inference settings.

From a source checkout, Node 22+, use a private real-speech corpus with
`manifest.jsonl` entries containing `file`, `lang`, `reference` and `bucket`.
Run engine/model pairs separately, since the harness uses a Cartesian product:

```powershell
$Corpus = 'C:\ParakeetEval\private\real-corpus'
node scripts/perf-harness.mjs --corpus $Corpus --engines parakeet --models parakeet-tdt-0.6b-v3 --bin $Bin --reps 5 --out "$Out\parakeet-repeats"
if ($LASTEXITCODE -ne 0) { throw 'Parakeet harness failed' }
node scripts/perf-harness.mjs --corpus $Corpus --engines whisper --models $WhisperModel --bin $Bin --reps 5 --out "$Out\whisper-repeats"
if ($LASTEXITCODE -ne 0) { throw 'Whisper harness failed' }
```

Inspect every report's ERROR/SKIPPED rows even when the harness exits zero.
These are repeated **cold-process** CLI runs, not retained-session warm runs.
The existing harness does not measure Windows RSS, same-process warm inference
or cancellation. Do not claim those gates passed from these reports.

## Hardware evidence and acceptance

Use Windows 10 and 11 x64, including a CPU-only 8 GB reference machine. Record:
reviewed code SHA; CI run/artifact SHA256; OS build; CPU/core count; RAM; power
mode; storage; runtime/model revision; intra threads `min(4, cores)`, inter 1;
settings; anonymized clip ID, language, bucket, PCM SHA256 and duration.
For each run record cold model-load ms, decode ms, inference ms, end-to-end ms,
RTF definition, peak process RSS/working set bytes, repetitions and errors.
Retain raw transcripts/references only in private evidence. Public summaries
use edit counts, reference-word counts, WER and content-free outcomes.

- [ ] Real speech: English plus German, Spanish and French, short/quiet/clipped
  speech and mixed channels/sample rates. Same clips/settings against Whisper.
  Report per-clip WER and corpus-weighted WER separately. No unexplained matched
  PCM regression; a single clip's WER is not a language aggregate.
- [ ] Original 52 s empty-single-pass regression, quiet-gap and continuous
  speech, utterances crossing 15/30 s boundaries, plus a long recording.
  Inspect boundary and last words for omission, duplication and timestamp drift.
- [ ] Cold load in a fresh app process. Warm at least five runs in one retained
  desktop session using file upload after preload, not five CLI invocations.
  Capture content-free timings with a debugger/profiler if necessary; the CLI
  exposes no retained-session benchmark command. Warm inference RTF <0.2,
  excluding load/decode, with end-to-end time reported separately.
- [ ] Measure whole-app peak RSS/working set with Windows Process Explorer or
  an equivalent per-process high-water monitor across cold load, long inference
  and switching. State the metric and include children separately. Target
  <2.5 GB on 8 GB CPU-only hardware; report bytes and GB/GiB conversion.
- [ ] Cancel before load, while queued on the shared gate, during native load,
  immediately after load, during a chunk and between chunks. Start a new take.
  No stale text may paste; cancellation may wait for native inference to return.
- [ ] Switch Whisper -> Parakeet -> Whisper during preload, inference and remote
  activity, then delete/quit. No deadlock, wrong-model result, leftover worker
  or silent engine fallback. Explicit Whisper choice after load failure works.

## Installed-app and recovery matrix

Repeat relevant cases on Win10/Win11 x64, NSIS and, after trusted integration,
MSIX. Use disposable accounts/VMs and operator-owned model storage only.

- [ ] Clean machine without dev tools or ORT DLLs; install, `--version`, launch,
  transcribe, uninstall. Inspect installed exe with
  `src-tauri/windows/assert-no-onnxruntime-import.ps1 -ExePath ... -MapPath ...`
  in a Developer PowerShell, using the matching build MAP.
- [ ] Preserve the official Microsoft DLL's Authenticode signature and pinned
  SHA256. NSIS installs beside the executable, MSIX at package root; verify both.
  Keep existing installer/package signing flows; do not re-sign this DLL.
- [ ] Ivy Bridge: `--version` and shadow VAD silence/real-WAV return without
  illegal instruction (VAD `runtime_unsupported`), no ORT loaded at startup.
  On supported x64, first Parakeet use loads only the installed official DLL.
- [ ] Missing, corrupt or wrong-version installed DLL: first Parakeet use gives
  a clear unavailable error; app, Whisper and cloud engines remain alive.
- [ ] Offline installation with required WebView2 prerequisites present.
  Separately test a missing prerequisite offline, first launch without a model,
  and offline transcription with an already verified model. No download
  success indication or silent fallback when unavailable.
- [ ] Verify pinned download revision, three exact sizes and hashes from
  `parakeet/onnx/download.rs`: total 670,479,942 bytes. Tampering, bad length,
  HTTP error and 30 s stall never publish a usable model.
- [ ] Interrupt/cancel downloads at each file, restart the app and retry.
  Partial staging is reclaimed; verified backup is restored when appropriate;
  no partial model is listed as downloaded. Retry restarts the download,
  **not HTTP byte-range resume**.
- [ ] Disk-full before/during transfer, marker write and directory rename.
  Antivirus/file locks at staging, backup and destination, including startup
  recovery and delete. Never delete an unreadable backup merely because its
  state could not be inspected; retry after releasing the lock.
- [ ] Delete while loaded/loading/transcribing, cancel then delete, quit during
  native work, rapid engine changes and relaunch after interrupted publication.
  Confirm files are removed only after native ownership ends.
- [ ] ONNX GPU, live preview, custom vocabulary bias and automatic diarization
  controls stay unavailable with a clear CPU/final-only explanation.
- [ ] macOS regression smoke: unchanged Swift sidecar stdout JSON, preview,
  vocabulary, selection/downloads, back-to-back dictation and exit order.

## Remaining risks and follow-ups

Native ORT calls are cooperative only at call/chunk boundaries, without a hard
inference timeout; an unsupported CPU instruction can terminate the process
rather than become a friendly Rust error. Young export/runtime compatibility,
peak RAM, power-loss durability and Windows antivirus rename/delete behavior
need actual hardware evidence. Rust pre-call CPU guards alone cannot protect
against static initializers before main; the official DLL is lazy and native
imports/static definitions are rejected. GGML_NATIVE=OFF, AVX/AVX2/FMA/F16C=ON
and AVX512=OFF also constrain in-process Whisper's CPU build. VAD itself is
CPU-only whisper.cpp, with no ORT. SDE YAML is not a claim of a Windows run.
A previously retained Whisper preview Arc can
outlive cache eviction, so one-heavy-model memory residency during that preview
is not proven by the shared gate alone (076b follow-up).

Independent review, Windows compile/CI, matched-PCM accuracy, real RAM/warm
timings and both OS installed-app smoke remain required before acceptance.
