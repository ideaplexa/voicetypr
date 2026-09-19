# 068 — Whisper cancellation callback correctness (#116)

LOCAL CHECKS PASSED / NEEDS-SMOKE — Amp 2026-09-19. Not released.

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

Callback storage now retains its concrete type for the entire synchronous native
inference call without the broken helper's leaked boxes. No retry policy changed.

- The repository regression failed before the fix with `GenericError(-6)` while
  its actual `CancellationToken` was false. After the fix, Base English succeeds
  with cancellation false, returns -6 when the native abort poll reads true, and
  succeeds again on the same context. Both Metal (Apple M4 Pro) and CPU passed;
  captured Arc ownership returns to one after each inference, including errors.
- Fixture: existing `test-audio.wav`, first 53,931 mono 16kHz samples; 11 threads.
  Model: ggerganov/whisper.cpp `ggml-base.en.bin`, SHA256
  `a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002`.
- Repeat from `src-tauri` with an existing model (no auto-download):
  `VOICETYPR_TEST_WHISPER_MODEL=/path/to/base.en.bin cargo test --lib real_engine_captured_cancellation_and_context_reuse -- --ignored --nocapture`.
- `cargo test --workspace`: 1,577 passed, 17 ignored (the new real-engine test
  above was executed separately). `cargo fmt --all --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` pass.
- Packaged desktop cancellation/insertion and the original customer's environment
  remain unchecked in `SMOKE.md` (068-S1/S2). Issue #116 stays open pending rollout
  verification; the reproduced defect does not establish every possible -6 cause.
