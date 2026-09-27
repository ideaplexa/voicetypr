# Plan 050 — Structural refactor: god-file split + engine unification + settings hardening

> Reconciles TWO verified sources: the arch roadmap (`docs/review/2026-07-03-arch-roadmap.md`,
> waves 0/1/3a ALREADY SHIPPED via plans 046/047/048) and the fresh deep-review
> (`docs/review/2026-07-architecture-deep-review.md`, 3 parallel audits run 2026-07-06 against
> the post-046/047/048 + post-049 tree — its file:line refs are the current anchors).
> This plan is the remaining structural work, phased so every slice ships independently,
> behavior-preserving, gated. **No big-bang** (CLAUDE.local.md rule).
>
> ⚠️ NOT for the campaign PR #109. Own branch: `refactor/050-structure` off
> `integrate/perf-ux-2026-07` AFTER feat/049 merges. Worktree: voicetypr-integration
> (or a fresh one if 049 smoke drags on).
>
> ⚠️ Already shipped — do NOT re-do: #25/#24/#8/#16-discrete (046), #18 dead bytes command +
> #21 engine layer out of audio.rs (047), #16-full uploads-through-executor (048), #12/#15
> (slice 045), #28 ggml-metal exit assert (RunEvent::Exit cache clear, this campaign),
> D1/D3/D4-types safe-wins (fc13fa0). #14b line_json extraction stays DEFERRED (high risk,
> not a blocker — roadmap §7). #14a capability routing belongs to plan 043, not here.
>
> Guardrails from the deep-review (binding): NO uniform `trait Transcriber` — the executor's
> enum dispatch (executor.rs:176-326) is correct; unify INTO it. Don't merge the 2 sidecar
> IPC layers. Don't merge the 2 normalizers. ai.rs + writing.rs stay (cohesive).

## Execution model

Claude reasons/plans/gates, GLM 5.2 (omp) implements per-slice specs, Codex second-opinions
each phase's diff. Per-slice gate: `cargo build` + `clippy --workspace --all-targets -D warnings`
+ `cargo test` + `pnpm typecheck/lint/test` + windows-check dispatch. One commit per slice,
`pub use` shims keep external paths stable. Any slice that can't stay behavior-identical →
STOP and report, don't improvise.

## Phase A — Safe mechanical extractions (audio.rs 7182 → ~5k) + leaf dedup

Order matters (later slices import earlier ones). Each is structure-only.
**Codex review (SOUND-WITH-CHANGES, 2026-07-06) applied:** slices are delimited **by symbol,
not line range** (several cited ranges overlap in the current tree — the implementer greps
each symbol); A7 extracts BEFORE A6, and A6 BEFORE A5 (history saves call generation-aware
helpers, e.g. `persist_if_current`; the preview sink sits inside A6's old range).

1. **A1 · pill_toast → `commands/pill_toast.rs`** (S, low). Symbols: TOAST_ID_COUNTER,
   `PillToastAction/Variant/EventPayload`, `next_toast_id`…`should_hide_pill`,
   `emit_recording_too_short_feedback`. Verified self-contained (only `AppHandle` + emit).
   7 modules import these via audio.rs — keep shim.
2. **A2 · failure classification → `commands/transcription_failure.rs`** (M, low). Symbols
   ONLY: TranscriptionFailure impl, remote_client_error_kind, LocalFailureKind,
   classify_local_failure, ai_failure_*, is_ai_auth_error, notify_ai_polish_failure,
   is_non_speech_transcript. (The old 983-1441 range also contained builder code — that is
   A4's, split by symbol.) Near-pure; tests move with it.
3. **A3 · retranscription status → `commands/retranscription.rs`** (S-M, low). Symbols:
   NormalizedTempFile, RETRANSCRIPTION_SESSION_MARKER, TranscriptionStatus,
   apply_retranscription_status, sync_retranscription_failure_metadata, parse_history_key.
4. **A4 · request/result builders → `commands/transcription_request_build.rs`** (M, **med** —
   touches executor request contracts, AppState cancellation, ProviderEngine mapping,
   writing metadata, diarized upload formatting, remote context). Symbols:
   build_transcription_job, build_desktop/remote_*_request, compile_remote_request_context
   [re-export: remote.rs calls it], build_remote_transcription_result,
   build_writing/translation_*_metadata, plan_desktop_writing_success, load_ai_enabled.
5. **A7 · Parakeet preview sink → `commands/parakeet_stream_sink.rs`** (S, low). Symbols:
   ParakeetPreviewStreamSink + StreamTapSink impl, emit_parakeet_stream_event,
   build_parakeet_stream_sink_factory. NOTE: plan 043 renames/generalizes this — if 043
   lands first, re-anchor. (Ordered before A6: it occupies the same top-of-file region.)
6. **A6 · recording generation + in-flight tracking → `commands/recording_generation.rs`**
   (S, med — concurrency-load-bearing, mechanical move only). Symbols: RECORDING_GENERATION,
   begin/current_recording_generation, recording_generation_is_stale,
   IN_FLIGHT_TRANSCRIPTION_AUDIO, clear_in_flight…, revoke_saved_recording, StopInFlightGuard,
   stop_should_reset_to_idle, transcription_task_in_flight,
   take_and_remove_current_recording_path. stream_tap.rs + transcription/stream.rs depend via shim.
7. **A5 · history persistence → `commands/transcription_history.rs`** (M, **med** — emits
   frontend events, refreshes tray menus, dedups rows, reconciles stale retranscriptions,
   uses generation commit guards). Symbols: the "transcriptions"-store functions
   (save_transcription*, save_failed_*, get_/delete_/clear_/update_, save_retranscription,
   cleanup_old_transcriptions). Sole touchers of that store. REQUIRES A1+A6 landed.
8. **A8 · leaf dedup (deep-review F4+F5)** (S, **low-med** — the gpu_sidecar copies are
   Windows-exercised; windows-check MUST gate this slice): one `seconds_to_duration_ms`
   (fold the f64 gpu_sidecar copy + engines.rs copy — engines.rs already has the finite
   guard from fc13fa0), one segment→TranscriptionSegment mapper, one `wav_duration_ms`
   (parakeet/messages.rs, gpu_sidecar.rs, executor.rs), one `transcription_budget()` +
   shared TIMEOUT consts (engines.rs, gpu_sidecar.rs, cloud_stt/common.rs,
   parakeet/messages.rs). Keep the sidecar's *layered* internal deadline — share only the formula.
9. **A9 · warm-up dedup (F6)** (S, **low-med** — the two warms differ: cloud uses the
   https-only shared client, AI has provider/key resolution): one `warm_origin(client, url)`;
   keep the two client pools (intentional). Collapse the twin spawn blocks at
   audio.rs:3827/3843.

## Phase B — Engine unification (roadmap Wave-3 tail = deep-review F1/F2/F3)

10. **B1 · remote-send helper (F3/#19)** (S-M, low): one
    `send_remote_transcription(...)` for the 4 copies (audio.rs:5111 + 6477, remote.rs:1583,
    cli.rs:789). Pure extraction first; executor `Explicit{Remote}` route (Stage 5) only if
    trivially clean afterwards.
11. **B2 · single engine resolver (F2/#22)** (M, **med-high** — NOT a mechanical fold: the
    stop-path inline resolver owns fallback selection, remote precedence, missing-model
    aborts, cloud key checks, AND UI fallback events): fold the pure decision logic from
    stop_recording's inline resolver (audio.rs ~4620-4849) into
    `resolve_engine_for_model` (engines.rs); delete the inline copy; kill the
    double-resolve-per-dictation. Resolver returns a decision enum (incl. fallback-used and
    abort reasons); ALL UI side-effects (model-fallback emit, toasts) stay at the call site.
12. **B3 · remote host through executor (F1/#17, Stage 4 `HostDefault`)** (L, med-high):
    remote/transcription.rs:276-482 re-implements the Whisper+Parakeet flow with its own
    TranscriberCache (2 caches, 2 mutex flavors). **Design prerequisite (Codex):**
    `EngineSelection::HostDefault` currently carries NO shared-model snapshot and is
    explicitly rejected in executor.rs:123 — B3 must first extend the request contract so
    the host's `SharedModelState { model_name, model_path, engine }`
    (remote/transcription.rs:276) reaches executor resolution (e.g.
    `HostDefault { snapshot }` variant or a resolved-selection pre-pass). Then route
    ServerContext::transcribe → `block_on(transcribe_with_app)` (it already block_ons
    Parakeet). Preserves: host serialization semantics, custom-vocab parity, kills double
    RAM. **Acceptance MUST include:** unknown/unsupported engine id → typed error (current
    code silently falls through to Whisper for any non-parakeet engine,
    remote/transcription.rs:331 — that silent fallback dies here); Windows host gains
    GPU-path parity (currently CPU-only).
13. **B4 · #13 ClientSlot** (M, med): stream session stops holding the ParakeetClient RwLock
    write guard for the whole recording (typed Busy instead).

## Phase C — Settings hardening (roadmap Wave-4 = deep-review S2/D2)

14. **C0 · D2 DECISION FIRST (gate for C1/C2)**: today TS AppSettings marks ~21 fields
    optional that Rust serde requires — defended only by SettingsContext's full-spread.
    Options: (a) Rust-side read-merge-write partial save (**BEHAVIOR CHANGE** — needs
    store-lock semantics so concurrent saves don't interleave); (b) make the TS fields
    required (type-only; matches Rust; churns test fixtures). Neither is default — the
    owner + Codex decide BEFORE any C implementation. Do NOT do both.
15. **C1 · settings key constants (S2/#11-lite)** (M, **low-med**): `settings_keys` const
    module for the ~40 bare-string keys across 17 modules; mechanical swap. The CI grep
    gate must be SCOPED (Codex: a blanket `.get("`/`.set("` literal ban false-positives on
    legitimate non-settings stores — remote settings, transcriptions store, menu, AI) —
    gate only the `"settings"` store accessor paths, or use an allowlist file.
16. **C2 · implement whichever C0 chose** (M, med if (a) / S if (b)).
17. **C3 · defaults single-sourcing (S6)** (S, low): collapse the 3 Rust "auto"/mode default
    definitions; add the Rust↔TS voice-commands default-parity test (writing.ts:46 footgun).
18. **DEFER: full typed-settings module + versioned migration (#11+#10 heavy form) and
    tauri-specta codegen** — real fix, real churn; own plan when settings next hurt.

## Phase D — The monsters (highest risk, LAST, each its own smoke)

19. **D-1 · #20 stop_recording decomposition** (L, HIGH): extract the ~930-line spawned
    delivery task (audio.rs ~5073+) → `deliver_transcription(...)` + pure
    `delivery_disposition` with unit tests. Encodes 3 cancellation/stale-generation race
    rechecks — behavior-identical or STOP. Requires A1-A6 landed (delivery calls toast,
    failure, request/result, history, and generation helpers throughout).
    Characterization tests (plan 003) must pass unchanged.
20. **D-2 · start_recording slim** (L, med-high): validate→configure→arm helpers
    (audio.rs:3622-4340).
21. **D-3 · save_settings side-effects split** (M, med): persistence vs
    `apply_settings_side_effects` tail (settings.rs:573-983); ordering preserved.
22. **D-4 · lib.rs run() split** (M-L, med): state-init block → setup module;
    perform_startup_checks → startup.rs; .manage() ordering preserved exactly.

## Investigations (bounded, not slices)

- **I1 · D4-normalization asymmetry**: uploads/executor path skips normalizer.rs gain+dither
  that local recordings get. Run the WER harness A/B on real upload fixtures; only if it
  measurably hurts accuracy does a fix slice get authored. (Whisper-Metal is broken on the
  dev box — run via CI/other machine or Parakeet-only.)
- **I2 · roadmap Wave-5 leftovers** (#2+5 ModelCatalogContext, #27 error-surface, frontend
  S-batch): re-triage after Phase D; they block nothing.
- **I3 · #23 lifecycle statics → AppState** (explicitly deferred, per Codex ask): A6 only
  MOVES the process-global statics (RECORDING_GENERATION etc., with callers in
  stream_tap.rs, transcription/stream.rs, hotkeys.rs) into their own module — migrating
  them INTO AppState is behavior-adjacent (init order, test isolation) and waits until
  after Phase D, when deliver_transcription gives them a single owner. If the known test
  flake bites first, the roadmap's shared-test-lock interim applies.

## Codex plan review

2026-07-06: **SOUND-WITH-CHANGES** — all requested changes applied above: A-slice reorder
(A7→A6→A5) + symbol-based delimitation, risk raises (A4/A5/A8/A9/B2/C1), B3 HostDefault
snapshot design prerequisite + unknown-engine typed-error acceptance, C0 decision gate
before C1/C2, #23 explicit deferral. Also confirmed by Codex: 046/047/048/#24/#25 done in
tree; D1's guard fixed but gpu_sidecar dedup still pending (A8); D3/S3/S4 type drifts fixed;
#14a intentionally lives in plan 043, not here.

## Sequencing vs plan 043 (Soniox streaming)

Independent. 043 needs NONE of this (correction §7: its enabler #14a folds into 043 itself).
Either order works; if 043 goes first, re-anchor A7 and B-phase line refs (they touch the
same audio.rs regions). Do NOT run both concurrently in the same files.

## Acceptance (every phase)

- Zero behavior delta (test suite passes UNCHANGED; no test edits except moves).
- Gates: cargo build/clippy -D warnings/test, pnpm typecheck/lint/test, windows-check green.
- audio.rs LOC checkpoint after Phase A (~5k) and Phase D (~3k target).
- Phase-end Codex review; desktop smoke (SMOKE.md protocol) after B3 and each D slice.
