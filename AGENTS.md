# VoiceTypr — guide for agents

VoiceTypr is a desktop dictation app for **macOS 14+ and Windows 10+**: press a
hotkey, speak, and the text is inserted at the cursor. Speech recognition runs
locally (Whisper, Parakeet), in the cloud (Soniox, Deepgram, OpenAI, Groq,
Cohere), or on a stronger machine on the same LAN ("network sharing"). An
optional **Polish** step cleans the text with an LLM. A floating **pill** shows
live preview text while the user speaks. Stack: Tauri v2, Rust, React 19,
TypeScript 6, Tailwind 4, Base UI (shadcn-style primitives), and a Swift
sidecar on macOS.

This file describes the code as it is (verified 2026-09-27). If you change
something it describes, update it in the same change.

## Ground rules

- Follow the founder's requirements to the letter; check `plans/` (and
  `specs/` if it exists) for the task first.
- **Never push, release or publish** without the founder's explicit
  instruction. Commit locally; work in git worktrees under
  `/Volumes/1tb-drive/developer/oss/worktrees/`, one per slice.
- **Evidence before claims.** Green CI and unit tests prove compilation and
  contracts, not runtime behaviour. Anything that needs real hardware stays
  `NEEDS-SMOKE` until someone ran it; never report it as passed.
- **Engine and accuracy decisions use real speech**, never the synthetic TTS
  clips in `perf-corpus/synthetic` (they wrongly condemned two working Parakeet
  engines in September 2026). Compare word error rate against batch.
- **Privacy is a product promise.** No transcript, audio, clipboard, prompt,
  key, path or window title in logs, GlitchTip or PostHog (see Telemetry).
- **Secrets go in `secure_store.rs`**, never in the `settings` JSON store.
- **Keep it simple and readable.** Prefer an existing Tauri plugin/API over
  custom platform code; no speculative abstractions; fully implemented code
  over clever shortcuts. Behaviour-preserving refactors are separate commits
  from feature changes.
- Strict TypeScript (no `any`); `@/` imports; keep Tauri ↔ Rust ↔ React
  contracts explicit and typed; extend `src/components/ui/*` primitives by
  composition, never edit them in place.

## Commands

```bash
pnpm install
pnpm tauri:dev            # full app (builds the Parakeet sidecar on macOS)
pnpm dev                  # frontend only (Vite, port 1420)
pnpm check                # typecheck → oxlint → vitest → cargo test → clippy -D warnings
pnpm typecheck            # tsc --noEmit
pnpm lint                 # oxlint src --deny-warnings   (not ESLint)
pnpm format               # oxfmt --write
pnpm exec vitest run      # frontend tests once (`pnpm test` = watch mode)
pnpm test:backend         # cd src-tauri && cargo test
cd src-tauri && cargo clippy --workspace --all-targets -- -D warnings   # what CI runs
cd src-tauri && cargo fmt --check
bash sidecar/parakeet-swift/build.sh [debug|release] [--clean]
pnpm tauri build          # native bundle
```

- Rust is pinned by `rust-toolchain.toml` (1.91.1). Node ≥ 22.19.
- Windows: run Rust tests with `src-tauri/run-tests.ps1` (plain `cargo test`
  hits the TaskDialog manifest issue); it builds and executes the tests.
- Run focused tests while iterating, then the full gate before committing.
  Frontend tests assert user-visible behaviour; backend tests cover state
  transitions and error paths.

## Repository map

```
src/                         React main window, pill, toast
  main.tsx / App.tsx         main window; provider stack in App.tsx
  pill.tsx, pill.css         pill window — vanilla DOM, NO React (keep it tiny)
  toast.tsx                  feedback toast window
  components/
    AppContainer.tsx         onboarding vs app; bootstrap/events in components/app/
    AppShell.tsx, Sidebar.tsx, navigation.ts   shell + nav (ScreenId is the route source of truth)
    tabs/TabContainer.tsx    ScreenId → screen (eager imports)
    sections/, sections/models/, onboarding/, polish/
    settings/settings-ui.tsx SettingsPage / SettingsCard / SettingRow layout kit
    ui/                      Base UI primitives (do not modify)
  contexts/                  Settings, License, ModelAvailability, ModelManagement, Readiness
  state/                     zustand stores (enhancements, upload)
  hooks/                     useRecording, useInAppRecordingHotkey, useTauriEvent, …
  lib/EventCoordinator.ts    routes backend events to main / pill / onboarding
  types.ts, types/           shared types — mirror Rust structs by hand
src-tauri/                   Rust backend (workspace: ., crates/keytrigger,
                             crates/transcript-text, crates/vulkan-device-select)
sidecar/parakeet-swift/      macOS Parakeet engine (Swift + FluidAudio 0.17.4)
sidecar/whisper-vulkan/      Windows GPU Whisper engine (Rust + whisper-rs/Vulkan)
plans/                       plan ledger (README.md), SMOKE.md, MASTER-PLAN-2026-Q4.md
docs/                        research, reports, reviews
.github/workflows/           ci, native-ci, windows-check, release, update-beta-channel, store-msix
```

Backend modules (`src-tauri/src/`):

| Module | Responsibility |
|---|---|
| `lib.rs` | bootstrap, tray/windows, command registry (`invoke_handler`), `RunEvent::Exit` teardown |
| `main.rs`, `cli.rs` | the CLI runs before the GUI boots when argv is a CLI subcommand |
| `state/`, `state_machine.rs` | `AppState`, `RecordingState` and allowed transitions |
| `recording/`, `trigger/` | hotkeys (toggle, push-to-talk, hold) via the `keytrigger` crate |
| `audio/` | CPAL capture (`recorder.rs`), stream tap, speech evidence, pure-Rust decode/resample/normalize (no ffmpeg) |
| `commands/` | Tauri commands by area (`audio.rs` owns the recording/transcription flow) |
| `transcription/` | engine-agnostic layer: `executor.rs` (single entry `transcribe_with_app`), `engines.rs`, `stream.rs` (live preview contract), `capabilities.rs` |
| `whisper/` | in-process Whisper, model cache, decode-ahead preview, Windows GPU sidecar client |
| `parakeet/` | Parakeet sidecar process client, protocol messages, model catalog |
| `cloud_stt/` | Soniox and Deepgram (REST + realtime WebSocket), OpenAI, Groq, Cohere |
| `remote/` | LAN network sharing (warp HTTP server/client, UDP discovery) |
| `writing/` | post-recognition text: vocabulary, library rules, app category, Polish pipeline |
| `ai/` | Polish providers: HTTP APIs, agent CLIs (Claude Code, pi, omp), prompts, catalog |
| `license/`, `secure_store.rs` | licensing; AES-256-GCM encrypted `secure.dat` for secrets |
| `telemetry.rs`, `product_analytics.rs` | GlitchTip errors; PostHog product events |
| `media/`, `menu/`, `window_manager.rs`, `utils/` | media pause, tray, window/pill placement, logging |

Oversized files to split (2.1 "clean core" work, see the master plan):
`commands/audio.rs` (~8.7k lines), `cloud_stt/soniox.rs` (~4.6k),
`ai/agent_cli.rs` (~4.4k), `remote/http.rs` (~3k), `lib.rs`,
`commands/remote.rs`, `commands/ai.rs`, `audio/recorder.rs`,
`commands/settings.rs`. Don't grow them; put new logic in focused modules.

## The dictation pipeline

1. **Hotkey** — `recording/hotkeys.rs` (`handle_toggle_mode`, `handle_ptt_mode`,
   hold-to-record) spawns `start_recording` / `stop_recording`
   (`commands/audio.rs`). The in-window fallback is `useInAppRecordingHotkey.ts`.
2. **Start** — `start_recording -> Result<bool, String>`: validates license/mic/
   model (`validate_recording_requirements`), opens a new recording generation
   (`begin_recording_generation`), `Idle → Starting → Recording`, optionally
   builds a live-preview sink factory, starts the recorder.
3. **Capture** — `audio/recorder.rs`: CPAL callback → writer thread (WAV) +
   stream tap (live preview) + level meter + silence detector + capture metrics.
4. **Stop** — `stop_recording` → `stop_recording_with_mode(.., STOP_POST_ROLL)`:
   `StopInFlightGuard`, `Recording → Stopping`, 250 ms interruptible post-roll,
   drain barrier, finalize writer and tap, `finish_capture`.
5. **Speech gate** — `audio/speech_evidence.rs::classify_speech_evidence`:
   high-confidence no-speech/no-input skips the engine entirely.
6. **Recognition** — `transcription/executor.rs::transcribe_with_app` picks the
   engine and runs it under a timeout policy. Soniox/Deepgram may take the
   realtime WebSocket final instead (`take_cloud_ws_final`).
7. **Text** — `writing::process_transcription` (`writing/pipeline.rs`):
   sanitize, vocabulary and library rules, language transform, optional Polish.
8. **Deliver** — `commands/text.rs::insert_text` (clipboard + paste, clipboard
   restored); history is saved.

`RecordingState`: `Idle → Starting → Recording → Stopping → Transcribing →
Idle`; any → `Error`; `Error → Idle`. Transitions happen in
`commands/audio.rs`, not in the hotkey layer.

## Engines and live preview

| Engine | Where | Live preview | Notes |
|---|---|---|---|
| Whisper | in-process (`whisper/`); Windows GPU via sidecar | decode-ahead re-decode | Metal on Apple Silicon; English default, no auto-detect |
| Parakeet TDT v3 | Swift sidecar | full-context decode-ahead (plan 070) | 25 European languages; optional CTC custom vocabulary |
| Parakeet Unified (EN) | Swift sidecar | native streaming | best English preview |
| Nemotron multilingual | Swift sidecar | native streaming | |
| Soniox | cloud | realtime WS (`stt-rt-v5`) | WS final authoritative; REST `stt-async-v5` fallback |
| Deepgram | cloud | realtime WS | same authority model as Soniox |
| OpenAI, Groq, Cohere | cloud | final only | |
| Remote (LAN) | another VoiceTypr (strong host, weak client) | final only today | local network only; needs two-machine smoke |

Contract (`transcription/stream.rs`): `EngineStreamCapabilities::for_engine`;
events on `transcription-stream` (`Started/Partial/Final/Cancelled/Error`).
`StreamSessionGate` rejects stale sessions and non-increasing revisions, closes
after a terminal event, and the committed prefix only grows
(`assert_committed_monotonic`). The pasted text is the batch result, except
Soniox/Deepgram, where a complete WS final (delivered through
`CLOUD_WS_FINAL`, keyed by recording generation, invalidated by dropped frames)
is authoritative.

## Invariants — do not break

1. **The real-time audio callback never allocates or blocks** (chunk pool and
   channel sizing in `audio/recorder.rs`).
2. **Post-roll is for user stops only**; cancel, silence auto-stop, size limit,
   device error and shutdown stop immediately; a later stop interrupts it.
3. **`start_recording` returns `true` only if this call started the
   recording.** Hold-to-talk pairs its stop with its own start using that bool.
4. **Thread the recording generation** through anything async in the
   recording lifecycle; never assume "the current recording".
5. **Keep `StopInFlightGuard`** (duplicate stops are no-ops).
6. **Exit teardown order** (`lib.rs`): clear `TranscriberCache`, stop the remote
   server, macOS media-pause cleanup, analytics shutdown. Skipping the cache
   clear reintroduces a Metal SIGABRT on quit.
7. **Stop budget**: `STOP_JOIN_TIMEOUT` (8 s) covers post-roll + drain + stream
   drop + writer finalize; keep sub-budgets inside it.
8. **Parakeet sidecar stdout is the JSON protocol.** Protocol writes go through
   the duplicated descriptor (`writeProtocolLine`); native-library output is
   redirected around FluidAudio calls. Never print to stdout directly.
9. **Pill**: vanilla DOM only; `pill.css` keeps the `[hidden]` guard block
   (checked by `RecordingPill.css.test.ts`).
10. **Settings Rust ↔ TypeScript stay in lockstep** (below).

## Settings

`Settings` lives in `src-tauri/src/commands/settings.rs`, stored in the
`settings` tauri-plugin-store file. To add a field:

1. Add it to `Settings` with `#[serde(default = ..)]` (or `Option<T>`), plus the
   read arm in `get_settings` and the write arm in `save_settings`.
2. Mirror it in `src/types.ts` (`AppSettings`, same snake_case key).
3. Read it through `SettingsContext` (`useSettings` / `useSetting`).
4. For renames: read the old key as a fallback, delete it on save, and grep for
   every other reader (e.g. `play_sound_on_recording_end` →
   `play_sound_on_transcription_complete`, also read in `audio_feedback.rs`).

Secrets (cloud keys, license, remote passwords) go through `secure_store`
(`secure_set` / `secure_get`); `settings` only records flags such as
`has_password`.

## Telemetry and privacy

- `telemetry.rs` → **GlitchTip** (errors, crashes, curated logs): release builds
  only, opt-out, every event rebuilt from an allowlist before sending.
- `product_analytics.rs` → **PostHog EU**: consent-gated, closed set of typed
  events, personless, allowlist-scrubbed; no frontend SDK.
- Never add transcript text, audio-derived strings, paths, window titles, keys
  or provider error strings as properties. The allowlists are a backstop, not
  the plan.

## Sidecars and worktrees

- `src-tauri/build.rs` runs `sidecar/parakeet-swift/build.sh release` on every
  macOS cargo build and verifies `dist/parakeet-sidecar-<target-triple>`.
  Bundling: `tauri.macos.conf.json` (Parakeet) and `tauri.windows.conf.json`
  (Vulkan sidecar + runtime installers).
- Parakeet protocol: newline JSON on stdin/stdout (`load_model`, `transcribe`,
  `start_stream` / `audio_chunk` / `finalize_stream` / `cancel_stream`,
  `download_ctc_models`, `status`, `shutdown`, …; `start_stream` takes an
  optional `language`). Self-checks: `--decode-ahead-v2-harness`,
  `--decode-ahead-token-harness`.
- Windows GPU: CI/release build `sidecar/whisper-vulkan` into
  `sidecar/whisper-vulkan/dist/whisper-vulkan-sidecar-x86_64-pc-windows-msvc.exe`;
  the main app must not import `vulkan-1.dll`.
- `dist/` folders are gitignored. A new macOS worktree rebuilds the Parakeet
  sidecar automatically; on Windows build or copy the Vulkan sidecar first.

## CI and releases

- `ci.yml`: change classifier → workflow lint → frontend (oxlint, tsc, vitest,
  build) → `native-ci.yml` (macOS: cargo test, workspace clippy, release build;
  Windows: clippy, `run-tests.ps1`, CPU build, no-Vulkan-import check).
  `windows-check.yml` is a faster Windows lane.
- `release.yml` (manual): `channel` stable|beta, `release_type`
  current|patch|minor|major, `beta_number`, `dry_run`. Stable commits the
  version bump and tag to `main`; beta pushes only the tag. Releases are created
  as **drafts**; publishing is a separate, founder-approved step. The job fails
  if `main` moves during the build, so don't push to `main` while a release
  runs. `CHANGELOG.md` is edited by hand before releasing.
- `update-beta-channel.yml` updates the rolling `beta` prerelease manifest when
  a release is published. Updater endpoints (`commands/updater.rs`): stable
  `releases/latest/download/latest.json`; beta
  `releases/download/beta/latest.json`. Betas must be GitHub prereleases.
- `store-msix.yml` builds the Store MSIX for an exact reviewed commit on `main`
  (artifact only; submission is manual). Store installs are Microsoft-signed;
  the direct Windows installer is not Authenticode-signed yet (SmartScreen).

## Planning and release discipline

- Claim non-trivial work in `plans/README.md`; plans are `NNN-slug.md` (letter
  suffix for follow-ups, e.g. `070b`). Done plans move to `plans/archive/`.
  Hardware checks go in `plans/SMOKE.md`; `NEEDS-SMOKE` means code-frozen and
  unverified, not permission to re-implement.
- Triage before fixing: reproduce or gather evidence (version, OS, logs,
  expected vs actual) before changing code.
- Keep beta scope explicit. Any code change after a beta is published needs a
  new `X.Y.Z-beta.N+1` and a rerun of the affected smoke. Stable promotes the
  tested final beta without mixing in unrelated changes.

## Gotchas

1. The pill is an NSPanel on macOS so it doesn't steal focus; test focus and
   back-to-back dictation after touching window code.
2. Tauri permission changes go in `src-tauri/capabilities/`.
3. Speech evidence is asymmetric: detecting speech is strong evidence; failing
   to detect it is not proof of silence. New pre-engine gates start in shadow
   mode.
4. The CLI (`voicetypr status|models|transcribe|record`) returns raw engine text
   for local models — it skips vocabulary and Polish (the `--server` path runs
   them). CLI parity is planned work.
5. Base UI, not Radix: tests mock pointer capture and `getAnimations`
   (`src/test/setup.ts`).
6. React StrictMode double-mounts effects in the main and toast windows (see
   `polish/usePolishSettingsLoad.ts`).
7. FluidAudio v3 can return whole-window blank decodes on some 11–13 s windows;
   0.17.x retries internally, and decode-ahead treats blanks as "no
   information".

## Key references

- `plans/MASTER-PLAN-2026-Q4.md` — roadmap and decisions
- `plans/070-parakeet-full-context-preview.md`, `plans/072-main-merge-decisions.md`
- `docs/RESEARCH-AND-RECOVERY.md` — audio/streaming research entry point
- `README.md` — product overview
