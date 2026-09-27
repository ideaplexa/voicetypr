# Plan 058 — Media pause v2: upgrade existing controller to VoiceInk-grade

**Status:** TODO
**Priority:** P1
**Effort:** M
**Depends on:** — (057 not required; independent)

## Problem

A media pause controller ALREADY EXISTS and is wired end-to-end
(`src-tauri/src/media/controller.rs`, hooks at audio.rs:4176-4188 start /
4799 stop / 7469 cancel / 4519+4562 early-cleanup, settings gate at start).
It is partially naive:

### macOS today (controller.rs:18-129, 216-292)
- Now-playing check: JXA via `/usr/bin/osascript` dlopening the private
  MediaRemote framework directly (`MRNowPlayingRequest.localIsPlaying`).
  **Known fragility**: macOS 14.4+ restricts mediaremoted XPC to entitled
  callers — when this fails the snapshot returns `None` → `is_playing=false`
  → feature silently no-ops (consistent with user report that VoiceInk's
  pause works while ours does not). The VoiceInk ecosystem solved this with an
  **entitled Perl bridge** (`ejbills/mediaremote-adapter`, vendorable ~2-file
  approach) because `/usr/bin/perl` retains the entitlement.
- Pause/resume: `tell application "System Events" to key code 100` (F8 toggle)
  — requires a **System Events Automation permission** prompt; no resume
  delay; no per-app verification.
- Resume (controller.rs:247-264): re-checks now-playing, toggles F8. No
  same-app check, no app-still-running check.

### Windows today (controller.rs:299-624)
- Proper GSMTC/SMTC with explicit `TryPauseAsync`/`TryPlayAsync` (no toggle).
- Gaps: session ledger is a single `was_playing_before_recording: AtomicBool`
  (controller.rs:132-139) — cannot remember WHICH sessions we paused vs the
  user's own manual pauses; MSIX/Store build may lack the required
  `globalMediaControl` manifest capability (verify; NSIS unaffected).

### Missing product surface (both OS)
- No resume delay (VoiceInk ships 0–5s).
- No mute fallback for apps that ignore media keys (VoiceInk's
  `kAudioDevicePropertyMute` path with was-muted-before bookkeeping +
  generation counter).
- No telemetry on pause/resume outcomes.

## Verified reference design (VoiceInk source, read 2026-08-21)

`Beingpax/VoiceInk/PlaybackController.swift` + `MediaController.swift`:
pause via MediaRemote (perl bridge) only when `isPlaying == true`; resume
after delay only if original app still running, still the current
now-playing app, and still paused; resume uses the **NX_KEYTYPE_PLAY HID
event** (F8) because some apps (Plexamp) ignore MediaRemote `play`; mute
fallback with generation counter; resume-task cancellation on re-record.

## Scope (upgrade in place, keep the module + hooks)

1. **macOS state layer**: replace the JXA direct-dlopen with the perl-bridge
   now-playing query (vendor adapter's run.pl; JSON stdout protocol). Keep the
   JXA path as first-choice fallback chain: perl → JXA → assume-not-playing.
2. **macOS action layer**: replace System Events AppleScript with CGEvent
   synthesis of the media play/pause key (NX system-defined event, keycode 16)
   via our **existing enigo dependency** — the app already holds the
   Accessibility trust enigo needs for text insertion; no new permission
   prompt. System Events route becomes last-resort fallback.
3. **Bookkeeping**: per-recording ledger { bundle_id / session ids, was_paused,
    generation }; resume only what we paused; verify still-paused +
    app-still-running; cancel pending resume on re-record (generation counter).
4. **Resume delay setting** (0–5s, default 0) + **mute fallback toggle**
   (CoreAudio `kAudioDevicePropertyMute`, was-muted-before bookkeeping).
5. **Windows**: session-id ledger instead of single bool; verify/add
   `globalMediaControl` capability to the Store manifest.
6. Telemetry: pause attempted/paused/resumed/failed per mechanism (no app
   names — privacy).

## Non-goals

- System-audio capture (057 scope note), Linux, per-app allowlists.

## Acceptance

1. Gates green; unit tests for the ledger + generation state machine (fake
   clock, concurrent recording generations).
2. Smoke (SMOKE.md when claimed):
   - macOS: Spotify + Music + YouTube in Safari/Chrome pause on record, resume
     after configured delay; nothing playing → no-op (never starts playback);
     user-paused-before stays paused; cancel path resumes; re-record within
     delay cancels pending resume.
   - Windows: same matrix; Store MSIX build exercises SMTC successfully.
3. Failure-mode smoke: kill the perl bridge path (rename binary) → JXA
   fallback engages, feature still functions or cleanly no-ops (never starts
   playback on resume).

## STOP conditions

- If the perl bridge AND JXA both fail on current macOS: ship mute fallback +
  honest UI copy; do not chase other private-API paths.
