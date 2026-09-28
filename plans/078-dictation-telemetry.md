# Plan 078 — One privacy-safe `dictation.completed` event (observability 0.1b)

Status: SPEC — Claude 2026-09-28. Target: 2.1.0-beta.2. Founder: betas are
judged by beta users + numbers instead of founder smoke tests, so every
dictation needs typed numbers — never content.

## Today

`product_analytics.rs` sends `recording.started`, `recording.stopped`
(duration bucket), `transcription.stage_finished` (stage/outcome/duration
bucket/engine kind) and `polish.finished`. Every event is rebuilt from an
allowlist in `validated_dynamic_properties`; consent-gated, personless, no
GeoIP. We cannot see stop → text per engine, start latency, the Soniox
transport, post-roll effect, paste failures or preview use.

## Change

Add `dictation.completed`, captured once per finished dictation (success,
empty/no-speech, failure or cancel) from the existing stop → deliver flow in
`commands/audio.rs`, with only these properties:

| Property | Type / values |
|---|---|
| `outcome` | `delivered` · `no_speech` · `empty` · `failed` · `cancelled` |
| `engine` | existing `EngineKind` values |
| `model` | catalog model id only (reuse `safe_model_id`-style allowlisting; unknown → `other`) |
| `transport` | `local` · `ws` · `rest` · `rest_fallback` · `remote` |
| `live_preview` | bool |
| `recording_ms` | integer, clamped 0..=600000, rounded to 10 ms |
| `start_to_first_audio_ms` | integer or absent (plan 074 metric), clamped 0..=10000 |
| `stop_to_text_ms` | integer, clamped 0..=600000 (stop request → text ready for delivery) |
| `post_roll_speech_detected` / `post_roll_interrupted` | bool |
| `words_bucket` | `0` · `1_5` · `6_20` · `21_60` · `61_200` · `gt_200` |
| `polish` | existing `PolishOutcome` values |
| `paste` | `succeeded` · `failed` · `skipped` |
| `app_category` | existing `AppCategory` values (never app name, title or path) |

Rules: numbers are validated as integers in range or the whole event is
dropped (same fail-closed stance as today); no free-form strings; no text,
audio, app names, titles, paths, keys, prompts. Soniox `ws` vs `rest_fallback`
comes from whether `take_cloud_ws_final` supplied the text.

## Tests

- Allowlist: valid event passes; each property out of range / unknown value /
  extra key → dropped (property tests like the existing ones).
- Capture site: a pure builder turns the per-dictation facts into the event;
  unit-test each outcome path (delivered, no_speech, failed, cancelled) and the
  Soniox transport choice.
- Snapshot of the exact property set so new fields need a deliberate change.

## Acceptance

`pnpm check` green; a PostHog dashboard definition (insights: stop→text p50/p95
by engine+transport, start_to_first_audio p50/p95 by OS, outcome mix, paste
failure rate by app category) documented in `docs/ARCHITECTURE.md` telemetry
section.

## Known gaps

- Quitting the app during a recording emits no completion event.
- Captures stopped inside `start_recording` (quick PTT release during
  microphone initialization or Escape before capture) emit no completion event.
