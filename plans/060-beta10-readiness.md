# Plan 060 — Beta10 release remediation (silent-failures work migrated from PR 047)

**Status:** MERGED / RELEASED AS `v2.0.6-beta.10` — packaged smoke remains unchecked.
See `060-pr140-review-followup.md` for current findings and validation.
Release published 2026-09-14: https://github.com/ideaplexa/voicetypr/releases/tag/v2.0.6-beta.10
Smoke must now be run against that exact build, not against the PR head or older betas.

Renumbered from PR 140's `plans/047-silent-failures-soniox-alerts-diagnostics.md`
to free 047 for main's landed `polish-provider-ux-audio-fast-path.md` (via #136).
This file is now the beta10 remediation umbrella: the migrated silent-failures
plan below, plus the six concurrent beta10 remediation slices reviewed against
main `4f9e497d` + PR 140 `eace981b`. Upstream plans 044–059 are untouched.

Baseline: working tree 2026-08-20 (post v2.0.5). Trigger: paid-user bug report
(Windows, v2.0.5, 2026-08-19) — Soniox transcription permanently failing with
HTTP 429 `limit_exceeded` ("Total file count limit has been exceeded") and local
Whisper large-v3-turbo failing on CPU-only fallback; user saw only
"Transcription failed" / "Could not reach the transcription service", and
GlitchTip raised nothing because handled failures are curated *logs*, never
issues.

Corrected release contract (2026-09-05):

- Soniox transcription deletion does **not** cascade to the uploaded file.
  Delete each resource explicitly, without deleting files still needed by
  active or surviving records. Storage counts use provider count endpoints; cleanup listings are paginated.
- Retained-file capacity and retained-transcription capacity are separate.
  RPM, concurrency, pending-job, and unknown limits remain rate-limit errors;
  they never authorize automatic backlog deletion.
- File-management RPM is undocumented; cleanup uses paced requests and a
  bounded backoff. The existing REST architecture remains unchanged.
- The earlier dependency rider incorrectly removed `conventional-changelog-cli`:
  `scripts/release-separate.sh` still needs it. It is restored at 5.0.0 and
  invoked through `pnpm exec`; the Tauri CLI is pinned exactly to 2.6.2.

## A — Soniox storage lifecycle (P0, customer-blocked)

1. Typed and diarized terminal paths explicitly clean up the transcription
   and its uploaded file. A processing refusal or failed record deletion
   retains the referenced file; failed create operations clean up orphan uploads.
   Terminal cleanup runs in the background so it cannot delay transcript delivery.
   Interrupted flows release their active guard; only IDs created by this app
   session remain eligible for cleanup retries.
2. Classify only recognized retained-storage walls as
   `SttError::LimitExceeded { file_storage }`. Separate file and record
   deletion counters wake the relevant quota waiter; capture baselines before
   starting cleanup. Wait at most eight seconds, or until cleanup finishes,
   then retry the complete flow once.
3. Map a remaining storage wall to the existing actionable storage-limit
   error. Never expose raw provider response bodies.
4. Cleanup only deletes records proven to belong to the current app session,
   scoped to the configured API key. Unknown historical records and records
   from other apps/devices are untouched; the UI directs users to review those
   in the Soniox console. It also protects active uploads/jobs and shared file references.
   Re-list references under the coordination gate before the file pass.
   Missing or malformed reference metadata fails file deletion closed;
   documented `file_id: null` URL records remain valid. Report processed work,
   actual deletions, skipped items, and errors honestly.
5. HTTP regressions cover terminal cleanup, storage-vs-rate classification,
   shared/active references, between-pass changes, incomplete metadata, and
   record-capacity wakeup while another deletion keeps cleanup running.
   Flow fixtures serialize their shared production cleanup state.

## B — Telemetry: alertable failure events, funnel removal (P0)

Replaces 031's stage "curated logs + sampled traces" (031 owner approved this
pivot 2026-08-20): delete `log_transcription`/`TranscriptionPhase` funnel,
`TelemetryTransaction`/`TelemetrySpan`, sentry `logs` cargo feature,
`before_send_log`/`scrub_log` + their tests. Keep: panics, scrubbed frontend
error events, consent gates.

Add `capture_failure_event(message, tags)` (fixed strings only):

- `flow.transcription.failed` — sink `audio.rs:5416-5426` decode failure (incl.
  timeout via executor) with tags: engine, model, backend, failure_class,
  duration_ms.
- `flow.paste.failed` — `audio.rs:5865-5875` / `5916-5921` (engine captured
  pre-spawn at `5237-5238`).
- `flow.model_load.failed` — `whisper/cache.rs:99-102`.
- `flow.remote.failed` — `audio.rs:6023-6055`.
- backend tag: task-local state scoped to the recording's spawned future,
  recorded before backend initialization and updated on CPU fallback.
  Preloads, remote initialization, and earlier recordings cannot overwrite it;
  a failure with no selected backend omits the tag.

## C — Bug-report system specs swallow (P1)

`crashReport.ts:21-23` `.catch(() => undefined)` → surface "System specs
collection failed: <reason>" row + log. Verified NOT a missing-registration
issue (registered 2026-06-13, present in v2.0.5) — on the reporter's machine
the invoke rejected; today's code erases the evidence.

## D — DEBUG ring buffer (P1)

Always-on in-memory ring (~1000 entries, capped bytes) capturing `log::debug!`
in release (tauri-plugin-log filters file sink at Info) via a custom
Debug-level target if tauri-plugin-log v2 supports per-target levels, else a
wrapper logger at init. Ring dump redacted through `redact_log_content`
(`commands/logs.rs:275`) and auto-attached to every bug report. No user-facing
toggle. Also: fix rotated `.log.N` files never being cleaned
(`clear_old_logs`/`find_newest_log` only match bare names).

## E — Dependency cleanup rider (P2)

Retain only verified dependency removals. The release-script changelog dependency
is restored rather than assumed dead from a static dependency scan. CPAL
upgrades and other unrelated dependency work remain outside this remediation.

## Beta10 remediation slices (concurrent; Main gates after barrier)

Integration branch `fix/060-beta10-readiness` (worktree
`/tmp/voicetypr-beta10-integration`) = PR 140 `eace981b` merged with main
`4f9e497d`. Workers own disjoint files on their own branches; IntegrationOwner
cherry-picks only on Main's instruction. No validation mid-flight (Main runs the
consolidated gate); no remote pushes; user WIP (`agent/`, `videos/`,
`plans/032-*.md`) untouched.

| Slice | Owner / base | Scope (review-proven defects) | Acceptance |
|-------|--------------|-------------------------------|------------|
| 060.1 recorder/audio integration | IntegrationOwner, main after merge; `src-tauri/src/audio/recorder.rs`, `src-tauri/src/commands/audio.rs` | Final WAV drain branch bypasses speech-evidence observe (dropped/unwritten audio must not fabricate evidence; no second scan, no RT-callback allocations); no-speech gate must not delete recordings with only uncertain absence evidence; `stop_recording()` error must restore media via `MEDIA_CONTROLLER.resume_if_we_paused` before propagation (ESC/custom + normal-stop paths); Windows sidecar backend tag must record the actual attempt (pre-await) and CPU on fallback, attempt-local, no global stale attribution | Focused regressions per transition in owned test files; commit `fix: preserve final speech and restore media on cancellation errors` |
| 060.2 Soniox | SonioxFixes, PR HEAD `eace981b`; soniox.rs seam + its tests | Remaining Soniox silent-failure defects from the beta10 review (storage lifecycle / alertable-failure seam) | Focused fixes + regressions on their branch; validated at Main's consolidated gate |
| 060.3 Report diagnostics | DiagnosticsFixes, PR HEAD `eace981b`; ReportProblemSection + crash-report consumers | Remaining report/diagnostics defects from the beta10 review (specs swallow seam, DEBUG ring attachment) preserving extracted components | Focused fixes + regressions on their branch |
| 060.4 Release tooling | ReleaseToolingFixes, main `4f9e497d`; workflow + packaging files | Release-workflow/tooling defects from the beta10 review (42 fast-path, pins, caches) | Focused fixes + regressions on their branch |
| 060.5 Polish workflow | PolishFixes, main `4f9e497d`; polish seam files | Polish-workflow defects from the beta10 review; `classify_polish_outcome`, model selection, prefetch preserved | Focused fixes + regressions on their branch |
| 060.6 Frontend state | FrontendFixes, main `4f9e497d`; extracted hooks/state files | Frontend-state defects from the beta10 review; extracted hooks, PostHog journeys/consent preserved | Focused fixes + regressions on their branch |

Packaged smoke is tracked in `SMOKE.md` under 060-S1–S10, with license
preservation in 061-S1/S2. Existing 045-S1–S6, 050-S1–S3, 058-S1/S2 and
059-S1/S2 remain unverified; earlier beta observations are not proof for the
new candidate.

## Verification

Local macOS verification:

- `pnpm typecheck`, `pnpm lint`: pass.
- `pnpm exec vitest run --dir src`: 684 passed across 64 files.
- `pnpm build`: pass; existing large-chunk and future Vite config-loader
  warnings remain non-blocking.
- `cargo test --workspace --quiet`: 1,509 passed, 16 ignored.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- Scoped `rustfmt` ran only on changed Rust files, with `skip_children=true`.
- Executed CLI versions: Tauri 2.6.2; conventional-changelog 5.0.0.
- Independent reviews cleared the final integrated Soniox, license, and
  attempt-backend contracts after corrections. The integration compile gate
  caught and resolved incomplete merge hunks before acceptance.

No Windows runtime, real-account cleanup, consent/alert delivery, or published
beta smoke is claimed. A copied Swift module cache needed a local clean rebuild;
that was a build-cache relocation issue, not a product change.

Local native smoke: the development bundle built with
`com.ideaplexa.voicetypr.dev`, ad-hoc signing, and no notarization credentials.
It completed startup and loaded the cached Parakeet model. Its saved menubar
mode kept the dashboard hidden; the native automation tools could not resolve
that hidden window's AX surface and refused background input. Navigation was
not performed, no foreground escalation was attempted, and the owned process
was stopped. This is startup evidence only, not published-beta or UI-flow proof.
The inherited release signing identity was ambiguous locally; an explicit
ad-hoc identity resolved the development bundle without changing release code.

## NEEDS-SMOKE (after code freeze)

- Real Soniox key: dictation → transcription succeeds AND stored counts stay
  flat (delete fired); limit-429 shows honest message; cleanup button drains
  counts. macOS + Windows.
- GlitchTip: failure event arrives as issue (Discord alert fires) with correct
  tags; no logs/traces after removal; opt-out still fully inert.
- Bug report from release build contains System specs (or visible failure
  reason) and redacted DEBUG ring section.
