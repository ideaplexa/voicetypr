# Architecture deep-review — god-files, per-engine duplication, settings typing

Three parallel read-only audits (2026-07-06). Everything below is `file:line`-anchored and
scored **value × safety**. The guiding rule for this campaign is "stable as fuck" — so items
are split into **safe wins (do now)**, **structural refactor (deliberate follow-up)**, and
**explicitly not worth doing**.

---

## Real defects surfaced (worth fixing regardless of refactor appetite)

- **D1 — divergent duration guard.** `seconds_to_duration_ms` is copied: `whisper/gpu_sidecar.rs:621-627`
  has a finite/negative guard; the `transcription/engines.rs:18-20` copy does **not**. A NaN or
  negative timestamp from the Parakeet/engines path is unguarded. (from F4)
- **D2 — latent settings deserialize failure.** `save_settings(settings: Settings)`
  (`commands/settings.rs:573`) requires ~21 fields that TS `AppSettings` (`src/types.ts:81-122`)
  marks optional. Any `save_settings` call not built from a full `get_settings` snapshot throws
  `missing field ...` at runtime. Masked today only because `SettingsContext.updateSettings`
  always spreads `{...prev, ...updates}`. (from S1)
- **D3 — 3 concrete Rust↔TS type drifts** already being patched by hand:
  `aiModelNeedsReselection` (Rust sends, TS type omits — forces the `AISettingsResponse` shim in
  `EnhancementsSection.tsx:52`), `enhancement_options` (TS type promises, backend never sends),
  `pill_position` (Rust persists, TS type omits), `sharing_password` (TS type advertises
  persistable, backend deletes it on save by design). (from S3/S4)
- **D4 — normalization asymmetry (needs a look, not a blind fix).** The desktop *local recording*
  path applies speech-gated gain + dither via `audio/normalizer.rs::normalize_to_whisper_wav`,
  but the executor's `prepare_normalized_input` (`transcription/executor.rs:452`) and the file-upload/
  cloud paths only use `audio/decode.rs::normalize_to_wav_async` — so uploaded/executor audio never
  gets that boost. Possible transcription-quality difference on uploads. Behavior change → gate on WER
  harness before touching. (from PA-3)

## Safe, low-risk dedup wins (mechanical, behavior-preserving)

- **F4 — consolidate leaf helpers** into `transcription/` (or `audio::wav`): one
  `seconds_to_duration_ms` (fixes D1), one segment→`TranscriptionSegment` mapper, one
  `wav_duration_ms` (3 copies: `parakeet/messages.rs:219`, `gpu_sidecar.rs:784`, `executor.rs:486`).
  RISK low · EFFORT S · VALUE med (closes D1).
- **F5 — single timeout budget.** The formula `ceil(dur_s)*4 + 60` clamped `[180,1800]` is verbatim
  in `engines.rs:22-33` and `gpu_sidecar.rs:769-782`; the `30*60` ceiling re-hardcoded in
  `cloud_stt/common.rs:88` and `parakeet/messages.rs:110`. One `transcription_budget()` + shared
  `TRANSCRIPTION_TIMEOUT_{MIN,MAX}_SECS`. RISK low · EFFORT S · VALUE med.
- **S1/D2 — make the settings contract honest.** Preferred: mark the ~21 fields required in TS
  `AppSettings` (matches Rust; no behavior change). *Not* the container `#[serde(default)]` route —
  that would let a partial save silently clobber existing values with type-defaults. RISK low · EFFORT S.
- **S3/S4/D3 — fix the 3 drifts**: add `aiModelNeedsReselection` + drop `enhancement_options` from
  `types/ai.ts` (delete the `AISettingsResponse` shim); add `pill_position?: [number,number]|null`
  and remove/annotate `sharing_password` in `types.ts`. RISK low · EFFORT S.
- **F6 — de-dup connection warm-up.** `cloud_stt/common.rs:124` and `commands/ai.rs:911` are the same
  HEAD-warm with different pooled clients; two near-identical spawn blocks at `commands/audio.rs:3827`
  & `:3843`. One `warm_origin(client, url)` (keep the two pools). RISK low · EFFORT S · VALUE low-med.
- **S2 — settings key constants.** ~40 store keys are bare string literals across 17 modules with no
  registry (`get_settings`/`save_settings` re-type each independently). A `settings_keys` const module
  kills the drift class. RISK low · EFFORT M (mechanical, many sites).

## Structural refactor — high value, needs a deliberate follow-up (NOT mid-campaign)

`commands/audio.rs` is a genuine 7182-line god-file (two `#[tauri::command]` monsters:
`start_recording` 718 lines, **`stop_recording` 1553 lines**; plus a test module wedged at 1555-2747).

- **Safe extractions** (each self-contained, keep a `pub use` shim; ~2000 lines out of audio.rs):
  pill-toast UI → `commands/pill_toast.rs` (used by 7 modules, 0 recorder coupling — the cleanest);
  transcription-history store I/O → `commands/transcription_history.rs`; failure classification →
  `commands/transcription_failure.rs`; retranscription status → `commands/retranscription.rs`;
  request/result builders → `commands/transcription_request_build.rs`; recording-generation guards →
  `commands/recording_generation.rs`; Parakeet stream sink → `commands/parakeet_stream_sink.rs`.
- **Deeper (high risk):** decompose `stop_recording`'s ~930-line spawned delivery task (encodes 3
  cancellation/stale-generation race rechecks) → `deliver_transcription(...)`; slim `start_recording`;
  split `save_settings` persistence from its side-effects tail; split `lib.rs::run()` (1216 lines).
- **Engine unification (per-engine dup):** F1 — Remote **host** (`remote/transcription.rs:276-482`)
  re-implements the executor's Whisper+Parakeet flow (2 caches, 2 mutex flavors); route it through the
  executor's reserved `EngineSelection::HostDefault`. F2 — engine selection lives in 3 divergent
  places (`engines.rs:242`, inline `audio.rs:4647-4849`, `remote/transcription.rs:331`) → double-resolve
  per dictation; consolidate to one resolver. F3 — remote send-to-peer copied across 4 sites
  (`audio.rs:5111` & `:6477`, `remote.rs:1583`, `cli.rs:789`) → one helper.

## Explicitly NOT worth doing (avoid churn / premature abstraction)

- No uniform `trait Transcriber` — the executor's **enum dispatch** (`executor.rs:176-326`) is correct;
  it lets Whisper (blocking + retry + cancel-flag), Cloud (future-drop timeout), and Parakeet
  (load-then-transcribe) keep incompatible control flow. Unify *into* it, don't replace it. (PA-1)
- Don't merge the two sidecar IPC layers (Vulkan one-shot vs Swift long-lived+streaming). (PA-2)
- Don't merge the two normalizers — deliberate fast-path/fallback pair (but see D4). (PA-3)
- `commands/ai.rs` and `writing.rs` are large-but-cohesive — leave them (writing.rs is half tests,
  zero tauri commands; ai.rs is a coherent provider module).
- `tauri-specta` codegen for settings is the "correct" long-term fix but is infra churn — defer.
