# Observability (plan 080 O1)

PostHog Cloud EU is the only telemetry destination. The release build embeds
the existing POSTHOG_PROJECT_TOKEN ingestion token. Debug builds create no
client, install no remote panic hook, and send nothing. This is not a frontend
SDK: there is no autocapture, identify, replay, session recording, breadcrumb,
request capture, AI prompt capture, or operational log upload.

The pinned posthog-rs 0.22.0 supports manual exceptions with error-tracking
(minimum 0.12.0) and opt-in panic capture with init_global (minimum 0.15.0).
We enable the feature on the existing version and use one global client.
Sources: the installed crate's CHANGELOG.md and
[official Rust SDK](https://github.com/PostHog/posthog-rs/tree/v0.22.0).

## Consent and privacy

The one pre-ticked consent reads:

> Share anonymous crash reports and usage numbers — never your words, audio or app content.

“What’s shared” explains device categories, choices, outcomes, speed/count
numbers, coded crash locations, and random IDs. Settings → Privacy uses the
same choice. Declining or deferring closes the shared gate immediately.
Re-enabling works in the same session; no restart is required.

Persistence uses telemetry_enabled and telemetry_install_id, plus
privacy_consent_version=2 for acknowledgement. Migration reads the old
analytics_enabled and crash preference (formerly also telemetry_enabled;
crash_reporting_enabled is recognized too). If either is explicitly false,
sharing stays off. An acknowledged new choice can enable both. Saves delete
analytics_enabled, analytics_install_id and crash_reporting_enabled. A valid
old PostHog UUID is preferred to the former crash UUID; opt-out deletes IDs.
Generic settings saves migrate stored consent, preserving opt-outs and never
acknowledging on behalf of the user or applying a stale form's consent value.

Never send words, audio, clipboard, prompts, provider response bodies, keys,
file paths, app names, window titles, email, URLs, IPs, or CPU/device brands.
App category is an enum. Model IDs must match the maintained model catalogue
or fixed buckets. Language codes come from the closed language catalogue;
unknown codes become other. Durations are bounded. Installation and dictation
UUIDs are random v4 IDs, never content-derived. Dictation IDs are properties,
not person IDs. Person profiles and GeoIP enrichment are disabled.

The queue rechecks consent and its epoch before sending. A request already
handed to HTTP can finish after revocation; retries are disabled. An opt-out
invalidates pending events and clears the pending peek count.

Exceptions pass through telemetry::capture_error(code, context), at most
three captures per code per process. The helper supplies only AppError with
a fixed code/value, fingerprint, level, closed provider/code context and the
dictation UUID. The before_send scrubber rebuilds the entire event. It drops
exception chains, source context, locals, debug images and absolute paths.
Resolved function names and crate-relative source locations remain.
Dependency frames are marked out of app using in_app_exclude_paths.
Release line tables and strip="none" remain enabled.

Panic payloads never enter our coded event or local logs. The SDK's opt-in
hook is enabled; a chained coded hook snapshots consent and trace ID, supplies
only panic plus the compiler location, then invokes the SDK's bounded flush.
The unstamped SDK duplicate is dropped, so it cannot bypass consent epochs.
A fixed local panic notice replaces the default payload-printing hook.

## Correlation

Recording generation maps to one random dictation_id. Starts from hotkeys,
pointer or tray share this implementation. Completion snapshots the generation
before awaiting work; island and recovery producers pass their generation.
Recovery retries inherit the original UUID. The in-memory ring retains the
last five IDs for support and never persists them.

A completion describes the original take. Recovery may still be pending:
recovery_resolution=none in that completion is expected. Join the subsequent
recovery_resolved event on dictation_id for the final disposition.

## Event catalogue

Base fields: app_version, release_channel, os, arch, disabled person-profile
and GeoIP flags. The internal consent epoch is removed before egress.
Events belonging to a take include dictation_id when a trace exists.

| Name | Properties | When | Why |
|---|---|---|---|
| app_started | version, os_version (numeric components or unknown), cpu_class (small/medium/large/unknown), engine_ready, pill_mode | Once after native setup and recognition availability snapshot | Version/device/readiness cohorts |
| app_updated | from, to (validated app versions) | Startup detects a changed stored version | Compare upgrade behavior |
| onboarding.completed | Base fields | Completion durably saved | Activation |
| recording.started | dictation_id | Capture starts | Dictation funnel |
| recording.stopped | duration_bucket, dictation_id | Capture stops | Funnel/drop-off |
| transcription.stage_finished | stage, outcome, duration_bucket, engine?, dictation_id | Decode/formatting/delivery terminates | Identify the failing stage |
| polish.finished | outcome, attempted, preset, provider, model, dictation_id | Polish terminates | Formatting adoption/fallback |
| dictation_completed | outcome, engine, model, transport, live_preview, recording_ms, first-audio and stop-to-text latency, post-roll booleans, words_bucket, polish, paste, app_category; start_source, mode, start_card_shown, island_start_details, language, polish_skip_used, polish_guard_reason, polish_keep_words, polish_style, recovery_kind, recovery_resolution, focus_safe; dictation_id | Exactly once for a stopped/cancelled desktop take, including early failures | End-to-end outcome and latency |
| dictation_blocked | kind, dictation_id? | Native start/delivery blocker presented | Know why the user could not proceed |
| island_action | action, source (island/tray), dictation_id? | An island action is attempted or a shared tray action succeeds | Actions taken to recover or dictate |
| island_state | state, dictation_id? | Note, notice, too-short, Escape hint or recovery is presented | Understand new visible states without content |
| recovery_resolved | kind, resolution, alt_engine_kind, latency_ms, dictation_id | Retry completes, transcribe-anyway succeeds, discard or expiry | Recovery effectiveness |
| quick_setting_changed | setting (polish/engine/mic/language/mode/live_preview), source (island/tray/app) | Successful choice/save | Discover where configuration changes happen |
| island_peek_opened | count, utc_day (numeric UTC epoch day) | Aggregated by UTC day, flushed on next entry in a new day or session shutdown | Discoverability with bounded volume |
| insights_viewed | period (week/month/all) | Insights mounts or period changes | Insights use |
| share_card_action | action (copy/save/post_x) | Share action requested | Sharing intent, including failed/cancelled actions |
| settings_opened | pane, source | Settings pane opens/changes; native navigation carries its source | Settings discovery |
| pill_native_error | code (panel_conversion/positioning/show/hit_testing), dictation_id? | Native operation fails; capped at 3/code/session | Native overlay reliability |
| $exception | code, fixed AppError/panic value, fingerprint/level, sanitized stack, provider?, provider_code?, dictation_id? | Coded failure/panic, max 3/code/session | Error Tracking issue diagnosis |

Completion vocabularies: start_source=hotkey/pointer/tray; mode=hold/toggle;
details=always/changed/never; words buckets=0/1_5/6_20/21_60/61_200/gt_200;
guard=meta_reply/answered/none; style=off/clean/writing/notes/message/code.
Polish skip_used measures the existing conservative zero-wait skip decision.
start_to_first_audio_ms is absent when the recorder has no first-audio measurement.

Blocked kinds and actions match src/types/island-events.ts.
Recovery kinds: cloud_failed, network_offline, model_missing, remote_offline,
no_speech, integrity, mic_dropped_empty.
Resolutions: retried_ok, retried_failed, transcribed_anyway, discarded, expired.
Island states are the fixed native note/notice vocabulary, recording_too_short,
escape_hint and recovery. No per-frame, pointer-motion or hover analytics.
The only peek IPC is on entry; it increments a counter rather than capturing
one PostHog event for each entry.

## Coded exceptions

panic, frontend_error, pill_panel_conversion, pill_positioning, pill_show,
pill_hit_testing, retry_failed, kept_storage_failed, island_action_failed,
model_load_failed, cloud_stt_failed, polish_provider_failed, audio_device_failed,
transcription_failed, paste_failed.

Provider context uses known provider IDs or unknown. Provider codes:
timeout, rate_limited, network, unauthorized, unavailable, cancelled,
invalid_response, unknown. Cloud/remote terminal classes additionally include auth, transport, cloud_storage_limit,
model_unavailable, engine_failed and the closed remote failure categories. Unknown classes
become unknown. Provider message bodies are never an argument to the helper.
Model-load failures carry the most recent generation when available.

## Finding a VT report

1. Find the VT-XXXXX message in Discord.
2. Read its Diagnostics block: version, OS, architecture, anonymous install ID
   (only when sharing is active), last five dictation UUIDs, engine short name and
   pill mode. The block validates every field and includes no text or paths.
3. In PostHog Events, filter distinct_id by the anonymous install UUID, or
   filter dictation_id by a reported UUID. Match the report's version and time.
4. Open Error Tracking and filter code/version or follow the related exception
   events. Join recovery_resolved to the same dictation_id.

The VT report ID itself is not an analytics event or person ID. The block is
the lookup bridge. If consent is off there is no install ID and no remote event
history for that period. Trace IDs alone cannot manufacture absent events.
Existing user-written report messages and explicitly submitted attachments
remain separate from the content-free Diagnostics block.

## Insights to create in PostHog

| Insight | Definition |
|---|---|
| Dictation funnel | recording.started → recording.stopped → dictation_completed, matched by dictation_id; completion outcome breakdown |
| Stop-to-text | p50/p95 stop_to_text_ms, outcome=delivered; engine and app_version breakdown |
| Failure rate | failed completions / all completions; join typed error/recovery kind by dictation_id |
| Blocked rate | dictation_blocked by kind / (starts + blocked attempts); treat delivery blockers separately from start blockers |
| Recovery resolution | recovery_resolved count/share by kind and resolution; alternative engine breakdown |
| Guard fallback | completions with polish_guard_reason != none / Polish attempts; engine/version breakdown |
| Quick-setting source | quick_setting_changed count/share by setting and source |
| Errors by code | Error Tracking issues/frequency grouped by fixed fingerprint/code, version and OS |

## Validation and runtime gates

Unit contracts cover serialized event schemas, exception/panic scrubbing,
dedupe, legacy migration/deletion, generation correlation and report blocks.
Frontend gates: typecheck, oxlint, Vitest, production build. Rust format is a
separate gate; Claude must compile, cargo test and clippy this patch.

NEEDS-SMOKE: release ingestion on PostHog EU; Error Tracking issue grouping and
in-process symbols; coded panic delivery/location without payload text;
consent off/on while events are queued; cold startup/update; real-speech
dictation trace continuity across success/block/failure/retry/expiry; native
panel focus/back-to-back dictation on macOS and Windows; failed native
position/show/hit-test operations; one-day/session peek flush; actual Discord
report lookup. Debug builds cannot prove release ingestion.
