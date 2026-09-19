# 068 — Whisper cancellation callback correctness (#116)

IN PROGRESS — claimed Amp 2026-09-19.

## Evidence and scope

- whisper-rs 0.16.0 boxes `Box<dyn FnMut() -> bool>` but casts its address
  back to the original closure type in `set_abort_callback_safe`: undefined
  behavior and a leaked allocation. The desktop uses a captured cancellation token.
- An external real-engine harness using Base English, 53,931 samples, 11 threads
  and an `Arc<AtomicBool>` closure returned `GenericError(-6)` with cancellation
  false on both Metal and CPU. Correct raw callback wiring transcribed the same
  audio; true cancellation returned -6; the cached context then worked again.
- This reproduces a mechanism matching #116, not the customer's exact machine.
  Historical `WHISPER_INPUT` energy/peak values were hard-coded zeros, not evidence
  of silence. No model upgrade, CPU recovery policy, or silence-gate changes.

## Implementation and verification

Keep callback storage alive with the correct concrete type for the entire
synchronous native inference call; do not use the broken dependency helper.
Add a real-engine regression that exercises the captured token with cancellation
false, true during encoder execution, and false again on a reused context, on
Metal and CPU. Check captured ownership is released. Run focused/full Rust tests,
format and Clippy checks. Keep packaged desktop and original-customer verification
unchecked in `SMOKE.md`; this is a beta.11 candidate, not a release.
