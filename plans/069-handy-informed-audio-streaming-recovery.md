# Plan 069 — Handy-informed audio and streaming recovery

Status: DONE — research consolidation, 2026-09-22. Product recovery stages below
are proposed and have not been executed by this documentation change.

## Working home and provenance

- Worktree: `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-integration`.
- Branch: `feat/049-pure-rust-audio`.
- Integration application baseline: `58ec176adc34e4e03ac9b3930dade68f13acd43f`.
- Main baseline: `c47e1465f7ef34ac2ca31b5b5b226981b41b17ab`.
- Source: `research/handy-teardown` at `af63ab13`, plus 15 previously untracked
  Markdown documents. All 15 are preserved byte-for-byte under
  [Handy research](../docs/handy-teardown/00-README.md) and
  [performance research](../docs/voicetypr-perf/00-MASTER-ROADMAP.md).
- [Import manifest](../docs/research-source-manifest.json): source paths,
  document sizes, and SHA-256 hashes. Originals remain in the research worktree.

The user's direction on 2026-09-22 is to combine these two lines of work:
learn from Handy and continue building in the integration worktree. This is
the current entry point. Historical instructions about particular agents,
models, timing, and branches are context; current repository instructions and
the user's current decisions control new work.

## Product direction and lessons to retain

1. **Make progress visible while speaking.** Stable committed text plus a
   revisable tentative tail is the useful interaction. Measure first text,
   stable-prefix progress, and stop-to-insertion separately.
2. **Keep capture and lifecycle predictable.** Bounded ingress, explicit queue
   overflow behavior, accepted audio before finalize, immediate cancellation,
   session/revision gating, and terminal events protect recordings. Decode
   and UI work belong outside the realtime callback.
3. **Protect the final transcript.** Preview quality, authoritative final text,
   Polish, and insertion are separate contracts. A quick first partial does not
   prove final words survived or text reached the intended application.
4. **Preserve focus and clipboard behavior.** The non-activating pill, focus
   ownership, back-to-back dictation, and clipboard restoration are part of
   the result, including when preview is off.
5. **Measure before optimizing.** Use fixed multilingual inputs, word-error and
   last-word checks, cold/warm runs, p50/p95 latency, CPU/RSS, and exact
   source/model/binary identifiers. Preserve language and translation intent.
6. **Deliver useful slices.** Clear model readiness, errors, and settings matter
   more than copying Handy's architecture wholesale. Preserve VoiceTypr's
   existing product direction.

Sources: [Handy synthesis](../docs/handy-teardown/00-README.md),
[streaming review](../docs/handy-teardown/06-oracle-decision.md),
[performance roadmap](../docs/voicetypr-perf/00-MASTER-ROADMAP.md), and
[pressure test](../docs/voicetypr-perf/08-roadmap-review.md).

## Research-to-implementation map

"Implemented" means source exists at the integration application baseline.
It is not a fresh automated, packaged-app, or release validation claim.

Live preview stays in the pill; final insertion happens once at stop. Local
Whisper/Parakeet batch output remains authoritative. Soniox/Deepgram can use a
proven complete WebSocket final; incomplete streaming falls back to batch.
Fallback after partial provider processing needs billing verification too.

| Research idea | Integration evidence | Disposition |
|---|---|---|
| Bounded audio tap and ordered worker | [stream_tap.rs](../src-tauri/src/audio/stream_tap.rs), `d80f8e73` | Implemented; preserve finalize/overflow/cancel contracts. |
| Session/revision and committed-prefix contract | [stream.rs](../src-tauri/src/transcription/stream.rs), `1a267833` | Implemented; prove invariants through each actual adapter. |
| Lightweight pill and committed/tentative preview | [pill.tsx](../src/pill.tsx), `94bb3e78`, `0969b2ca` | Implemented; reconcile modern main's pill/settings and inspect rendered behavior. |
| Stop polling, bounded level meter, guarded logging | [plan 030](030-perf-tier1-free-wins.md), `35cd1c51` | Implemented; retain measured wins without changing timeout semantics. |
| Latency/WER harness and Parakeet warm prediction | [plan 032](032-t10-latency-wer-harness.md), [plan 033](033-parakeet-warm-predict.md), `2c06683d` | Tooling exists; refresh exact-binary/model evidence. |
| Adaptive Whisper audio context and explicit speed mode | [Whisper plan 044](044-whisper-wer-wins.md), `fd464b9d` | Implemented; require multilingual and last-word checks. |
| In-process audio decoding and smaller packages | [decode.rs](../src-tauri/src/audio/decode.rs), `07d461e6` through `dada3f14` | Implemented; verify codec/container coverage and packaged size. libopus is native code; "pure Rust" is shorthand for removing FFmpeg. |
| Whisper live preview | [decode_ahead.rs](../src-tauri/src/whisper/decode_ahead.rs), [decode_stream.rs](../src-tauri/src/whisper/decode_stream.rs), `0cda85a7` and follow-ups | Implemented; validate cancellation, tail preservation, resource use, and platform routes. |
| Soniox / Deepgram streaming and final authority | [soniox_ws.rs](../src-tauri/src/cloud_stt/soniox_ws.rs), [deepgram_ws.rs](../src-tauri/src/cloud_stt/deepgram_ws.rs), `cc5421a9`, `b2fcd18f` | Implemented; live handshake, disconnect, fallback, and billing behavior need proof. |
| Parakeet decode-ahead and native models | [catalog](../src-tauri/src/parakeet/models.rs), [routing](../src-tauri/src/commands/audio.rs), [sidecar](../sidecar/parakeet-swift/Sources/main.swift), `b8ff14cf`, `58ec176a` | TDT routes to decode-ahead; Unified English and Nemotron route to their native engines. Validate separately. |
| Onboarding, typed IPC, lifecycle coordinator, model lifecycle improvements | Historical Handy recommendations; [structural plan](050-structural-refactor.md) | Separate opportunities after recovery; recheck current main first. |

Four feature commits are beyond the published branch baseline `d6c8d0dc`:
`cc5421a9`, `b2fcd18f`, `b8ff14cf`, `58ec176a`. Consolidation adds documentation
commits after those. No push is part of this consolidation.

## Evidence that needs reconciliation

- **Old research says batch-only:** that predates the integration stack. Reuse
  its rationale and acceptance criteria, not its implementation status.
- **FFmpeg retention versus removal:** research warns about unsupported formats;
  later code adds libopus and removes FFmpeg. Decide the supported codec/container
  matrix explicitly and prove it with real files, corrupt/truncated inputs,
  long files, and resource bounds. Libopus alone does not resolve WMA/HE-AAC or
  every other format concern. The [size report](../docs/reports/2026-07-pure-rust-audio.md)
  is historical; its universal format and "cannot crash" claims are not proof.
- **Parakeet plan versus latest source:** [plan 051](051-parakeet-decode-ahead.md)
  describes decode-ahead as the public path and old EOU as dormant. `58ec176a`
  subsequently adds selectable Unified English and Nemotron native paths.
  Neither the old EOU failure nor the old decode-ahead timing proves these paths.
- **Timing versus quality:** untracked `.tmp/plan051-corrected-bench-results.json`
  records roughly one-second first partials, but also `ref_similar: false`,
  long-clip `batch_match: false`, and a cadence failure. Another local report,
  `.tmp/margin-bench-results.json`, marks visibly different text as a match.
  Review comparison semantics and refresh measurements with exact provenance.
  These files establish unresolved evidence, not a current regression verdict
  or a current quality pass.
- **Historical handoff:** local `docs/HANDOFF-2026-07-10.md` predates the July 14
  commits; its "Parakeet uncommitted" statement is obsolete. That handoff,
  `.tmp`, `perf-corpus`, and `perf-report.*` files remain untouched and local.
- **Plan collisions:** integration 030/031/032/033/044/046–051 overlap different
  work on main. Use full filenames plus commit IDs now; reconcile names, ledger
  rows, and links during main integration. This plan uses 069, after main's
  highest occupied number (068 at the recorded baseline).

## Continuation order

### 1. Establish an exact recovery baseline

Inventory the four local feature commits and source/binary/model versions;
preserve existing artifacts. Benchmark output must identify Git SHA, dirty
state, sidecar hash, model ID, input hash, and actual input duration. Compare
current main and the integration baseline before making performance claims.
Do not infer duration from names such as `en-5s`; saved inputs can be longer.

### 2. Reconcile with current main

Refresh both heads. Keep this worktree as the home, and decide between a
reviewed rebase and selective porting from the overlap analysis; neither was
executed here. Preserve the old branch tip before any history rewrite. Resolve
overlapping behavior deliberately rather than replacing newer main files
with old branch copies. Preserve at least:

- Whisper cancellation callback correction (`2329b32a`), generation races,
  cleanup, and error handling (`4f9e497d`, `a2970b87`).
- Windows license/identity preservation and quiet-input recovery (`94a3690b`,
  `a2970b87`), GPU fallback, and injected-hotkey behavior.
- Speech evidence/no-speech policy (`deb8768b`, `6fce2a98`, `92c02a21`):
  uncertain or soft speech remains fail-open; preview/VAD must not silently
  discard audio needed by authoritative transcription.
- Modern Polish/app rules/CLI providers, privacy-safe analytics, current
  frontend organization, media pause, tray recovery, and history/upload flows.
- Stable/Beta updater contracts, release tooling, CI, dependencies, and
  platform packaging. The branch's old version and historical green CI do
  not prove a new candidate works.

Port/reconcile the shared contract and audio substrate first, then engine
adapters in bounded slices. Gate pure-Rust decode on format coverage. Resolve
plan collisions and run affected automated checks on the combined tree.

### 3. Complete and prove existing flows

Use reproduced failures or conflicting evidence to choose fixes. Reuse the
existing stream contract. Keep the structural refactor as separate work.

| Flow | Required observations |
|---|---|
| Batch + preview off | Correct final text, no lost short/soft speech, no stop-latency/resource regression. |
| Parakeet TDT / Unified / Nemotron | Download/load/select; actual routed engine; language/translation behavior; short/long/silence-tail accuracy; cancel/restart; batch comparison where applicable. |
| Soniox, then Deepgram | Live handshake, proven terminal completion, final insertion once, REST fallback on gaps/timeout/network loss, normal/fallback billing behavior. |
| Whisper preview | First-partial/stable-prefix timing, multilingual final accuracy, no stale stop/cancel events, supported platform/acceleration routes, preserved main cancellation fix. |
| Pill + insertion | Stable committed prefix, bounded tentative text, original target focus, single final insertion, clipboard preservation, immediate cancel, rapid consecutive recordings. |
| Imported recordings | Real supported formats, malformed/truncated inputs, large-file memory bounds, accurate duration, cleanup, and readable errors. |

Run focused checks during implementation, then batch full frontend/backend and
packaging gates around coherent completed work. Record manual cases/results in
`plans/SMOKE.md` with exact build and platform. Keep local, CI, packaged runtime,
Beta upgrade, and release acceptance separate. Product changes after a published
Beta require a new candidate and affected smoke.

### 4. Choose the next product experiments

Useful choices after recovery: preview legibility and model-capability copy;
measured startup/idle footprint; cold-model preparation; and targeted
architecture work where complexity obstructs delivery. New providers,
always-on capture, wholesale onboarding changes, a full IPC migration, and a
coordinator rewrite need their own bounded scope and evidence.

Other concrete research candidates are binary PCM sidecar transport (current
transport is newline JSON/base64), stream-lock contention during cancellation
or model switching, resumable/checksummed model assets, idle model unload, and
adaptive Soniox REST fallback polling. Profile or reproduce the relevant cost
before selecting any of these. Keep transcribe-cpp as a research reference;
the current engine stack remains the recovery target.

## Consolidation verification

- All 15 research documents imported unchanged with a SHA-256 manifest.
- Research map checked against source/commits at the recorded baselines.
- Original research and existing benchmark/corpus/handoff files preserved.
- All 32 authored-document local links, import hashes, ledger consistency,
  authored-file whitespace, and the scoped Git diff were checked. The full
  staged whitespace check reports one existing trailing space in the unchanged
  source `docs/voicetypr-perf/04-parakeet-engine.md:240`; it is deliberately
  preserved to keep the historical documents byte-identical.
- No application code, dependency, runtime setting, or release artifact changed.
  Builds, tests, provider calls, and hardware smoke were not run for this
  documentation-only consolidation.
