## Unreleased

### Features

* **ai:** harden Claude Code, pi, and oh-my-pi subscription-based Polish with isolated, deadline-bounded processes; add curated/discovered model selection in Advanced settings

### Improvements

* **formatting:** explain that speech models infer punctuation, document Clean Dictation's AI correction role, and add privacy-safe decision diagnostics showing whether AI formatting ran or was skipped
* **ci:** keep routine PR feedback cheap on GitHub-hosted runners, make Intel a manual/release-only legacy lane, and reserve Store packaging for immutable release-candidate commits on `main`

### Bug Fixes

* **ai:** reject truncated or nonzero-exit CLI output, preserve raw dictation on every CLI failure, kill entire CLI process groups on timeout or cancellation, and safely resolve native executables without running Windows script shims
* **tray:** recover from transient system-tray startup failures, keep the dashboard reachable, expose retry help, and attach tray diagnostics to support reports
* **uploads:** constrain long speaker timelines so Copy and Save remain reachable, and keep an in-flight transcript bound to its original file when another file is dropped
* **licensing:** preserve paid access through temporary verification outages, retry validation three times, repair stale device activation automatically, show a revalidation warning instead of `Trial expired`, and keep loading owned by the latest overlapping status request
* **support:** replace the cramped problem-report modal with a dedicated page that collects a reply email, previews attached system configuration, gathers diagnostics automatically, and keeps copy/support-email fallbacks
* **updater:** default direct-install prerelease builds to the Beta update channel when no channel preference has been saved, and preserve the post-update announcement when automatic relaunch fails
* **recording:** include final recorder buffers in speech evidence and restore paused or muted media even when stopping a recording fails
* **ai:** preserve standalone “Sure” and other user-authored text, refresh CLI capabilities and model lists, and allow ready CLI providers to be enabled from tray and shortcut controls
* **soniox:** delete transcription records and uploaded files independently, return transcripts before background cleanup, preserve unknown and active/shared records, and retry storage failures once after capacity frees, cleanup finishes, or an eight-second wait expires
* **licensing:** preserve unreadable saved licenses and credentials instead of deleting them or treating them as absent; report recovery errors without exposing stored contents
* **diagnostics:** isolate backend attribution per recording, show restart requirements when enabling crash reporting, and retain collected diagnostics in copied report fallbacks
* **settings:** complete Polish initialization under React StrictMode and restore model-picker refresh behavior
* **release:** pin the Tauri CLI to 2.6.2 and restore the changelog command used by the local release script

## [2.0.4](https://github.com/moinulmoin/voicetypr/compare/v2.0.3...v2.0.4) (2026-07-01)

### Features

* **stt, ai:** warm the active cloud STT and AI-enhancement provider's connection at recording start so the first request skips the DNS+TCP+TLS handshake, and reuse one pooled HTTP client across requests instead of rebuilding it per call — also speeds back-to-back dictations ([#106](https://github.com/moinulmoin/voicetypr/pull/106))


## [2.0.3](https://github.com/moinulmoin/voicetypr/compare/v2.0.2...v2.0.3) (2026-06-30)

### Bug Fixes

* **cloud-stt:** fix Deepgram API-key validation (check the key via Deepgram's documented token endpoint, and surface a precise "model unavailable — check the key's scopes and your plan" message instead of a misleading "invalid key"), update Soniox to its current async model, and show the underlying transcription model on each cloud provider card ([#105](https://github.com/moinulmoin/voicetypr/pull/105))


## [2.0.2](https://github.com/moinulmoin/voicetypr/compare/v2.0.1...v2.0.2) (2026-06-30)

### Features

* **hotkeys:** retire the global-shortcut plugin and unify all triggers — combos, bare-modifier holds, and isolated taps — on the native keytrigger engine ([#99](https://github.com/moinulmoin/voicetypr/pull/99))
* **hotkeys:** in-app fallback for bare-modifier (isolated-tap) triggers, so a lone-modifier recording hotkey still fires while a Voicetypr text field is focused ([#103](https://github.com/moinulmoin/voicetypr/pull/103))
* **tray:** theme-adaptive tray icon that matches the Windows light/dark taskbar ([#97](https://github.com/moinulmoin/voicetypr/pull/97))

### Bug Fixes

* **tray:** never crash on startup when the system tray fails to initialize (Windows `E_FAIL`) ([#96](https://github.com/moinulmoin/voicetypr/pull/96))
* **telemetry:** ship Windows debug symbols (`.pdb`) so crash reports symbolicate to real frames ([#98](https://github.com/moinulmoin/voicetypr/pull/98))
* **hotkeys:** arm an upgraded combo at startup and never consume bare modifiers on Windows ([#100](https://github.com/moinulmoin/voicetypr/pull/100))
* **hotkeys:** clear the combo hotkey before saving a bare-modifier primary so recording can't double-fire ([#102](https://github.com/moinulmoin/voicetypr/pull/102))
* **windows:** guard tao monitor queries against a stale-handle panic (`Os` error 1461) that could crash on a display change ([#104](https://github.com/moinulmoin/voicetypr/pull/104))


## [2.0.1](https://github.com/moinulmoin/voicetypr/compare/v2.0.0...v2.0.1) (2026-06-25)

### Features

* **telemetry:** default crash/error reporting to opt-out (on by default), with clearer diagnostics-consent copy

### Bug Fixes

* **audio:** prevent a Windows recording crash from an unbounded WASAPI buffer size, and catch panics in the cpal input callback so a device hiccup can't abort the process
* **audio:** bound recorder stop-teardown with a disconnect-independent finalize path
* **keytrigger:** `catch_unwind` around the Windows low-level keyboard hook so a hook panic can't take down input
* **transcription:** hard-timeout the in-process Whisper decode to prevent indefinite "Transcribing…" hangs, and abort the watchdog before remapping completed decodes
* **hotkey:** single-tap (isolated-tap) triggers now work in onboarding, matching the Shortcuts page
* **settings:** avoid a `WindowManager` state panic when changing the pill offset
* **models:** keep a known model selected on startup and open native microphone settings; show animated progress for coarse Parakeet downloads
* **parakeet:** surface sidecar stderr and honor force-download re-fetch
* **onboarding:** expose the model Repair action for parity with the dashboard
* **permissions:** route grant buttons to the native settings panes


## [2.0.0](https://github.com/moinulmoin/voicetypr/compare/v1.13.0...v2.0.0) (2026-06-25)

### Features

* **stt:** cloud speech-to-text providers — Soniox, OpenAI, Groq, Deepgram, and Cohere — selectable per transcription (plan 019)
* **ai:** Rust-native AI polish for OpenAI, Anthropic, Gemini, and custom OpenAI-compatible endpoints (replacing the bundled sidecar), with a deterministic fallback when polish fails (plan 016)
* **ai:** searchable provider/model catalog (~94 models) generated from models.dev (plan 017)
* **transcription:** unify local, cloud, and remote transcription behind a shared executor with never-lose-speech recovery — cancel-anywhere, decode watchdog, silence handling, and device-disconnect recovery (plans 015, 020)
* **upload:** add Save button to file-upload transcript result; opens a native save dialog with .txt and .md filters; .md output includes a `# <filename>` heading; cancel is a no-op; success and error toasts fire
* **diarization:** add cloud speaker diarization for file uploads (Deepgram, Soniox); a diarization-capable provider produces a speaker-attributed transcript ("Speaker 0: … / Speaker 1: …") shown and saved to .txt/.md, while non-diarizing providers fall back to plain text
* **history:** persist rich per-entry metadata (source, engine/model, language, duration, diarized, and — for opted-in dictation — the target app) and add source/app/date filters with per-entry detail
* **cli:** make `--json` consistent across status/models/transcribe/record (human-readable by default) and emit the structured transcription artifact (text, words, metadata) from transcribe/record
* **cli:** install the `voicetypr` command on your PATH from Settings → Advanced so terminals, scripts, and AI agents can drive transcription directly (`voicetypr transcribe <file> --json`, `voicetypr --help`); macOS installs a launcher in /usr/local/bin (prompts for admin only if needed), Windows adds the install directory to your user PATH, and onboarding now points to it
* **feedback:** add actionable remediation to the pill feedback overlay so key recording/permission failures show how to fix them
* **transcription:** let Windows users choose GPU / CPU / Auto transcription acceleration in Settings and onboarding (Auto uses the GPU when available and falls back to CPU; macOS stays on Metal)
* **windows:** add a Microsoft Store (MSIX) distribution path — Store installs are detected at runtime and defer updates to the Microsoft Store, while the existing direct download / NSIS installer path is unchanged
* **shortcuts:** make single-key push-to-talk a clear, first-class option on Hold-to-record, and clarify that General hosts your primary recording shortcut while Shortcuts hosts additional per-action bindings
* **shortcuts:** add a bindable "Toggle AI formatting" shortcut to turn AI polish on/off with one key
* **shortcuts:** allow a single-key shortcut on any action (not just push-to-talk), limited to non-typing keys (function keys, numpad, navigation) and capped at 5 total, so a single key can't hijack normal typing
* **shortcuts:** accept a single non-typing key (function keys, numpad, navigation) for the primary recording hotkey too, so onboarding and General settings no longer reject single-key shortcuts — typing keys and modifier-only combos stay blocked so a hotkey can't hijack normal input
* **shortcuts:** the primary recording hotkey can now be a bare side-specific modifier (e.g. hold Right-⌥) — onboarding routes it through the keytrigger engine as push-to-talk (hold to record, release to stop) instead of the global-shortcut plugin, captures left/right (defaults to right), and keeps the trigger mutually exclusive (a key combo OR a hold-modifier, never both, so recording can't double-fire)
* **onboarding:** warm the selected model the moment it is chosen so the first-transcription test runs immediately instead of stalling on first-load model compilation
* **onboarding:** the first-transcription step now shows an example sentence to read aloud, an editable transcript box, and a "Skip for now" option
* **onboarding:** add a delete action to the setup model cards so a model can be removed and re-downloaded without leaving onboarding
* **logging:** route frontend logs through a leveled `logger` backed by `tauri-plugin-log`, so they land in the same rotated file as the Rust logs (and in Report Bug diagnostics) with a client-side level gate; collapse the model-status flood from ~14 log lines per refresh to a single summary line; and fetch model availability once through a shared provider instead of independently in every consumer
* **history:** add a per-entry "Show original / Show formatted" toggle in Recent Recordings so the raw transcript can be compared against the AI-formatted result; the pre-AI transcript is stored in local history only (never written to logs) and the toggle appears only when AI actually changed the text
* **shortcuts:** simplify setting a recording shortcut to one flow — current keys + Edit → press → Save/Cancel, with a single "Hold to talk" checkbox; removed the mode picker, left/right side selector, double-tap selector, and trigger-kind dropdowns from onboarding, General settings, and the Shortcuts list. A lone modifier now toggles recording on a single clean tap (new isolated-tap detector — a modifier tapped inside a real shortcut like ⌘C never toggles), or hold-to-talk when the checkbox is on

### Bug Fixes

* **transcription:** show short, plain messages in the recording overlay instead of long internal error strings; auth/model failures now point to Settings / model selection rather than an unhelpful "try again" (full detail still logged)
* **remote:** the Network Sharing card now shows the real number of recently-connected clients (distinct client IPs within the last 5 minutes) instead of always 0
* **parakeet:** restore the Parakeet sidecar in the macOS `tauri dev` build so local transcription works again — the dev config redefined `bundle.externalBin`, which is applied last in Tauri's config merge and clobbered the macOS platform config that registers the sidecar; the model still reported as available (availability only checks the downloaded model files, not the sidecar binary) while transcription failed with "model unavailable / No such file or directory"
* **parakeet:** receive the sidecar's JSON protocol responses from stderr as well as stdout — the Swift sidecar emits its load/transcribe/status responses on stderr (its stdout is redirected around native CoreML calls), so the Rust side received nothing and every local transcription hung until the 300s/180s timeout fired; recording now completes (or fails fast) instead of sitting on "Transcribing…" indefinitely
* **deps:** move the `hono` override from the deprecated `package.json` `pnpm.overrides` field to `pnpm-workspace.yaml`, restoring the supply-chain pin on pnpm v11 and silencing the "pnpm field is no longer read" warning
* **recording:** render the recording pill on a dark surface again instead of white — the pill window never receives the `.dark` theme class, so its styling no longer falls back to the light base
* **parakeet:** report the real recording length in Recent Recordings instead of 0:00 — the sidecar's FluidAudio `duration` is often 0.0, so the duration now falls back to measuring the transcribed WAV when the sidecar reports none
* **parakeet:** stop the per-transcription "Failed to parse sidecar response" ERROR/WARN log flood — the sidecar's stderr status banners now log once at debug without their raw contents (which can include transcript text), and a genuine stdout protocol parse failure logs as ERROR with only a byte count
* **ai:** validate a custom OpenAI-compatible connection by calling `/chat/completions` the same way formatting actually runs, instead of trusting the `/models` list — Test now correctly fails on a missing/invalid key or a non-chat-compatible endpoint (no more false "Connection successful" from a public `/models`) and correctly passes for usable models a curated/proxied `/models` doesn't advertise; provider error bodies are no longer surfaced to the UI or logs
* **cli:** print CLI output on Windows — the GUI-subsystem executable now attaches to the parent terminal's console when invoked as a CLI, so `voicetypr … --json` and `--help` appear instead of vanishing into a null handle; existing shell redirection (`> out.json`, `2> err.txt`, piped `--password-stdin`) is preserved

### Chores

* **dev:** `pnpm tauri:dev` now reaps an orphaned previous dev process before launching, so a stale single-instance ghost from an interrupted session no longer makes the launch exit 143 — via a small cross-platform Node launcher (path-scoped `pkill` on macOS/Linux; Windows just launches). Docs now point at `pnpm tauri:dev`, which isolates the dev build from an installed one via the `.dev` identifier


## [1.13.0](https://github.com/moinulmoin/voicetypr/compare/v1.12.10...v1.13.0) (2026-06-15)

### Features

* **recording:** replace destructive silence auto-stop with non-destructive warnings, so a quiet pause no longer discards in-progress audio ([31c3160](https://github.com/moinulmoin/voicetypr/commit/31c3160))
* **bug-report:** collect full system specs (GPU, CPU, model) in bug reports for faster diagnosis ([929351d](https://github.com/moinulmoin/voicetypr/commit/929351d))

### Bug Fixes

* **recording:** recover from autonomous recorder stops that previously left recording stuck ([d33e669](https://github.com/moinulmoin/voicetypr/commit/d33e669))
* **recording:** run stop cleanup on the recorder-error path so a failed recorder no longer hangs the session ([f008d67](https://github.com/moinulmoin/voicetypr/commit/f008d67))
* **recording:** require sustained voice before treating audio as speech, reducing false starts from brief noise ([3363b7c](https://github.com/moinulmoin/voicetypr/commit/3363b7c))
* **windows:** pause media that under-reports `IsPauseEnabled` during recording ([2bd683b](https://github.com/moinulmoin/voicetypr/commit/2bd683b))
* **bug-report:** skip software DXGI adapters in GPU detection so the real GPU is reported ([6505574](https://github.com/moinulmoin/voicetypr/commit/6505574))
* **recording:** default pause-media-during-recording to off ([e6acb22](https://github.com/moinulmoin/voicetypr/commit/e6acb22))


## [1.12.10](https://github.com/moinulmoin/voicetypr/compare/v1.12.9...v1.12.10) (2026-06-10)

### Bug Fixes

* **transcription:** bound CPU Whisper so slow CPU-only systems no longer get stuck on "Transcribing"; cut short-dictation latency by dropping the hidden temperature-fallback retry, add timeout/cancellation for stuck native decodes, and show "No speech detected" for empty / `[SOUND]` / `[BLANK_AUDIO]` output ([63cd921](https://github.com/moinulmoin/voicetypr/commit/63cd921))

### Chores

* **ci:** pin Windows runners to VS 2022 to avoid VS 2026 CMake generator failures in whisper-rs-sys ([bfd7899](https://github.com/moinulmoin/voicetypr/commit/bfd7899))


## [1.12.9](https://github.com/moinulmoin/voicetypr/compare/v1.12.8...v1.12.9) (2026-06-08)

### Bug Fixes

* **hotkey:** prevent repeated toggle-hotkey stops from a held shortcut producing empty recordings ([9fca33f](https://github.com/moinulmoin/voicetypr/commit/9fca33f))
* **models:** repair model download verification so valid downloads are no longer rejected ([2ab4b0c](https://github.com/moinulmoin/voicetypr/commit/2ab4b0c))


## [1.12.8](https://github.com/moinulmoin/voicetypr/compare/v1.12.7...v1.12.8) (2026-06-05)

### Bug Fixes

* **models:** keep model management responsive during downloads ([#89](https://github.com/moinulmoin/voicetypr/pull/89))


## [1.12.7](https://github.com/moinulmoin/voicetypr/compare/v1.12.6...v1.12.7) (2026-06-05)

### Bug Fixes

* **windows:** repair model catalog downloads ([#87](https://github.com/moinulmoin/voicetypr/pull/87))
* **windows:** warm the Vulkan sidecar more reliably and fix spacing ([#88](https://github.com/moinulmoin/voicetypr/pull/88))


## [1.12.6](https://github.com/moinulmoin/voicetypr/compare/v1.12.5...v1.12.6) (2026-06-04)

### Bug Fixes

* **windows:** optimize transcription fallback so CPU-only recovery is more usable when GPU acceleration is unavailable ([#85](https://github.com/moinulmoin/voicetypr/pull/85))


## [1.12.5](https://github.com/moinulmoin/voicetypr/compare/v1.12.4...v1.12.5) (2026-06-02)

### Bug Fixes

* **windows:** fix Vulkan sidecar timing fields so successful GPU responses parse instead of falling back to slow CPU mode ([fb83d1b](https://github.com/moinulmoin/voicetypr/commit/fb83d1b))


## [1.12.4](https://github.com/moinulmoin/voicetypr/compare/v1.12.3...v1.12.4) (2026-06-01)

### Features

* **ai:** restore the native Anthropic provider with current model aliases and recommended-model metadata ([9c5e224](https://github.com/moinulmoin/voicetypr/commit/9c5e224), [38a9c19](https://github.com/moinulmoin/voicetypr/commit/38a9c19))

### Bug Fixes

* **windows:** isolate Vulkan Whisper acceleration in a sidecar so the main app stays CPU-safe on machines without Vulkan drivers ([e08b8c8](https://github.com/moinulmoin/voicetypr/commit/e08b8c8))
* **updater:** require user confirmation or explicit automatic-update opt-in before downloading and installing updates ([e08b8c8](https://github.com/moinulmoin/voicetypr/commit/e08b8c8))
* **windows:** use basic UI installer mode so updater elevation can show the UAC prompt correctly ([e08b8c8](https://github.com/moinulmoin/voicetypr/commit/e08b8c8))
* **recording:** prevent duplicate push-to-talk press events from starting overlapping recording flows ([e08b8c8](https://github.com/moinulmoin/voicetypr/commit/e08b8c8))
* **support:** add hotkey diagnostics and report context for shortcut-registration and event-delivery failures ([e08b8c8](https://github.com/moinulmoin/voicetypr/commit/e08b8c8), [a0c1688](https://github.com/moinulmoin/voicetypr/commit/a0c1688))
* **macos:** keep Parakeet sidecar stdout reserved for JSON so CoreML diagnostics cannot break model autoload responses ([741f032](https://github.com/moinulmoin/voicetypr/commit/741f032))


## [1.12.3](https://github.com/moinulmoin/voicetypr/compare/v1.12.2...v1.12.3) (2026-04-29)

### Features

* **settings:** add an auto-paste toggle so users can keep transcripts in history without automatically inserting them ([1c47ac3](https://github.com/moinulmoin/voicetypr/commit/1c47ac3))
* **support:** add in-app bug and crash report submission with system info and the latest redacted app log excerpt ([77a9b26](https://github.com/moinulmoin/voicetypr/commit/77a9b26), [be90102](https://github.com/moinulmoin/voicetypr/commit/be90102))

### Bug Fixes

* **support:** harden report validation, stale-submit guards, fallback behavior, and log redaction for private report delivery ([b8291e3](https://github.com/moinulmoin/voicetypr/commit/b8291e3), [6953429](https://github.com/moinulmoin/voicetypr/commit/6953429), [fa45160](https://github.com/moinulmoin/voicetypr/commit/fa45160))
* **ui:** restore scrolling for Upload, General, Help, Advanced, and Formatting settings pages after the sidebar layout changes ([551fc4e](https://github.com/moinulmoin/voicetypr/commit/551fc4e), [10eb66f](https://github.com/moinulmoin/voicetypr/commit/10eb66f))


## [1.12.2](https://github.com/moinulmoin/voicetypr/compare/v1.12.1...v1.12.2) (2026-04-27)

### Bug Fixes

* **transcription:** fix truncation in long recordings and preserve explicit insertion boundaries ([4719f3d](https://github.com/moinulmoin/voicetypr/commit/4719f3d), [4627145](https://github.com/moinulmoin/voicetypr/commit/4627145))
* **transcription:** preserve sentence spacing on insert and use compatible punctuation matching ([987d881](https://github.com/moinulmoin/voicetypr/commit/987d881), [bb63e21](https://github.com/moinulmoin/voicetypr/commit/bb63e21))
* **recording:** harden push-to-talk startup, license fallback, and abort handling ([2205bc1](https://github.com/moinulmoin/voicetypr/commit/2205bc1), [d38f8cb](https://github.com/moinulmoin/voicetypr/commit/d38f8cb), [7328b5c](https://github.com/moinulmoin/voicetypr/commit/7328b5c), [aa30d5e](https://github.com/moinulmoin/voicetypr/commit/aa30d5e), [86d2f17](https://github.com/moinulmoin/voicetypr/commit/86d2f17))
* **windows:** reduce Windows crash paths and bundle the WebView2 offline installer for clean first-run installs ([4719f3d](https://github.com/moinulmoin/voicetypr/commit/4719f3d), [8c58cd2](https://github.com/moinulmoin/voicetypr/commit/8c58cd2))
* **ui:** keep sidebar navigation accessible and route settings navigation to the correct tab ([1d70be3](https://github.com/moinulmoin/voicetypr/commit/1d70be3))
* **license:** show the Manage License button when a license is already active ([4719f3d](https://github.com/moinulmoin/voicetypr/commit/4719f3d))
* **ai:** persist AI formatting across restart by warming cached credentials from secure storage ([482dacb](https://github.com/moinulmoin/voicetypr/commit/482dacb))
* **updater:** remove stale v1 dialog config and use a versioned update marker for post-update UX ([ad84a1d](https://github.com/moinulmoin/voicetypr/commit/ad84a1d))
* **autostart:** move autostart toggles to backend-owned commands for atomic OS state management ([9aa7bdd](https://github.com/moinulmoin/voicetypr/commit/9aa7bdd))


## [1.12.1](https://github.com/moinulmoin/voicetypr/compare/v1.12.0...v1.12.1) (2026-02-22)


### Bug Fixes

* **openai:** handle token and temperature compatibility for gpt-5 ([42300b9](https://github.com/moinulmoin/voicetypr/commit/42300b96148ef6c4bcf05e33ca5ce8708834ffc9))


### Refactors

* **ai:** remove anthropic provider support ([2caf98c](https://github.com/moinulmoin/voicetypr/commit/2caf98cbaa1a9e9e5455c449a72f9f0022adda83))



## [1.12.0](https://github.com/moinulmoin/voicetypr/compare/v1.11.2...v1.12.0) (2026-02-21)


### Features
* add sound on recording end with toggle option ([12095a3](https://github.com/moinulmoin/voicetypr/commit/12095a3de9385918fa44573da63b271db546fc5c))
* add pill indicator mode dropdown with Never/Always/When Recording options ([7dfa707](https://github.com/moinulmoin/voicetypr/commit/7dfa707c78e18251221d4348d9f12e16c7aced0c))
* add recording indicator position setting with 6 options and configurable edge offset ([e2b71f5](https://github.com/moinulmoin/voicetypr/commit/e2b71f5b1f2eb76062e5059955cdf8f6f93bd2a9))
* **macos:** add Intel Mac (x86_64) support ([96abe0f](https://github.com/moinulmoin/voicetypr/commit/96abe0fc1642041ea2cbb7811269c82e45f60dbc))
* language-aware AI formatting and stale microphone validation ([ccb8f56](https://github.com/moinulmoin/voicetypr/commit/ccb8f569205865cab7c3ab4c6288c09949c19ef3))
* add multi-provider AI formatting support (OpenAI, Anthropic, Gemini) with curated model lists ([a5084a9](https://github.com/moinulmoin/voicetypr/commit/a5084a9fbc0d242afc9c1da2a43e3eeacc343eb5))
* add pause media during recording feature ([b501ca1](https://github.com/moinulmoin/voicetypr/commit/b501ca1cf590dba9266b963ac345a8a6ac22126d))
* add crash reporter UI with GitHub issue integration ([cbd836a](https://github.com/moinulmoin/voicetypr/commit/cbd836a3758e46c697d4d90a320b3aa9c2bd031b))


### Bug Fixes
* **windows:** hide console window flash when starting recording ([99c467d](https://github.com/moinulmoin/voicetypr/commit/99c467db4152a60f4e114b3ff5fc3b371ece4459))
* **windows:** ARM64 crash fix - disable Vulkan GPU, optimize CPU threads ([1b91ba7](https://github.com/moinulmoin/voicetypr/commit/1b91ba7b2e7d77b33ab0df8b1f4982b498f2b814))
* **windows:** bundle runtime and harden fresh install behavior ([a9b3577](https://github.com/moinulmoin/voicetypr/commit/a9b357736c4f669aadfbf655910f5eb919c4e4ef))
* **audio:** prevent app hang when audio device doesn't respond to cleanup ([1a011c6](https://github.com/moinulmoin/voicetypr/commit/1a011c69154678610caf632e39d0bf593588b0da))
* **audio:** use platform-specific stream cleanup to prevent Windows hang ([816ff98](https://github.com/moinulmoin/voicetypr/commit/816ff98e7cd05070319b980d5267773dcece98cb))
* add delay after start sound for Bluetooth headset compatibility ([6d4a681](https://github.com/moinulmoin/voicetypr/commit/6d4a681cf48482b05abd33e65d5fd6c0c61d3b57))
* **ui:** remove unwanted white ring border and add consistent ring styling to pill and toast ([57ddae1](https://github.com/moinulmoin/voicetypr/commit/57ddae1f28819fd1807a6c6023162e6b71319935))
* **media:** improve pause/resume reliability and use osascript playback toggle on macOS ([c7487fb](https://github.com/moinulmoin/voicetypr/commit/c7487fb7d0b04ed76d3be6659d9eff7f2700a65d))
* isolate custom and openai provider key/config handling ([48ee91b](https://github.com/moinulmoin/voicetypr/commit/48ee91b9a6ecad6a42e6d16fffdc546868f5cf51))
* slider showing range instead of single value ([dba6e7c](https://github.com/moinulmoin/voicetypr/commit/dba6e7c9a85eb8ea0925e4e62e9bc4008469acc1))
* per-request HTTP clients and media resume on recording errors ([9a480ad](https://github.com/moinulmoin/voicetypr/commit/9a480ad3313c9e8fdfb2fcca3f1fab71a9635307))



## [1.11.2](https://github.com/moinulmoin/voicetypr/compare/v1.11.1...v1.11.2) (2025-12-16)


### Bug Fixes

* auto-install updates in background and use updateService in AboutSection ([76ec247](https://github.com/moinulmoin/voicetypr/commit/76ec247deaf80d3ef5cab56437eab7d10dc892aa))
* avoid dropping empty password in Windows signing ([f60cd8b](https://github.com/moinulmoin/voicetypr/commit/f60cd8b5628fa0d6f68d4817719164d41fd3e4fa))
* use -f flag for private key file path in signer ([93811c5](https://github.com/moinulmoin/voicetypr/commit/93811c5609bf1fa8016cd3530232b6dd78e419a8))
* use pnpm tauri signer in Windows script ([a4e6e65](https://github.com/moinulmoin/voicetypr/commit/a4e6e65c826b457da5e822feac8634af7a853250))
* use pnpm tauri signer instead of cargo tauri ([cf5d085](https://github.com/moinulmoin/voicetypr/commit/cf5d085f7fc315fe7afc8e4bb03db6f0057d03d3))


### Features

* **macos:** toggle dock icon based on main window visibility ([0854459](https://github.com/moinulmoin/voicetypr/commit/08544590cb4c667aa2682d02e99a442845a48525))
* session-aware auto-updates with notification support ([cdd7d89](https://github.com/moinulmoin/voicetypr/commit/cdd7d89e3290dccd1690a3f7ef7391b7a01b7b52))



## [1.11.1](https://github.com/moinulmoin/voicetypr/compare/v1.10.0...v1.11.1) (2025-12-14)


### Bug Fixes

* add debug output and explicit GITHUB_TOKEN export ([340ebc9](https://github.com/moinulmoin/voicetypr/commit/340ebc949a3bfa883055345e374e615842df2178))
* add missing typecheck and requireCommits check from release-it ([a4cbd35](https://github.com/moinulmoin/voicetypr/commit/a4cbd354ce23e2f470aa0220eea6acea42a87707))
* add skipChecks to bypass release-it token validation ([72e5e64](https://github.com/moinulmoin/voicetypr/commit/72e5e6462bc8db754c7dc238a0c60a2f25d15129))
* add toast window to capabilities and increase pill-toast gap ([138c902](https://github.com/moinulmoin/voicetypr/commit/138c902cd530140a0ea62f409231e1049c35a2d7))
* align OpenAI config flow ([#34](https://github.com/moinulmoin/voicetypr/issues/34)) ([80c2cb6](https://github.com/moinulmoin/voicetypr/commit/80c2cb6a981bf14dcda95a878446e84244f0db67))
* auto-set GITHUB_TOKEN from gh CLI for release-it ([9469a0e](https://github.com/moinulmoin/voicetypr/commit/9469a0e2142ac92652b6ffbc342b2f97b606928d))
* avoid double log extension ([820bb1e](https://github.com/moinulmoin/voicetypr/commit/820bb1e4ad5e1966caa3721849eae1508e301993))
* defer mic permission prompts until onboarding complete ([a51b379](https://github.com/moinulmoin/voicetypr/commit/a51b379d1c137d08cf641d3075674defdfc04246))
* ensure audio device listeners clean up ([51d5e3f](https://github.com/moinulmoin/voicetypr/commit/51d5e3f2d5510376a2ed32d85623e0ea9cfb1f60))
* improve AI config auto-selection ([311c61e](https://github.com/moinulmoin/voicetypr/commit/311c61eeb7c176ec42797aeb47b2209988e7b58e))
* improve OpenAI config UX with Update button and better switch visibility ([0a0fa64](https://github.com/moinulmoin/voicetypr/commit/0a0fa6401191d0f1de9c447df7e3750dacf51d61))
* improve recording feedback with sound, throttle, and duration gates ([a0627c1](https://github.com/moinulmoin/voicetypr/commit/a0627c199939d086e366e4fbd8c255a3fa5cad43))
* improve toast notification with dynamic width and better error messages ([04f90f1](https://github.com/moinulmoin/voicetypr/commit/04f90f18cb41e5e4827f660c84a7d11b06b6023b))
* license errors, app reset, and tauri config ([70238af](https://github.com/moinulmoin/voicetypr/commit/70238afa71865a12f3976f14bc4b6398431e7b60))
* redact API key logging ([e0ceb8d](https://github.com/moinulmoin/voicetypr/commit/e0ceb8d52cc72970396b78ca7c5e955847649e0d))
* remove unnecessary hooks and additional directories from AI config ([ade912b](https://github.com/moinulmoin/voicetypr/commit/ade912b57dbd8e245c646e220d0dff9f028874e3))
* route recording errors via pill toast ([3fba56f](https://github.com/moinulmoin/voicetypr/commit/3fba56f75103e1589998e615f94eece75af887a1))
* sync microphone list with system ([6de9eaa](https://github.com/moinulmoin/voicetypr/commit/6de9eaa7da0ed279cb49ea3ccd383b0f04820e61))
* treat only watchdog timeout as timeout ([edc260e](https://github.com/moinulmoin/voicetypr/commit/edc260e3efb1ba7dbc8bedd547a61f14bb638dff))
* use gh CLI for GitHub releases instead of release-it ([89a4d8f](https://github.com/moinulmoin/voicetypr/commit/89a4d8fac24ba0e7d5a8157fc891893a580a0f1f))
* Windows device ID fallback + license status timeout ([27ab86d](https://github.com/moinulmoin/voicetypr/commit/27ab86d9393a2a32a201f9db00b05bd942d9b6d3))


### Features

* add --build-only flag to release script for resuming builds ([7328a35](https://github.com/moinulmoin/voicetypr/commit/7328a3593d5f14ff9f1c6f32669e62fec908f2bb))
* add --dry-run flag to release script ([6778622](https://github.com/moinulmoin/voicetypr/commit/67786227bc43a043a5b11b370755121bf7efbba5))
* add clipboard retention preference ([ba80d62](https://github.com/moinulmoin/voicetypr/commit/ba80d62ca92902861d9b412495c2f6938df1161f))
* improve AI model availability and auto-selection ([a44bdd9](https://github.com/moinulmoin/voicetypr/commit/a44bdd9b83a30d7239356e5a6ab277460fd98a76))
* move pill/toast lower and auto-reposition on monitor change ([39ab78b](https://github.com/moinulmoin/voicetypr/commit/39ab78b12d2300bf94a87da2e2670df30878350e))
* queue critical pill events for reliable delivery ([2799928](https://github.com/moinulmoin/voicetypr/commit/2799928b95287eb4e6257feb90bf56b5b51684b8))
* simplify pill UI to minimal 3-dot indicator with separate toast window ([defc907](https://github.com/moinulmoin/voicetypr/commit/defc90753145f00f2018968d67b15542bd6e13e8))



# Changelog

# [1.11.0](https://github.com/moinulmoin/voicetypr/compare/v1.10.0...v1.11.0) (2025-12-14)


### Bug Fixes

* add debug output and explicit GITHUB_TOKEN export ([340ebc9](https://github.com/moinulmoin/voicetypr/commit/340ebc949a3bfa883055345e374e615842df2178))
* add skipChecks to bypass release-it token validation ([72e5e64](https://github.com/moinulmoin/voicetypr/commit/72e5e6462bc8db754c7dc238a0c60a2f25d15129))
* add toast window to capabilities and increase pill-toast gap ([138c902](https://github.com/moinulmoin/voicetypr/commit/138c902cd530140a0ea62f409231e1049c35a2d7))
* align OpenAI config flow ([#34](https://github.com/moinulmoin/voicetypr/issues/34)) ([80c2cb6](https://github.com/moinulmoin/voicetypr/commit/80c2cb6a981bf14dcda95a878446e84244f0db67))
* auto-set GITHUB_TOKEN from gh CLI for release-it ([9469a0e](https://github.com/moinulmoin/voicetypr/commit/9469a0e2142ac92652b6ffbc342b2f97b606928d))
* avoid double log extension ([820bb1e](https://github.com/moinulmoin/voicetypr/commit/820bb1e4ad5e1966caa3721849eae1508e301993))
* defer mic permission prompts until onboarding complete ([a51b379](https://github.com/moinulmoin/voicetypr/commit/a51b379d1c137d08cf641d3075674defdfc04246))
* ensure audio device listeners clean up ([51d5e3f](https://github.com/moinulmoin/voicetypr/commit/51d5e3f2d5510376a2ed32d85623e0ea9cfb1f60))
* improve AI config auto-selection ([311c61e](https://github.com/moinulmoin/voicetypr/commit/311c61eeb7c176ec42797aeb47b2209988e7b58e))
* improve OpenAI config UX with Update button and better switch visibility ([0a0fa64](https://github.com/moinulmoin/voicetypr/commit/0a0fa6401191d0f1de9c447df7e3750dacf51d61))
* improve recording feedback with sound, throttle, and duration gates ([a0627c1](https://github.com/moinulmoin/voicetypr/commit/a0627c199939d086e366e4fbd8c255a3fa5cad43))
* improve toast notification with dynamic width and better error messages ([04f90f1](https://github.com/moinulmoin/voicetypr/commit/04f90f18cb41e5e4827f660c84a7d11b06b6023b))
* license errors, app reset, and tauri config ([70238af](https://github.com/moinulmoin/voicetypr/commit/70238afa71865a12f3976f14bc4b6398431e7b60))
* redact API key logging ([e0ceb8d](https://github.com/moinulmoin/voicetypr/commit/e0ceb8d52cc72970396b78ca7c5e955847649e0d))
* remove unnecessary hooks and additional directories from AI config ([ade912b](https://github.com/moinulmoin/voicetypr/commit/ade912b57dbd8e245c646e220d0dff9f028874e3))
* route recording errors via pill toast ([3fba56f](https://github.com/moinulmoin/voicetypr/commit/3fba56f75103e1589998e615f94eece75af887a1))
* sync microphone list with system ([6de9eaa](https://github.com/moinulmoin/voicetypr/commit/6de9eaa7da0ed279cb49ea3ccd383b0f04820e61))
* treat only watchdog timeout as timeout ([edc260e](https://github.com/moinulmoin/voicetypr/commit/edc260e3efb1ba7dbc8bedd547a61f14bb638dff))
* Windows device ID fallback + license status timeout ([27ab86d](https://github.com/moinulmoin/voicetypr/commit/27ab86d9393a2a32a201f9db00b05bd942d9b6d3))


### Features

* add clipboard retention preference ([ba80d62](https://github.com/moinulmoin/voicetypr/commit/ba80d62ca92902861d9b412495c2f6938df1161f))
* improve AI model availability and auto-selection ([a44bdd9](https://github.com/moinulmoin/voicetypr/commit/a44bdd9b83a30d7239356e5a6ab277460fd98a76))
* move pill/toast lower and auto-reposition on monitor change ([39ab78b](https://github.com/moinulmoin/voicetypr/commit/39ab78b12d2300bf94a87da2e2670df30878350e))
* queue critical pill events for reliable delivery ([2799928](https://github.com/moinulmoin/voicetypr/commit/2799928b95287eb4e6257feb90bf56b5b51684b8))
* simplify pill UI to minimal 3-dot indicator with separate toast window ([defc907](https://github.com/moinulmoin/voicetypr/commit/defc90753145f00f2018968d67b15542bd6e13e8))

# [1.10.0](https://github.com/moinulmoin/voicetypr/compare/v1.9.0...v1.10.0) (2025-10-24)


### Bug Fixes

* **audio:** prevent empty or ultra-short recordings from causing errors; add mode-specific min durations (PTT≥1s, Toggle≥3s); auto-recover from Error→Idle and gate start transitions ([97e9fad](https://github.com/moinulmoin/voicetypr/commit/97e9fad0de58714cecf5f9a55f20cd17500bca78))
* avoid panic when requesting parakeet sidecar ([855a2ed](https://github.com/moinulmoin/voicetypr/commit/855a2ed9d2190eb1673a3cc166132fdacf612ff7))
* **macOS): bundle ffmpeg/ffprobe via platform config and resolve lookup in Contents/MacOS to prevent normalization failures in packaged app; chore(stt/soniox:** update async model to stt-async-v3 ([17108f6](https://github.com/moinulmoin/voicetypr/commit/17108f6dda9755ce52a8ac24ca28ddb3e9923e87))
* persist Parakeet model selection across restarts ([fffba52](https://github.com/moinulmoin/voicetypr/commit/fffba520d2ada9a9b489cc7c12d7cbdf570477f8))
* resolve Parakeet sidecar MLX module loading and build issues ([33c3913](https://github.com/moinulmoin/voicetypr/commit/33c39134e829363bb066e71a2dd006ff94928592))
* **tauri:** scope Parakeet sidecar to macOS builds and improve UX ([2f54a53](https://github.com/moinulmoin/voicetypr/commit/2f54a5307d5c0c93662d51fa00f06f448702246a))
* **tray:** gate selection until onboarding completes; add tests for tray label and selection logic; ui: simplify Upload section copy ([0ac219a](https://github.com/moinulmoin/voicetypr/commit/0ac219a0fcbe8e6f46c940a53dcc1cde2f103be7))


### Features

* :sparkles: update windows ffmpeg ([734fddb](https://github.com/moinulmoin/voicetypr/commit/734fddb335b627e7e66426c0ac50d7b4b705c2c4))
* add Parakeet MLX sidecar integration ([ead96e3](https://github.com/moinulmoin/voicetypr/commit/ead96e3f804bcba277f32767269d54aa42a9804e))
* **parakeet:** add version-aware v2/v3 download/load/delete with FluidAudio 0.6.1; fix false-success download for v2 by verifying loaded model and cache; autoload selected Parakeet model on startup ([bec2e3e](https://github.com/moinulmoin/voicetypr/commit/bec2e3e62bdc32c7ccad1af4bacc7280e4f53d4c))
* replace Python Parakeet with Swift/FluidAudio implementation ([10909df](https://github.com/moinulmoin/voicetypr/commit/10909df13b05074948333233307536c88ec3cad3))
* **tray:** unify model selection and keep tray/dashboard in sync ([748675e](https://github.com/moinulmoin/voicetypr/commit/748675e0a2df69b7521fdf3810f0da266c3fe04f))

# [1.9.0](https://github.com/moinulmoin/voicetypr/compare/v1.8.0...v1.9.0) (2025-10-12)


### Bug Fixes

* **ai:** restore Groq/Gemini enhancement path and keep OpenAI-compatible config; cache non-openai keys in backend ([b2664f8](https://github.com/moinulmoin/voicetypr/commit/b2664f8c355f5de165afbb1cc855f44fef7143de))


### Features

* **ai/openai): accept any OpenAI-compatible model; remove dead_code attr. refactor(ui,keyring): standardize Tauri args to snake_case for OpenAI test/save. chore(audio:** keep mutable bindings in normalizer for future ops. ([5167f0c](https://github.com/moinulmoin/voicetypr/commit/5167f0c3f2f738f4689a30907b3dce053532bba3))
* **ai:** add OpenAI-compatible provider and endpoint normalization; fix pill feedback timing; cleanups ([a28030b](https://github.com/moinulmoin/voicetypr/commit/a28030bcb4fb2ed7a3f5c48f7fd940cbd5edee13))
* **audio:** normalize all inputs to 16k mono s16 + 5s gate & reduce ([937bbb3](https://github.com/moinulmoin/voicetypr/commit/937bbb3620ce218967ac63b64e456d683fcf4295))
* **help:** include hashed Device ID in ‘Copy System Info’ via new get_device_id Tauri command; hide mic/accessibility permission lines on Windows to avoid confusion ([ac80b7b](https://github.com/moinulmoin/voicetypr/commit/ac80b7b686d4e8a0ed306a3f9c8ca408f9610d45))
* **onboarding:** add centered model legend (Speed, Accuracy, Size, Recommended) before model list to match dashboard ([3257614](https://github.com/moinulmoin/voicetypr/commit/3257614f0a5beab163f9d39e8380b28dbde8d985))
* **tray:** add Recent Transcriptions copy, Recording Mode submenu, and Check for Updates; sync Dashboard↔Tray mode changes; refresh tray on history changes ([7accfdb](https://github.com/moinulmoin/voicetypr/commit/7accfdbc2f205abca95677654239f5031d0d21b6))

# [1.8.0](https://github.com/moinulmoin/voicetypr/compare/v1.7.0...v1.8.0) (2025-09-20)


### Bug Fixes

* Add support for multi-channel audio recording (4+ channels) ([54e2a0e](https://github.com/moinulmoin/voicetypr/commit/54e2a0e61b53e61fd0699ab77573a7494928e187))
* Enable macOS Cmd+Ctrl hotkey combinations ([e427cae](https://github.com/moinulmoin/voicetypr/commit/e427caed5432a4d31a12866d29dc665d13b83f3a))
* Extend offline grace period to 90 days for Issue [#15](https://github.com/moinulmoin/voicetypr/issues/15) ([7f7bef8](https://github.com/moinulmoin/voicetypr/commit/7f7bef83a8b0ff35108aa0a7f446c553398c24bc))
* Implement secure licensing with grace periods ([b123c36](https://github.com/moinulmoin/voicetypr/commit/b123c36e778b4010a20f29338a915a52edc68796))
* Improve hotkey system and shorten feedback messages ([bc5ee35](https://github.com/moinulmoin/voicetypr/commit/bc5ee356fe3951d197f0c84e19afe2214f3834e9))
* Improve language selection UX with alphabetical sorting and name-based search ([5f2d631](https://github.com/moinulmoin/voicetypr/commit/5f2d631d0a5562fb82c31db0c0a9a4c9a3137d35))
* Reduce drag-and-drop area height for better UI balance ([5a3dd8c](https://github.com/moinulmoin/voicetypr/commit/5a3dd8c320c4bfb7d46b7be5b1a486e340ca6d0c))
* Remove redundant audio validation to solve Issue [#16](https://github.com/moinulmoin/voicetypr/issues/16) "Audio too quiet" ([b977f7b](https://github.com/moinulmoin/voicetypr/commit/b977f7bde0182cad6d262ec4fb53c16083ca22a6))
* resolve UnwindSafe trait issues in panic prevention tests ([f431ee1](https://github.com/moinulmoin/voicetypr/commit/f431ee1a1024cf349daf8aa552021e0da7104e4a))
* Restore download progress updates for model downloads ([e3c13b8](https://github.com/moinulmoin/voicetypr/commit/e3c13b8e54ac34453bdc43d267535f449997f923))


### Features

* Add audio file upload and transcription support ([beed3d0](https://github.com/moinulmoin/voicetypr/commit/beed3d0d8435ba2c1cb8c3bf50ddde7459ec8009))
* Add audio file upload for transcription with multi-format support ([38f0c58](https://github.com/moinulmoin/voicetypr/commit/38f0c583b20ce090a5ce5ff40ba6579171d419a0))
* add export to JSON functionality with backend implementation ([7a18ce1](https://github.com/moinulmoin/voicetypr/commit/7a18ce140f2f42d9fa59498ed21bcc34d1a196bf))
* add Help section and share stats functionality ([e67dc68](https://github.com/moinulmoin/voicetypr/commit/e67dc683e3d8dfc67bde3ad5dc8a7af4f662b137))
* add microphone selection with bidirectional sync ([1791519](https://github.com/moinulmoin/voicetypr/commit/179151968315c9bb5bd322026f43a0a4fd0e11df))
* Add push-to-talk recording mode ([#13](https://github.com/moinulmoin/voicetypr/issues/13)) ([a6ef9ae](https://github.com/moinulmoin/voicetypr/commit/a6ef9aeaedd27ebbe02a9619a14da99e1170c967))
* Comprehensive hotkey system improvements with physical key support ([6938472](https://github.com/moinulmoin/voicetypr/commit/693847238c7dc8cfc4c471653a904791cf746e62))
* enhance history page with search and improved UI ([8bd2135](https://github.com/moinulmoin/voicetypr/commit/8bd2135dc727847254557ca1d1c499b8168fffde))
* Enhance HotkeyInput with editing state notification and update onboarding hotkey initialization ([43dca35](https://github.com/moinulmoin/voicetypr/commit/43dca35176afcd99592b4323a0e267c0b8971964))
* Enhanced license activation with OS metadata for better device identification ([c7b52da](https://github.com/moinulmoin/voicetypr/commit/c7b52da74e73582f45924474996dcb0b155ffba9))
* Improve history card display ([d1a0e67](https://github.com/moinulmoin/voicetypr/commit/d1a0e67983b88a404a9dacf16bb6b46343cb039b))
* improve UI/UX for support links and Quick Tips section ([c3f818f](https://github.com/moinulmoin/voicetypr/commit/c3f818f8b910f51e38075b8bf64d144c7be8a804))
* optimize transcription updates with append-only system ([5852078](https://github.com/moinulmoin/voicetypr/commit/5852078621ef67687a6b5b9f4c993760f34e5c4a))
* redesign Advanced section UI/UX ([ae5f1ca](https://github.com/moinulmoin/voicetypr/commit/ae5f1ca8a141fbcb81a047db42adfcdf3765351c))
* redesign Formatting page (formerly Enhancements) with improved UI/UX ([e006317](https://github.com/moinulmoin/voicetypr/commit/e006317a25a6f055a1949e82cbbea4ff0f1c857d))
* redesign Models page with improved UI/UX ([53f8837](https://github.com/moinulmoin/voicetypr/commit/53f88379a82ec790b1af0968f0c620d2bc601317))
* redesign overview dashboard with streak tracking and interactive stats ([415d730](https://github.com/moinulmoin/voicetypr/commit/415d730ca388219cef9bb5746aebd8da2054131f))
* redesign Settings page with modern UI ([2d77331](https://github.com/moinulmoin/voicetypr/commit/2d773311e9289e9a11c17f14836006c3fbbac40e))
* split Account section into License and About sections ([4a06489](https://github.com/moinulmoin/voicetypr/commit/4a06489a1873761f03515f7ee37d9aa3fcfd3b6e))


### Performance Improvements

* implement caching layer for settings and license to reduce I/O overhead ([391d752](https://github.com/moinulmoin/voicetypr/commit/391d752185f67eeb2f976011a7f6e3ffaee4f1a6))
* optimize transcription flow by reducing delays and improving UX ([22e303a](https://github.com/moinulmoin/voicetypr/commit/22e303aca2e98b3053f6b295c02de1bb5c50cccf))

# [1.7.0](https://github.com/moinulmoin/voicetypr/compare/v1.6.2...v1.7.0) (2025-08-20)


### Bug Fixes

* handle misheard filler words in base enhancement prompt ([b2db125](https://github.com/moinulmoin/voicetypr/commit/b2db125527508fc40855fe360420eb5b2bc06873))
* hotkey display platform-specific symbols ([04dde4b](https://github.com/moinulmoin/voicetypr/commit/04dde4bea3b3bbcb0e5097f3cac9064aae6693d8))
* make enhancement prompt comprehensive for all error types ([73a1612](https://github.com/moinulmoin/voicetypr/commit/73a16128d8ecc3df9aff6a1ac7a7eb134f8b4dfb))
* resolve menu bar updates and AI enhancement issues ([6f3b15f](https://github.com/moinulmoin/voicetypr/commit/6f3b15f905bc4a90d3a750846f755aaa497bfbdd))
* restore license expiration dashboard navigation on hotkey press ([0cdbfef](https://github.com/moinulmoin/voicetypr/commit/0cdbfef6801f2a5c3483a22c6941877a432b921d))
* update enhancement mode descriptions for accuracy ([29d2679](https://github.com/moinulmoin/voicetypr/commit/29d2679abe2406d978d21d43889ff9a68d44cd3e))


### Features

* improve AI enhancement settings discoverability ([84dac26](https://github.com/moinulmoin/voicetypr/commit/84dac2654f0d2f6fa444290d197c04c90dbc0c30))

## [1.6.2](https://github.com/moinulmoin/voicetypr/compare/v1.6.0...v1.6.2) (2025-08-14)


### Bug Fixes

* enable runtime hotkey updates without app restart ([f2701f8](https://github.com/moinulmoin/voicetypr/commit/f2701f85f84a2d1d45b81cc0d3fe7b2bc7fe92eb))


### Features

* :sparkles: add audio validation and enhanced error logging system ([d9ecc06](https://github.com/moinulmoin/voicetypr/commit/d9ecc06d870e807f50a4c9fa0f1925ae9e299e24))
* enhance logging system with comprehensive diagnostics and monitoring ([64fb1c2](https://github.com/moinulmoin/voicetypr/commit/64fb1c21a31a4ebdf6f39595542966b23b1e0014))
* implement comprehensive production-ready logging system ([5c8bee1](https://github.com/moinulmoin/voicetypr/commit/5c8bee1ebbb75c69643b76673fd9bfea41ed9775))
* implement smart Windows installer with automatic GPU detection and fallback ([f314952](https://github.com/moinulmoin/voicetypr/commit/f314952df72fc7948333021e2d48799f82319d44))


### BREAKING CHANGES

* Removed apply_pending_shortcut command (was internal only)

## [1.6.1](https://github.com/moinulmoin/voicetypr/compare/v1.6.0...v1.6.1) (2025-08-14)


### Features

* :sparkles: add audio validation and enhanced error logging system ([d9ecc06](https://github.com/moinulmoin/voicetypr/commit/d9ecc06d870e807f50a4c9fa0f1925ae9e299e24))
* enhance logging system with comprehensive diagnostics and monitoring ([64fb1c2](https://github.com/moinulmoin/voicetypr/commit/64fb1c21a31a4ebdf6f39595542966b23b1e0014))
* implement comprehensive production-ready logging system ([5c8bee1](https://github.com/moinulmoin/voicetypr/commit/5c8bee1ebbb75c69643b76673fd9bfea41ed9775))
* implement smart Windows installer with automatic GPU detection and fallback ([f314952](https://github.com/moinulmoin/voicetypr/commit/f314952df72fc7948333021e2d48799f82319d44))

# [1.6.0](https://github.com/moinulmoin/voicetypr/compare/v1.5.0...v1.6.0) (2025-08-08)


### Features

* :sparkles: add Windows dual build system with CPU and Vulkan GPU variants ([302d2f0](https://github.com/moinulmoin/voicetypr/commit/302d2f0e39aeee2af10d1f67738bb34b87b5e8fc))
* :sparkles: move vulkan feature flag to windows-specific dependency for whisper-rs ([f3dae0b](https://github.com/moinulmoin/voicetypr/commit/f3dae0b83dfa5243e29131cb7f429113712291be))
* :sparkles: set small English model as recommended for improved language support ([f95087f](https://github.com/moinulmoin/voicetypr/commit/f95087fc5e49d7ed5c35b894911f06ef4f6886ab))
* add GPU-aware smart installer with auto-detection and update system ([3319d1e](https://github.com/moinulmoin/voicetypr/commit/3319d1ed4a512ca8487a2eeb812d92ccf7c7bcee))
* add smart GPU detection and installer for Windows with Vulkan support ([81ba6ca](https://github.com/moinulmoin/voicetypr/commit/81ba6cac1a6b7ebcdd59efe07f11265ec3060400))
* add Windows dual build release script with CPU and GPU variants ([ad05880](https://github.com/moinulmoin/voicetypr/commit/ad05880d08a58ef089c9742aee651fa277449e37))
* add Windows GPU support with Vulkan installer and release scripts ([6d85863](https://github.com/moinulmoin/voicetypr/commit/6d858635bd6e555c93fa92d410e84efb71fab8a5))
* add Windows release script with MSI installer and update artifacts ([ed55678](https://github.com/moinulmoin/voicetypr/commit/ed5567866034824cdf960da93cd37a5bc9e58b30))
* implement robust Windows GPU dual-build support ([0aa71b6](https://github.com/moinulmoin/voicetypr/commit/0aa71b6863dd92a74ee9943fce7c01132bd135c6))


### BREAKING CHANGES

* Windows now has separate CPU and GPU builds

Changes:
- Add Vulkan GPU support for Windows (works with NVIDIA, AMD, Intel)
- Implement macOS Metal fallback to CPU on initialization failure
- Add VULKAN_SDK pre-flight check in Windows release script
- Enhance README with GPU requirements and troubleshooting
- Improve runtime logging to show active backend

Critical fixes after review:
- Fix macOS Metal early return to ensure consistent logging
- Remove impossible conditional compilation branches
- Track actual GPU usage for accurate backend reporting

The CPU build ensures universal Windows compatibility while the GPU
build with Vulkan provides ~2-3x faster transcription for users with
compatible GPUs.

# [1.5.0](https://github.com/moinulmoin/voicetypr/compare/v1.4.0...v1.5.0) (2025-08-06)


### Bug Fixes

* :bug: simplify Windows window flag application by removing redundant error handling ([2c04f14](https://github.com/moinulmoin/voicetypr/commit/2c04f14478ab142f65f52d706463515e5fe4c1fb))


### Features

* :sparkles: add timeout to recorder thread, improve window handling, and enhance security validations ([758231d](https://github.com/moinulmoin/voicetypr/commit/758231dc3cb035d7a0517231075211a56a851d10))
* :sparkles: refactor: optimize platform detection with static boolean values ([204e4f3](https://github.com/moinulmoin/voicetypr/commit/204e4f35edffd8291f81f68e4d0ffa46cc855d73))
* add advanced settings section with permissions, reset options and diagnostics ([56bcc9b](https://github.com/moinulmoin/voicetypr/commit/56bcc9b03c5c7df1d4b618597f8438b65536c2bf))
* add license management and log cleanup functionality ([5834ce6](https://github.com/moinulmoin/voicetypr/commit/5834ce69e0dca69839651815316ac6f21f6e774f))
* add platform-specific settings and advanced configuration UI ([18d5e46](https://github.com/moinulmoin/voicetypr/commit/18d5e46b123c70454d36a7274c4384f578f20221))
* add Windows release scripts and configuration files for cross-platform builds ([e1516de](https://github.com/moinulmoin/voicetypr/commit/e1516ded527c60ed7933a5c2bf8677e0fe212e94))
* refactor the app with cross-platform permissions and settings UI ([930f9a2](https://github.com/moinulmoin/voicetypr/commit/930f9a23ce6cd8efcf59a762246b7dd0a82ac09c))

# [1.4.0](https://github.com/moinulmoin/voicetypr/compare/v1.3.0...v1.4.0) (2025-08-01)


### Bug Fixes

* :bug: test and remove sentry ([57be5a3](https://github.com/moinulmoin/voicetypr/commit/57be5a38df7d6ba86a371158312c5585ee39d3da))


### Features

* :sparkles: add AI enhancement integration with API key management and model selection ([994f21b](https://github.com/moinulmoin/voicetypr/commit/994f21b747167cfad5a1fbd080ba423efecdf027))
* :sparkles: add Gemini provider integration and related tests ([1263503](https://github.com/moinulmoin/voicetypr/commit/1263503745966d0ee7ebca17a31c949274dffbb0))
* :sparkles: added sentry well ([2f1ceb5](https://github.com/moinulmoin/voicetypr/commit/2f1ceb5d3d7fe45aa79e1878af4764b1f8b959ba))
* :sparkles: fixes ([682c267](https://github.com/moinulmoin/voicetypr/commit/682c26775badf7ac6fa691c6ea420a0b382a1dc2))
* :sparkles: implement AI enhancement options and settings management ([843f830](https://github.com/moinulmoin/voicetypr/commit/843f8304da1146ed2028d51c17f05814f9140ac6))
* :sparkles: implement better key storing ([e6a5344](https://github.com/moinulmoin/voicetypr/commit/e6a5344bde1662cab0540b20d5eaf8013e4e1603))
* :sparkles: improve checker state ([876fda7](https://github.com/moinulmoin/voicetypr/commit/876fda73688a8b632f944b202903e627e895d01d))
* :sparkles: improve prompts and ui ([5cd995d](https://github.com/moinulmoin/voicetypr/commit/5cd995d0c2c0a5361b71e85f6d71c6d7f21b2ced))
* :sparkles: investigating audio issue ([fd70cc9](https://github.com/moinulmoin/voicetypr/commit/fd70cc9d0f9ba8e905193bbbdde898cc28997bb8))
* :sparkles: little fix and improvements ([d481603](https://github.com/moinulmoin/voicetypr/commit/d48160335a448295db699abd39118068098bd507))
* :sparkles: refactor audio processing and permission handling ([bdc7f35](https://github.com/moinulmoin/voicetypr/commit/bdc7f35a4c403400ae67862453528dabf84acddb))
* :sparkles: remove debug logging and add proper resampling ([1b415a2](https://github.com/moinulmoin/voicetypr/commit/1b415a2ed04a825170549d6c630d595691facc26))
* :sparkles: remove deprecated Sentry test commands and related documentation ([5aab0a7](https://github.com/moinulmoin/voicetypr/commit/5aab0a75443f45ff801a0bd5bb238ecc39e44229))
* :sparkles: update ai enchance and general settings ([e844915](https://github.com/moinulmoin/voicetypr/commit/e844915a3e2abec9b9983bf25fd58a06d53b66de))

# [1.3.0](https://github.com/moinulmoin/voicetypr/compare/v1.2.2...v1.3.0) (2025-07-22)


### Features

* :sparkles: add automatic update checks and tray menu functionality, suppress blank and non speech voice ([c812355](https://github.com/moinulmoin/voicetypr/commit/c81235554cc99d282965a89ebf0f5e4510821024))
* :sparkles: add huge perf improvement and improved audio level meter and silence detection ([aa2601a](https://github.com/moinulmoin/voicetypr/commit/aa2601a279d27646812609f43a7f0811c767c514))
* :sparkles: add new models, keep only 2 permission, add reset app data ([3d3dd56](https://github.com/moinulmoin/voicetypr/commit/3d3dd5613588f9512202316ad6eee06b8ecdc7ed))
* :sparkles: add sentry, fix esc not stopping, add pill tooltip feedback ([82c88b1](https://github.com/moinulmoin/voicetypr/commit/82c88b151a377cb0ce0b520cc6363aa1b9db8781))
* :sparkles: add translation, make download and read seperate action ([3e9e61a](https://github.com/moinulmoin/voicetypr/commit/3e9e61a294f6c0363da92ba64682f2226471f0d3))
* :sparkles: enable GPU acceleration and multi-threading for improved transcription performance ([1f3c1b3](https://github.com/moinulmoin/voicetypr/commit/1f3c1b39e570790b4f3424bed4be4e2d77db63c8))

## [1.2.2](https://github.com/moinulmoin/voicetypr/compare/v1.2.1...v1.2.2) (2025-07-20)


### Features

* :sparkles: fix download updates ([6d72f2d](https://github.com/moinulmoin/voicetypr/commit/6d72f2d33c650d282e5b701fb49f01f1cacd4b79))

## [1.2.1](https://github.com/moinulmoin/voicetypr/compare/v1.2.0...v1.2.1) (2025-07-20)

# [1.2.0](https://github.com/moinulmoin/voicetypr/compare/v1.1.1...v1.2.0) (2025-07-20)


### Features

* :sparkles: enhance permissions management in OnboardingDesktop, add automation permission checks, and update sidebar to include advanced section ([ca56910](https://github.com/moinulmoin/voicetypr/commit/ca56910ceec1b686a3fa76ee725017de22a9b52c))

## [1.1.1](https://github.com/moinulmoin/voicetypr/compare/v1.1.0...v1.1.1) (2025-07-19)


### Features

* :sparkles: add tauri-plugin-macos-permissions-api dependency, enhance model management in App component, and improve accessibility permission handling ([fd96e05](https://github.com/moinulmoin/voicetypr/commit/fd96e05853fb35794eac1565fc5670855ba0705c))
* :sparkles: fix external link handling in AboutSection,  add updater capabilities in default.json ([d627ff5](https://github.com/moinulmoin/voicetypr/commit/d627ff5194257d9f1ad91320c51c9f7849fceaac))
* :sparkles: refactor model management integration in App and OnboardingDesktop components, enhance loading state handling, and improve model status response structure in Tauri commands ([82fd144](https://github.com/moinulmoin/voicetypr/commit/82fd144ee717e79315a7957367878ba2a0498055))
* :sparkles: remove modelManagement prop from OnboardingDesktop, update useModelManagement hook for onboarding context, and adjust event handling for model downloads ([d0e079c](https://github.com/moinulmoin/voicetypr/commit/d0e079ca154b2f6ce41152856aeb7b4828c8e3bd))
* :sparkles: show loading while verifying downloads ([1197069](https://github.com/moinulmoin/voicetypr/commit/11970698c976c6728025809633e9ebf775ba8675))
* :sparkles: streamline model download handling in Tauri commands, enhance logging for download progress, and simplify event emissions in useModelManagement hook ([462ad1d](https://github.com/moinulmoin/voicetypr/commit/462ad1d53658cbe789a1f396ce3aaae2d51f8310))

# [1.1.0](https://github.com/moinulmoin/voicetypr/compare/v1.0.0...v1.1.0) (2025-07-18)


### Features

* :sparkles: fix license cache ([435f8a5](https://github.com/moinulmoin/voicetypr/commit/435f8a557962fb845302d20dd5aac57acbe9a26f))
* :sparkles: remove CI, release, and test workflows from GitHub Actions for project restructuring ([c763ae0](https://github.com/moinulmoin/voicetypr/commit/c763ae083c63f2982edeb066d43b0ddc0e87881e))
* :sparkles: reorganize imports in App component, update active section state, enhance AboutSection with app version retrieval, and clear license cache on startup for fresh checks ([762e158](https://github.com/moinulmoin/voicetypr/commit/762e1583b2aa6eac6b4ceb5736e406671a0e5318))
* :sparkles: replace Twitter icon with XformerlyTwitter in AboutSection and reorganize imports for better structure ([97c05c6](https://github.com/moinulmoin/voicetypr/commit/97c05c6f4515ef29a1b7bf4cbe05e7991bb3116d))
* :sparkles: update .gitignore to include release files, clean up AboutSection and LicenseContext for improved readability, and fix URL in license commands ([1f9d501](https://github.com/moinulmoin/voicetypr/commit/1f9d501b5e56a7f5a8d66251d17e420ca2b246f4))
* :sparkles: update script ([ff5ebeb](https://github.com/moinulmoin/voicetypr/commit/ff5ebeb4816325efaaa2cc74e4a08801bade61c9))

# 1.0.0 (2025-07-18)


### Bug Fixes

* :bug: pill showing ([b67c1d1](https://github.com/moinulmoin/voicetypr/commit/b67c1d1cd926f67a132637f6bf870815cae19b10))
* :bug: silence audio checking after recording ([4de6aa1](https://github.com/moinulmoin/voicetypr/commit/4de6aa14bc346b487930d8bfdaa87eda40dfc603))
* :bug: sync with recent history ([0ccecec](https://github.com/moinulmoin/voicetypr/commit/0ccececb2bb23a1a45ea894e216540c0f1b34e4a))


### Features

* :sparkles: add compact recording status setting, enhance feedback messages in RecordingPill, and implement cancellation handling in transcription process ([f3b42ff](https://github.com/moinulmoin/voicetypr/commit/f3b42ff3571efceb2d6e1ff9d27c5d899cfc7f72))
* :sparkles: add dialog plugin support and enhance model management with improved UI interactions and state handling ([9b7d32e](https://github.com/moinulmoin/voicetypr/commit/9b7d32eea4619fd6bee128470c277888234ea477))
* :sparkles: add formatting script to package.json and refine model management with updated model descriptions and UI enhancements ([2c2a997](https://github.com/moinulmoin/voicetypr/commit/2c2a9978f776fe71cb781b542242de511e6fa583))
* :sparkles: add iOS spin animation, refactor ModelCard component, and update RecordingPill to use IOSSpinner ([f30422e](https://github.com/moinulmoin/voicetypr/commit/f30422e37244282a55c4932b5f9ef54bf30d6535))
* :sparkles: add new dependencies and update configuration for Voicetypr ([34d30c9](https://github.com/moinulmoin/voicetypr/commit/34d30c99a91731d76fa2021bc383cf26aec7bbc4))
* :sparkles: adjust compact mode styles in RecordingPill for better visual consistency and update audio wave animation scaling ([fb7e4c7](https://github.com/moinulmoin/voicetypr/commit/fb7e4c71f42a8ec39011f1395ade4126da2107bb))
* :sparkles: adjust RecordingPill layout for better feedback message display and update window size calculations for improved visibility ([22409d4](https://github.com/moinulmoin/voicetypr/commit/22409d41cd5a0396b072817a9cb04ce01c97b5db))
* :sparkles: clean up release-universal.sh by commenting out test command and removing unnecessary blank lines for improved readability ([0d39040](https://github.com/moinulmoin/voicetypr/commit/0d390404b46c955110ea4870dc09e31413a8c211))
* :sparkles: dd autostart functionality with settings toggle, update dependencies, and improve UI components for better user experience ([da24d37](https://github.com/moinulmoin/voicetypr/commit/da24d373601325891da7c0026decaae17ec2d6cc))
* :sparkles: enhance audio recording features with real-time audio level visualization, implement update checks in AboutSection, and integrate tauri-plugin-updater for seamless application updates ([6494a96](https://github.com/moinulmoin/voicetypr/commit/6494a96a33c01c181c4939d3927112956a5b248e))
* :sparkles: enhance audio visualization in AudioWaveAnimation with improved animation logic and state management, and refactor RecordingPill for better feedback handling and cleanup on unmount ([971c9ee](https://github.com/moinulmoin/voicetypr/commit/971c9ee6bd0ae2357b6f28a55a22986624615ccd))
* :sparkles: enhance transcription management with cleanup settings and history retrieval, improve UI with pill widget toggle, and update tests for new settings ([943d398](https://github.com/moinulmoin/voicetypr/commit/943d398219063081bdd78fe2a019d4dc44053149))
* :sparkles: enhance transcription management with cleanup settings and history retrieval, improve UI with pill widget toggle, and update tests for new settings ([9ffa3f2](https://github.com/moinulmoin/voicetypr/commit/9ffa3f2a463a193a3952fb7e7069e7f0ce8566b9))
* :sparkles: implement download cancellation feature, enhance download progress management, and improve hotkey input handling ([3ea8ab8](https://github.com/moinulmoin/voicetypr/commit/3ea8ab83a94be0b59b5e5b1ac7be414dd841ce7d))
* :sparkles: implement model sorting and management enhancements, including balanced performance scoring, model deletion, and improved hotkey input handling ([bfcbb86](https://github.com/moinulmoin/voicetypr/commit/bfcbb8630f1174c70bee64f3ce2670f15f05412f))
* :sparkles: implement transcription deletion feature, enhance AboutSection with update checks and external links, and improve RecentRecordings component with history refresh ([be5b9e6](https://github.com/moinulmoin/voicetypr/commit/be5b9e67ee2cf64957184aacb0bf11e966d8cb91))
* :sparkles: improve UI components and enhance error handling in recording and model management ([6d26e9a](https://github.com/moinulmoin/voicetypr/commit/6d26e9a94e93893caec6a610cf2d1a73dfbebf9c))
* :sparkles: improvements ([a042c86](https://github.com/moinulmoin/voicetypr/commit/a042c863711642390cb1a0d6e153431d03b9e6e7))
* :sparkles: init ([3089d43](https://github.com/moinulmoin/voicetypr/commit/3089d43a5926f0eb4b11b33f1de1d8c029b7ed1c))
* :sparkles: integrate react-error-boundary for enhanced error handling and improve performance with useCallback and useMemo optimizations in App component ([cc92f93](https://github.com/moinulmoin/voicetypr/commit/cc92f931fe5be2d70a80340d16456c0ff626b614))
* :sparkles: make the pill working right ([010d96f](https://github.com/moinulmoin/voicetypr/commit/010d96f839c5d4ebab04d290661ef41184d43fe6))
* :sparkles: nhance RecordingPill with feedback messages for transcription events, improve ESC key handling for recording cancellation, and update GeneralSettings with tips for users ([89a9e07](https://github.com/moinulmoin/voicetypr/commit/89a9e07b8601bad91017782d928e2191f4db42a2))
* :sparkles: nhance Voicetypr with new features, including global shortcut support, model management, and improved UI for recording and settings ([ca47873](https://github.com/moinulmoin/voicetypr/commit/ca47873a46856ec5e72c0fffee8286aa79f2a2c6))
* :sparkles: refactor GitHub Actions workflow for improved macOS target handling and streamline dependency installation ([c89eaff](https://github.com/moinulmoin/voicetypr/commit/c89eaff4bb4a0757551101495060fa73921ca31f))
* :sparkles: remove recording form the window ([5d51fae](https://github.com/moinulmoin/voicetypr/commit/5d51faef30a32ba8a676bc819538ed56ae0f1c3d))
* :sparkles: reorganize imports and enhance transcription handling in App and useRecording hook ([5b217a1](https://github.com/moinulmoin/voicetypr/commit/5b217a172eb4ac6007f0a396ae8769236c91ebca))
* :sparkles: update app title to Voicetypr, add framer-motion and geist dependencies, and remove unused SVG files to streamline project assets ([04c41ee](https://github.com/moinulmoin/voicetypr/commit/04c41ee61c9b1479593350b25301a92f8bdfec6b))
* :sparkles: update dependencies in package.json, add styles to pill.html for full height layout, and remove unused components to streamline the codebase ([747d8bf](https://github.com/moinulmoin/voicetypr/commit/747d8bfb2f94784b77a3b854c41cb708c1acba84))
* :sparkles: update IOSSpinner component for improved animation timing and adjust RecordingPill layout for better alignment ([a4f269b](https://github.com/moinulmoin/voicetypr/commit/a4f269bdd8365c78c7c3b42093e9a049c44d21a1))
* :sparkles: update window size for better visibility, remove unused download cleanup logic, and streamline cancellation handling ([3ed8ac0](https://github.com/moinulmoin/voicetypr/commit/3ed8ac026db9b69fbbfb4d0dc711e96ff0ec06b6))
