# Plan 072 — Merge `main` into `feat/049-pure-rust-audio`: resolution decisions

Status: DECIDED by Claude (2026-09-27); executed by the implementer; verified by
Claude; adversarial review after. Evidence: the per-file conflict map (session
scratch `merge-map.md`, summarized here). Merge base `af63ab13`; `main` at
`84970f6c` (v2.0.6 + #146 post-roll). Backup of the branch tip before merging:
`backup/feat-049-pre-main-merge-2026-09-27`.

Principle: keep every fix `main` shipped to users; keep every feature this
branch built; where both solve the same thing, keep the better implementation
and port the other side's behaviour into it. Nothing shipped disappears
silently.

## Product decisions (Claude, founder may override)

1. **Parakeet custom vocabulary stays** (main ships it: `supports_vocabulary_terms:
   true`, CTC download, `writing/vocabulary.rs::compile_parakeet_custom_vocabulary`,
   `get_parakeet_vocabulary_status`, `ParakeetVocabularyTerm`). The branch's
   removal (58ec176a) is NOT carried over. Keep the branch's native-model
   selection; custom vocabulary applies to the TDT batch path exactly as on main.
   Keep main's EnhancementSettings copy that names Parakeet.
2. **Onboarding "first_transcription" step stays removed** (main removed it; it
   was a July founder decision to simplify onboarding). Drop the branch's
   onboarding-only step code (skip-confirm state machine, sample-textarea
   autofocus). The branch's hotkey/recording hook fixes are unaffected and stay.
3. **Pill = the branch's vanilla micro-bundle** (`src/pill.tsx`, live committed/
   tentative text). Port main's user-facing pill features into it so existing
   settings keep working: `pill_indicator_style`, all `pill_indicator_position`
   values, state labels, elapsed timer, activity indicator. Keep main's
   multi-monitor `DesktopArea` positioning. Pill size: the branch's 260×64
   (room for live text).
4. **Sounds = main's redesign** (`audio_feedback.rs`, `play_sound_on_transcription_complete`,
   `play_sound_on_paste_success`, the migration deleting `play_sound_on_recording_end`).
   Remove the branch's read of `play_sound_on_recording_end` in `commands/audio.rs`.
5. **Tooling = main's** (oxlint/oxfmt, Base UI, `pnpm check`, pinned Tauri CLI
   2.6.2, reusable `native-ci.yml`, manual Store workflow). The branch's ffmpeg
   removal wins everywhere: no ffmpeg script, step, cache or path remains.

## Per-file resolutions

Backend
- `writing.rs` → take main's `writing/` split; keep vocabulary code (decision 1).
- `cloud_stt/common.rs` → union.
- `cloud_stt/mod.rs` → keep main's model catalog and the `model: &str` param on
  every provider's `transcribe_typed`; keep the branch's removal of the
  `CloudProvider::transcribe()` wrapper and port its callers (now in
  `transcription/engines.rs`) to `transcribe_typed(.., model, ..)`; the Soniox/
  Deepgram realtime modules' REST fallbacks pass the selected model too.
- `parakeet/sidecar.rs` → branch's `log_parakeet_stderr()` extraction + main's
  `is_benign_coreml_shape_probe()` inside it + main's test; keep all streaming code.
- `whisper/transcriber.rs` → `new(model_path, speed_mode)`; keep main's
  `full_with_cancel` UB fix and `backend` field at every return; keep branch's
  `adaptive_audio_ctx`, `WhisperTranscriptionTimings`, earlier duration calc;
  keep main's `normalize_transcript_spacing`.
- `window_manager.rs` → main's `DesktopArea` signature; branch's
  `.accept_first_mouse(true)` and Windows HWND fix; size 260×64; update tests.
- `settings.rs` + `tests/settings_commands.rs` + `src/types.ts` → union of fields
  (main: `settings_mode`, `play_sound_on_transcription_complete`,
  `play_sound_on_paste_success`, `pill_indicator_style`, `update_channel`;
  branch: `whisper_speed_mode`, `transcription_mode`, `pill_position` type fix);
  drop `play_sound_on_recording_end` (decision 4). Rust and TS in lockstep.
- `commands/model.rs` → both new fields on `UnifiedModelInfo`
  (`available_models`, `supported_languages`) at all constructors; keep
  `get_parakeet_vocabulary_status` (decision 1).
- `lib.rs` → `match` form; `RunEvent::Exit` runs BOTH: the branch's
  TranscriberCache clear + RemoteServerManager stop (SIGABRT fix #28), then
  main's media-pause cleanup (macOS) and analytics shutdown. Union all other
  `run()`/setup hunks and command registrations.
- `audio/mod.rs` → keep `speech_evidence`, `stream_tap`, `decode` modules.
- `audio/recorder.rs` → keep the branch's `start_recording` signature (stream
  tap params). CPAL callback captures both `capture_metrics` and the stream tap.
  `drain_final_callback()` also feeds the final buffer to the stream tap. Stop
  sequence: stop command → main's interruptible post-roll (#146, keeps feeding
  writer AND stream tap) → drain barrier → finalize stream tap →
  `join_writer_bounded` → `join_stream_tap_bounded` → main's `finish_capture(...)`.
  Keep the 5 ms stop poll. Keep main's `CaptureAudioMetrics` incl. post-roll fields.
- `commands/audio.rs` → line-by-line 3-way merge. Keep the branch's engine
  layer in `transcription/engines.rs` and port main's `analytics_kind()`/
  `route()` there. Keep `start_recording -> Result<bool, String>` (ownership
  for deferred hold-stop) and thread main's no-speech gate, quiet-recording
  preservation, license-state and analytics logic through it. Keep main's
  `stop_recording_with_mode` + post-roll. Keep all streaming/stream-sink
  wiring and Soniox/Deepgram WS-final authority. Sounds per decision 4.
- `keytrigger/backend/windows.rs` → both (`is_own_injection` + `map_vk(vk, scan_code, flags)`).
- `sidecar/parakeet-swift/Sources/main.swift` → branch as base; re-apply main's
  `languageHint`/`language:` to the non-streaming batch `transcribe` calls.
- `sidecar/parakeet-swift/build.sh` → branch wholesale.
- `Cargo.lock` → do not hand-merge; regenerate from the merged `Cargo.toml`.

Frontend
- `src/pill.tsx` → branch + decision 3 port. `PillShell.tsx`, `usePillController.ts`,
  `RecordingPill.tsx` stay deleted; no dangling imports. `RecordingPill.test.tsx`
  → branch's DOM tests + tests for the ported style/position/labels/timer.
- `useRecording.ts` → `startRecording(): Promise<boolean>` via `invoke<boolean>`.
- `useInAppRecordingHotkey.ts` (+ test) → branch logic and tests; re-format.
- `ModelsSection.tsx` → main's restructure; port the branch's Whisper speed-mode
  switch, Regular/Live-preview control (`transcription_mode`,
  `activate_live_preview`), streaming-capability gating and native-model
  detection into the right `./models/*` files.
- `OnboardingDesktop.tsx` (+ test) → main's (decision 2); union remaining tests
  that still apply.
- `ModelsTab.tsx` (+ test) → both edits (`sourceFilterProps` + `supported_languages`).
- `LanguageSelection.tsx` → main's version + branch's `supportedLanguages` prop.
- `EnhancementSettings.tsx` → main's (vocabulary copy per decision 1).
- `types/ai.ts` → keep main's provider fields; take the branch's correction
  (remove `enhancement_options`, add `aiModelNeedsReselection`) and repoint any
  reader of `aiSettings.enhancement_options` to `get_enhancement_options`.

CI / scripts / docs
- `ci.yml` → main's; port the branch's hardening (Rust pin, workspace clippy
  all-targets, Parakeet Swift build cache, arch-split tests, Windows
  `run-tests.ps1`) into `native-ci.yml`.
- Delete every ffmpeg step/cache/path in `ci.yml`, `native-ci.yml`,
  `release.yml`, `store-msix.yml`, `package.json`; `scripts/ensure-ffmpeg-sidecar.cjs`
  stays deleted. Also check `scripts/build-msix-store.ps1`, `release-separate.sh`.
- `package.json` → main's tooling/deps + branch's script edits (no ffmpeg script,
  `perf-harness`).
- `plans/README.md` → keep both ledgers; branch rows whose numbers collide with
  main's (046–048) get a `b` suffix.

## Verification (Claude)

`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`,
`pnpm check` (lint + typecheck + tests as main defines it), Swift sidecar build,
a grep that no ffmpeg references remain, a grep for dangling imports of deleted
pill files, the real-speech suite, and `pnpm tauri dev` smoke: dictate with each
engine, live preview on Whisper/Unified/Soniox, pill style/position settings,
sounds, post-roll, cancel.
