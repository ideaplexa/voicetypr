# Plan 046 — Wave-0 correctness fixes (verified live bugs)

> From `docs/review/2026-07-03-arch-roadmap.md` Wave 0. Four verified, currently-
> reproducing user-facing bugs, each untouched by slice 045. Batched like slice 045:
> Codex fixes with regression tests, Claude gates. The two heavier correctness bugs
> (#13 RwLock, #17 remote shadow stack) are DEFERRED — they fold into the transport/
> executor refactor waves and need the user's scope steer.
>
> ⚠️ Line refs are current-tree (post-045) per the roadmap, but Codex must re-verify
> each site by semantics before editing and stop-and-report on genuine mismatch.

## Fix 1 — #25 · `recording-too-short` swallowed when pill disabled (S)

**Bug:** pill mode `"never"` + a sub-1s recording → no toast, no error, no feedback; app
silently goes Idle. The "unnoticed for months" class.
**Cause:** the pre-transcription too-short gate emits `recording-too-short` only to the
`"pill"` window (`commands/audio.rs` ~:5308); the pill window doesn't exist when
`pill_indicator_mode == "never"` (`audio.rs` ~:4626); `emit_to_window`'s critical-event
whitelist (`window_manager.rs` ~:484/498/534) excludes it → dropped at `log::debug`.
**Fix:** route this gate through `pill_toast()` (the post-transcription too-short path at
`audio.rs` ~:6049 already does — `pill_toast` emits app-wide `"toast"` + the always-present
toast window, so it survives pill-off). Optionally replace the thrice-duplicated `matches!`
whitelist with one `(event, DeliveryPolicy)` const table and raise the drop log debug→warn.
**Test:** Rust — emit `recording-too-short` with no pill window → queued/broadcast, not
dropped. Manual: pill "never", record <1s, feedback still appears.

## Fix 2 — #24-narrow · `"model"` substring misclassifies transient errors (S)

**Bug:** any transient engine error whose text contains "model" (e.g. "failed to load model
tensors") → classified `ModelUnavailable` (not in `default_retryable`) → `run_with_policy`
skips retry, user told "model unavailable, pick another" instead of "try again".
**Cause:** `transcription/error.rs:118` `... || lower.contains("model") => ModelUnavailable`.
**Fix (narrow, ship now):** drop the bare `|| lower.contains("model")`; keep only the exact
`"no speech recognition models"` marker. Everything else falls through to `EngineFailed`
(retryable). Regression test: a raw error containing "model" maps to retryable EngineFailed.
**Out of scope:** the full "carry TranscriptionErrorCode end-to-end, delete all substring
classifiers" is #27 (Wave 5). Cancel/timeout dispatch is already decoupled — leave it.

## Fix 3 — #8-hotkey · cold-start overwrites a returning user's custom hotkey (S)

**Bug:** on a cold start where `get_settings` loses the race to `get_model_status`, `settings`
is null when OnboardingDesktop mounts; the hotkey step seeds `useState(settings?.hotkey ||
"Alt+Space")` with NO resync, and `saveHotkeySettings` persists that local `Alt+Space` —
overwriting the user's real hotkey if they pass the hotkey step.
**Cause:** `components/onboarding/OnboardingDesktop.tsx` ~:234-236 seeds once; only `setHotkey`
sites are user edits (~:1349/1351).
**Fix:** resync the hotkey local state when `settings` first becomes non-null, guarded by a
`userEdited`/pristine ref so a real user edit is never clobbered.
**Test:** mount with `settings===null`, then resolve to a custom hotkey → local value becomes
the custom one; a user edit made before settings resolve is preserved.
**Deferred (maintainability, not this slice):** the 5+ uncoordinated remote-server fetchers →
`useRemoteServers()` hook.

## Fix 4 — #16-discrete · upload/CLI dispatch: no timeout, wrong duration, no cloud guard (M)

**Bug:** an upload against a wedged sidecar/slow decode **hangs unbounded**; a translate task
on a translate-incapable cloud provider silently transcribes untranslated; Parakeet uploads
land `duration 0ms` in history.
**Cause:** `transcribe_audio_file_impl` (`commands/audio.rs` ~6623-6972): Whisper arm `|| false`
should_cancel, Parakeet `cancel_flag: None`, no timeout wrapper (vs executor
`run_with_policy`); bare `seconds_to_duration_ms(duration)` (vs `effective_parakeet_audio_
duration_ms`); cloud arm missing `ensure_cloud_task_supported`.
**Fix (discrete, ship now — do NOT do the full route-through-executor here, that's Wave 3):**
wrap each arm in the timeout policy the executor uses; swap in `effective_parakeet_audio_
duration_ms(duration, input_path)`; add the `ensure_cloud_task_supported` guard before the
cloud arm.
**Test:** upload against a hung engine returns Timeout not a hang; Parakeet upload of a
duration-0 WAV yields header-derived `duration_ms`; a translate task on a translate-incapable
provider is rejected, not silently transcribed.

## Acceptance
- `cargo test` + `cargo clippy --workspace --all-targets -- -D warnings` + `pnpm typecheck/
  lint/test` green.
- Each fix has a regression test that fails before / passes after.
- No behavior change beyond the four fixes.
- Do not commit (Claude commits after review).
