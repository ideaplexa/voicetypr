# Plan 066 — Stable Windows license identity

Status: IN PROGRESS — Amp 2026-09-19. Follow-up to 065, not a claim that
customer recovery or Windows runtime smoke has passed.

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
- Bootstrap after single-instance setup, before secure-store consumers. Publish
  state only after required persistence succeeds; no network entitlement bypass.
- Bound process-tree execution and pipe collection with one deadline.

## Verification

Synthetic identities only. Test source changes, restart using persisted identity
without discovery, mixed legacy keys, no matching key, malformed metadata/store,
write failure, and command descendants retaining pipes. Real Windows DPAPI,
activation and restart require packaged smoke before any new beta release.
Do not merge or release PR142 as part of this work.
