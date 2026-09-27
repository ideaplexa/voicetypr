# 031 — GlitchTip observability migration

Status: IN PROGRESS — claimed Main 2026-07-14.

## Goal

Move VoiceTypr Desktop from Bugsink to the self-hosted GlitchTip 6.2 project and enable privacy-safe errors, native symbolication, curated application logs, and sampled transcription traces in the next Beta.

## Non-negotiable privacy contract

- Telemetry remains release-only, consent-gated, immediately revocable, and inert in debug builds.
- Never send audio, transcripts, clipboard data, prompts, API keys, target application/window names, hostnames, user paths, URLs, IPs, emails, or arbitrary log messages/fields.
- Keep breadcrumbs, session tracking, browser SDK injection, replay, default PII, request/user/context capture, and native minidumps disabled.
- Structured logs and trace names/attributes use compile-time allowlists. They do not bypass consent or accept free-form user/environment strings.
- Preserve only the native debug identifiers, image basenames/sizes/addresses, and frame addresses required for server-side symbolication; scrub full paths.

## Change

1. Replace the release-only Bugsink DSN with the `VoiceTypr Desktop` GlitchTip DSN. Preserve `voicetypr@<version>`, add `environment=production` and a stable/beta release-channel tag, set `auto_session_tracking=false`, and sample traces at 1%.
2. Enable Sentry Rust `debug-images` and structured-log support. Extend the event scrubber to preserve only sanitized native debug metadata and native frame addresses.
3. Add a closed operational-event API for transcription start/success/failure/cancellation with safe enums, integer durations, and booleans only. Do not forward the existing `log` stream.
4. Instrument the desktop transcription lifecycle with one sampled transaction and fixed child spans for decode, writing, and delivery where those phases exist.
5. Upload Windows PDB and macOS dSYM/debug files from release CI using pinned `glitchtip-cli`, organization `ideaplexa`, project `voicetypr-desktop`, and a GitHub `GLITCHTIP_AUTH_TOKEN` secret. Symbol upload must fail the release when configured and unsuccessful; missing configuration must be explicit rather than silently pretending symbols were uploaded.

## Automated acceptance

- Scrubber tests prove path-bearing debug-image names become basenames while IDs, sizes, and addresses survive.
- Tests prove operational logs contain only the fixed body/attribute allowlist and respect consent.
- Tests prove environment/release/channel tags survive without restoring arbitrary event sections.
- Focused Rust tests, `pnpm quality-gate`, and `cargo clippy --release --lib -- -D warnings` pass.
- Windows and both macOS release jobs build successfully.

## Runtime acceptance

- A controlled Beta error appears in GlitchTip under release `voicetypr@<version>` with environment, channel, OS, and architecture tags and no forbidden data.
- A controlled native Beta panic resolves to VoiceTypr frames after PDB/dSYM upload.
- One transcription emits only the curated lifecycle logs and a sampled transaction/span tree; cancellation/failure outcomes remain distinguishable.
- Disabling diagnostics prevents subsequent errors, logs, and traces from reaching GlitchTip during that session.
