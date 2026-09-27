# Plan 057 — cpal 0.18 stream resilience + auto device recovery

**Status:** TODO
**Priority:** P1
**Effort:** M
**Depends on:** — (independent; pairs with 058/059 smoke passes)

## Problem

cpal is pinned at 0.16 (`src-tauri/Cargo.toml`). Two release lines of capability are
on the table (0.17/0.18, verified against upstream release notes):

1. **No exact device-loss signal.** Today `StreamError` is opaque; when a device
   vanishes mid-recording (Bluetooth disconnect) we cannot distinguish "rebuild
   the stream" from "fatal". Current recovery machinery is heuristic:
   `device_watcher.rs:56-183` polls every 1500ms and only acts **when idle**;
   `recorder_watchdog.rs:35-86` (250ms) only detects a dead worker and
   dispatches stop — **no path resumes a live recording session**.
2. **No stable device IDs** — devices are matched by name; two identically named
   devices (paired AirPods) or localized names break persistence.
3. **Permission/busy are indistinguishable** — mic-permission denial (macOS TCC)
   and Windows exclusive-mode busy both surface as generic errors, defeating the
   Diagnostics quick-fix UX.

## What 0.17/0.18 give us (upstream-verified)

- `StreamError::StreamInvalidated` (0.17) — OS-fired "device gone, rebuild".
- `DeviceTrait::id` / `HostTrait::device_by_id` (0.17) — stable device IDs.
- Unified `cpal::Error` + `ErrorKind::{DeviceBusy, PermissionDenied}` (0.18).
- `Send`+`Sync` streams on WASAPI **and** CoreAudio (0.17) — threading symmetry.
- Hardware-accurate timestamps + `StreamTrait::now()` (0.18) — better silence-
  detector alignment.
- CoreAudio loopback (0.17, macOS 14.6+) — future system-audio capture (macOS
  only; WASAPI loopback is NOT exposed by cpal — Windows would need the `wasapi`
  crate or hand-written `AUDCLNT_STREAMFLAGS_LOOPBACK`; out of scope here).

## Scope

1. Migrate `Cargo.toml` cpal 0.16 → 0.18. Mechanical fallout: `SampleRate`
   struct → `u32`; all per-op error enums → `cpal::Error` with `kind()`.
   Touchpoints: `recorder.rs` (~500 lines of cpal surface), `device_watcher.rs`,
   `resampler.rs`, `commands/audio.rs` error mapping.
2. **Auto-recovery state machine** in the recorder worker: on
   `StreamInvalidated` → hold session state (writer, metrics, duration) →
   re-resolve device via stable ID (fall back to new default) → rebuild stream →
   continue same recording session. Emit a toast + tray state so the user sees
   the blip, not a dead recording. Target: BT disconnect mid-dictation = <300ms
   gap, session survives.
3. **Error-kind UX mapping**: `PermissionDenied` → existing quick-fix flow;
   `DeviceBusy` → friendly retry guidance in the error card.
4. **Stable device IDs**: persist device id (not name) in settings;
   `device_watcher` uses IDs for exact change detection.
5. Adopt `Send`-stream semantics to drop any remaining macOS-specific stream
   juggling.

## Non-goals

- Windows system-audio loopback capture (separate effort if ever).
- Any recording-pill UX redesign (059 touches outcomes, not chrome).

## Acceptance

1. All gates green (`pnpm check`, `cargo test`, clippy, fmt).
2. Unit tests: fake host (0.17 custom-host feature) driving StreamInvalidated →
   session survives with correct sample accounting.
3. Manual smoke matrix (extend SMOKE.md when claimed):
   - macOS: disconnect BT mic mid-recording → session continues on built-in mic,
     transcript intact, toast shown.
   - Windows: default-device change mid-recording → same behavior.
   - Both: deny mic permission → quick-fix card, not generic failure.
4. Telemetry: device-loss + recovery events through the existing
   GlitchTip/PostHog channels.

## STOP conditions

- If cpal 0.18 regression found in whisper-rs/resampler interplay on real
  hardware → freeze, document, revert pin; do not band-aid.
