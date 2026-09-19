# Plan 066 — Stable Windows license identity

Status: LOCAL CHECKS PASSED / NEEDS-SMOKE — Amp 2026-09-19. Follow-up to 065,
not a claim that customer recovery or Windows runtime smoke has passed.

## Evidence

Device discovery is repeated for storage initialization, activation, validation
and reports. WMIC/CIM return a hardware UUID; the registry fallback returns a
different MachineGuid. AES derives its key from the first startup result, while
API calls rediscover it. The supplied report establishes registry fallback and
a 35-second lookup, but does not establish the customer's historical identity.
Reader-thread joins can outlive the nominal command timeout.

## Implementation

- Freeze one API identity per process and persist Windows identity/key-derivation
  inputs using current-user DPAPI, not plaintext. Keep secure.dat's AES format.
- Authenticate available legacy candidates per entry. Prefer the candidate that
  decrypts the license, retain other authenticated candidates for mixed-key stores.
- Never overwrite unreadable identity metadata or credentials. If the license
  cannot authenticate, require explicit activation before pinning a new identity.
- Bootstrap the desktop after single-instance setup, before secure-store consumers;
  the CLI loads the same protected identity. Publish state only after required
  persistence succeeds; no network entitlement bypass. Keep the identity across
  app resets, so resetting settings does not create a new trial identity.
- Bound process-tree execution and pipe collection with one deadline.

## Verification

Synthetic identities only; no customer keys submitted or stored in fixtures.

- `cargo test --workspace`: 1,576 passed, 16 ignored, zero failures on macOS ARM.
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and
  `git diff --check`: passed.
- Authenticated AES fixtures reproduce failure when hardware and registry hashes
  differ, then recover the registry-encrypted license with hardware discovery
  available. A separate test process reloads the protected record without discovery
  and decrypts the original, byte-preserved secure-store entry.
- Tests cover mixed legacy keys, unavailable original key, malformed/unsupported
  metadata, malformed store, failed protection and create-only commit, invalid UUID
  output, registry collection even when hardware succeeds, and pipe descendants
  outliving their parent. Timeout execution is exercised on Unix, not Windows.
- Exact Windows identity/discovery modules, including tests and DPAPI bindings,
  pass isolated `cargo check --target x86_64-pc-windows-msvc --tests` on this host.
  That harness stubs parent secure-store helpers: it is not a full Windows app
  build or a Windows runtime test. Portable persistence tests use an authenticated
  test envelope on macOS; their Windows configuration uses real DPAPI.

Real Windows DPAPI, direct/Store packaging, license-server activation and restart,
CLI/desktop coexistence, and Windows process-tree deadlines remain unchecked in
`SMOKE.md` (066-S1–S5). Earlier PR142 CI does not cover this local follow-up.
The source-switch mechanism is proven; the customers' historical identities are
not. Recovery requires an available matching candidate and cannot restore entries
already deleted by an older build. An unreadable DPAPI pin fails closed and needs
support rather than silent replacement. The user authorized pushing, reviewing
and merging this follow-up into PR142 on 2026-09-19, targeting a combined
`2.0.6-beta.11`. PR142 tracks published-head CI and merge status; earlier CI is
not evidence for a later head. No release is implied by merge, and affected
Windows smoke remains required.
