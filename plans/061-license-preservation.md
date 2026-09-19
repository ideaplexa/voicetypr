# Plan 061 — License preservation — non-destructive secure-store reads

**Status:** MERGED / RELEASED AS `v2.0.6-beta.10` — published 2026-09-15;
PR #140 follow-up verified 2026-09-06; packaged 061-S1/S2 remain unverified.
**Priority:** P0
**Effort:** S
**Depends on:** 060 (release remediation wave; independent file ownership)

## Problem (incident, Windows 2.0.5 — reproduced on main `4f9e497d`)

At startup the stored license decrypt failed and `secure_get`
(`src-tauri/src/secure_store.rs:145-195`) **deleted the saved record** and
returned `Ok(None)`. Consequence chain:

1. `check_license_status_impl` (`commands/license.rs:441`) only enters the paid
   branch on `Some`; `None` falls into the trial branch → server trial check →
   expired trial → recording blocked.
2. The saved entry was removed from the local store. This fix cannot restore
   already-deleted bytes; recovery through backups or support is separate.
3. The same deletion applies to corrupt **API-key** entries (cloud STT / AI
   providers), silently discarding recoverable credentials.

Root cause of the decrypt failure itself is **not established** (wrong AES key
derivation vs. on-disk corruption — GCM auth failure is identical for both).
Parent evidence: injected decrypt failure with a synthetic temp-backed store →
`secure_get` returned `Ok(None)` twice, persisted entry gone, one decrypt call.

## Fix (non-destructive read; no crypto or device-identity redesign)

`secure_store.rs` (plus Tauri test support). Generic secure read distinguishes three outcomes that
today all collapse to "absent":

| Situation | Old behavior | New behavior |
|---|---|---|
| Key absent | `Ok(None)` | `Ok(None)` (unchanged; trial flow stays normal) |
| Store file unreadable/corrupt (whole file) | `Ok(None)` (store build silently drops the load error) | `Err("Secure store file could not be read: …")` |
| Value present, decrypt fails (auth/length/UTF-8) | **delete + save + `Ok(None)`** | `Err("… could not be decrypted … the saved entry was preserved")`, **no mutation** |
| Value present, wrong JSON type | **delete + `Ok(None)`** | `Err("… unexpected format …")`, **no mutation** |
| Store path cannot be resolved | Store access failure could collapse to absent | Distinct error; no mutation |

Mechanics (revision 3): `secure_get` is strictly read-only and **never
registers the store**. `app.store()` builds+registers a store whose cache is
empty when the disk file is malformed (plugin `build_inner` discards load
errors), and the plugin saves every registered store on app exit — that empty
cache would overwrite the (possibly recoverable) file. So:

1. Serve cache hits from an already-open store via side-effect-free
   `get_store` (sees unsaved in-flight `secure_set` values; no disk IO).
2. Store not open: resolve the path with the plugin's `resolve_store_path`
   and read the file directly (`fs::read` + `serde_json` map parse):
   missing → `Ok(None)` (fresh install, nothing registered/created); IO
   error or bad JSON → distinct Err with the store never opened. No
   `Store::reload` anywhere — reads cannot clobber a concurrent `secure_set`
   before its save.
3. Once a store IS open, its cache is authoritative: a miss is a real
   absence and is **never backfilled from disk**, so an in-flight
   `secure_delete` (cache deleted, save not yet landed) cannot briefly
   resurrect the old credential; a failed `secure_delete` save leaves disk
   untouched and the caller already received the error.
4. Parse failures never interpolate `serde_json::Error` into user-visible
   messages (its `invalid type` text can embed a scalar payload, e.g. a
   mistakenly pasted key); only classification + line/column are logged and
   the returned message is payload-free.

Reads never call `delete`/`save`; `secure_delete`, deactivation, and Reset
keep their explicit user-action semantics (and `secure_delete`'s
`"Failed to access store"` error prefix, which `reset.rs:101` matches on).
Before `secure_set` or `secure_delete` registers a store, it validates the
whole file. Malformed files reject the write without registration, so plugin
exit cannot replace them with an empty cache. An already-open valid cache
remains authoritative. `secure_set` can still replace an unreadable individual
entry in a valid JSON store: re-entering that value is the recovery path. `check_migration_needed`
(dead code) also switched to a read-only existence check.

Startup UI audit: on the storage error, `useAppReadiness.isLoading` stays true
(`licenseStatus === null`) but **no component consumes `isLoading` as a
blocker** — settings/Account navigation is independent. Recovery is reachable:
AccountSection exposes `checkStatus`/`revalidateLicense`/`activateLicense`
(re-entering the key overwrites the corrupt record) and Reset removes the
file. No frontend change required.

Consumer audit (all `secure_get`/`secure_has` callers, no changes needed):
`license/keychain.rs` (`?` propagates → `check_license_status`/`restore`/
`deactivate`/`activate` read-back), `commands/ai.rs` warm cache (Err arm logs,
skips), `cloud_stt/mod.rs` (`?` → transcription error), `commands/remote.rs`
(explicit Err arm fails closed), `transcription/executor.rs` (missing-key
error), `commands/keyring.rs` (honest error to settings UI), `audio.rs`/
`model.rs`/`tray.rs`/`model_selection.rs` (`unwrap_or(false)` fail-safe).
Frontend: `LicenseContext.checkStatus` on command error toasts the message and
keeps `status = null` → app-not-ready; **no falsely expired / no-license state**,
so no frontend change. Error messages carry key names and plugin error text
only — never ciphertext, plaintext keys, or device fingerprints.

## Tests (`secure_store.rs`, real temp-backed store files + real AES-GCM)

`read_store_file` / `decrypt_raw_entry` are exercised against actual files in
`tempfile` dirs (the read path takes a `&Path`/`&Value`, so no app handle or
fake trait is needed; preservation is asserted by comparing file bytes):

1. `tamper_is_a_valid_length_gcm_auth_failure_not_a_decode_error` — flipped
   ciphertext byte → GCM auth failure ("Decryption failed"), the incident
   signature.
2. `missing_store_file_reads_as_absent_fresh_install` → `Ok(None)`.
3. `unreadable_store_file_is_distinct_from_missing_entry` — malformed JSON →
   Err containing "Secure store", not absent.
4. `corrupt_value_read_preserves_saved_record_and_reports_error` → distinct
   Err, file bytes unchanged.
5. `repeat_read_keeps_preserved_record_and_same_error` → same Err twice, file
   bytes unchanged.
6. `invalid_value_type_preserves_saved_record` → Err, file bytes unchanged.
7. `valid_license_roundtrip_reads_back_from_file` (+ absent key in a valid
   file → `Ok(None)`).
8. A malformed top-level scalar cannot disclose its synthetic secret through
   the returned parse error; logging contains only category and position.

`secure_get` itself is thin glue over these (side-effect-free `get_store` +
`resolve_store_path`); the no-registration/no-write guarantee is structural —
the read path has no `Store` write surface at all.

Local macOS workspace verification: 1,509 Rust tests passed, 16 ignored;
Clippy with warnings denied passed. Independent security review cleared the
integrated read path and error redaction. The 2026-09-06 follow-up adds MockRuntime/AppHandle tests invoking the actual
store plugin exit callback: malformed JSON/null/array files reject reads and
writes, remain unregistered and byte-identical after exit; valid stores preserve
unreadable licenses across unrelated writes and permit explicit replacement;
a cache deletion remains authoritative. Windows identity changes and packaged
exit/relaunch behavior remain unchecked in `SMOKE.md`.

## Out of scope / explicit non-goals

- No `device.rs`, PBKDF2, or device-hash changes; no new key-migration scheme.
- No entitlement bypass or auto-activation; unreadable ≠ licensed.
- Server-revocation behavior (`should_delete_invalid_license`) untouched.
- Already-deleted licenses are not restored by this fix.
- The initiating decryption failure remains unexplained. Before any recovery
  write, preserve `secure.dat` and relevant logs. Do not reset app data, delete
  the file, deactivate, or repeatedly activate. Support should verify the
  entitlement and coordinate recovery with the original license key.
- Retrying a read in the fixed build preserves the evidence; explicit writes
  can replace unreadable data and are not covered by the read-only guarantee.

Follow-up validation: 1,525 Rust workspace tests passed (16 ignored); Clippy
workspace/all-targets passes with warnings denied. See `060-pr140-review-followup.md`.
