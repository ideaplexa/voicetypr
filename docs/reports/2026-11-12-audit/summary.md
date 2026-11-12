# Voicetypr audit — 2026-11-12

Audited: `release/2.1.0-beta.8` (`5d908536`), 136k lines Rust, 66k lines TypeScript, 3.5k Swift.
Static reading only: nothing was built, run or changed. "Checked" means I opened the cited lines myself; the rest is as reported by the audit agents.

Detail files in this folder: `recording.md`, `engines.md`, `appcore.md`, `frontend.md`, `build.md`, `competitors.md`, `identity-map.md`, `indic-and-watch.md`.

## 1. Code audit

### What is in good shape
- No dead engines, no leftover ffmpeg or formatting sidecar in the shipped build.
- Dependencies are almost all used (exceptions: `tokio-test`, `mockall`, `lazy_static` for one regex, frontend `@tauri-apps/plugin-autostart`).
- Sidecar stdout protocol is centralized in `writeProtocolLine`; FluidAudio is pinned in `Package.resolved`.
- Licence check has a 90-day offline grace; no trivial bypass found.

### A. Privacy rule broken (AGENTS.md: no transcript, path, key or window title in logs) — fix first
| # | Where | What | Checked | Effort |
|---|---|---|---|---|
| A1 | `sidecar/parakeet-swift/Sources/main.swift:2858` → forwarded by `parakeet/sidecar.rs:170-185` | Logs the last 5 recognized tokens per audio chunk on the EOU streaming path (that path is reported dormant) | yes | S |
| A2 | `commands/debug.rs:55-61`, registered at `lib.rs:1273` | `test_transcription_event` logs its text argument; reachable from the webview in release; no frontend caller | yes | S |
| A3 | `ai/agent_cli/private_input.rs:177-183` | OpenCode 1.x gets the dictated text as a command-line argument, visible to other processes (commented as a known compromise) | yes | M |
| A4 | `commands/ai.rs:774-780`, `:1377-1382` | Logs the user's custom AI endpoint URL and model on failure (URL may hold credentials) | yes | S |
| A5 | `lib.rs:756-767`, `whisper/cache.rs:69-106`, `whisper/manager.rs:294`, `commands/utils.rs:74-95`, `commands/audio.rs:7554` | Absolute paths and recording filenames in logs (paths contain the OS username) | lib.rs, cache.rs | S–M |
| A6 | `main.swift:3041`, `:1925`, `:2051` | Sidecar logs audio and model paths and raw native errors | no | M |
| A7 | `commands/audio.rs:5817`, `audio/device_watcher.rs:103-147` | Logs LAN server names, host:port, microphone names | no | S |
| A8 | `remote/client.rs:635-644` → `commands/audio.rs:1827` | A remote server's error body is stored verbatim in failed history | no | S |

Root cause: nothing enforces the rule. Fix once at the sink (scrub in the log pipeline and in the sidecar stderr forwarder) plus a CI check, instead of ten separate patches.

### B. Security
| # | Where | What | Checked | Effort |
|---|---|---|---|---|
| B1 | `remote/client.rs:255-259`, `remote/lifecycle.rs:292-321` | Network sharing is plain HTTP: password, context and audio cross the LAN unencrypted | yes | L |
| B2 | `remote/http.rs:522-563` | `/status` with no password returns hostname and the device hash that also identifies the licence | yes | S |
| B3 | `remote/http.rs:203-222` | Each upload is buffered up to 512 MiB before the one-at-a-time permit; many slow uploads can exhaust host RAM | no | L |
| B4 | `capabilities/default.json:5-34` | The pill window has the same permissions as the main window (store, fs, shell open, updater, process exit) | yes | M |
| B5 | `remote/client.rs:457-579` | Status and model-control responses are read with no size cap | no | S |

### C. Bugs
| # | Where | What | Checked | Effort |
|---|---|---|---|---|
| C1 | `commands/audio.rs:7700-7742`, `:7922-7934` | A diarized Soniox/Deepgram file upload returns before the writing pipeline: no vocabulary, corrections, spoken punctuation or Polish | yes | M |
| C2 | same | When that diarized request returns no speaker words, its transcript is thrown away and a second paid request is sent | yes | S |
| C3 | `src/contexts/SettingsContext.tsx:84-96` + `commands/settings.rs:857-895` | Every settings change saves the whole settings object; two quick changes can finish out of order and the older one overwrites the newer | frontend side | M |
| C4 | `cloud_stt/soniox_ws.rs:65,104`, `deepgram_ws.rs:68,107` | Unbounded audio queue to the cloud socket; a stalled connection holds the whole recording in RAM | yes | M |
| C5 | `cloud_stt/soniox.rs:1299-1350` | Upload is retried on timeout; if the first upload succeeded, an orphan file stays in the user's Soniox storage quota | no | M |
| C6 | `commands/audio.rs:5510-5561` | Stop holds a std mutex and blocks an async worker for up to 8 s; cancel waits behind it | yes (call site) | M |
| C7 | `remote/http.rs:765-782` | The host holds its context lock for the whole transcription, so status and model-control freeze | no | M |
| C8 | `commands/settings.rs:998` | Any settings save deletes the legacy sharing password even if migration to secure storage failed | no | S |
| C9 | `src/hooks/useRecording.ts:39-115`, `useTranscriptionHistory.ts:125-153` | Listener leak on remount; initial state can overwrite a newer event; history events missed during first load | no | S–M |
| C10 | `recording/hotkeys.rs:76-85,203-215` | A failed older start can force a newer recording into Error | no | S |
| C11 | `commands/audio.rs:1097-1113,7443-7461` | Cancel during history save can leave cancelled text in history | no | M |
| C12 | `whisper/manager.rs:297-359` | Whisper model download has no stall timeout (the Windows Parakeet downloader has one) | no | S |

### D. Structure
- `commands/audio.rs`: 8,585 lines, at least 11 responsibilities. `stop_recording_with_mode_at` is one function of 1,842 lines (5425–7267), 72 awaits, nested 11 levels deep (checked).
- Other large files: `soniox.rs` 4,624 (1,828 production + 2,795 tests), `agent_cli.rs` 4,034, `remote/http.rs` 2,982 (834 production), `commands/ai.rs` 2,339, `recorder.rs` 2,285, `commands/remote.rs` 2,208, `settings.rs` 1,959, `lib.rs` 1,915.
- The record → transcribe → write → deliver flow exists in about 7 variants (desktop mic, GUI upload, history retry, kept retry, CLI local, CLI remote, GUI remote). The remote request is built separately in 3 places and CLI remote resolves the translate task differently.
- Engine abstraction is partial: `transcribe_with_app` rejects remote and host-default, so callers branch around it; preview capabilities and request capabilities are defined in two places.
- Soniox and Deepgram WebSocket clients are near copies; Soniox typed and diarized jobs are near copies.
- `writing/pipeline.rs`, `writing/settings.rs`, Soniox and Deepgram clients take a Tauri `AppHandle`. This is what blocks reuse on mobile.
- Legacy settings keys (`language`, `translate_to_english`, `show_pill_indicator`) are still dual-written and read in 5 places.
- Suggested split seams for `audio.rs` are in `audit-recording.md` (line 26).

### E. Process
- `release.yml` builds and publishes with no test, lint or CI requirement for the commit (checked: v2.0.12 went out this way).
- `scripts/release-windows.ps1` skips the installer checks CI runs and ignores upload failures.
- `plans/SMOKE.md` has 133 unchecked items; some describe behaviour that no longer exists.
- `plans/README.md` status is stale for plans 075, 076, 079–081, 086.
- 28 worktrees, 67 local branches already merged into beta.8. `voicetypr-ui` has uncommitted changes.
- Vitest collects tests from the git-ignored `agent/` folder (`vitest.config.ts:12`).
- Sidecar build prints a warning and succeeds when its own protocol check fails (`build.sh:108-112`).
- Spoken punctuation (2.0.12) is not on any 2.1 branch.

### F. Small cleanup
- Dead commands still registered: `debug_transcription_flow`, `test_transcription_event`, `get_current_audio_device`, `test_automation_permission`, `get_log_directory`, `open_logs_folder`, `refresh_remote_servers`, `disable_ai_enhancement`.
- Unused: `RecordingConfig.ai_provider/ai_model/show_recording_status`, `StreamTapMsg::Cancel`, `useCanRecord`, `Settings.sharing_password`, `scripts/clear-license-cache.js`, `scripts/verify-signature.sh`.
- Tests that test nothing real: `ai_failure_notice` (test-only function with wording assertions), `plan_desktop_writing_success` test, three layout-locking tests in `EnhancementsSection.test.tsx:1282-1352`.
- README still says Parakeet is Mac-only and network sharing is "authenticated".

## 2. What is already planned (from `plans/` and past sessions)
- Clean core (master plan 0.4a, plan 050): planned, not started. CLI parity (0.4b): partly done.
- Accounts, credits, sync: Phase 4 "Plus" ($9/month, $79–90/year). Core stays one-time ($39 now; $59–69 for 2 devices once mobile exists).
- Mobile: Phase 6, after Phase 3. Expo app + iOS keyboard extension + Android keyboard.
- Voice commands and Command Mode: 2.2. Actions through agents: 2.3+.
- MCP: deferred on 2026-06-16 until a host needs it.
- Licence: founder wants Polar as the single owner of activation.
- Own Polish model (plan 084, Liquid family): separate track with licence and eval gates.
- 2.1 stable target: before 2027-01-15; beta.9 is the last beta.
- Not in any plan: this audit, a competitive-growth plan, customer-feedback reconciliation, industry vocabularies.
- Open founder decisions: Discord invite URL, Plus device limit, Plus yearly price, Core price with mobile, recording donation, always-open mic.

## 3. Competitors (snapshot 2026-11-12, sources in `competitors.md`)
| Product | Stars | Platforms | Mobile | Price | Account |
|---|---|---|---|---|---|
| Voicetypr | 747 | Mac, Windows | no | $39 once | none |
| Handy | 38,544 | Mac, Windows, Linux | not established | free | not established |
| FluidVoice | 12,981 | Mac only (Windows, iOS waitlisted) | planned | free | not established |
| OpenWhispr | 11,420 | Mac, Windows, Linux, iOS | iOS keyboard | free local; $8/month cloud | only for cloud and sync |
| VoiceInk | 7,097 | Mac, iPhone | iPhone (separate licence) | $25–49 once | licence key |
| Hex | 6,462 | Mac, Windows, Linux | not established | not established | not established |
| Superwhisper | closed | Mac, Windows, iOS, Android | yes | $8.49/month or $249.99 lifetime | licence key |
| Wispr Flow | closed | Mac, Windows, iOS, Android | yes | $15/month | required |
| Aqua Voice | closed | Mac, Windows, iOS, Android | yes | $12/month | sign-up |

Voice for agents: Claude Code `/voice` is cloud dictation and needs a Claude.ai account; Cursor shipped two-way voice chat on Nov 6; ChatGPT desktop has voice for Codex. Third-party: Handy (hooks, CLI), Aqua (MCP), VivaDicta (CLI bridge), OpenWhispr (assistant hotkey), FluidVoice (Command Mode).

FluidAudio is at 0.18.3 (we pin 0.17.4) and now includes streaming ASR, VAD, diarization and several text-to-speech models.

User complaints found: the iOS keyboard app-switch "kills the magic"; dictation that only appears after you stop; no read-aloud of the result; no vendor-neutral microphone for terminal agents.

## 4. What we do about it

See [`plans/MASTER-PLAN.md`](../../../plans/MASTER-PLAN.md).
