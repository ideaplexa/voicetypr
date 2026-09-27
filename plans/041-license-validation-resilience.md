# 041 — Paid-license validation resilience

**Status: DONE — independent reviewer PASS 2026-07-24**

## Problem

A paid user successfully activates Voicetypr, then later sees `Trial expired` and must enter the same license key again. A transient network or server failure must not erase or downgrade a paid entitlement, and must never be presented as a definitive trial/license expiration.

## Locked behavior

1. Only a definitive entitlement response from the licensing service may mark a paid license invalid or expired.
2. Transport errors, timeouts, and server-unavailable responses keep the last verified paid entitlement through its existing offline-grace contract.
3. Validation uses bounded retries and records consecutive scheduled verification failures without retrying indefinitely.
4. Repeated transient failures escalate to a verification warning, not `Trial expired`; the UI offers an explicit revalidation action.
5. Activation and validation never delete a stored paid key solely because the service was unreachable.

## Acceptance

- A paid cached license remains usable through transient validation failures.
- A fresh paid activation survives restart and a simulated unavailable licensing service.
- Three consecutive scheduled verification failures produce a truthful verification warning while preserving entitlement.
- A definitive invalid/expired server response still revokes paid status immediately.
- Focused Rust and frontend behavioral tests cover the state transitions.
- No license key, device identifier, or raw server response is added to logs or telemetry.

## Implementation

- Startup clears only the short-lived status cache; the last successful validation timestamp now survives restarts.
- License validation retries transport, malformed-response, `408`, `429`, and `5xx` failures up to three times with bounded backoff. Other `4xx` responses are not retried and retain a structured rejection outcome so definitive revocations cannot be mistaken for outages.
- A stored key preserves `Licensed` status through the exact persisted 90-day grace deadline. Verified and offline runtime states both require online revalidation at that boundary; failures 1–2 use `offline_grace`, while failure 3+ uses `needs_revalidation`.
- The 2.0.4 migration path seeds grace once from a stored key. A durable initialization marker prevents cache invalidation or failed keychain deletion from replaying that migration.
- Explicit invalid/not-found/expired/revoked responses delete the stored key whether returned as a successful validation payload or a non-retryable `4xx`. Device mismatch attempts a five-second, server-enforced activation repair and otherwise preserves bounded offline access.
- Account UI keeps `Pro Licensed` visible, explains that the license has not expired, and keeps revalidation reachable from both verified and warning states. License commands use a 60-second UI deadline.
- Activation no longer transmits the machine hostname.
- A shared async lock serializes checks, revalidation, activation, restoration, and deactivation so cache and failure-counter transitions cannot race. Transient restore also refreshes the runtime recording gate.

## Validation

- `cargo test license:: --lib` — 9 passed.
- `cargo test` — 1,240 passed, 7 ignored.
- `cargo test recording_license_state_requires_verification_after_offline_deadline --lib` — 1 passed.
- `cargo clippy --lib -- -D warnings` — passed.
- `cargo check --lib` — passed.
- Touched Rust files formatted with `rustfmt`.
- `LicenseContext.test.tsx` + `AccountSection.test.tsx` — 4 passed.
- `pnpm test` — 584 passed, 1 skipped.
- `pnpm typecheck` — passed.
- `pnpm lint` — passed.
- Independent final current-source review — **PASS**, no remaining P0/P1/P2 findings after the public cache-invalidation race was corrected.
- Browser smoke rendered the real Account section in verified state, confirmed `Revalidate License` was reachable, exercised it, and observed the non-expiry `needs_revalidation` warning with `Revalidate now`.
