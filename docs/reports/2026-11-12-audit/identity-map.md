# Voicetypr beta8: identity, gating, sharing, portability

Root: `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-beta8` (read-only). `[INFERENCE]` = not directly observed.

## 1. License and trial
- Backend is external: `api.voicetypr.com/api/v1` (`src-tauri/src/license/api_client.rs:14-27`). No server code in repo.
- Endpoints: `POST /trial/check {deviceHash}`; `/license/validate` and `/license/activate {licenseKey, deviceHash, appVersion, osType, osVersion}`; `/license/deactivate`. Validate retries 3x, 5 s apart (`api_client.rs` ~100-220).
- Response types (`license/types.rs:5-55`): `LicenseStatus {status, trial_days_left, license_type, license_key, expires_at, verification_state, verification_expires_at}`; `TrialData {is_expired, days_left, expires_at}`. Client states: `LicenseState {Licensed, Trial, Expired, None}`.
- Trial length and start are server-side; client receives only `days_left`/`expires_at`. No client reset path found [INFERENCE: reset needs server change].
- Caching and grace (`commands/license.rs:75-84`): licensed offline grace 90 d; trial offline grace 1 d; cache TTL 8 h; runtime cache 6 h (`CachedLicense` ~L37). Trial cached only with >1 day left (L609-611). Expired never cached (L544-549). Offline trial uses cached `TRIAL_EXPIRES_KEY` within grace; no cache → `None` (L758-766).
- Device ID (`license/device.rs:8-24`): SHA-256 hex of machine UUID (macOS IOPlatformUUID; Linux machine-id; Windows `windows_identity::device_hash`). Sent as `deviceHash`. No device-limit logic visible client-side [INFERENCE: enforced server-side].
- Gating (`license/recording_readiness.rs:61-64`): Licensed and Trial → Ready; Expired and None → Blocked. Enforced at `commands/audio.rs:4581-4586`, `commands/recording_runtime.rs:36-37` ("Recording license unavailable"), surfaced in `recording/island.rs:396-398`. Loading/Failed also block. Polish, cloud STT, and remote are not license-gated in what I read.
- Offline: Licensed works 90 d unvalidated; trial 1 d past cache; otherwise recording blocked.

## 2. Identifiers today
| Identifier | Storage | Sent to |
|---|---|---|
| License key | `secure_store` key `license` (`license/keychain.rs:4-7`); unreadable → error, not deleted (L12-22) | License API |
| Device hash | Derived each call (`device.rs:8-24`), not stored | License API `deviceHash`; remote `/status` `machine_id` (`remote/http.rs:555-563`) |
| Analytics install id (UUID v4) | `settings` key `analytics_install_id` (`product_analytics.rs:14`) | PostHog EU `distinct_id` (`product_analytics.rs:19`, 390-420) |
| Analytics enabled | `settings` `analytics_enabled`, default **true** (`product_analytics.rs:13,17`) | gate |
| Telemetry install id (second) | `settings` `telemetry_install_id` (`telemetry.rs:9-10`) | telemetry sink; default **true** (`telemetry.rs:8-10`) |
| PostHog token | compile-time `option_env!("POSTHOG_PROJECT_TOKEN")`, release only (`product_analytics.rs:22-25`) | `https://eu.i.posthog.com` (L19) |
| Sharing password (this Mac as server) | `secure_store` `remote_sharing_password` (`commands/remote.rs:27,273`) | clients send as `X-Voicetypr-Key` |
| Connection password (per saved host) | `secure_store` `remote_connection_password_<id>` (`remote.rs:29-30,59`); JSON copy cleared on load (`remote.rs:1809`) | that host |
| Saved connections + sharing config | store file `voicetypr-store.json`, key `remote_settings` (`remote.rs:1649-1656, 1685-1696`) | local only |
| Hostname | `hostname::get()` (`remote.rs:215-220`) | returned as `name` in `/status` |

Panic hook → PostHog capture (`telemetry.rs:515-525`; `lib.rs:660-688`). AGENTS.md forbids transcript, audio, clipboard, prompt, key, path, or window title in logs/PostHog; secrets only in `secure_store`.

## 3. Network sharing (`src-tauri/src/remote/`)
- warp HTTP server (`remote/http.rs`). Headers: `X-Voicetypr-Key` (`http.rs:22`), `X-Voicetypr-Context` (L23).
- Discovery: UDP 47842, protocol `voicetypr.remote-discovery.v1` (`discovery.rs:16-17`); request/response carry `machine_id` for self-detection (`discovery.rs:106-122,154-159`).
- Routes:
  - `GET /api/v1/status` (`http.rs:166-170`) → `status, version, model, name, machine_id, protocol_version, engine, capabilities` (L555-563).
  - `POST /api/v1/transcribe` (`http.rs:199-246`) with `Speech-Language`, `Transcription-Task`, `Context` headers; `audio/*` body, `content_length_limit`; one serialised permit.
  - `GET`/`PATCH /api/v1/control/models` (`http.rs:317-333`), auth required (`check_control_auth`, L440-458), gated by `allow_model_control`.
- Auth: `handle_status` enforces the key **only when a sharing password is set** (`http.rs:522-546`). With no password, `/status` is open and returns hostname and hashed device ID. Transcribe enforces via preflight and handler (`http.rs:257-288, 703-723`).
- Client: `remote/client.rs` (`test_connection` L433, `get_model_control` L491, `update_model_control` L536, `transcribe_audio` L583).
- Phone-as-client gaps: (a) UDP discovery needs platform multicast permission [INFERENCE]; (b) password entry and per-host storage in phone keychain; (c) server identity is an unsigned hash of hardware UUID; (d) client must upload phone-mic audio as `audio/*`; (e) plain HTTP on LAN, so the password travels in clear. None exists on a phone side.

## 4. CLI and agent surface
- `cli.rs`: `transcribe --server host:port` with `--password`, `--password-stdin`, or `VOICETYPR_REMOTE_PASSWORD` (`cli.rs:120-124, 917-933`). Uses the shared writing pipeline (AGENTS.md Gotchas). Reads cloud keys from keyring (`cli.rs:445-455`). `check`/`status` → `check_license_status` (`cli.rs:18`).
- No MCP server, hooks, or local HTTP API in `src-tauri/src`. Hits are only `--strict-mcp-config` (`ai/agent_cli.rs:175,2922,3021`) and disabled opencode hooks (`agent_cli.rs:299`). Only networked local API: `remote/`.
- Agents driven (`ai/agent_cli.rs`, `ai/opencode_acp.rs`): claude, pi, omp, codex, opencode. Launched with MCP off (`--strict-mcp-config`, `--no-chrome`, `--no-session-persistence`); opencode via ACP on stdio.

## 5. Portability of the Rust core
Mobile is partly anticipated: `lib.rs:515` `#[cfg_attr(mobile, tauri::mobile_entry_point)]`; `Cargo.toml:148-151` desktop plugins behind `cfg(not(any(android, ios)))`. No `gen/apple` or `gen/android` in `src-tauri/` [INFERENCE from listing]. `license/device.rs` has UUID arms only for macOS, Linux, Windows, so iOS/Android would not compile there [INFERENCE].

| Module | Verdict | Evidence |
|---|---|---|
| `license/api_client.rs`, `license/types.rs` | Portable | reqwest + serde |
| `license/keychain.rs`, `device.rs` | Desktop/Tauri | `AppHandle` (`keychain.rs:2,7`); cfg arms (`device.rs:8-24`) |
| `writing/vocabulary.rs`, `format.rs`, `app_category.rs` | Portable | no tauri hits |
| `writing/pipeline.rs`, `writing/settings.rs` | Tauri-coupled | `AppHandle` + `app.store("settings")` (`pipeline.rs:189-190`; `settings.rs:320-340`) |
| `cloud_stt/mod.rs` | Mixed | `selected_model(app)` (`mod.rs:216,371-373`); transcribe fns take `AppHandle` (L293-355) |
| `cloud_stt/deepgram.rs`, `soniox.rs` | Tauri-coupled | `compile_keyterms(app)` (`deepgram.rs:64-65`); `stored_key(app)` (`soniox.rs:436-437`) |
| `cloud_stt/openai.rs`, `groq.rs`, `cohere.rs` | Near-portable | `app` only in signature (`openai.rs:21`, `groq.rs:21`, `cohere.rs:22`) |
| `ai/keep_words.rs`, `polish_cli.rs`, `settings_selection.rs` | Tauri-coupled | `app.store("settings")` |
| `ai/native_settings.rs` | Tauri commands | `#[tauri::command]` (L67-96) |
| `remote/*` | Mostly portable (warp + tokio); start takes `AppHandle` | `lifecycle.rs:7-8`, `model_control.rs:5-6` |
| `audio/*`, `recording/*`, `commands/*` | Desktop-only | cpal, hotkeys, NSPanel pill, Tauri events |

Seam: a store trait replacing `AppHandle` in `writing/settings.rs`, `writing/pipeline.rs`, `cloud_stt/*`. That makes the transcript-text, cloud-client, and license-API core portable.

## 6. Sync candidates (device-only data)
| Data | Store and key | Format |
|---|---|---|
| Custom words (vocabulary) | `settings` `writing_settings` → `custom_words` (`writing/settings.rs:10`) | `{phrase, spoken_form?, language?, enabled}` |
| Replacements (corrections) | `writing_settings` → `replacements` | `{from, to, language?, enabled}` |
| Snippets | `writing_settings` → `snippets` | `{trigger, body, language?, enabled, preserve_literal}` |
| App formatting rules | `writing_settings` → `app_formatting_rules` | `{app_name, preset, enabled}` |
| Dictation history | `transcriptions` store, key RFC3339 timestamp (`commands/audio.rs:7416-7430`) | `{text, model, timestamp, recording_file?, writing?}` |
| AI and provider settings | `settings`: `ai_provider`, `ai_model`, `ai_enabled`, `ai_custom_base_url`, `ai_models_by_provider`, `enhancement_options` | scalars, maps |
| Cloud STT model choice | `settings` `cloud_stt_models_by_provider` (`cloud_stt/mod.rs:371-374`) | map |
| Hotkeys and modes | `settings`: `hotkey`, `recording_mode`, shortcut bindings (`commands/shortcuts.rs:830-945`) | JSON |
| Remote connections | `voicetypr-store.json` `remote_settings` | JSON; passwords in secure_store |
| License, API keys | `secure_store` | secrets: must not sync in clear |
| Analytics/telemetry IDs | `settings` | keep per-device |
Recording files (`recording_file`) are local audio and should not sync by default.

## 7. Plans touching accounts, mobile, backend, identity
- No plan on accounts, sign-in, or a mobile app. Grep for account/mobile/sync/backend hit only generic terms (032, 019).
- Identity and license: `066-windows-license-identity.md`, `061-license-preservation.md`, `065-windows-customer-recovery.md`.
- Analytics: `045-posthog-product-analytics.md`, `078-dictation-telemetry.md`, `032-product-analytics-formatting-corrections.md` (L162 "PostHog Cloud EU").
- Secrets: `archive/011-secrets-os-keychain.md`. History: `archive/010-history-store-paged-reads.md`.
- Agents: `044-agent-cli-hardening-model-selection.md`, `048-expanded-local-agent-clis.md`, `049-local-agent-model-discovery.md`.
- Personalisation: `030-ai-polish-personalization.md`. Roadmap: `MASTER-PLAN-2026-Q4.md`.

## Findings (for ranked report)
1. [High] [security-privacy] `remote/http.rs:522-546` — unauthenticated `/api/v1/status` leaks hostname and hashed device ID when no sharing password is set. Fix: minimal unauthenticated response or require password. S.
2. [Med] [security-privacy] `remote/http.rs:22` — plain HTTP on LAN; passwords in clear. Fix: TLS or pinned key. L.
3. [Med] [bad-practice] `writing/*`, `cloud_stt/*` — `AppHandle` coupling blocks mobile reuse. Fix: store trait seam. M.
4. [Med] [drift] `commands/remote.rs:1649` — remote settings live in `voicetypr-store.json`, not `settings`; AGENTS.md is silent [INFERENCE: docs unchecked]. S.
5. [Low] [bad-practice] `license/device.rs:8-24` — no iOS/Android arm. S when needed.
6. [Low] [privacy] `product_analytics.rs:13-17`, `telemetry.rs:8-10` — two default-on install IDs. S.

## Not reached
- Full `commands/license.rs` and frontend license gate in `src/`.
- Whether `save_remote_settings` (`remote.rs:1649`) strips connection passwords before writing JSON (only load-time clearing verified, L1809).
- `cli.rs` check/status body; `docs/ARCHITECTURE.md` drift check.
