# Plan 065 — Windows customer recovery

Status: LOCAL FIXES VERIFIED / NEEDS-SMOKE — Amp 2026-09-19. Original license
decryption/loss cause remains unresolved; no release or customer recovery claimed.

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

## Implemented and checked

- Separate finite nonzero signal presence from the stricter sustained-speech
  latch. Quiet audio no longer produces a dead-mic warning; uncertain audio is
  transcribed rather than discarded after five minutes without signal. Ongoing
  quiet input resets the silence timers, including after sustained speech.
  Background noise can also keep recording active; signal presence is not a
  speech classification. Engine gates are unchanged.
- Windows cues call native Beep on a worker, preserving cue preferences and
  frequencies and logging native failures. No PowerShell process dependency.
  This removes an unobserved failure path, not proof of the customer's cause.
- Runtime license cache distinguishes a failed check from loading. Failed
  checks route the recording hotkey to License recovery; secure reads provide
  re-entry guidance and preserve data. Missing entitlement guidance includes
  existing-key activation rather than directing every user to purchase.
- Real AES-GCM failure fixture verifies preservation, explicit replacement and
  fresh-app readback from disk. DOM integration verifies check failure → existing
  key entry → Pro state, with no reset or purchase command. Activation API is
  mocked; this is not a real customer entitlement check.
- `cargo test --workspace`: 1,565 passed, 16 ignored; `cargo fmt --check` and
  workspace/all-target Clippy with warnings denied passed.
- `pnpm typecheck`, `pnpm lint`, `pnpm exec vitest run src`: passed, 732 tests.
  The unscoped Vitest command also discovered 67 failing suites in the existing
  untracked `agent/skills/` tree; it was not green and those files were untouched.
- Isolated Windows-target check of native Beep/thread code with windows 0.62.2
  passed. Full Windows app cross-check on this Mac is blocked by missing MSVC C
  headers in `ring`; physical Windows playback is unverified.

## Remaining evidence required

Windows encryption derives its key from the device ID; WMIC/CIM query the
hardware UUID while the final fallback queries a different registry MachineGuid.
A source change can therefore change key derivation, but the supplied customer
logs begin after encryption initialization and do not establish the original
source or distinguish wrong-key authentication failure from damaged ciphertext.
No crypto migration or speculative identity change was made.

Review reproduced two remaining quiet-input failures with failing regressions:
continuous quiet input stopped at five minutes, and soft input after louder
speech triggered a long-silence warning. Both timers now follow signal presence.
Recovery guidance also avoids promising that re-entry repairs a malformed store
file: that case still refuses writes to preserve the data and needs support.

For Wtin, use the existing-key activation path, then verify a fresh restart with
diagnostics. For Roger, collect activation-success and subsequent restart evidence;
his log establishes trial fallback, not the event that lost the license. Do not
ask either customer to reset/delete secure.dat or purchase again. Keep these
reports open until customer recovery and the Windows smoke rows are confirmed.
