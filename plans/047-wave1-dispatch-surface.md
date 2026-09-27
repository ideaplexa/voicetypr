# Plan 047 — Wave 1: shrink & stabilize the engine dispatch surface

> From `docs/review/2026-07-03-arch-roadmap.md` Wave 1. Two behavior-PRESERVING
> refactors that must ship as pure structural changes (zero behavior delta), verified
> by the existing test suite passing UNCHANGED. Order matters: #18 first (delete dead
> code shrinks audio.rs), then #21 (move engine impls out). Line refs are current-tree
> anchors — re-verify by semantics.

## Fix 1 — #18 · Delete the dead `transcribe_audio` bytes command (dead-code, S)

**Fact:** `transcribe_audio` (commands/audio.rs, the bytes-input command, ~7028+) is a
fourth full engine dispatch with **zero frontend callers** — `grep -rn "transcribe_audio[^_]" src/`
finds no `invoke('transcribe_audio')` (only `transcribe_audio_file` is used, in
`src/state/upload.ts` + `RecentRecordings.tsx`). It's the most-drifted copy (no ffmpeg
normalization, `|| false`/`cancel_flag: None`, bare `seconds_to_duration_ms`,
`provider.transcribe` instead of the typed/diarized variants) and every engine change
must still be audited against it.

**Change:** delete `transcribe_audio` (the whole `pub async fn transcribe_audio` + its
4-arm match) and its registration in `lib.rs` invoke_handler (~:1357), plus any
capabilities/permissions entry referencing it. Do NOT touch `transcribe_audio_file` /
`transcribe_audio_file_impl` (those are live).

**Acceptance:** `grep -rn "transcribe_audio\b" src/` and `src-tauri` shows no remaining
`invoke`/registration of the bare bytes command; `pnpm typecheck/lint/test` + `cargo
test` + clippy green; dispatch-site count drops from 4 to 3.

## Fix 2 — #21 · Move the engine layer out of `commands/audio.rs` (layering, M)

**Fact:** `transcription/executor.rs` imports engine implementations back from the
COMMAND layer: `use crate::commands::audio::{compile_parakeet_custom_vocabulary_for_transcription,
parakeet_segments_to_transcription_segments, resolve_engine_for_model,
transcribe_whisper_with_acceleration, transcription_watchdog_budget, ActiveEngineSelection}`.
14 modules import from `commands::audio`. This makes the command file the crate's
dependency hub and blocks clean extraction.

**Change (pure code move, zero behavior change):** create
`src-tauri/src/transcription/engines.rs` and MOVE these items there (with their unit
tests, currently ~audio.rs:1841-1872):
- `ActiveEngineSelection` (enum)
- `resolve_engine_for_model`
- `transcribe_whisper_with_acceleration`
- `compile_parakeet_custom_vocabulary_for_transcription`
- `parakeet_segments_to_transcription_segments`
- `transcription_watchdog_budget` (and the `watchdog_budget_for` / `LOCAL_ENGINE_TIMEOUT_GRACE`
  / `effective_parakeet_audio_duration_ms` / `ensure_cloud_task_supported` helpers if they
  naturally belong with the engine layer — but if they currently live in executor.rs from
  Wave 0, LEAVE them there; only move what's in commands/audio.rs).
Add `pub(crate) use crate::transcription::engines::*;` (or explicit re-exports) in
commands/audio.rs so NO other callsite changes in this slice. Point `executor.rs` at
`crate::transcription::engines::…` directly (drop its `crate::commands` import).

**Constraints:** NO `#[tauri::command]` function is relocated (commands stay in the
command layer). `git log --follow` should show moves, not rewrites. Behavior identical.

**Acceptance:** `cargo test` + `cargo clippy --workspace --all-targets -- -D warnings`
green on macOS (and must not break the Windows cfg paths — the whisper acceleration fn
has `#[cfg(target_os="windows")]` GPU-sidecar branches, move them intact); `executor.rs`
no longer imports from `crate::commands`; existing engine unit tests pass unchanged.

## Whole-slice acceptance
- Zero behavior change — the entire existing suite passes unmodified (only moved/deleted).
- `cargo test` + clippy + `pnpm typecheck/lint/test` green.
- Do not commit.
