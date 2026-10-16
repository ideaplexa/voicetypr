# CrispASR and Auto language Implementation Plan

> **For agentic workers:** Use `executing-plans` inline, as approved by the founder.
> Implement each task with focused red/green checks and a conventional commit.

**Goal:** Ship the three approved quantized local models, background recognition,
and an end-to-end Auto spoken-language choice.

**Architecture:** A persistent C++ CrispASR sidecar owns native inference. Rust
owns model management, process lifecycle and generation-keyed stream results;
React and native menus reuse the existing model and language surfaces.

**Tech Stack:** Tauri 2, Rust 1.91.1, React 19/TypeScript, C++20/CMake,
CrispASR v0.8.41 at `340d7085eaa53c40a46dcb73a6d3d0448a480006`.

---

## Task 1 — Auto language as a spoken-language capability

Files: `src-tauri/src/whisper/languages.rs`,
`src-tauri/src/commands/settings.rs`, `src-tauri/src/menu/languages.rs`,
`src-tauri/src/whisper/transcriber.rs`,
`sidecar/whisper-vulkan/src/main.rs`, `src-tauri/src/transcription.rs`,
`src-tauri/src/writing/pipeline.rs`, `src/components/LanguageSelection.tsx`,
and colocated tests. Keep translation-target language lists separate.

- [x] Add tests for Auto selection, supported-language filtering, English-only
  models and explicit-language-only Cohere. In native tests assert:
  ```rust
  assert_eq!(normalize_speech_language_for_model("whisper", "base", "auto"), "auto");
  assert_eq!(normalize_speech_language_for_model("whisper", "base.en", "auto"), "en");
  assert_eq!(TranscriptionTask::Transcribe.fallback_transcript_language(Some("auto")), None);
  ```
- [x] Run focused tests and confirm the absent Auto behavior is the failure.
- [x] Preserve `auto` in spoken settings, normalize it to an unconstrained
  engine request, and pass `Some("")` to Whisper's `FullParams::set_language`.
  Use `WhisperState::full_lang_id_from_state()` (verify the pinned binding's
  spelling before use) for detected-language metadata, without another model
  pass. Missing settings keep the existing English default.
- [x] Add Auto to the selector and native menu capability paths; never add it
  to the shared list used for explicit output-language selection. Keep unknown
  transcript output language as `same_as_transcript` in the writing pipeline.
- [x] Run `pnpm exec vitest run src/components/LanguageSelection.test.tsx`,
  `pnpm typecheck`, and focused Windows language tests with `run-tests.ps1`.
- [x] Commit Auto support (`ee11107c`); startup/model-switch follow-ups in `3ef928c5`.

## Task 2 — Reproducible native sidecar and protocol

Create `sidecar/crispasr/targets.cmake`, `sidecar/crispasr/src/main.cpp`,
`sidecar/crispasr/src/session.{h,cpp}`, `sidecar/crispasr/tests/`,
`scripts/build-crispasr-sidecar.mjs`, and `sidecar/crispasr/README.md`.

- [x] Add a protocol harness that sends `status`, an invalid model load, a
  stale stream command, and `shutdown`; assert typed responses and clean exit.
  The harness must reject missing executable/invalid JSON as a failed check,
  not as a skipped success.
- [x] Build from the pinned upstream source and its pinned ggml dependency.
  Keep only the required adapters in the executable where upstream's link
  structure permits. Build CPU first; add Metal/Vulkan configurations.
- [x] Implement an owning session class around `CrispasrBackend`:
  ```cpp
  std::unique_ptr<CrispasrBackend> backend;
  std::unique_ptr<CrispasrRealtimeSession> stream;
  ```
  Serialize backend access, reuse matching loaded models, explicitly warm,
  and destroy stream state before unloading the backend.
- [x] Implement versioned JSON commands and structured errors. Require request
  and session identities; validate PCM size/rate and model paths. Keep stdout
  protocol-only and suppress native payload-bearing diagnostics.
- [x] Run CTest and the protocol harness against the built CPU executable.
- [x] Commit native sidecar (`4f570fc1`) and checked failure propagation (`206d95a0`).

## Task 3 — Catalog, downloads and Rust process client

Create `src-tauri/src/crispasr/{mod,models,messages,manager,sidecar}.rs`.
Modify `src-tauri/src/lib.rs`, `src-tauri/src/commands/model.rs`,
`src-tauri/src/commands/settings.rs`, `src/types.ts`, and model lifecycle tests.

- [x] Test the three catalog identities, distinct sizes/checksums, language
  sets, and downloaded-state handling for missing/partial files.
- [x] Pin the HF model revisions and SHA-256s recorded by the upstream tree
  APIs. Use a streaming download and digest, with rename only after validation.
- [x] Test protocol IDs, cancellation, timeout and process termination using a
  committed deterministic fixture process. Implement the client with
  `tokio::process`, bounded requests, `kill_on_drop` and lazy restart.
- [x] Register `CrispasrManager` as managed app state, route local lifecycle
  commands to it, and reuse existing progress/error events and UI model rows.
- [x] Route preload on selection through the manager and unload on shutdown
  without disturbing the existing Whisper/remote/media/analytics teardown order.
- [x] Run focused native lifecycle tests, model-management Vitest and typecheck.
- [x] Commit catalog in the buildable integration slice (`3ef928c5`).

## Task 4 — Shared batch execution and capability integration

Modify `src-tauri/src/provider_capabilities.rs`,
`src-tauri/src/transcription/{engines,executor,stream}.rs`,
`src-tauri/src/commands/audio.rs`, `src-tauri/src/cli.rs`,
`src-tauri/src/menu/`, `src-tauri/src/pill/`, and readiness/model consumers.

- [x] Add dispatch and capability truth-table cases for `crispasr`.
- [x] Reject unsupported translate tasks and retain model-specific language
  sets. Convert native segments/timestamps to `TranscriptionResult` and leave
  missing language/timestamps unknown rather than fabricating values.
- [x] Route desktop, uploads and CLI through the same manager. Prepare mono
  16 kHz input without copying Whisper-only gain/trim/decode parameters.
- [x] Test catalog rejection, Auto/model selection, missing runtime UI, capability
  and readiness contracts. Packaged entry-point behavior remains 082-S2/S5.
- [x] Run focused executor/model tests and frontend model/readiness tests.
- [x] Commit shared recognition routing (`3ef928c5`).

## Task 5 — Background streams and final authority

Create `src-tauri/src/crispasr/{stream,final_result}.rs`; extend native session
tests and sidecar handling. Keep wiring in `commands/audio.rs` small by
placing the new sink/factory and result lifecycle in the engine module.

- [x] Test final-result admission by generation, model, language, complete
  sample coverage and no dropped frames; test cancellation and stale results.
- [x] Feed the existing stream tap through the streaming resampler, including
  `finish()` at stop. Bound ingress to 5 s and retained stream audio to 120 s.
- [x] Implement Parakeet context-preserving preview scheduling with one active
  decode and a replaceable tentative tail (no premature commits). Finalize once
  over complete audio and deliver that result through the handoff.
- [x] Use upstream `create_realtime_session` / `append(..., flush, callback)`
  for R2T2. Keep upstream prefix rollback and language semantics. A valid final
  bypasses batch; invalid/incomplete sessions use complete-file fallback.
- [x] Wire background processing independently of preview visibility. Keep
  final inference outside recorder join deadlines and cancel stale work.
- [x] Run ordered-audio, overflow, cancellation, final-authority and PCM boundary
  tests, followed by a real-speech sidecar stream smoke.
- [x] Commit background capture/final reuse with shared routing (`3ef928c5`).

## Task 6 — Packaging and final evidence

Files: `src-tauri/tauri.{macos,windows}.conf.json`, relevant build/CI workflow
steps, `docs/ARCHITECTURE.md`, `plans/SMOKE.md`, the real-speech harness under
`scripts/`, and this plan's completion notes.

- [x] Stage target-qualified sidecars and required runtime licenses through
  the Node build helper; verify CPU fallback does not require a Vulkan driver.
- [x] Run `pnpm typecheck`, `pnpm lint`, focused/full relevant Vitest, Windows
  backend tests, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --check`, and native CTest without forced clean rebuilds.
- [x] Run the committed harness on redistributable real speech for every model
  available locally. Record load, first partial, stop-to-final, WER/text and
  stream-vs-batch differences. Record unavailable hardware as `NEEDS-SMOKE`.
- [x] Inspect final diff, privacy of diagnostics, source pins, and build
  outputs; package configuration and verification evidence accompany this update.

## Execution notes

- Follow-up decision superseding the display-only request below: Parakeet Ultra
  Q8 is now the first-install default when its runtime is present, and the first
  automatic choice when downloaded and no model is selected. Existing saved
  choices win, downloads still require user confirmation, and unavailable Ultra
  falls back to the established selection policy.

  Follow-up validation: 1,262 frontend tests, typecheck, oxlint, frontend build,
  workspace/all-target clippy, format, 18 workflow-helper tests and actionlint
  pass. Native test relinking timed out twice; the earlier full native pass does
  not certify the final default-policy change. This limitation is recorded for
  upstream review rather than repeating unbounded local builds.

- Founder-approved follow-up: show Parakeet Ultra Q8 as Recommended in the
  first-install model list and settings, alongside existing recommendations.
  This is a display-only endorsement: onboarding's default-selection policy
  continues using the original catalog flag.

- 2026-10-05: implementation and Windows verification complete; packaged desktop,
  native macOS builds/runtime and multilingual/long-speech quality remain
  `NEEDS-SMOKE` in `plans/SMOKE.md` (082-S1–S5). Evidence and exact limitations:
  `docs/reports/2026-10-05-crispasr.md`.
- Final gates: 1,788 Windows library tests, 1,259 frontend tests, typecheck,
  oxlint, frontend production build, workspace/all-target clippy, Rust format,
  actionlint, CPU/Vulkan CTest and native protocol checks. All three models also
  pass the Rust-to-native real-speech batch/recording test. Full suites run
  sequentially with four workers to avoid observed timer contention.
- Tasks 3–5 landed together in `3ef928c5`: manager registration, engine enum
  routing and stream ownership reference each other, so the slice remains
  buildable. Native runtime, native failure handling and packaging are separate
  commits. No push/release was performed.
- Native build adaptation: upstream assumes it owns `CMAKE_SOURCE_DIR`, so
  `targets.cmake` is injected into its pinned root project. The local adapter
  uses C++20 designated initialization while upstream libraries retain their
  own build settings. No upstream source edits are required.
- Review regressions were reproduced before fixes: aborted requests leaving a
  live child, missed cleanup during preload, Auto lost on React model selection,
  and encoder/prefill/decoder errors reported as successful native finals. All
  now pass; follow-up review found no remaining blockers in those fixes.

- Auto language slice: red/green verified (three selector failures and three
  native policy/metadata failures before implementation; prompt test exposed
  and then verified removal of the implicit English instruction). Now 45
  focused frontend tests, 51 native language tests, TypeScript and oxlint pass.
  Vulkan-sidecar Rust checking passes. Workspace clippy exceeded the bounded
  local timeout; no successful clippy result is claimed for that command.
  Native language tests use application-only test profile overrides
  `profile.test.package.voicetypr.debug=0` and `opt-level=0` to avoid repeated
  oversized debug links, then the normal Windows manifest embedding step.

- Founder requested and approved a Windows shipped-bindings prerequisite:
  local `whisper-rs-sys` patch, original checksum-pinned 1.76 MB source archive,
  x64/ARM64 generated snapshots, and native C/Rust ABI checks. The unpatched
  option reproduced three Linux CRT size-assertion failures on Windows. The
  x64 ABI test now passes with an invalid `LIBCLANG_PATH`. The application
  compiled with shipped bindings and its 46 language tests passed after the
  standard Common-Controls manifest was embedded. Vulkan-sidecar Rust checking
  also passed (`DOCS_RS=1`, so this is not a native Vulkan link/smoke check).
- The founder approved moving this session's build cache to
  `D:\voicetypr-build\crispasr-auto` after C: ran out of disk during app linking.
  A worktree-local `src-tauri/target` junction preserves cached native paths.

- 2026-10-04: 43 baseline frontend tests pass across model preview controls,
  island quick settings and settings persistence. Existing Vite config warning
  and the expected rejected-settings test diagnostic are present at baseline.
- Windows environment has Node 24.12.0, pnpm 11.8.0, Rust 1.91.1, CMake 4.4.3
  and Visual Studio 18 Build Tools. Native tools still need environment setup.
