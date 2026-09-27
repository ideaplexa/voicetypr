# Plan 032: Ship consented product analytics and automatic formatting corrections

> **Executor instructions**: Follow this plan step by step. Run every verification
> command and confirm the expected result before moving on. Do not send any
> telemetry from debug builds or before the user has completed the consent flow.
> If a STOP condition occurs, stop and report instead of improvising. When done,
> update this plan's row in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 92e1643e..HEAD -- src-tauri src .github/workflows package.json pnpm-workspace.yaml telemetry-worker`
> If an in-scope file changed, compare the current-state excerpts below against
> live code before proceeding. Coordinate rather than overwriting concurrent work.

## Status

- **Priority**: P1
- **Effort**: XL
- **Risk**: HIGH
- **Depends on**: Plan 031's GlitchTip client/consent code being merged; native symbolication completion is not required
- **Category**: direction, migration, privacy
- **Planned at**: commit `92e1643e`, 2026-07-16
- **Approval gate**: approved 2026-07-16 — PostHog Cloud EU for non-content events; a first-party Cloudflare Worker + isolated R2 bucket for formatting-correction text

## Why this matters

VoiceTypr currently reports crashes and a sampled transcription lifecycle to
GlitchTip, but it cannot answer adoption, funnel, reliability-distribution, or
formatting-quality questions. The current single `telemetry_enabled` preference
also cannot truthfully represent separate crash reporting and usage analytics.
This plan adds two independently revocable controls, a closed non-content event
pipeline, and automatic collection of high-confidence user corrections to text
VoiceTypr just pasted. It does not add generic click tracking, browser
instrumentation, audio collection, or cross-application keylogging.

## Product decisions

These decisions are load-bearing. Do not silently broaden or reinterpret them.

1. **Two visible controls, both normally ON**:
   - `Crash & error reporting` controls GlitchTip errors, curated logs, and sampled traces.
   - `Usage analytics` controls PostHog product events and automatic formatting-correction samples.
2. **Existing explicit crash opt-outs remain opt-outs.** The legacy
   `telemetry_enabled=false` value migrates to crash reporting OFF. No migration
   may turn it back on.
3. **Existing users must acknowledge a one-time modal.** Until they press
   `Continue`, the new build sends neither category. Closing/Escape pauses both
   categories for that session and shows the modal again next launch.
4. **New users choose during onboarding.** Both controls are shown ON by default
   and persist only when onboarding completes.
5. **Settings changes are immediate.** Turning either control off blocks new
   egress immediately, cancels in-flight work for that category, clears its local
   queue, and deletes that category's local anonymous installation ID. Turning it
   on must not require an app restart.
6. **Formatting correction collection is automatic and narrow.** Observe only
   the same focused editable control and the range VoiceTypr inserted, for at
   most 30 seconds. Upload only when a user edit overlaps that inserted range and
   the corrected range can be isolated unambiguously. Never record keystrokes,
   whole documents, surrounding text, target app/window names, audio, clipboard
   history, or unrelated typing.
7. **Text never enters PostHog or GlitchTip.** PostHog gets a non-content
   `formatting.correction_observed` event with buckets only. The raw transcript,
   AI-formatted output, and corrected output go only to the first-party sample
   endpoint.
8. **No unchanged-text baseline in the first release.** Upload every
   high-confidence correction under consent, and zero random ordinary
   transcripts. Revisit baseline sampling only after the privacy and usefulness
   of correction samples are proven.

## User-facing copy

Use this copy exactly unless the operator changes it before implementation.
Keep the modal short; do not add a release-notes wall.

**Modal title**

> Help improve VoiceTypr

**Modal description**

> Both options are on by default. You can change them anytime in Settings.

**Crash row**

- Label: `Crash & error reporting`
- Description: `Shares crash and error reports so we can fix problems faster.`

**Usage row**

- Label: `Usage analytics`
- Description: `Help improve VoiceTypr by sharing product usage, performance data, and formatting corrections.`

**Primary action**: `Continue`

Settings and onboarding must use the same labels and substantively identical
one-line descriptions. A separate `Exactly what is collected` disclosure may
list the full contract, but it must not be forced into the primary modal.

## Current state

### Consent and GlitchTip

- `src-tauri/src/telemetry.rs:98-129` owns the legacy keys
  `telemetry_enabled`/`telemetry_install_id`, defaults diagnostics ON, and uses a
  single process-wide atomic gate.
- `src-tauri/src/telemetry.rs:396-438` currently creates the Sentry/GlitchTip
  client only when startup consent is enabled. This is why re-enabling currently
  requires a restart. The new design must initialize the release-only client
  whenever a DSN exists while keeping egress gates closed until acknowledged.
- `src-tauri/src/commands/telemetry.rs:16-92` exposes one
  `TelemetryStatus { enabled, available }` and one
  `set_telemetry_consent(enabled)` command.
- `src-tauri/src/lib.rs:375-382` reads consent before Tauri is built and retains
  the Sentry guard for the process lifetime.
- Plan 031's privacy contract remains in force: no audio, transcripts, clipboard
  contents, prompts, keys, target app/window names, arbitrary logs, replay,
  breadcrumbs, browser SDK, session tracking, or native minidumps.

### Frontend surfaces

- `src/components/sections/TelemetrySection.tsx:26-104` renders one Diagnostics
  switch and promises crash/error reports only.
- `src/components/onboarding/OnboardingDesktop.tsx:1468-1480` renders one
  default-checked anonymous-error checkbox and calls
  `set_telemetry_consent` at `:703-708`.
- `src/components/AppContainer.tsx:80-89,319-329` owns post-update dialog display.
  `UpdateAnnouncementDialog.tsx` is a minimal shadcn dialog and is the visual
  pattern to reuse, but telemetry acknowledgement must be driven by persisted
  consent-version state, not solely by an updater marker.

### Formatting and paste boundary

- `src-tauri/src/writing.rs:246-261` returns both `raw_text` and `final_text` in
  `WritingResult`; it also indicates `ai_applied`.
- `src-tauri/src/commands/audio.rs:5549-5601` currently reduces that result to
  final text and history metadata before delivery.
- `src-tauri/src/commands/audio.rs:5805-5827` calls
  `commands::text::insert_text` and knows whether paste succeeded.
- `src-tauri/src/commands/text.rs:92-138` applies insertion-boundary spacing and
  pastes via the clipboard but returns only `Result<(), String>`.
- `src-tauri/src/commands/text.rs:673-708` and `:710-778` synthesize Cmd+V on
  macOS and Ctrl+V on Windows. There is no focused-control identity, selected
  range, or post-paste text observer today.
- `active-win-pos-rs` provides only active-window metadata. It is not sufficient
  for range-scoped text observation.

### Platform capability direction

- macOS capability spike: use Accessibility APIs on the focused element,
  `kAXSelectedTextRangeAttribute`, range-scoped string access, and
  `kAXValueChangedNotification` via an `AXObserver`. Do not read/store the whole
  value when a range query is available.
- Windows capability spike: use UI Automation focused-element identity,
  `IUIAutomationTextPattern`/`IUIAutomationTextPattern2` ranges and text-change
  events. Add the required `windows` crate Accessibility/COM features; do not
  synthesize or hook user keystrokes.
- Both platforms must fail closed for password/secure fields, unsupported custom
  editors, focus changes, ambiguous range movement, selection replacements that
  cannot be tracked, and text exceeding the payload bound.

## Backend architecture

### Non-content analytics: PostHog Cloud EU

Use the official `posthog-rs` SDK (0.10.x or the current compatible release at
execution time) from Rust only. No frontend SDK and no autocapture.

- Host: PostHog Cloud EU.
- Manual capture only through a closed `ProductEvent` enum.
- Disable GeoIP enrichment.
- Set `$process_person_profile=false` on every event; never call identify or set
  person properties.
- Use a category-specific random `analytics_install_id` as `distinct_id`.
- Project key is a public ingestion key supplied to release builds through a
  GitHub repository variable; it is absent in debug builds.
- Every event-capture task must be cancellation-aware. Opt-out aborts in-flight
  captures rather than flushing them.

### Content samples: first-party Cloudflare Worker + R2

Add `telemetry-worker/` as an isolated pnpm workspace package. Use a bare typed
Worker, generated Wrangler binding types, and an R2 binding; do not add a web
framework just for one route.

Endpoint:

```text
POST /v1/formatting-corrections
Content-Type: application/json
```

Server behavior:

- Reject methods/routes other than the endpoint with 404/405.
- Reject missing/incorrect content type, unknown JSON fields, payloads over 64
  KiB, and any text field over 8,000 Unicode scalar values.
- Validate closed enum/bucket fields; never accept free-form app names, paths,
  URLs, prompts, or provider error text.
- Generate the object key server-side with `crypto.randomUUID()`.
- Store one JSON object in a dedicated R2 bucket; no shared debug-symbol bucket.
- Do not persist request IP, headers, user agent, installation ID, or Worker logs
  containing request bodies.
- Configure R2 lifecycle deletion after 90 days.
- Apply Cloudflare rate limiting at the route. The client contains no secret that
  can authenticate an untrusted desktop binary, so do not pretend a hardcoded
  token is a security boundary.
- Enable low-rate structured operational logs containing status code and fixed
  rejection reason only, never payload fields.
- Deploy at a dedicated hostname such as `telemetry.voicetypr.com`; the exact
  production URL is supplied to release builds through a repository variable.

Formatting sample schema:

```text
schema_version: 1
sample_id: client UUID (deduplication only; not an installation ID)
created_at: RFC3339 UTC
app_version: bounded semver
release_channel: stable | beta
os_family: macos | windows
architecture: x86_64 | aarch64
provider_family: closed provider ID | local
model_catalog_id: known catalog ID | custom
preset: personal_dictation | clean_dictation | message | email | custom
custom_prompt_used: boolean
selected_language: supported language code | auto
raw_transcript: text
formatted_output: text
corrected_output: text
edit_latency_bucket: lt_2s | 2_5s | 5_15s | 15_30s
change_ratio_bucket: lt_5pct | 5_20pct | 20_50pct | gt_50pct
```

The client must omit a sample if any required metadata cannot be normalized to
these closed values. Do not add an analytics or crash installation ID to this
payload.

## Product event contract

Implement one Rust-owned `ProductEvent` enum. Event names and properties are
closed types; no caller may pass an arbitrary event name or free-form property
map. Every event automatically receives only: `app_version`, `release_channel`,
`os_family`, `architecture`, and `$process_person_profile=false`.

Initial event set:

| Event | Required safe properties |
|---|---|
| `app.started` | `first_launch`, `first_launch_after_update` |
| `onboarding.started` | none |
| `onboarding.step_completed` | closed `step` |
| `onboarding.completed` | closed transcription source, duration bucket |
| `onboarding.blocked` | closed reason code |
| `model.download_started` | engine, catalog model ID |
| `model.download_completed` | engine, catalog model ID, duration bucket |
| `model.download_failed` | engine, catalog model ID, safe failure code |
| `model.download_cancelled` | engine, catalog model ID |
| `recording.started` | route, mode |
| `recording.completed` | route, mode, duration bucket, speech-evidence class |
| `recording.cancelled` | route, mode, stage |
| `recording.failed` | route, mode, safe failure code |
| `transcription.completed` | route, engine, provider family, catalog model ID, duration bucket, output-length bucket, fallback used |
| `transcription.failed` | route, engine, provider family, safe failure code, fallback used |
| `transcription.cancelled` | route, engine, stage |
| `formatting.completed` | provider family, catalog model ID, preset, custom-prompt-used boolean, latency bucket, input/output-length buckets, change-ratio bucket, fallback used |
| `formatting.failed` | provider family, catalog model ID, preset, safe failure code, fallback used |
| `formatting.cancelled` | preset, stage |
| `delivery.completed` | auto-paste or clipboard, duration bucket |
| `delivery.failed` | delivery method, safe failure code |
| `update.available` | channel |
| `update.completed` | channel |
| `update.failed` | channel, safe failure code |
| `formatting.correction_observed` | provider/model/preset metadata, edit-latency bucket, change-ratio bucket; no text |

Rules:

- Custom provider/model names become `custom`; never send the user-entered name.
- Language is the configured/returned normalized language code, never inferred by
  re-reading transcript content.
- Error properties are closed categories already used by VoiceTypr's failure
  mapping; never send raw provider responses or `Display` strings.
- Durations, lengths, and ratios are buckets, not exact values.
- Do not add settings-page views, button clicks, target apps, window titles,
  navigation dwell, mouse movement, machine IDs, hardware serials, IPs, or
  full GPU names.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Frontend types | `pnpm typecheck` | exit 0, no errors |
| Frontend lint | `pnpm lint` | exit 0, zero warnings |
| Frontend tests | `CI=true pnpm exec vitest run` | all tests pass |
| Rust formatting | run `cargo fmt --check` from `src-tauri` after formatting only touched Rust files | exit 0 |
| Rust tests | `cd src-tauri && cargo test --lib` | all pass |
| Rust lint | `cd src-tauri && cargo clippy --release --lib -- -D warnings` | exit 0, zero warnings |
| Worker types | `pnpm --dir telemetry-worker wrangler types` | generated binding types match config |
| Worker tests | `pnpm --dir telemetry-worker test` | all tests pass |
| Worker deploy dry-run | `pnpm --dir telemetry-worker wrangler deploy --dry-run` | exit 0 |
| Windows compile proof | `build-windows` GitHub Actions job | success; local macOS is not proof for `cfg(windows)` |

Run Rust gates sequentially because the Parakeet Swift build directory is not
safe under concurrent Rust builds.

## Suggested executor toolkit

- Load `refero-design` before implementing the modal and Settings controls; the
  selected direction is a compact single-column consent dialog, not a legal wall.
- Load `workers-best-practices`, `cloudflare`, and `wrangler` before creating the
  Worker or config. Retrieve current Workers types/config schema at execution
  time.
- Load `react-best-practices` after changing the modal, onboarding, and settings
  components.
- Use LSP references before changing `insert_text`, `TelemetryStatus`, or exported
  Tauri commands when a Rust/TypeScript language server is available.

## Scope

**In scope**:

- `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
- `src-tauri/src/lib.rs`
- `src-tauri/src/telemetry.rs` and its existing tests
- `src-tauri/src/analytics.rs` (new)
- `src-tauri/src/formatting_corrections.rs` plus target-specific submodules (new)
- `src-tauri/src/commands/telemetry.rs`
- `src-tauri/src/commands/audio.rs`
- `src-tauri/src/commands/text.rs`
- `src-tauri/src/writing.rs`
- model-download and updater command modules only where the named events have a
  real lifecycle boundary
- `src/components/sections/TelemetrySection.tsx` and its tests
- `src/components/onboarding/OnboardingDesktop.tsx` and its tests
- `src/components/TelemetryConsentDialog.tsx` and its test (new)
- `src/components/AppContainer.tsx` and its tests
- `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- `package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`
- `telemetry-worker/**` (new isolated Worker package)
- privacy policy/Store disclosure files only if they already exist in this repo;
  otherwise record the required external updates in the PR description
- `plans/README.md` and this plan's status

**Out of scope**:

- GlitchTip symbolication or debug-symbol upload fixes owned by Plan 031.
- Audio upload, audio retention, audio sampling, or voice-content logging.
- Browser/frontend analytics SDKs, autocapture, session replay, feature flags,
  experiments, user profiles, generic navigation/click analytics, or target-app
  tracking.
- Global keyboard hooks for correction capture. Existing hotkey hooks must not be
  reused to inspect user text.
- AI training, fine-tuning, automatic prompt mutation, or model switching. This
  plan collects evidence; it does not consume it automatically.
- Random unchanged-transcript sampling.
- Linux correction observation; Linux must report capability unavailable and
  upload no correction samples.

## Git workflow

- Branch: `feat/product-analytics`
- Claim Plan 032 in `plans/README.md` before source edits.
- Conventional commits, one logical commit per stage; examples:
  `feat(telemetry): split diagnostics and analytics consent`,
  `feat(analytics): instrument transcription funnel`,
  `feat(analytics): collect bounded formatting corrections`.
- Do not push or open a PR unless instructed.

## Steps

### Step 1: Build the first-party formatting sample service

Create `telemetry-worker/` with a typed Worker, R2 binding, generated Env types,
strict schema validation, request-size limits, and Worker tests. Add it to the
pnpm workspace and root CI. Configure the dedicated R2 bucket and 90-day lifecycle
outside source control; use non-secret repository/environment variables for the
public endpoint. Do not instrument the desktop yet.

Tests must prove: valid payload accepted; unknown fields rejected; oversized body
rejected before parsing; oversized text rejected; invalid enum rejected; wrong
method/content-type rejected; stored object contains only the schema; no request
headers/IP/user agent are copied.

**Verify**: Worker tests and deploy dry-run both exit 0.

### Step 2: Replace the single consent value with versioned preferences

Introduce a versioned backend contract, for example:

```text
TelemetryPreferences {
  crash_reporting_enabled: bool,
  usage_analytics_enabled: bool,
  notice_required: bool,
  crash_reporting_available: bool,
  usage_analytics_available: bool,
  correction_capture_available: bool,
}
```

Use dedicated keys:

```text
crash_reporting_enabled
crash_reporting_install_id
usage_analytics_enabled
analytics_install_id
telemetry_notice_version
```

Migration rules:

- legacy `telemetry_enabled=false` -> crash OFF;
- legacy `telemetry_enabled=true` or missing -> crash ON;
- no prior analytics preference -> usage analytics ON in the UI, but gate closed
  until notice version 1 is acknowledged;
- existing legacy installation ID may move only to crash reporting; analytics
  receives a separately generated ID;
- malformed/unreadable storage fails closed for egress and requires the notice;
- after successful migration/save, delete legacy keys; no long-lived aliases.

Replace `get_telemetry_status`/`set_telemetry_consent` with
`get_telemetry_preferences` and `set_telemetry_preferences`. Migrate every caller
and command registration in the same change; leave no deprecated Tauri commands.
The setter accepts both booleans and `acknowledge_notice`, persists atomically,
and updates both in-process gates only after successful storage.

Change GlitchTip initialization so the release client exists whenever its DSN is
compiled, while every event/log/span and transport still checks the crash gate.
This removes the restart requirement without allowing pre-consent egress.

**Verify**: focused Rust consent/migration tests pass, including old false, old
true, missing, malformed, modal-not-acknowledged, revoke, and re-enable cases.

### Step 3: Add the closed PostHog analytics client

Create `src-tauri/src/analytics.rs` with:

- a closed `ProductEvent` enum and closed property enums;
- bucket functions for durations, lengths, and ratios;
- release-only configuration using the public PostHog project key;
- EU host, GeoIP disabled, person-profile processing disabled;
- one category-specific random installation ID;
- cancellation-aware capture tasks tracked by a manager owned by Tauri state;
- immediate revoke that closes the gate, aborts in-flight requests, drops queued
  events, and removes the client/ID;
- no capture API accepting arbitrary names, strings, or JSON maps.

Do not use the frontend SDK. In debug builds or without the compiled public key,
report `usage_analytics_available=false` and perform no network request.

**Verify**: unit tests with `wiremock` prove exact host/path/body allowlists,
consent gating, `$process_person_profile=false`, GeoIP disablement, cancellation,
and absence of forbidden keys/text.

### Step 4: Implement the one-time modal, onboarding, and Settings controls

Create `TelemetryConsentDialog` using the existing shadcn Dialog/Switch/Button
primitives and the exact copy above. Drive it from
`get_telemetry_preferences().notice_required`, not from updater state. In
`AppContainer`, modal order is:

1. onboarding, if required;
2. telemetry consent, if notice required;
3. ordinary update announcement.

If the modal closes without Continue, do not acknowledge or enable egress; do
not show it again during that session, but show it on next launch. Pressing
Continue persists both visible values and notice version 1 before enabling either
category.

Replace onboarding's single checkbox with the same two controls. Replace
`TelemetrySection` with a `Diagnostics & Analytics` section containing two
independent switches. Settings changes call the same preference command and take
effect immediately; remove restart toasts and stale crash-only copy.

**Verify**: React tests prove defaults, legacy crash opt-out display, Continue
persistence, close/Escape behavior, modal ordering, independent Settings toggles,
onboarding persistence, failure rollback, and unavailable debug-build states.

### Step 5: Instrument the meaningful product funnel

Add only the initial events listed in the contract. Reuse real lifecycle
boundaries instead of adding parallel state machines. The transcription hot path
already has cancellation-safe RAII guards and stage outcomes; attach analytics at
those same boundaries. Model download and updater events go at their existing
start/terminal state transitions.

Derive metadata from typed settings/results already in memory. Add a
`FormattingAnalyticsContext` to `WritingResult` if needed so the audio command
does not re-read settings or infer language/provider from text. Normalize custom
providers/models to `custom` before the analytics boundary.

No capture may block recording, transcription, paste, history persistence, or UI
updates. A telemetry network failure is swallowed after a sanitized local debug
log and must not alter the product result.

**Verify**: table-driven unit tests cover every event serialization and forbidden
property; focused lifecycle tests prove exactly one terminal event per started
operation across success/failure/cancel/fallback.

### Step 6: Prove range-scoped correction observation on each platform

Before wiring uploads, implement target-specific observer adapters behind a small
platform-neutral contract:

```text
begin_before_paste(inserted_text) -> ObservationHandle | Unsupported
confirm_after_paste(handle) -> ConfirmedInsertedRange | Ambiguous
wait_for_correction(handle, max_30s) -> CorrectedText | Unchanged | Ambiguous
cancel_all()
```

The adapter may retain opaque control/range handles and the inserted text locally
for 30 seconds. It must not expose whole-document strings to the shared layer.
Secure/password fields return `Unsupported` before paste. A focus/control change,
missing range API, range mismatch, unrelated appended text, or ambiguous edit
returns `Ambiguous` and destroys the candidate.

Change the internal `insert_text` return type so the desktop audio path can
receive an optional confirmed observation handle after a successful paste.
Preserve the public Tauri command response shape if frontend callers depend on
`Result<(), String>`; split an internal helper rather than breaking external
callers.

Required Beta capability matrix:

- macOS: TextEdit, Notes, Slack editor;
- Windows: Notepad, Word, Slack editor;
- at least one Chromium textarea/contenteditable control on each OS.

Unsupported controls are acceptable only when they fail closed and normal paste
still succeeds. If no platform API can isolate the inserted range without reading
whole-document content for a required app, stop and report the reduced supported
matrix; do not add keyboard logging or document-wide polling.

**Verify**: platform-neutral range/diff tests pass locally; Windows code compiles
in `build-windows`; real-device Beta matrix is recorded in `plans/SMOKE.md`.

### Step 7: Upload only confirmed corrections

At formatting time, create an in-memory candidate only when all are true:

- usage analytics is enabled and notice acknowledged;
- AI formatting actually ran successfully;
- raw transcript and formatted output differ;
- auto-paste is enabled;
- all metadata maps to the closed sample schema;
- each text field is within the client bound.

After paste succeeds, start the 30-second observer. Debounce overlapping edits
for 1.5 seconds, then upload the final corrected range only if it differs from the
formatted output and is unambiguous. Compute buckets locally. Send the sample to
the Worker and a text-free `formatting.correction_observed` event to PostHog only
after the Worker accepts the sample. Network failure drops the sample; do not
retry across launches or persist dictated text to a queue.

Opt-out or app shutdown cancels observers and HTTP requests and drops every
in-memory candidate. Never use transcription history as a retry queue.

**Verify**: tests prove no upload for disabled consent, unacknowledged notice,
unchanged text, deterministic-only cleanup, clipboard-only delivery, unsupported
control, focus change, unrelated appended text, ambiguous ranges, oversized text,
or network failure. A successful test proves exact three-text payload and
text-free PostHog event.

### Step 8: Configure dashboards and perform the Beta privacy audit

Create PostHog dashboards for:

- activation: app start -> onboarding completion -> first successful delivery;
- reliability: recording/transcription/formatting/delivery failure rates;
- performance: duration buckets by engine/provider/model/preset;
- adoption: local/cloud/remote routes, AI formatting, presets, GPU/CPU fallback;
- retention: D1/D7/D30 returning anonymous installations;
- formatting: completion/fallback/correction-observed rates by provider/model/preset.

Set PostHog event retention to 12 months and verify person profiles, GeoIP,
autocapture, replay, and frontend SDKs are absent. Confirm R2 lifecycle deletion
at 90 days. Update the in-app detailed disclosure and external privacy/Store
statements before the Beta build.

Run a release-build network capture on macOS and Windows covering: pre-modal,
Continue with both on, each switch independently off, both off, restart, and one
confirmed correction. Inspect actual PostHog, Worker/R2, and GlitchTip payloads.
Treat any forbidden field or pre-consent request as a release blocker.

**Verify**: automated gates pass; CI release builds pass; manual smoke evidence is
recorded. Do not label the plan DONE while platform/runtime checks remain.

## Test plan

### Rust

- Consent migration matrix and malformed-store fail-closed behavior.
- Independent gates/IDs and immediate revocation.
- Closed event serialization for every event/property variant.
- Bucketing boundaries.
- Wiremock assertions for PostHog EU requests and Worker sample requests.
- No text in PostHog events and no install ID in sample payloads.
- Correction observer state machine: confirmed, unchanged, focus changed,
  appended unrelated text, overlapping edit, ambiguous edit, timeout, cancellation.
- Existing transcription cancellation/generation tests remain green.

### Frontend

- `TelemetryConsentDialog.test.tsx`: exact copy, default values, interaction,
  close/Escape, pending/error states, Continue payload.
- `AppContainer.test.tsx`: modal priority and notice-required behavior independent
  of updater marker.
- `TelemetrySection.test.tsx`: independent switches, optimistic-state rollback,
  debug unavailable state, no restart copy.
- `OnboardingDesktop.test.tsx`: both defaults ON and both values persisted only
  on successful completion.

### Worker

- Strict route/method/content type/body limit/schema tests.
- R2 write uses server UUID and exact allowed object shape.
- Invalid/oversized payloads never write.
- Logs and errors never echo request bodies.

### Runtime

- macOS and Windows pre-consent packet capture shows zero telemetry egress.
- Each category can be disabled independently without restart.
- Required correction-capture app matrix passes or is explicitly reduced after a
  STOP report and user decision.
- Ordinary paste latency and reliability are unchanged when observation is
  unsupported or disabled.

## Done criteria

All must hold:

- [ ] Backend split explicitly approved by the operator.
- [ ] Legacy explicit crash opt-out is preserved by tested migration.
- [ ] No category sends before modal/onboarding acknowledgement.
- [ ] Both controls default ON for users without an explicit prior choice.
- [ ] Disabling either category is immediate and deletes only that category's
      local installation ID.
- [ ] PostHog receives only the closed non-content schema; no frontend SDK exists.
- [ ] Formatting text reaches only the first-party Worker/R2 path.
- [ ] No random unchanged transcripts are uploaded.
- [ ] Automatic correction capture never reads/stores surrounding document text,
      target app/window names, or keystrokes.
- [ ] Required automated commands exit 0.
- [ ] `build-windows` compiles the Windows UI Automation implementation.
- [ ] macOS and Windows release-network captures prove consent behavior.
- [ ] R2 90-day lifecycle and PostHog 12-month retention are configured.
- [ ] Detailed in-app and external privacy/Store disclosures match actual data.
- [ ] Reviewer pass finds no P0/P1/P2 privacy, correctness, or race findings.
- [ ] `plans/README.md` is updated to DONE or NEEDS-SMOKE accurately.

## STOP conditions

Stop and report; do not improvise if:

- The operator does not approve PostHog Cloud EU plus first-party Worker/R2.
- Plan 031 changes the GlitchTip consent keys/client initialization after this
  plan's drift baseline and the migrations cannot be reconciled cleanly.
- Any proposed SDK or OS API requires a frontend/browser analytics SDK,
  user-level identification, person profiles, GeoIP, or session replay.
- A required editor exposes correction text only by reading the whole document,
  installing a global keylogger, or observing unrelated controls.
- Opt-out cannot cancel an in-flight analytics/sample request reliably.
- The public sample endpoint cannot be rate-limited and schema-bounded without
  storing request identifiers or raw payload logs.
- The implementation needs to persist dictated-text samples locally for retry.
- Windows UI Automation code cannot be compile-proven in CI or the macOS
  Accessibility observer cannot be runtime-smoked on a real app.
- Any network capture contains a forbidden field or a request before consent.

## Maintenance notes

- GlitchTip, PostHog, and the correction Worker are three transport paths behind
  two user controls: diagnostics owns GlitchTip; usage analytics owns both
  PostHog and correction samples. UI copy must continue to describe this
  truthfully.
- New product events require an enum variant, exact property allowlist, tests,
  and disclosure review. Do not add a generic `capture(name, json)` escape hatch.
- Any new formatting sample field requires synchronized client/Worker schema
  versions and a privacy review.
- If correction coverage is poor in a major editor, treat that as a capability
  limitation. Never solve it by broadening to document-wide or keyboard capture.
- Formatting samples are evidence for manual prompt/model evaluation. Automatic
  training or prompt mutation needs a separate approved plan.
