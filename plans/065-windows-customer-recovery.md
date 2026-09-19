# Plan 065 — Windows customer recovery

Status: IN PROGRESS — claimed Amp 2026-09-19.

## Evidence and scope

- Windows 2.0.5: three successful dictations with capture RMS 0.0029–0.0040,
  no sustained-speech latch, positive prepared speech evidence, and successful
  paste. Customer reports false no-audio warnings and missing beeps.
- Windows beta.10: repeated license decrypt failures preserve the entry but
  leave runtime status uninitialized; recording incorrectly says still loading.
- Windows 2.0.5: paid customer reaches expired-trial path; the supplied log does
  not establish how the stored license became absent.

Fix confirmed failure paths without changing punctuation or granting unverified
licenses. Preserve unreadable secure-store data. Investigate device-derived key
compatibility before proposing recovery; never guess customer keys or delete
their settings. Playback cause remains unconfirmed until Windows reproduction.

## Verification

Add focused regression tests for quiet-input warning policy and failed-license
readiness/recovery. Run backend tests, formatting and Clippy, plus frontend checks
if UI behavior changes. Windows playback, actual customer license recovery and
packaged runtime verification remain explicit smoke requirements in SMOKE.md.
Any released product changes require beta.11 or later and affected smoke; this
plan does not authorize a release, push, or production data changes.
