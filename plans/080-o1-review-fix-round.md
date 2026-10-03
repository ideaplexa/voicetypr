# Plan 080 O1 — observability review fix round

All nine review findings are patched in the working tree. No commit, push or
release was performed. This sandbox did not build Rust.

| Finding | Fix | Rust regression test (not run here) |
|---|---|---|
| 1. SDK enrichment drops completions | `src-tauri/src/product_analytics.rs:702` strips exactly the pinned V0 SDK enrichment keys before schema validation and rebuilds the payload. | `product_analytics.rs:1204`, `sdk_batch_enrichment_and_revocation_use_real_worker`: real SDK client/worker → enrichment → hooks → mock HTTP; checks completion plus recording.started and onboarding.completed survive with no enrichment/internal epoch on the wire. |
| 2. Concurrent save undoes opt-out | `commands/settings.rs:57` runs consent reads, migration, writes, persistence and failure reload under the same async mutex as `commands/telemetry.rs:6`. Consent status/settings reads also share the mutex. | `settings.rs:1954`, `generic_save_waits_for_opt_out_transaction_and_preserves_it`: pauses the dedicated transaction, polls the real generic persistence helper, writes opt-out and verifies the final persisted choice and deleted IDs. |
| 3. Already accepted batches escape revocation | `product_analytics.rs:465` advances the epoch and synchronously drains the SDK with the gate off. Every event/exception already carries its acceptance epoch. No retries. | `sdk_batch_enrichment_and_revocation_use_real_worker` tests queued events/exceptions and a late enqueue after re-enable; `product_analytics.rs:1284`, `revocation_waits_for_prepared_batch_and_drops_the_remaining_queue` pauses the worker after scrubbing and verifies synchronous revoke waits, one residual request is permitted and subsequent queued items are dropped. |
| 4. Provider response bodies in logs | `cloud_stt/common.rs:269` logs closed provider, HTTP status and classified code only. WebSocket connect/read failures in deepgram_ws.rs and soniox_ws.rs no longer format tungstenite errors, which can contain HTTP upgrade response bodies. Logging calls in cloud_stt and ai were scanned. | `common.rs:1112`, `http_log_contains_only_closed_provider_status_and_classification`; source regression scans every logging call in both modules and rejects response-body variables and raw WebSocket response errors. Existing Soniox quota-body tests retain classification coverage. |
| 5. Recovery loses trace after ring eviction | `observability.rs:15` separates reference-counted outstanding mappings from the five-entry ring. Completion work, clips and retries pin mappings; retry aliases preserve original mappings. `recording/kept.rs:862` resolves from the clip's saved trace. Failed retries retain their alias pin. | `observability.rs:405`, `outstanding_trace_survives_ring_eviction_and_retry_alias`; `kept.rs:910`, `recovery_resolution_uses_saved_uuid_after_mapping_eviction`. |
| 6. Late stages attach to next take | `commands/audio.rs:1160` and `:1205` capture guard generations; recording start/stop, formatting, Polish, async notices and paste exceptions use captured generations. Silence callbacks also retain their generation. `writing/pipeline.rs:502` → `commands/ai.rs:1447` threads that generation through Polish, including skips and exceptions. File/recovery processing passes its captured upload generation. | `audio.rs:1258`, `late_cancelled_stages_keep_the_captured_take_after_next_start` observes both production guard drop paths after starting B; source regression checks both writing callers, guard producers and Polish helper threading; `commands/island_notice.rs`, `late_notice_keeps_its_original_generation`, observes a late production notice with the original UUID. |
| 7. Discarded retries become failures | `recording/kept.rs` distinguishes RetryError::Cancelled and uses one shared atomic terminal claim for retry/discard/expiry. Only retried_failed emits retry_failed. Older lease cleanup cannot release a newer attempt of the same clip. | `kept.rs:930`, `discarded_or_expired_retry_has_one_terminal_and_no_failure`; saved-UUID resolution test also checks that a second terminal does not emit. |
| 8. Peek revocation race | `observability.rs:328` stamps the counter with consent epoch and validates on increment/flush; capture_at_epoch rechecks before accepting the flushed count, retaining that epoch through dispatch. | `observability.rs:438`, `peek_counter_rejects_revocation_races_on_increment_and_flush`, covering same-day accumulation, revocation, opt-in and day rollover. |
| 9. Crash file retains source paths | `telemetry.rs:508` builds only sanitized crate-relative file:line (or unknown) and UTC time. lib.rs uses it instead of debugging PanicHookInfo. | `telemetry.rs:715`, `crash_file_has_only_relative_location_and_time`; source regression rejects the old debug dump in the actual panic hook. |

## Observed gates

- `src/lib/observability-review.test.ts`: all nine source integration regressions
  failed against an isolated `git archive HEAD` baseline and passed with the
  patch. Production files were not replaced during this check.
- `pnpm typecheck && pnpm lint && pnpm exec vitest run && pnpm build`: passed;
  103 test files, 1,249 tests. Existing Vite native-loader/chunk-size warnings remain.
- `cd src-tauri && cargo fmt --check`: passed.
- `git diff --check`: passed.
- Rust compilation, cargo test and clippy: **NOT RUN**, explicitly assigned to
  Claude. Rust runtime red/green is not claimed by the source regression results.

## Consent residual window

Pinned posthog-rs 0.22.0 has no pre-HTTP dispatch hook. The permitted fallback
synchronously flushes after closing consent and advancing the epoch. At most
one worker batch (20 events maximum) already past before_send can still send,
even if HTTP has not begun: serialization/compression and request setup lie
between the last epoch check and dispatch. That single request has a 5-second
HTTP timeout and no retry. Revoke waits for its attempt and the queue drain;
5 seconds is not a promise about total revoke latency. Details are also in
docs/OBSERVABILITY.md.

NEEDS-SMOKE: release PostHog ingestion; real opt-out/on with queued/prepared
batches; real-speech trace continuity across late cancellation, >5 takes,
kept clips and retries; discard/expiry during a pending retry; one-day/session
peek flushing; a real panic crash file and coded panic delivery. Existing
native focus/device/runtime gates remain open.
