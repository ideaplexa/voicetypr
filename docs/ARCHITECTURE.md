# Voicetypr architecture

Reference for how the code fits together (verified 2026-09-27). The short
rules agents must follow live in [`AGENTS.md`](../AGENTS.md).

## Repository map

```
src/                         React main window, pill, toast
  main.tsx / App.tsx         main window; provider stack in App.tsx
  pill.tsx, pill.css         pill window — vanilla DOM, no React
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
docs/                        research, reports, reviews, this file, RELEASING.md
```

## Backend modules (`src-tauri/src/`)

| Module | Responsibility |
|---|---|
| `lib.rs` | bootstrap, tray/windows, command registry (`invoke_handler`), `RunEvent::Exit` teardown |
| `main.rs`, `cli.rs` | the CLI runs before the GUI boots when argv is a CLI subcommand |
| `state/`, `state_machine.rs` | `AppState`, `RecordingState` and allowed transitions |
| `recording/`, `trigger/` | hotkeys (toggle, push-to-talk, hold) via the `keytrigger` crate |
| `audio/` | CPAL capture (`recorder.rs`), stream tap, speech evidence, pure-Rust decode/resample/normalize (no ffmpeg) |
| `commands/` | Tauri commands by area (`audio.rs` owns the recording/transcription flow) |
| `transcription/` | engine-agnostic layer: `executor.rs` (`transcribe_with_app`), `engines.rs`, `stream.rs` (live preview contract), `capabilities.rs` |
| `whisper/` | in-process Whisper, model cache, decode-ahead preview, Windows GPU sidecar client |
| `parakeet/` | Parakeet sidecar process client, protocol messages, model catalog |
| `cloud_stt/` | Soniox and Deepgram (REST + realtime WebSocket), OpenAI, Groq, Cohere |
| `remote/` | LAN network sharing (warp HTTP server/client, UDP discovery) |
| `writing/` | post-recognition text: vocabulary, library rules, app category, Polish pipeline |
| `ai/` | Polish providers: HTTP APIs, agent CLIs (Claude Code, pi, omp), prompts, catalog |
| `license/`, `secure_store.rs` | licensing; AES-256-GCM encrypted `secure.dat` for secrets |
| `telemetry.rs`, `product_analytics.rs` | GlitchTip errors; PostHog product events |
| `media/`, `menu/`, `window_manager.rs`, `utils/` | media pause, tray, window/pill placement, logging |

Oversized files (split them as part of the 2.1 clean-core work; don't grow
them): `commands/audio.rs` (~8.7k lines), `cloud_stt/soniox.rs` (~4.6k),
`ai/agent_cli.rs` (~4.4k), `remote/http.rs` (~3k), `lib.rs`,
`commands/remote.rs`, `commands/ai.rs`, `audio/recorder.rs`,
`commands/settings.rs`.

## Dictation pipeline

1. **Hotkey** — `recording/hotkeys.rs` (`handle_toggle_mode`, `handle_ptt_mode`,
   hold-to-record) spawns `start_recording` / `stop_recording`
   (`commands/audio.rs`). In-window fallback: `useInAppRecordingHotkey.ts`.
2. **Start** — `start_recording -> Result<bool, String>`: validates license/mic/
   model (`validate_recording_requirements`), opens a recording generation
   (`begin_recording_generation`), `Idle → Starting → Recording`, optionally
   builds a live-preview sink factory, starts the recorder.
3. **Capture** — `audio/recorder.rs`: CPAL callback → writer thread (WAV) +
   stream tap (live preview) + level meter + silence detector + capture metrics.
4. **Stop** — `stop_recording` → `stop_recording_with_mode(.., STOP_POST_ROLL)`:
   `StopInFlightGuard`, `Recording → Stopping`, 250 ms interruptible post-roll,
   drain barrier, finalize writer and tap, `finish_capture`.
5. **Speech gate** — `audio/speech_evidence.rs::classify_speech_evidence`:
   high-confidence no-speech/no-input skips the engine.
6. **Recognition** — `transcription/executor.rs::transcribe_with_app` picks the
   engine and runs it under a timeout policy; Soniox/Deepgram may take the
   realtime WebSocket final (`take_cloud_ws_final`).
7. **Text** — `writing::process_transcription` (`writing/pipeline.rs`):
   sanitize, vocabulary and library rules, language transform, optional Polish.
8. **Deliver** — `commands/text.rs::insert_text` (clipboard + paste, clipboard
   restored); history saved.

`RecordingState`: `Idle → Starting → Recording → Stopping → Transcribing →
Idle`; any → `Error`; `Error → Idle`. Transitions happen in
`commands/audio.rs`, not in the hotkey layer.

## Engines and live preview

| Engine | Where | Live preview | Notes |
|---|---|---|---|
| Whisper | in-process; Windows GPU via sidecar | decode-ahead re-decode | Metal on Apple Silicon; English default |
| Parakeet TDT v3 | Swift sidecar | full-context decode-ahead (plan 070) | 25 European languages; optional CTC custom vocabulary |
| Parakeet Unified (EN) | Swift sidecar | native streaming | best English preview |
| Nemotron multilingual | Swift sidecar | native streaming | |
| Soniox | cloud | realtime WS (`stt-rt-v5`) | WS final authoritative; REST `stt-async-v5` fallback |
| Deepgram | cloud | realtime WS | same authority model as Soniox |
| OpenAI, Groq, Cohere | cloud | final only | |
| Remote (LAN) | another Voicetypr (strong host, weak client) | final only | local network only |

Contract (`transcription/stream.rs`): `EngineStreamCapabilities::for_engine`;
events on `transcription-stream` (`Started/Partial/Final/Cancelled/Error`).
`StreamSessionGate` rejects stale sessions and non-increasing revisions, closes
after a terminal event, and the committed prefix only grows
(`assert_committed_monotonic`). The pasted text is the batch result, except
Soniox/Deepgram, where a complete WS final (via `CLOUD_WS_FINAL`, keyed by
recording generation, invalidated by dropped frames) is authoritative.

## Settings

`Settings` lives in `src-tauri/src/commands/settings.rs`, stored in the
`settings` tauri-plugin-store file. To add a field:

1. Add it to `Settings` with `#[serde(default = ..)]` (or `Option<T>`), plus the
   read arm in `get_settings` and the write arm in `save_settings`.
2. Mirror it in `src/types.ts` (`AppSettings`, same snake_case key).
3. Read it through `SettingsContext` (`useSettings` / `useSetting`).
4. Renames: read the old key as a fallback, delete it on save, and grep for
   every other reader (e.g. `play_sound_on_recording_end` →
   `play_sound_on_transcription_complete`, also read in `audio_feedback.rs`).

Secrets (cloud keys, license, remote passwords) go through `secure_store`
(`secure_set` / `secure_get`); `settings` only records flags such as
`has_password`.

## Telemetry

- `telemetry.rs` → **GlitchTip** (errors, crashes, curated logs): release builds
  only, opt-out, every event rebuilt from an allowlist.
- `product_analytics.rs` → **PostHog EU**: consent-gated, closed set of typed
  events, personless, allowlist-scrubbed; no frontend SDK.
- `commands/dictation_telemetry.rs` owns the completion guard for the stop and
  cancel flows. `dictation.completed` is emitted once per stopped desktop dictation, including
  cancellation, no speech, empty audio, failure, and successful delivery. It
  carries only closed categories and bounded numbers; no transcript, audio,
  clipboard, prompt, key, path, app name, or window title. `stop_to_text_ms`
  measures stop request to text ready for delivery; on paths without text it
  measures stop request to terminal outcome. Clipboard-only delivery has
  `paste=skipped`.

PostHog dashboard definition (all insights filter to `dictation.completed`):

| Insight | Measure | Breakdowns / filters |
|---|---|---|
| Stop to text latency | p50 and p95 of `stop_to_text_ms` | `engine`, `transport`; filter `outcome=delivered` |
| First audio latency | p50 and p95 of `start_to_first_audio_ms` | `os`; exclude events without the property |
| Outcome mix | Count and share of events | `outcome`; optionally break down by `engine` |
| Paste failure rate | `paste=failed` / (`paste=succeeded` + `paste=failed`) | `app_category`; exclude `paste=skipped` |

## Sidecars

- `src-tauri/build.rs` runs `sidecar/parakeet-swift/build.sh release` on every
  macOS cargo build and verifies `dist/parakeet-sidecar-<target-triple>`.
  Bundling: `tauri.macos.conf.json` (Parakeet), `tauri.windows.conf.json`
  (Vulkan sidecar + runtime installers).
- Parakeet protocol: newline JSON on stdin/stdout (`load_model`, `transcribe`,
  `start_stream` / `audio_chunk` / `finalize_stream` / `cancel_stream`,
  `download_ctc_models`, `status`, `shutdown`, …; `start_stream` takes an
  optional `language`). Self-checks: `--decode-ahead-v2-harness`,
  `--decode-ahead-token-harness`.
- Windows GPU: CI/release build `sidecar/whisper-vulkan` into
  `sidecar/whisper-vulkan/dist/whisper-vulkan-sidecar-x86_64-pc-windows-msvc.exe`.
