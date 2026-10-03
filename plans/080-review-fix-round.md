# Plan 080 Phase A — review fix round

Base: `feat/2.1-beta4` at `183a8bd8`. All eleven review findings have source changes and regression coverage. No commit, push, Rust build, cargo test, or clippy was performed.

## Fix and test mapping

| # | Change | Regression coverage |
|---|---|---|
| 1 | `src-tauri/src/recording/kept.rs:233,533`: retry cancellation is tracked with a watch channel. Escape/Discard cancel through the kept store; an accepted new start cancels synchronously and publishes Idle before opening its new generation. The Stop guard covers synchronous lease setup only, never decoding/network I/O or Idle publication. Retry upload uses app cancellation. A superseding take preserves the original kept clip. | `kept.rs:734` `cancel_pending_retry_then_new_recording_can_stop`; `:774` `new_take_cancels_retry_and_preserves_the_kept_clip`. The Stop regression exercises the real guard with a simulated capture owner, not hardware capture. |
| 2 | `commands/audio.rs:4918`, `recording/start_source.rs:1`, `recording/hotkeys.rs:168`, `menu/actions.rs:121`, `src/pill.tsx:56`: explicit pointer/tray/hotkey source. Both PTT release checkpoints apply only to hotkey starts. Pointer/tray takes use toggle behavior; next island/tray/hotkey press stops, including a queued tray stop during Starting. Redundant starts retain the existing false return. Pointer/tray start failures use content-free island feedback. | `start_source.rs:23` `pointer_and_tray_start_in_both_modes_with_no_key_held`; `menu/actions.rs:224` `tray_toggle_stops_active_or_starting_pointer_take_in_either_mode`; `src/pill/quick-settings.test.ts:77,84` second-click and failure tests in both modes. |
| 3 | `recording/kept.rs:26,259,409`, `menu/actions.rs:146`, `src/pill/feedback.ts:36`: store/event keep the stable model ID separately from its label. Tray and island pass that ID; an unavailable alternative cannot fall back to the failing current engine. Tray labels come from the retained recovery rather than a fresh alternative lookup. | `kept.rs:813` `alternative_keeps_model_identifier_separate_from_label`; `menu/actions.rs:211` `tray_recovery_passes_the_kept_alternative_id`; `src/pill/feedback.test.ts:57` island retry transport on both platforms. |
| 4 | `commands/text.rs:210,469,482`: clipboard commits check cancellation/generation after lock/open waits; paste checks after settle and platform waits, immediately before OS dispatch/fallback/retry. Key-up cleanup still runs after an injected key-down. `commands/audio.rs:7213` ordinary copy-only dictation uses the guarded dictation copy command. | `text.rs:996` `cancel_or_new_generation_during_clipboard_wait_prevents_write`; `:1023` `copy_only_checks_after_injected_clipboard_wait`; `:1044` `escape_during_settle_prevents_paste_dispatch`. Existing clipboard sequencing tests updated to supply the commit check. |
| 5 | `recording/kept.rs:263`: fallback ownership is installed before fallible storage/awaits and relinquished only after acknowledged retention. Failed transfers stay owned for exit cleanup, are removed from normal finalization, and emit only `storage_failed`. All nine audio handoff sites use this path. `recording/island.rs:76`, `src/types/island-events.ts`, `src/pill/feedback.ts:45` carry/render “Couldn't keep the recording.” | `kept.rs:796` `failed_storage_handoff_keeps_fallback_until_exit`; island note serialization coverage; `src/pill/feedback.test.ts:17` storage-failure copy. |
| 6 | `trigger/engine_host.rs:87`: installed `escape-cancel` uses the same non-consuming Chord as the observer. Esc-twice dispatch remains unchanged. | `engine_host.rs:421` `complete_installed_bindings_never_consume_escape`, covering both recording modes and Starting/Recording/Stopping/Transcribing with the complete primary/PTT/cancel/observer installation. |
| 7 | `src/pill/renderer.ts:168`, `commands/pill_feedback.rs:212`, `window_manager.rs:347`: Never releases feedback ownership in renderer and native code, forces hide even for sticky feedback, and reaches pointer-poller shutdown. | `src/pill/feedback.test.ts:47` sticky ownership release/native-hide IPC; `pill_feedback.rs:124` `never_releases_even_sticky_feedback_ownership`. Native panel/poller acceptance remains NEEDS-SMOKE. |
| 8 | `src/pill/quick-settings.ts:15`, `src/pill/renderer.ts:338,427`, `pill/context.rs:267`, `menu/quick.rs`: list says “Default style,” displays a disabled app override hint, and refreshes the effective style using the take's pinned app context. A default pick cannot optimistically replace the effective take style. | `src/pill/quick-settings.test.ts:61` `default style pick refreshes the effective app style instead of changing the take`. |
| 9 | `menu/refresh.rs:3`, `commands/settings.rs:1534`, `menu/tray.rs:128,334`: debounce 150 ms; discard superseded generations before snapshots and before native menu construction/installation. History uses `page_history_keys(..., 5)` and reads at most five values rather than cloning all text. | `menu/refresh.rs:11` `burst_builds_only_the_latest_refresh`. Existing paging/history tests continue covering the shared helper. |
| 10 | `commands/audio.rs`: removed sensitive arguments from fifteen path/filename logging sites listed below. Reviewed changed logging calls throughout the branch against merge base `c8f58512`; no further path/title/transcript-text arguments were found in those changed calls. | `src/lib/recording-log-privacy.test.ts:4` scans balanced logging calls and rejects recording path/filename arguments. |
| 11 | `commands/settings.rs:1500`: saving the new visibility key deletes `show_pill_indicator`; legacy fallback reads remain. | `settings.rs:1792` `saving_visibility_deletes_the_legacy_key`, using the actual store write helper with the store plugin. |

## Privacy cleanup

Sensitive log arguments/fields removed in `src-tauri/src/commands/audio.rs`:

- Saved-recording revocation success/failure: lines 1121 and 1123 (filename).
- Save with cleanup / without cleanup: lines 4273 and 4334 (destination path).
- Retention cleanup failure/success: lines 4382 and 4384 (recording path).
- Recording file preparation: line 5290 (`log_file_operation` received the path).
- Recorder initialization/start failures: lines 5377 and 5420 (`audio_path` fields).
- Cancelled/aborted start cleanup: lines 5530 and 5570 (`path.display()`).
- Recording-start success: line 5680 (`audio_path` field).
- Normalized-audio metrics: line 6448 (`path` field).
- Pre-transcription context: line 6482 (`audio_path` field).
- History save with a recording: line 7693 (recording filename).

Timing, audio format metadata, and fixed lifecycle messages remain.

## Validation and remaining gates

- Observed frontend red/green: temporarily restored the original production implementations from `183a8bd8` while retaining the new tests. Ten tests failed across quick settings, recovery feedback, and log privacy. Restored the patch; all regression tests passed. The restore used a `finally` block, and the working-tree patch is intact.
- Final requested frontend gate passed: `pnpm typecheck && pnpm lint && pnpm exec vitest run && pnpm build`. **101 test files / 1,233 tests passed.** Vite emits its existing configuration/chunk-size warnings; the build exits successfully.
- `cd src-tauri && cargo fmt --check` passed. `git diff --check` passed.
- **Rust tests/clippy are unrun.** The user explicitly assigned Rust compilation to Claude because this sandbox cannot build the SwiftPM sidecar. Rust regression failures before the fix and passes after the fix have therefore not been observed here.
- **NEEDS-SMOKE:** Escape/Discard during a live pending retry followed by a new recording and Stop; real PTT pointer/tray/hotkey interactions; cancellation during native clipboard/paste waits; native Never panel hide/poller shutdown; focus safety and back-to-back dictation on macOS/Windows.
