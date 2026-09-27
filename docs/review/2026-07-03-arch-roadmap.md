# VoiceTypr Architecture Refactor Roadmap

Date: 2026-07-03
Branch: `integrate/perf-ux-2026-07`
Source: verified pass over the 27 raw architecture issues (`docs/review/arch-issues-raw.md`), re-grounded against current code **after** slice 045 (stream-leak / tap-join / stop-gate fixes) landed.

This doc is the decision surface for whether to push the arch work. Every claim below was re-checked against current `file:line` (line numbers here are current-tree, not the raw doc's pre-045 numbers).

---

## 1. Executive summary

- **6 live correctness bugs** worth shipping now: **#25** (silent too-short drop), **#24** (`"model"` substring misclassifies transient failures as non-retryable), **#16** (upload dispatch: no timeout/cancel + wrong duration + no cloud-translate guard), **#8** (cold-start hotkey overwrite), **#13** (stream holds `RwLock` write guard whole recording → concurrent command hangs), **#17** (remote inbound shadow engine: double RAM, Windows CPU-only, no custom vocab, silent Whisper fallback).
- **16 refactors / cleanups** (14 maintainability+perf refactors, 2 dead-code deletions). The high-leverage ones directly de-risk **plan 043 / task #11 Soniox WS streaming**: **#14** (transport/capability layer — the direct blocker), **#21** (layering inversion), **#16/#17/#19/#22** (kill the 4 parallel engine dispatches), plus broad de-risk from **#11/#10** (typed settings + migration), **#2+5** (model catalog), **#23** (lifecycle statics), **#20** (stop_recording monolith), **#27** (error-surface convention).
- **2 already-fixed by slice 045** — do NOT re-do: **#12** (Parakeet stream-session `stream_busy` leak) and **#15** (preview-finalize on stop path + unbounded tap-worker join).
- **1 downgraded / not-a-bug**: **#9** (whole-struct `save_settings` "resurrect") — the claimed correctness bug does **not** reproduce; kept only as low-priority maintainability debt.

Guiding rule (from the raw doc + CLAUDE.local.md): **no big-bang.** Every slice below ships independently, behind existing tests, tauri-first.

---

## 2. TIER 1 — Live correctness bugs to push NOW

Ranked by user impact. Each is a real, currently-reproducing wrong behavior, untouched by slice 045.

### T1-A · #25 — `recording-too-short` swallowed when pill disabled (effort: S)
- **Failure**: pill mode `"never"` + a sub-1s recording → **no toast, no error, no feedback at all**; app silently transitions to Idle. This is the "went unnoticed for months" bug.
- **Why**: gate emits `recording-too-short` only to the `"pill"` window (`commands/audio.rs:5308-5313`); the pill window is never created when `pill_indicator_mode == "never"` (`audio.rs:4626` `should_show_pill = show_pill_widget && pill_indicator_mode != "never"`); `emit_to_window`'s critical-event whitelist (`window_manager.rs:484/498/534`, `matches!(event, "recording-state-changed" | "transcription-complete")`) excludes `recording-too-short`, so with no pill window it hits the else branch and is dropped at `log::debug` — no `queue_pill_event`, no `app.emit` fallback.
- **Slice**: route this gate through `pill_toast()` (the post-transcription too-short path at `audio.rs:6049` already does — `pill_toast` emits app-wide `"toast"` + the always-registered toast window, so it survives pill-off). Optionally add a `(event, DeliveryPolicy)` const table in `window_manager.rs` replacing the thrice-duplicated `matches!`, and raise the drop log `debug → warn`.
- **Acceptance**: Rust test — emit `recording-too-short` with no pill window → queued and returned by `drain_queued_pill_events`; manual smoke: pill `"never"`, record <1s, user still gets feedback.

### T1-B · #24 — `"model"` substring misclassifies transient failures as non-retryable (effort: S for the narrow fix)
- **Failure**: any transient engine error whose text contains the common word `"model"` (e.g. `"failed to load model tensors"`) is classified `ModelUnavailable`, which is **not** in `default_retryable` → `run_with_policy` skips the retry and the user is told *"the selected transcription model is unavailable"* instead of *"try again"*.
- **Why**: confirmed `transcription/error.rs:118` `else if lower.contains("no speech recognition models") || lower.contains("model")` → `ModelUnavailable`; `default_retryable` (`error.rs:127-135`) excludes it. Reached on live Whisper (`executor.rs:201`) and Parakeet (`executor.rs:268`) arms.
- **Slice (narrow, ship now)**: drop the bare `|| lower.contains("model")` marker — keep only the exact `"no speech recognition models"` marker; anything else falls through to `EngineFailed` (retryable). Add a regression test.
- **Full fix (TIER 2, folds into #27)**: replace `TranscriptionFailure::Local(String)` with `Local { code, user_message, detail }` and match on `code`, deleting the `.contains()/.starts_with()` classifiers in `desktop_failure_from_transcription_error` / `classify_local_failure` (`audio.rs:1046-1056`, `1170-1182`) so editing copy strings can no longer change control flow.
- **Note**: cancel/timeout dispatch is *already* decoupled (hardcoded strings at `audio.rs:1174-1175`); the `.contains("permission"/"accessibility")` sites (`4460`, `5916`) classify raw OS errors never in the typed system — out of scope.

### T1-C · #16 — upload/CLI dispatch has drifted from the executor (no timeout/cancel, wrong Parakeet duration, no cloud-translate guard) (effort: M for the discrete fixes; L for full route-through-executor)
- **Failure**: an upload against a wedged sidecar or slow decode **hangs unbounded** (executor gets the watchdog + hard timeout + Whisper retry; the upload path does not). A translate task on a translate-incapable cloud provider **silently transcribes untranslated**. Parakeet uploads land `duration 0ms` in history.
- **Why**: in `transcribe_audio_file_impl` (`commands/audio.rs` ~6623-6972): Whisper arm passes `|| false` should_cancel (`~6747`) and Parakeet `cancel_flag: None` (`~6793`) with **no timeout wrapper** vs `executor::run_with_policy` (`executor.rs:306-401`); Parakeet uses bare `seconds_to_duration_ms(duration)` (`~6808`) vs executor's `effective_parakeet_audio_duration_ms` (`executor.rs:255`); cloud arm (`~6823`) has no `ensure_cloud_task_supported` guard (`executor.rs:275`); normalization inverted — upload ffmpeg-normalizes every arm incl. cloud (`~6824-6832`) vs executor skipping ffmpeg for Cloud/Remote and already-16k WAVs.
- **Slice (discrete bug-fixes, ship now = M)**: wrap each arm in the timeout policy, swap in `effective_parakeet_audio_duration_ms`, add the `ensure_cloud_task_supported` guard before the cloud arm.
- **Full slice (TIER 2 = L)**: route `transcribe_audio_file_impl` through `transcribe_with_app` — needs `audio_ctx: Option<i32>` on `TranscriptionRequest`, `span_timings_ms` plumbing into the executor result, a diarized-cloud pre-dispatch branch, `TimeoutPolicy::Upload` + `CleanupPolicy::CallerOwns`; delete the ~220-line match. Remote arm stays inline until #19/#17.
- **Acceptance**: golden test that the CLI JSON contract is unchanged; new test that an upload against a hung engine returns `Timeout` not a hang; Parakeet upload of a duration-0 WAV yields header-derived `duration_ms`.

### T1-D · #8 — cold-start hotkey overwrite (returning user's custom hotkey silently reset) (effort: S for hotkey-only)
- **Failure**: on a cold start where `get_settings` loses the race against `get_model_status`, `settings` is still null when OnboardingDesktop mounts; the hotkey step seeds `useState(settings?.hotkey || 'Alt+Space')` with **no resync effect**, and `saveHotkeySettings` persists that local `Alt+Space` — **overwriting a returning user's real hotkey** if they pass the hotkey step.
- **Why**: `components/onboarding/OnboardingDesktop.tsx:234-236` seeds once; the only `setHotkey` sites are user edits (~1349/1351); `saveHotkeySettings` persists the local value. Reachable only via the cold-start race: `hasModels` inits null (`useModelAvailability.ts:74`), and in the no-models case `get_model_status` scans an empty dir and can beat `get_settings`. The runtime no-models-error recovery path (`AppContainer.tsx:211-227`) has settings already loaded, so blast radius is narrow.
- **Slice**: resync the hotkey when `settings` first becomes non-null (guard with a pristine/`userEdited` ref so a real edit is never clobbered).
- **Acceptance**: test that mounting with `settings===null` then resolving to a custom hotkey leaves the custom value; a user edit before settings resolve is preserved.
- **Deferred (maintainability, → TIER 2 #19-adjacent)**: the 5+ uncoordinated remote-server fetchers (OnboardingDesktop, ModelsSection, OverviewTab, RecentRecordings, AudioUploadSection, NetworkSharingCard) → extract a shared `useRemoteServers()` hook (M). Staleness/UX, not an active bug.

### T1-E · #13 — stream session holds `ParakeetClient` `RwLock` write guard for the whole recording (effort: M)
- **Failure**: during a `live_preview` recording, any ungated Parakeet command (status, `download_ctc`, `delete_model`, or a batch transcribe after stop) blocks **unbounded** — the per-command timeout can't fire because it wraps only the post-lock request.
- **Why**: `open_stream`'s spawned task takes `inner.write().await` (`parakeet/sidecar.rs:639`) and holds it to task end (`~881-882`); `ParakeetClient::send` re-acquires the same write lock in `ensure()` (`~500/519`) **before** `timed_request` wraps the request (`~521`). `real_transcription_busy`'s `AppState` peek (`parakeet/manager.rs:150-166`) only gates warmup (`~614`) — a workaround that both confirms the flaw and leaks app-global UI state into the domain manager. Narrow trigger (needs `live_preview` + a concurrent non-gated command), so medium severity.
- **Slice**: introduce a `ClientSlot` enum with a typed `Busy` state so a live stream doesn't hold the write guard; per-command dispatch returns/queues instead of blocking; delete the `AppState` peek. This is an M refactor of `sidecar.rs` + `manager.rs`.
- **Acceptance**: test that a status/`delete_model` command issued during a live stream returns a typed Busy (or completes) within the command timeout instead of hanging.

### T1-F · #17 — remote inbound `RealTranscriptionContext` is a third shadow engine stack (effort: L)
*(Correctness bug AND a TIER 2 executor-unification slice — see Wave 3.)*
- **Failure**: (1) a host that both serves and dictates loads the same Whisper model **twice** (~1.5 GB ×2 for large-v3-turbo); (2) an unknown/cloud engine string silently falls to the Whisper arm; (3) a Windows strong host serves inbound **CPU-only** (no GPU sidecar), defeating the "strong host" premise; (4) inbound Parakeet results **lack the user's custom vocabulary**, so remote ≠ local for identical audio.
- **Why**: `remote/transcription.rs` — private `Arc<StdMutex<TranscriberCache>>` (`~115`, built `~132/147`) alongside the app's `AsyncMutex<TranscriberCache>` (`lib.rs:606`); string dispatch `transcribe_inner` (`~331-352`, `"parakeet" => …, _ => whisper`); Whisper arm calls `transcribe_with_metadata_with_prompt` directly (`~392-401`) bypassing `transcribe_whisper_with_acceleration`; Parakeet arm calls `manager.transcribe(...)` (`~442-452`) with no custom vocab + a third inline duration formula (`~465`).
- **Slice**: wire executor Stage 4 — `transcribe_inner` snapshots `SharedModelState` → `EngineSelection` (typed error for unknown strings), builds `TranscriptionRequest { source: RemoteServer, … }`, calls `transcribe_with_app` via the `block_in_place/block_on` pattern the file already uses; delete the private cache + both helpers; remove the `HostDefault` deferred-route error at `executor.rs:136`.
- **Acceptance**: unknown engine string returns an error (not Whisper output); only the `lib.rs`-managed cache remains; weak-client → Windows-host smoke shows GPU sidecar log lines for inbound requests.

---

## 3. TIER 2 — High-leverage refactors that de-risk future work

Sequenced so each early slice de-risks the later ones. The **Soniox (plan 043 / task #11) critical path is `#18 → #21 → #14`**, then the executor-unification block (`#16-full / #17 / #19 / #22`).

### The engine-dispatch consolidation chain (unblocks Soniox + kills 4 parallel dispatches)

**#18 · Delete the dead `transcribe_audio` bytes command (dead-code, S) — do this FIRST.**
- Confirmed dead: defined `commands/audio.rs:7007`, registered `lib.rs:1357`, but the only frontend invoke is `transcribe_audio_file` (`src/state/upload.ts:62`, `RecentRecordings.tsx:372`); no caller of the bytes command in `src/` or `src-tauri/`. Carries a full 4-arm match (~250 lines) that has drifted furthest (raw temp path, no ffmpeg, `|| false`, bare `seconds_to_duration_ms`, `.transcribe` not `.transcribe_diarized`, shared `temp_audio.wav`).
- **Why first**: drops the dispatch count 4→3, collapses #22's triplication to a 2-way dup, and removes one of #19's three copies — shrinking the surface before unification.
- **Acceptance**: `pnpm typecheck && pnpm test && cargo test` green; grep confirms no `invoke('transcribe_audio')`.

**#21 · Fix the layering inversion — move engine impls out of `commands/audio.rs` (maintainability, M).**
- `executor.rs:29-33` imports 6 engine items **back** from `crate::commands::audio` (`resolve_engine_for_model`, `transcribe_whisper_with_acceleration`, `compile_parakeet_custom_vocabulary_for_transcription`, `parakeet_segments_to_transcription_segments`, `transcription_watchdog_budget`, `ActiveEngineSelection`); impls live at `audio.rs:1108/1198/1530/1596/3391/3478`. Non-test modules referencing `commands::audio` grew 14→18.
- **Slice**: pure code-move to `transcription/engines.rs` with `pub(crate)` re-exports. Mechanical, but `transcribe_whisper_with_acceleration` is a large generic with cfg-gated platform code that must still compile on macOS + Windows CI.
- **Why here**: establishes a clean engine seam *below* the command layer, so a new engine (Soniox) plugs into `transcription/` not `commands/`.

**#14 · Extract a transport/capability layer — THE direct Soniox blocker (maintainability, L).**
- Three hand-rolled transports with divergent guarantees: Parakeet `CommandEvent` (no request ids; `dispatch_cancellable` still parakeet-only, `parakeet/sidecar.rs:244`), Whisper GPU raw `tokio::process` (`whisper/gpu_sidecar.rs:278` next_id, id-mismatch kill `:567`, Notify abort `:282/529` — protections parakeet lacks), Remote HTTP (`remote/client.rs:403`). The `CommandEvent → ParakeetResponse` read loop is written **3×** in `sidecar.rs` (`:146`, `:322`, inline in `request_with_progress_and_cancel:414`). Routing is still a **string compare** — `audio.rs:169` `current_engine != "parakeet"`, `audio.rs:4351` `== "parakeet"` (both confirmed in current tree). `EngineStreamCapabilities` is *no longer strictly dead* (used by `commands/model.rs:76/164` for frontend metadata) but is **not** wired into the recording-path routing gate.
- **Slice (smallest coherent cut)**: extract `src-tauri/src/transport/line_json.rs` owning spawn/kill/respawn, framed line reads + stderr recovery, request ids, and the deadline+cancel dispatch (move `dispatch_cancellable` there); port **only** the parakeet path onto it; make the sink factory consult `EngineStreamCapabilities::for_engine` instead of the string compare. GPU port is a follow-up.
- **Why**: task #11 Soniox WS otherwise clones a 4th transport. After this, a new streaming engine registers by flipping a capability row + implementing a `StreamSession` trait.
- **Acceptance**: all existing parakeet sidecar/manager tests pass unchanged; new transport unit tests cover deadline-on-cancel, respawn-once, id-mismatch, stderr recovery; grep shows one read loop in the parakeet path.

**#16-full · Route `transcribe_audio_file_impl` through the executor (correctness+dedupe, L).** — see T1-C. Kills parallel dispatch #2.

**#17 · Wire executor Stage 4 for remote inbound (correctness+dedupe, L).** — see T1-F. Kills parallel dispatch #3.

**#19 · Remote send-to-peer as executor Stage 5 / one orchestration helper (maintainability, M).**
- `client::transcribe_audio` for send-to-peer appears 3× in `audio.rs`: desktop hot path (`~5504`), upload (`~6922`), bytes (`~7216`, DEAD per #18) — 2 live + 1 dead. Each hand-rolls `fs::read → RemoteServerConnection::new → resolve_remote_request_context → build → client::transcribe_audio → bespoke '🌐 [Remote/Upload/Clipboard]' logging`. Divergence: desktop maps errors to `TranscriptionFailure::Remote` (`~5523`), upload maps to plain `String` via `e.to_string()`. The "desktop arm has no timeout" sub-claim is **wrong** (remote requests carry their own `timeout_ms`). So this is maintainability (triple-synchronized wire-contract + inconsistent error taxonomy), not a live bug.
- **Slice**: extract one orchestration helper (or the executor Remote arm); #18 already removes the third copy.

**#22 · Dedupe the settings-resolution triplication (maintainability, M).**
- The ~40-line legacy-language/translate/`ai_enabled`/`transcription_task` block appears in `transcribe_audio_file_impl` (`~6672-6714`), `transcribe_audio` bytes (`~7040-7080`, dead), and `RecordingConfig::load_from_store` (`~3031-3060`). The claimed concurrent-clobber correctness bug is only in the **dead** bytes command (fixed `temp_audio.wav`), so **not an active bug**. After #18 this collapses to a 2-way dedupe (impl + `load_from_store`).
- **Minor nit** (narrow, separate): the `%Y%m%d_%H%M%S` second-granularity normalized filename could collide across two same-second live uploads.

### Broad de-risk (independent of the engine chain)

**#2+5 · Single `ModelCatalogContext` — delete the onboarding ref-dance (maintainability, M).**
- Two independent `get_model_status` fetches (`useModelManagement.ts:138` + `useModelAvailability.ts:199`) with divergent event subscriptions; the onboarding gate reads `modelAvailability.hasModels` (`AppContainer.tsx:266-270`) while OnboardingDesktop reads `modelManagement.models` (`OnboardingDesktop.tsx:224,354,372-373`). `model-downloaded` fires 2× `get_model_status`. The acute onboarding-flip is currently **papered over** by the reconciliation ref-dance (`previousHasModels`, `forceOnboardingNeedsFreshAvailability`, `setTimeout(0)` @245-264) — fragile debt + 2× IPC waste + latent correctness.
- **Slice**: `ModelManagementProvider` becomes the sole owner of `get_model_status` + lifecycle events; availability derives `hasModels`/`selectedModelAvailable` from the catalog + its recognition-availability channel; delete the ref-dance. ~4-5 files.
- **Acceptance**: grep shows exactly one `get_model_status` call site in `src/`; one invoke per `model-downloaded`; delete-last-model drives both stores to the no-model state in one `act()`.

**#11 · Typed settings layer (maintainability, L) + #10 · versioned migration (maintainability, M) — pair them.**
- **#11**: exactly **73** `store('settings')` sites across 16 files; `start_recording` opens the store **3×** back-to-back on the hot path (`audio.rs:4096, 4109, 4134`); `recording_mode` has 3+ sources of truth (`settings.rs:605-608`, `lib.rs:1079-1091`, `tray.rs:499-503`, plus `AppState.recording_mode` cache `app_state.rs:43`); `transcription_mode` compared as a raw literal `== 'live_preview'` (`audio.rs:4113`) bypassing `normalize_transcription_mode`; `ai_*` keys absent from the `Settings` struct (23 raw `ai_enabled` reads). Perf angle is marginal (store() is a cheap in-memory Arc). Slice: typed module + enum migration + `ai_*` struct fields + a CI grep gate.
- **#10**: 7 duplicated legacy language/translate fallback read sites (`settings.rs:356-379`, `audio.rs:3031-3060/6674-6700/7042-7047`, `remote.rs:1555-1558`, `ai.rs:1132-1136`, `lib.rs:1768`); no `settings_schema_version` / `migrate_settings` exists; `save_settings` dual-writes legacy keys on every save with no removal path (`settings.rs:653-659`). The `'en'` default drift is currently **harmless** (default matches the literals). Slice: versioned migration + stop dual-writing.
- **Why paired**: the typed layer gives the migration a single place to normalize; both de-risk every future settings-touching engine addition.

**#23 · Move recording-lifecycle statics into `AppState` (maintainability, M).**
- `static RECORDING_GENERATION` (`audio.rs:77`) and `static IN_FLIGHT_TRANSCRIPTION_AUDIO` (`audio.rs:320`) are process-global, mutated cross-module via `pub(crate)` free functions (`transcription/stream.rs`, `audio/stream_tap.rs:260-285/371`, `recording/hotkeys.rs`), split from their `AppState` siblings. Gating is **correct in production** (single process) — the real cost is audit-difficulty + a genuine **parallel-test flake**: four uncoordinated serialization schemes touch the same global counter (`recording_state_characterization.rs` `GENERATION_TEST_LOCK`, `audio_recording_tests.rs` `IN_FLIGHT_TEST_LOCK`, `stream_tap.rs` `#[serial]`, and `transcription/stream.rs` tests with **no lock**).
- **Slice**: move both statics into a `RecordingLifecycle` field on `AppState`; convert the ~7 free functions to thin wrappers over `&AppState`; rewrite the three test files to local instances. **Cheap interim if the flake bites**: one shared test lock across all four files.

**#20 · Extract the `stop_recording` delivery stage (maintainability, L).**
- `commands/audio.rs:4690-6247` (~1557 lines) with triple-nested spawns (`task_handle:5393 → enhance/paste/history:5672 → history-save/reset:5988,6053,6122,6211`) and the stale/cancel delivery gate re-checked inline at `5538,5554,5577,5879,5901,5944,6009`. Slice 045 added the small `delivery_aborted()` helper (`:387`) + `persist_if_current` but did **not** extract the stage. The gate behaves correctly today — the pain is an untestable paste-critical monolith.
- **Slice**: extract ~580 lines into `deliver_transcription` + a **pure** `delivery_disposition` while preserving spawn boundaries/event order; add unit tests for "succeeded-but-generation-went-stale-during-enhancement". Behavior-preserving; a week+.

**#27 · Error-surface convention (maintainability, L) — follow-up to the #25/#24 point-fixes, not a blocker.**
- Four parallel error channels confirmed: (a) `recording-state-changed` Error field, (b) `pill_toast` toast webview (`audio.rs:519-638`), (c) ad-hoc domain events (`tray-action-error` `lib.rs:893-1003`, `parakeet-unavailable` `lib.rs:1917`, `no-models-error` `audio.rs:3455,3749`, `remote-server-error` `audio.rs:6103`), (d) pill `flashError`. `abort_due_to_missing_model` (`audio.rs:3432-3472`) triple-surfaces the same fault; per-site reset durations are hand-picked (2500ms/3s), toast durations 1000-6000ms. This is the **substrate that produced #25** and the double-surfacing.
- **Slice**: a `surface.rs` helper owning Error + a single Idle reset + a canonical user-error event + one frontend handler; migrate the 3 worst sites. Land the #25/#24 point-fixes first.

---

## 4. TIER 3 — Lower-priority cleanups / dead-code / quick wins

- **#26 · Delete dead frontend error listeners (dead-code, S).** `SettingsTab.tsx:29` (`hotkey-registration-failed`) and `:39` (`no-speech-detected`) register toasts with **zero** production emitters (only self-emitting tests in `error_event_tests.rs:23-58`). Real no-speech goes through `pill_toast_with_suggestion("No speech detected")` (`audio.rs:5616`); `hotkey-registration-failed` has no emitter post trigger-engine migration (`EventCoordinator.ts:138-141` documents the orphaning). Minimal: delete the two listeners + the self-referential test. The full "single event-name source of truth + generated `src/types/events.ts` + CI sync test" is L and is gold-plating.
- **#1+7 · Split the `AppContainer` init effect (perf, S) — quick win.** `AppContainer.tsx:56-102` still deps `[settings]` (`:102`); every `updateSettings` (SettingsContext replaces the object `:55-58`) re-runs `loadApiKeysToCache` (per-provider keychain read, `keyring.ts:92-113`), `cleanup_old_transcriptions` (`:70`), and `updateService.dispose()+initialize()` (`:100/77`). The claimed timer "race" is overstated (clears-before-set, `updateService.ts:342-344`; singleton timer meant to live the session). Slice: run-once startup effect + narrow effect keyed on `settings?.check_updates_automatically`. <1 day.
- **#3 · Listener re-register on unstable deps (maintainability, S) — mechanical.** `useModelAvailability.ts:311` deps `[applyCanonicalAvailability, checkModels, state.selectedModelAvailable]`; root cause is the recognition-availability handler reading `state.selectedModelAvailable` directly (`:294`) instead of a ref → all 7 `listen()`s re-register on every refresh; `AppContainer.tsx:243` re-registers 7 on model select. Severity **overstated** — largely self-healing (a model change re-runs `checkModels` and re-derives the snapshot). Ref infra already exists (`:152-164`). Slice: reduce deps to `[]`/`[registerEvent]`, read `selectedModelAvailable` from the ref.
- **#4+6 · Hoist `useRecording` into a `RecordingProvider` (maintainability, S).** No `RecordingProvider` exists; `useRecording` is `useState`-based (`:29-31`) with two live consumers during onboarding (`useInAppRecordingHotkey` via `AppContainer.tsx:48` + `OnboardingDesktop.tsx:201`) → 2× listener sets, 2× unguarded initial fetch, 2 writers to `updateService.setSessionActive`. Races are narrow/self-correcting (both sync to the same backend events, overlap only during onboarding). Slice: hoist once, both consumers read via context, move the `updateService` effect into the provider, add an events-won guard to the initial fetch. **Not** touched by the stop-gate work (that was intra-instance).
- **#9 · Patch-write `save_settings` (maintainability, M) — NOT a bug, lowest priority.** The claimed "resurrect `translate_to_english`" does **not** reproduce: every AI-disable path already mirrors the reset (`EnhancementsSection.tsx:461-467` + `:328-331`), `transcription_task ⟺ final_text_language` are coupled by `normalize_*` on every save/read (`settings.rs:643-651, 368-379`), and the guard predates the review (never live). Underlying debt (full-struct save safe only by convention) is real but low-value; keep as optional patch-write cleanup.

---

## 5. ALREADY-FIXED by slice 045 — do NOT re-do

- **#12 · Parakeet stream-session `stream_busy` leak.** Every abnormal exit of the `open_stream` loop (`parakeet/sidecar.rs:764-882`) now sets `clear_after_exit=true` + `Self::clear_sidecar(guard)` at `:881`, killing the process and emptying the slot — inactivity-timeout (`:863-876`), stdout-parse-error (`:850-860`), control-channel-closed (`:767-774`), event-closed (`:811-818`), chunk-write-error (`:803-809`). Swift sets `activeStreamSession=nil` on finalize (`main.swift:1119`) + cancel (`:1148`). No exit wedges the slot; next `ensure()` respawns. More robust than the single `cancel_stream` the issue asked for.
- **#15 · Preview-finalize on stop path + unbounded tap-worker join.** The tap-worker join is now bounded: `recorder.rs:737` calls `join_stream_tap_bounded(worker, STREAM_TAP_JOIN_TIMEOUT)` (`=100ms`, `:63`); on timeout the worker is detached (`:926-948`), covered by `stream_tap_join_timeout_detaches_slow_worker` (`:1044`). `StreamTapFinalizer::finalize` (`stream_tap.rs:83-88`) is already non-blocking. The 30s `STREAM_FINALIZE_TIMEOUT` still exists but now runs inside the detached worker, off the stop→paste critical path; the WAV is finalized by the separate writer worker (bounded 3s, `:734`). Part (b) (lowering the 30s deadline) is unnecessary given the detach.

---

## 6. Recommended execution order

Rationale threaded so each wave de-risks the next. Effort in parens.

**Wave 0 — Correctness point-fixes (ship independently, ~days):**
1. **#25** (S) — route the too-short gate through `pill_toast` + delivery-policy table. *Highest user impact.*
2. **#24-narrow** (S) — drop the bare `"model"` marker in `error.rs:118`; add regression test.
3. **#8-hotkey** (S) — resync hotkey on first non-null settings + pristine ref.
4. **#16-discrete** (M) — add timeout wrapper + `effective_parakeet_audio_duration_ms` + `ensure_cloud_task_supported` to the upload arms.

**Wave 1 — Shrink the dispatch surface (prereqs for unification):**
5. **#18** (S) — delete the dead bytes command → 4 dispatches to 3, collapses #22 to 2-way, removes a #19 copy.
6. **#21** (M) — move engine impls to `transcription/engines.rs`; clean the layering inversion → a stable engine seam below the command layer.

**Wave 2 — Transport/capability layer (the Soniox unblock):**
7. **#14** (L) — extract `transport/line_json.rs`, port parakeet, wire `EngineStreamCapabilities::for_engine` into the sink factory. *Task #11 Soniox WS registers by capability row after this, not a 4th transport.*

**Wave 3 — Executor unification (kills the parallel dispatches; also lands #17's correctness fix):**
8. **#16-full** (L) — route `transcribe_audio_file_impl` through `transcribe_with_app`.
9. **#17** (L) — wire executor Stage 4 for remote inbound; delete the private cache.
10. **#13** (M) — `ClientSlot`/typed-Busy so the stream stops holding the write guard; delete the `AppState` peek (cleaner once the transport/executor seams exist).
11. **#19** (M) + **#22** (M) — one remote orchestration helper; dedupe the (now 2-way) settings-resolution block.

**Wave 4 — Settings + lifecycle hardening (broad de-risk):**
12. **#11** (L) + **#10** (M) — typed settings module + versioned migration (paired) + CI grep gate.
13. **#23** (M) — recording-lifecycle statics into `AppState` (or the shared test lock as interim if the flake bites first).

**Wave 5 — Frontend consolidation + delivery testability + error surface:**
14. **#2+5** (M) — single `ModelCatalogContext`; delete the ref-dance.
15. **Batch the S mechanical frontend cleanups**: **#3** (S), **#4+6** (S), **#1+7** (S), **#26** (S).
16. **#20** (L) — extract `deliver_transcription` + pure `delivery_disposition` with unit tests.
17. **#27** (L) — error-surface convention; migrate the worst 3 sites. *Follow-up consolidation, informed by #25/#24.*
18. **#9** (M, optional) — patch-write `save_settings` if the settings work leaves it worthwhile. Not a bug.

**Why this order:** correctness point-fixes ship value immediately and independently (Wave 0). #18+#21 shrink and stabilize the engine surface before any unification (Wave 1). #14 is the single slice that lets Soniox land without a 4th transport (Wave 2). The executor-unification block (Wave 3) collapses the remaining parallel dispatches so future engines are wired once. Settings/lifecycle hardening (Wave 4) de-risks every engine that touches config or lifecycle state. The frontend/delivery/error-surface consolidation (Wave 5) is valuable but blocks nothing on the Soniox path, so it trails.

---

## 7. CORRECTION (2026-07-04, after grounding Wave 2 against current code)

**The roadmap's premise that "#14 transport extraction unblocks Soniox" is partly wrong.**
Grounding the transport layer revealed #14 is really TWO separable pieces with very
different risk/value:

- **#14a — capability-based routing (LOW risk, the ACTUAL Soniox enabler):** replace the
  string compares `current_engine == "parakeet"` (commands/audio.rs:174 and :4203 — note
  drift from the roadmap's :169/:4351) with `EngineStreamCapabilities::for_engine(...)`
  lookups, so a new streaming engine registers by its capability row. Subtlety: the
  capability table has `PARAKEET = FINAL_ONLY` (dormant since EOU is broken upstream) while
  the string compare still routes Parakeet to the preview path — so this change must
  reconcile the two, and is cleanest done AS PART OF plan 043 (Soniox), where the SONIOX
  capability flips to supports_streaming=true and is tested end-to-end.
- **#14b — extract `transport/line_json.rs` (HIGH risk, NOT a Soniox blocker):** Soniox is a
  WebSocket engine (tokio-tungstenite) — it shares NOTHING at the transport level with the
  line-JSON *sidecar* transport. Extracting line_json.rs helps a FUTURE second sidecar-based
  engine, not Soniox. And it's the program's riskiest slice: the 3 read loops serve 3
  distinct jobs (batch cancel-poll+Progress, stream handshake, stream session partial-
  forward), all tangled with the slice-045 stop-gate semantics and the streaming teardown
  (#12/#13), verifiable fully only with real-device streaming smoke.

**Revised Wave 2:** DEFER #14b (line_json extraction) as a later maintainability slice
(clearly not a feature blocker). Fold #14a (capability routing) into plan 043 Soniox.
Proceed to either plan 043 (the cloud-streaming feature) or the lower-risk Wave 3/4
refactors, whichever the owner prefers. Anything touching the live-streaming transport
carries a device-smoke verification ceiling the sandbox can't clear.

---

## 8. NEW discovered issue (2026-07-04, found via the #16-full CLI A/B)

**#28 · ggml-metal teardown assert crashes the process on exit (pre-existing, NOT from any slice).**
The whisper CLI (and likely the app on quit) aborts on process exit with
`ggml_metal_device_free → GGML_ASSERT([rsets->data count] == 0) failed` (SIGABRT/134),
AFTER the transcription completes and correct output is produced. Confirmed pre-existing:
the integration-HEAD baseline binary crashes identically to the #16-full binary (3/3 runs
each). Backtrace cites the known ggml-metal issue (github.com/ggml-org/llama.cpp/pull/17869).

- **Impact:** (a) the perf-harness captures CLI `--json` via stdout redirection, and the
  exit-abort flakily truncates the buffered stdout flush → the sporadic empty/SKIPPED
  harness rows seen throughout the campaign. (b) The app process likely hits the same
  assert on quit (non-fatal to the user but a dirty exit / crash-reporter noise).
- **Likely fix:** ensure the whisper `WhisperContext` (and its Metal device) is explicitly
  dropped before process exit rather than during C++ static destructor `__cxa_finalize`
  (the assert fires because the Metal device is freed while resource sets are still live at
  atexit). For the CLI: drop the TranscriberCache/context before returning from the
  transcribe subcommand. For the app: drop on the shutdown/exit hook. OR bump
  whisper-rs-sys past the ggml fix once released.
- **Priority:** Medium — no user-facing data loss (transcription completes first), but it
  dirties CLI/harness reliability and app-quit. A contained fix (explicit context drop
  before exit). Verify by: CLI transcribe returns exit 0 and file-redirected `--json` is
  never truncated across 10 runs.
