# Oracle Architecture Decision — VoiceTypr speed + smooth streaming UX vs Handy

## Verdict

VoiceTypr should **not chase Handy by adopting Handy’s Whisper stack**; it should copy Handy’s *interaction contract and concurrency invariants* while exploiting VT’s multi-engine reality. The winning sequence is: **finish/re-baseline Phase 0 stop→insert plumbing first**, because Parakeet’s short-dictation latency is dominated by app tail rather than ANE inference; then build a **session-scoped streaming substrate + committed/tentative pill preview** as one vertical slice; then choose **Parakeet SlidingWindow only if it passes a short-dictation measurement gate**, otherwise ship **Parakeet StreamingEOU** despite the new CoreML activation cost. Keep final insertion **once at stop**. Use `transcribe-cpp` as a reference/benchmark only, not as a production dependency, because VT must normalize Whisper, Parakeet, and cloud partials through one engine-agnostic event contract. This validates plan 028’s broad direction, but it reorders its emphasis: **Phase 5’s live pill is not polish; it is the product value of streaming and must move forward.**

> Sources read: `local://oracle-brief-handy-vt-streaming.md`, `plans/028-transcription-latency-streaming.md`, `local://oracle-round1-slowmodel.md`, Handy teardown `00-README.md`, `01-streaming-engine.md`, `02-overlay-ux.md`, plus targeted VT checks in `src-tauri/src/commands/text.rs`, `src-tauri/src/commands/audio.rs`, `src-tauri/src/audio/recorder.rs`, `src-tauri/src/parakeet/messages.rs`, `sidecar/parakeet-swift/Sources/main.swift`, `src-tauri/src/whisper/cache.rs`, `src-tauri/src/window_manager.rs`, and pill frontend files. One important current-code note: parts of plan 028 Phase 0 appear to have already landed in the live tree — deferred guarded clipboard restore, CGEvent paste, and in-process normalization-first path — so the first implementation step is a **Phase 0 re-baseline + remaining-tail cleanup**, not blindly repeating the older 2026-06-20 latency table.

---

## Per-question positions

### 1. Does Handy validate plan 028’s ordering, or argue for a different sequence/emphasis? — **P1**

**Position: Handy validates plan 028’s architecture, but not its exact ordering.** Keep **Phase 0 first**, but pull **live pill preview / committed-tentative UX** forward from plan 028 Phase 5 into the first streaming vertical.

Plan 028 is right that VT must first remove self-inflicted tail latency before adding streaming: the brief and plan both identify batch-only recording, normalization, decode, and clipboard paste as the current user-visible path, and plan 028’s Phase 0 is the safest all-engine win. For Parakeet specifically, the brief says latency is “~100% plumbing” because FluidAudio TDT batch is very fast on Apple Silicon. That means starting with streaming would risk a bad UX: live text appears while speaking, then the app still hesitates before final insertion.

But Handy’s teardown changes the emphasis. Handy’s perceived “instant” feel is not just decode-ahead; it is the **visible streaming contract**: `StreamTextEvent { committed, tentative }`, append-only committed prefix, volatile tentative tail, rendered as two stable sibling spans. `00-README.md`, `01-streaming-engine.md`, and `02-overlay-ux.md` all point to this as the core perceptual mechanism. So plan 028’s “Phase 5 live partial UX” should not remain late polish after all engines stream. The revised order is:

1. **Phase 0 re-baseline / finish remaining plumbing.**
2. **Streaming substrate with session events and correctness gates.**
3. **Committed/tentative pill preview in the same vertical as the first streaming engine.**
4. Engine expansion: Parakeet, then cloud/Whisper by user mix and risk.
5. Type-as-you-speak remains deferred.

Current-code check nuance: plan 028’s Phase 0 table is partly stale in the live repo. `text.rs:178-352,437-481,591-680` already shows deferred guarded clipboard restore and macOS CGEvent paste with short sleeps, and `audio.rs:4901-5026` tries in-process normalization before ffmpeg fallback. That strengthens, not weakens, the decision: **measure current stop→insert first**, finish the remaining tail, then stream.

---

### 2. Minimum viable path to “feels like Handy” — **P1**

**Position: Minimum viable Handy feel is Phase 0 + streaming substrate + non-activating committed/tentative pill + one genuinely low-latency partial engine + final insert at stop. It is *not* type-as-you-speak. It is *not* default Parakeet SlidingWindow unless measured.**

The smallest user-perceived Handy-level jump is:

- **Fast final insertion after manual stop** from Phase 0 / remaining-tail cleanup.
- **Live preview while speaking** in the pill, not the target editor.
- **Committed/tentative split** with monotonic committed prefix and rewritable tentative tail.
- **Non-activating overlay** so the target app keeps focus.
- **Single final paste at stop**.

Handy itself does **not** type into the editor as the user speaks; it previews live text and inserts once at stop. Therefore type-as-you-speak is not part of the MVP and should stay out of scope until VT has a controlled editor abstraction or app-specific integrations.

On Parakeet: **Phase 0 + default SlidingWindow pill preview is not enough to promise Handy feel.** Plan 028 says FluidAudio `SlidingWindowAsrManager` reuses the current TDT v3 model but has defaults like 15s chunk / 10s min-confirm; the round-1 pressure test correctly flags that as likely too slow for normal 2–8s dictation. The path can be:

- **SlidingWindow first only as a measured bridge** if it can be tuned into low-latency short-dictation behavior.
- **StreamingEOU first for user-facing Handy parity** if SlidingWindow fails the gate below.

Decision gate for SlidingWindow-vs-EOU:

| Metric | SlidingWindow must hit to ship as “Handy-like” MVP | If it fails |
|---|---:|---|
| First usable partial from first VAD speech frame | p50 ≤ **700 ms**, p95 ≤ **1200 ms** | Use EOU |
| First stable/committed useful prefix, ≥1 real word or ≥8 chars | p50 ≤ **1800 ms**, p95 ≤ **2500 ms** on 2–8s utterances | Use EOU |
| Final text available after manual stop, after Phase 0 tail cleanup | p50 ≤ **300 ms**, p95 ≤ **600 ms** | Fix pipeline/engine before UX launch |
| Final WER delta vs current batch Parakeet on acceptance corpus | ≤ **+1.5 absolute WER points** or ≤ **+10% relative**, and no slice worse than **+3 absolute points** | Do not ship as default |
| Short utterance behavior | No “wait for 10s confirm window” for 2–4s dictation | Use EOU |

If SlidingWindow hits these numbers, ship it because it avoids a new model download. If it misses any P1 metric, do not let “no new asset” dictate product quality; ship EOU with proper activation UX.

---

### 3. Build vs reuse: adopt `transcribe-cpp` or reimplement stable-prefix/decode-ahead? — **P1**

**Position: Reject `transcribe-cpp` as a production dependency; use it as reference-only. Reimplement stable-prefix reduction and committed/tentative events at VT’s engine-agnostic layer.**

`transcribe-cpp` is the right answer for Handy’s narrower world: Whisper-only, crate-provided `StreamText { committed, tentative }`, stable-prefix policy, and one native runtime. VT is different:

- Whisper local via `whisper-rs 0.16` plus Windows Vulkan sidecar.
- Parakeet via a long-lived Swift/FluidAudio CoreML sidecar.
- Cloud REST today, with Soniox/Deepgram/OpenAI streaming possibilities.

Adopting `transcribe-cpp` would add a second native C++ Whisper stack beside `whisper-rs`, risk version/build skew, complicate packaging/notarization, and likely not fit the Windows Vulkan sidecar architecture. It also gives **zero help** for Parakeet or cloud, where VT still needs the same frontend event contract and session lifecycle.

The correct VT architecture is:

```rust
enum TranscriptionStreamEvent {
    Started { session_id, engine, revision },
    Partial { session_id, revision, committed, tentative },
    Final { session_id, revision, text, metadata },
    Cancelled { session_id, revision },
    Error { session_id, revision, error },
}
```

Then each engine maps into that:

- Parakeet EOU native partial/end-of-utterance → committed/tentative/final.
- Parakeet SlidingWindow repeated hypotheses → VT stable-prefix reducer.
- Whisper decode-ahead segments → conservative committed completed segments + tentative tail.
- Soniox/Deepgram/OpenAI WS → provider final tokens/segments as committed, interim as tentative.
- Batch-only engines → final-only event.

Use `transcribe-cpp` to compare behavior and as a reference oracle for stable-prefix agreement, but do not put it in the shipping dependency graph unless a later measured spike proves it beats VT’s own reducer enough to justify a second Whisper backend. My current bar for that exception is high: it would need a material latency/quality win on Whisper **and** a clean Windows Vulkan story.

---

### 4. Where Handy is genuinely ahead, and where VT can leapfrog — **P1/P2/P3**

**Position: Handy is ahead in streaming substrate discipline and focus-safe live UX; VT can leapfrog with Parakeet EOU on ANE and cloud WS partials if it does not copy Handy’s Whisper-only architecture.**

#### Handy genuinely ahead — must copy principles

**P1 — FIFO stream worker topology.** `01-streaming-engine.md` shows Handy’s single `mpsc` carrying `Feed`, `Finalize`, and `Cancel`, so finalization cannot overtake the last audio frame. VT’s Phase 1 cannot be just “add a PCM tap”; it must include **one ordered per-session command queue**.

**P1 — engine lease / no decode mutex.** Handy leases the engine out of the manager mutex during decode. VT’s `src-tauri/src/whisper/cache.rs:1-120` shows a single-model cache; decode-ahead must not hold a cache/model mutex across repeated `full()` calls. The streaming worker needs an actor/lease pattern, especially for Whisper and the Parakeet sidecar.

**P1 — session/generation gating.** Handy uses worker ids and token-checked cleanup. VT already has a strong generation-gated persistence chokepoint in `audio.rs:62-164` (`RECORDING_GENERATION`, `persist_if_current`, `delivery_aborted`), and that should be extended to partials/finals, not bypassed.

**P1 — non-activating overlay.** Handy’s `NSPanel` discipline in `02-overlay-ux.md` is product correctness, not polish. VT appears partially aligned (`window_manager.rs` uses `focused(false)` and NSPanel conversion; `PillShell.tsx` has `pointer-events-none`), but streaming preview must preserve “target editor retains focus” as a release criterion.

**P1 — committed/tentative visual contract.** Handy’s two sibling spans are the highest perceptual ROI. VT has pill bars/status today (`usePillController.ts` listens to `audio-level`; no stream text), so this contract must be added.

**P2 — VAD-gated stream with pre-roll/hangover.** Handy’s Silero VAD prevents silence churn. VT should gate streaming feed but retain enough raw/final audio safety and make manual stop finalize immediately.

**P2 — typed IPC contract.** Handy’s specta-generated event bindings reduce drift. VT does not need to adopt specta before Phase 0, but streaming events must be strongly typed and versioned. Full specta adoption is useful but not the first speed win.

#### Where VT can leapfrog Handy

**P1 — Parakeet StreamingEOU on Apple Silicon.** Handy’s Whisper-only path cannot match a good ANE/RNNT/EOU path on latency, battery, and thermals if VT integrates FluidAudio streaming correctly.

**P1/P2 — Cloud WebSocket partials.** Soniox REST currently has a 1s polling floor per plan 028; Soniox/Deepgram/OpenAI WS can provide native partial/final events that map cleanly to VT’s committed/tentative contract.

**P2 — Multi-engine abstraction.** Handy’s product is elegant but engine-specific. VT can provide one UX across local fast Parakeet, cross-platform Whisper, and cloud accuracy/language options.

**P2 — Existing generation-gated delivery.** VT already has robust final-side-effect gating in current code; if reused for streaming sessions, VT can avoid stale-paste classes from day one.

---

### 5. Is the honest headline “Phase 0 gets most perceived speed for Parakeet users today; streaming is mainly for Whisper decode-bound + long-form + smooth live-preview feel”? — **P1**

**Position: Yes, with a wording correction: Phase 0 gets most of the *stop-to-text* speed for Parakeet users; streaming gets the *while-speaking smoothness* and helps Whisper/cloud hide decode/provider latency.**

Do not message “streaming makes Parakeet fast.” Parakeet is already fast enough in batch for short clips; VT’s app plumbing is what made it feel slow. The user-facing message should be:

> First we remove artificial delay after you stop speaking. Then we add live preview so dictation feels immediate while you speak.

This distinction matters for prioritization:

- **Parakeet short dictation:** Phase 0 / remaining-tail cleanup is the main stop→insert win; EOU/SlidingWindow is mainly live confidence and long-form comfort.
- **Whisper:** Phase 0 helps, but decode-ahead can materially reduce post-stop waiting because decoding starts before stop.
- **Cloud:** Phase 0 helps paste; WS streaming removes REST upload/poll/finalization uncertainty, especially Soniox’s 1s floor.

Because current live code already appears to have landed some Phase 0 pieces, the honest next headline is even sharper: **prove the current Parakeet p50/p95 stop→insert numbers before starting streaming, then spend engineering only on the remaining measured tail.**

---

### 6. Correctness/UX traps exposed by the teardown — **P1/P2**

**Position: These are release blockers, not polish. Any streaming path that violates them should fall back to batch/final-only.**

**P1 traps:**

1. **Monotonic committed prefix.** Once text is in `committed`, it must never shrink or rewrite. Add debug/property assertions and preferably commit by segments/time units, not byte prefixes.
2. **Session-id / generation gating.** Every partial/final/error must carry session id + revision. Frontend and paste coordinator must drop stale events. Reuse VT’s existing `RECORDING_GENERATION` / `persist_if_current` discipline from `audio.rs:62-164`.
3. **Finalize-after-last-frame FIFO.** Audio frames and `Finalize` must share one ordered session queue. Separate audio/control channels are a lost-tail race waiting to happen.
4. **Engine lease vs single-model Whisper cache mutex.** Decode-ahead must not hold the cache/model mutex across repeated decode calls. Lease/actor the engine; explicitly handle model switch/unload/cancel.
5. **Revisable partial insertion.** Do not paste partials into arbitrary apps. Pill preview only; final paste once at stop.
6. **Clipboard restore guard.** Non-blocking restore must remain content/generation guarded. Current `text.rs:178-352,437-481` already shows the right pattern: restore only if clipboard still equals VT’s transcript and the generation is current.
7. **Overlay focus steal.** A streaming overlay that becomes key/focused breaks insertion correctness. Non-activating NSPanel/focusable-false behavior is mandatory.
8. **Manual stop must bypass VAD hangover.** VAD hangover can smooth streaming, but manual stop must flush/finalize immediately.
9. **WER bar.** Do not ship faster streaming if final WER regresses beyond the explicit gate above.

**P2 traps:**

10. **Unicode/whitespace prefix bugs.** Normalize text; commit at grapheme/word/segment boundaries. Avoid byte-prefix logic.
11. **Tentative churn.** If tentative rewrites are aggressive, consider dim styling, coalescing, or minimum-event intervals; do not let the whole line flicker.
12. **Event flood.** Mic levels should stay throttled/single-window; stream text emits only on text change. Handy uses `emit_to` for high-rate level events and broadcast only for lower-rate text.
13. **Sidecar stale output.** Parakeet Swift sidecar needs session protocol; stale partials from a cancelled stream must be ignored.
14. **Final vs committed reconciliation.** Final text may include formatting/punctuation adjustments. Decide whether final can replace the pill display at completion, but do not let it retroactively rewrite already “committed” live text during the session.
15. **Model activation dead ends.** If EOU needs new CoreML assets, activation must be rollback-safe and not leave users “downloaded but unusable.”

---

## Revised sequenced path

### Step 1 — Phase 0 re-baseline and remaining stop→insert cleanup — **P1, S, Low risk**

**Decision:** Do this before streaming. But treat plan 028’s old latency table as stale until measured against live main.

**Why:** The brief/plan identify Phase 0 as the highest ROI for Parakeet. Current code checks show some Phase 0 work may already be present: deferred guarded clipboard restore and CGEvent paste in `text.rs`, in-process normalization-first in `audio.rs`, and generation-gated side-effect delivery in `audio.rs`. The right first move is to prove current p50/p95 and close only the remaining measured tail.

**Concrete first action:** Add/enable a stop→insert span log around: stop received, recorder drain complete, normalization start/end + in-process-vs-ffmpeg fallback, decode start/end, enhancement start/end if enabled, pill hide start/end, clipboard set, paste event sent, paste success, restore scheduled/done.

**Effort:** Small.

**Risk:** Low if no behavior changes before measuring; medium if trimming focus/clipboard sleeps blindly.

**Acceptance / verification:** Fixed 2s/5s/15s clips across Parakeet and Whisper; measure p50/p95 stop→paste event. Manual paste QA in TextEdit/Notes/Chrome/VS Code/Slack/Google Docs. No clipboard clobber after rapid back-to-back dictations.

---

### Step 2 — Define the engine-agnostic streaming contract — **P1, S/M, Medium risk**

**Decision:** Create the canonical event and capability model before engine-specific work.

**Contract:** `Started`, `Partial { committed, tentative, revision }`, `Final`, `Cancelled`, `Error`, with `session_id`, `engine_id`, and monotonic `revision`. Capabilities include `supports_streaming`, `supports_committed_prefix`, `supports_tentative_tail`, `supports_endpointing`, `supports_word_timestamps`, and `final_only` fallback.

**Concrete first action:** Draft the Rust/TS event schema and invariants; decide whether to introduce specta now or hand-maintain a minimal generated/checked binding for this slice. Do not block Phase 0 on full specta adoption.

**Effort:** Small to medium.

**Risk:** Medium: schema churn can leak into all engines. Keep it minimal and final-only compatible.

**Acceptance / verification:** Unit tests for stale session drop, revision ordering, committed monotonicity, final-only engines emitting only `Final`.

---

### Step 3 — Build the streaming substrate, not just a PCM tap — **P1, M/L, High risk**

**Decision:** Plan 028 Phase 1 must be upgraded from “live PCM tap + request variant” to a Handy-style ordered session worker.

**Required architecture:**

- Live PCM tap from recorder hot path without blocking or allocating unpredictably.
- Bounded per-session command queue: `Feed(AudioFrame)`, `Finalize`, `Cancel` on the **same FIFO**.
- Session/generation id on every event.
- Engine actor/lease so decode does not hold global cache/manager mutexes.
- No Tauri event emission from audio callback.
- No behavior change for batch path until a feature flag/session is active.

**Concrete first action:** Implement a no-op streaming worker that receives frames and finalizes in FIFO order, with test-only counters proving last frame precedes finalize and batch output remains unaffected.

**Effort:** Medium to large.

**Risk:** High because it touches capture and lifecycle hot paths.

**Acceptance / verification:** Tests for Feed/Finalize FIFO, Cancel, stale events, queue overflow policy, no batch regression. Runtime capture test proving PCM tap observes frames mid-recording while batch WAV remains correct.

---

### Step 4 — Pull committed/tentative pill preview forward — **P1, M, Medium risk**

**Decision:** The pill preview ships with the first real streaming engine; it is not a later Phase 5 polish item.

**Scope:**

- Pill state `streaming` / live preview.
- Two always-present sibling spans: committed + tentative.
- Non-activating overlay preserved.
- Text events coalesced and emitted only on change.
- Final insertion once at stop.

**Concrete first action:** Add the pill UI path behind a feature flag using synthetic stream events so frontend behavior can be verified before engine partials exist.

**Effort:** Medium.

**Risk:** Medium: visual churn/focus bugs can ruin perceived smoothness.

**Acceptance / verification:** Focus remains in target app while pill appears/updates; committed span never shrinks in test stream; no whole-line flicker; final paste still goes to original target.

---

### Step 5 — Parakeet streaming vertical with SlidingWindow-vs-EOU measurement gate — **P1, L, Medium/High risk**

**Decision:** Parakeet is VT’s best chance to beat Handy on macOS, but default SlidingWindow is not automatically Handy-like. Use a hard gate.

**Implementation shape:** Extend `src-tauri/src/parakeet/messages.rs` beyond path-based `Transcribe` into a session protocol: `StartStream`, `AudioChunk`, `FinalizeStream`, `CancelStream`, plus `Partial`/`Final` responses. The current Swift sidecar uses `manager.transcribe(fileURL)` (`main.swift:449-513`); streaming needs a long-lived manager/session path.

**SlidingWindow use:** First as a no-new-model prototype and long-form decode-ahead path. It ships as the public Handy-like path only if it passes the metrics table in Q2.

**EOU use:** If SlidingWindow fails first-partial/stable-prefix/short-utterance/WER gates, ship `StreamingEouAsrManager` with a proper model activation/download flow. Treat APIs as verified in plan 028 but reconfirm at build time.

**Concrete first action:** Build a local measurement harness for the existing TDT SlidingWindow manager using the acceptance corpus; do not integrate full UX until first-partial/stable-prefix numbers are known.

**Effort:** Large.

**Risk:** Medium for SlidingWindow prototype; high for EOU because of new model assets, activation, and sidecar session lifecycle.

**Acceptance / verification:** Numeric gate from Q2. Additionally: cancel mid-stream, finalize after last frame, stale session ignored, no worse short-dictation final latency than Phase 0 batch.

---

### Step 6 — Cloud WebSocket streaming, Soniox first if cloud usage matters — **P2, M, Medium risk**

**Decision:** Move Soniox WS earlier than Whisper decode-ahead if real users use Soniox, because provider-native partial/final semantics are lower risk than building Whisper pseudo-streaming and remove the 1s REST poll floor.

**Concrete first action:** Add cloud streaming provider capability and a Soniox WS adapter that emits the same committed/tentative events; keep REST batch as fallback.

**Effort:** Medium.

**Risk:** Medium: auth/network reconnection/error taxonomy.

**Acceptance / verification:** First partial under current REST poll floor, final equals or improves batch WER on corpus, network errors fall back or fail clearly without stale paste.

---

### Step 7 — Whisper decode-ahead for Whisper/Windows users — **P2, L, High risk**

**Decision:** Reimplement segmented decode-ahead on VT’s existing Whisper stack; do not adopt `transcribe-cpp`.

**Scope:** Growing buffer, minimum decode window, emit all-but-last completed segment conservatively, hold tail tentative, advance head past emitted audio, stable-prefix reducer, cancellation. Mirror the architecture for Windows Vulkan sidecar rather than introducing a separate in-process C++ stack.

**Concrete first action:** Prototype the reducer and segmented decode loop against existing `whisper-rs 0.16` batch `full()` and `set_segment_callback_safe`, using scribble/transcribe-cpp behavior as references.

**Effort:** Large.

**Risk:** High: WER regressions, CPU/Vulkan redundant decode cost, cancellation/hard-timeout behavior, cache/lease complexity.

**Acceptance / verification:** Long-form stop→final latency improves materially; short-clip path not slower; WER within gate; no global model mutex held during repeated decode.

---

### Step 8 — Type-as-you-speak only as a later opt-in experiment — **P3, L, High risk**

**Decision:** Do not include in Handy-parity work.

**Concrete first action if ever revisited:** Define supported target surfaces and a replacement-range/undo model; otherwise leave arbitrary-app insertion final-only.

**Effort:** Large.

**Risk:** Very high: editor corruption, IME issues, undo grouping, web app latency, inaccessible/elevated targets.

**Acceptance / verification:** Only inside controlled integrations; never as default arbitrary-app behavior.

---

## Disagreements

### Where I diverge from plan 028

1. **Phase 5 is too late.** Plan 028 treats live partial UX as Phase 5 after engine work. Handy proves the pill preview is the visible value. Move committed/tentative pill preview into the first streaming vertical.

2. **Phase 1 is under-specified.** “Live PCM tap + streaming request variant + capabilities flag” is not enough. The substrate must include FIFO Feed/Finalize/Cancel, session/generation gating, engine lease/actor, stale event dropping, and no audio-callback blocking.

3. **Parakeet SlidingWindow should not be assumed MVP.** Plan 028 lists SlidingWindow v1 before EOU because it reuses the current model. I agree as a prototype/bridge, but not as a product promise unless it passes the numeric gate. Defaults like 15s/10s are incompatible with Handy-like short dictation.

4. **Cloud WS may deserve earlier priority than Whisper decode-ahead.** Plan 028 has Whisper Phase 3 before cloud Phase 4. If Soniox/cloud users are meaningful, Soniox WS should come right after substrate/UX or after Parakeet because native provider partials are lower risk and remove a known 1s poll floor.

5. **The Phase 0 table must be re-baselined.** The live code appears to already include some Phase 0 changes: guarded deferred clipboard restore, CGEvent paste, and in-process normalization-first. The plan should not keep quoting ~950ms as current fact without remeasurement.

6. **Typed IPC/specta is useful but not the first speed lever.** Handy’s specta contract is strong. VT should define typed streaming events, but full IPC migration should not precede Phase 0 or the streaming substrate unless it is scoped tightly to the streaming events.

### Where I diverge from / extend round-1 findings

1. **Round 1 says Phase 0 first; I agree, but update the action.** Because current code has already absorbed some Phase 0 pieces, the first action is not “remove the known 950ms” generically; it is **instrument and re-baseline current stop→paste p50/p95, then finish only measured remaining tail.**

2. **Round 1 leans toward EOU as likely macOS path; I make it a hard gate.** I agree default SlidingWindow is probably insufficient, but the decision should be numeric: if SlidingWindow can be tuned to first partial ≤700ms p50 and stable prefix ≤1800ms p50 without WER regression, ship it first; otherwise EOU.

3. **Round 1 treats VT’s session safety mostly as future work; current VT already has a valuable delivery chokepoint.** `audio.rs` has generation-gated `persist_if_current` and repeated delivery checks. Streaming should extend that pattern to partials and sidecar sessions rather than invent a parallel lifecycle.

4. **Round 1 frames specta as P3; I split it.** Full specta migration is P3, but a typed/checked streaming event contract is P1. The danger is not stringly-typed invokes in general; it is a partial/final schema that drifts across backend/frontend.

5. **Round 1 says cloud WS priority depends on user mix; I agree but sharpen sequencing.** If Soniox is recommended or materially used, cloud WS is probably lower risk and higher immediate payoff than Whisper decode-ahead after Parakeet, because provider final/interim events map naturally to committed/tentative.

---

## Correctness traps — must-handle checklist

- **Monotonic committed:** `committed` only grows within a session; tentative alone is revisable. Assert it.
- **Session-id/generation gating:** every event carries session id + revision; frontend/backend drop stale partials/finals/cancels.
- **Finalize-after-last-frame FIFO:** audio frames and `Finalize` share one ordered queue.
- **Engine lease vs single-model Whisper cache mutex:** no global cache/engine mutex held during frequent decode; model switch/unload has explicit behavior.
- **Clipboard-restore guard:** keep generation/content-checked async restore; never restore over user clipboard changes or newer dictation.
- **Non-activating overlay:** streaming pill never steals key/focus; final paste target remains original app.
- **VAD hangover vs short-utterance finalization:** VAD pre-roll/hangover for smooth streaming, but manual stop finalizes immediately and retains enough tail audio.
- **WER bar:** final WER delta gate before any streaming mode becomes default.
- **Revisable partial insertion:** no arbitrary-app type-as-you-speak in the MVP.
- **Stale sidecar output:** Parakeet/Whisper/cloud late events from cancelled sessions are ignored and cannot paste.
- **Unicode/whitespace boundaries:** stable-prefix reducer operates on safe grapheme/word/segment boundaries, not bytes.
- **Final text reconciliation:** final may format/punctuate, but live committed text must not appear to rollback during the session.
- **Event rate:** mic levels throttled/single-window; stream text emitted only on changed/coalesced text.
- **Activation rollback:** EOU model download/activation must rollback to current working engine on failure.

---

## Verification plan

1. **Phase 0/current baseline:** fixed clip harness + real mic manual runs. Report p50/p95 for stop received → paste event sent, stop → text visible if observable, normalize duration, decode duration, enhancement duration, paste chain duration, restore scheduled/done.
2. **Paste/focus QA:** TextEdit, Notes, Safari/Chrome fields, Slack/Discord, VS Code, Google Docs, Word if available, fullscreen/Spaces/multiple monitors; assert overlay never receives focus and paste lands in target.
3. **Streaming substrate tests:** FIFO Feed/Finalize, Cancel, stale session, queue overflow policy, worker panic/early return cleanup, model lease return, no batch regression.
4. **Pill UX tests:** synthetic stream events with aggressive tentative rewrites; committed span never shrinks; no whole-line remount/flicker; scroll/pin behavior if multiline.
5. **Parakeet gate harness:** 2s/5s/15s/60s clips across short commands, punctuation, quiet speaker, noisy room, pause-heavy speech, accents/domain words. Record first partial, first stable prefix, final after stop, WER delta.
6. **Cloud/Whisper follow-up:** provider disconnect/reconnect and Whisper cancel/hard-timeout tests before default enablement.

---

## Critical files for implementers

- Brief / decision inputs:
  - `local://oracle-brief-handy-vt-streaming.md`
  - `/Volumes/1tb-drive/developer/oss/voicetypr/plans/028-transcription-latency-streaming.md`
  - `local://oracle-round1-slowmodel.md`
  - `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-handy-teardown/docs/handy-teardown/00-README.md`
  - `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-handy-teardown/docs/handy-teardown/01-streaming-engine.md`
  - `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-handy-teardown/docs/handy-teardown/02-overlay-ux.md`
- VT stop/paste/current Phase 0:
  - `src-tauri/src/commands/audio.rs` — generation gating, normalization, delivery chokepoint, pre-insert path.
  - `src-tauri/src/commands/text.rs` — clipboard/paste/restore timing and guard.
  - `src-tauri/src/audio/recorder.rs` — CPAL hot path and stop drain.
- VT engine seams:
  - `src-tauri/src/parakeet/messages.rs`
  - `src-tauri/src/parakeet/sidecar.rs`
  - `sidecar/parakeet-swift/Sources/main.swift`
  - `src-tauri/src/whisper/cache.rs`
  - `src-tauri/src/whisper/transcriber.rs`
  - `src-tauri/src/whisper/gpu_sidecar.rs`
  - `src-tauri/src/cloud_stt/*`
- VT overlay:
  - `src-tauri/src/window_manager.rs`
  - `src/components/pill/PillShell.tsx`
  - `src/components/pill/usePillController.ts`

---

## The single “do this first”

**Do this first: claim a Phase-0 re-baseline/remaining-tail task that instruments the current Parakeet stop→paste path end-to-end and proves p50/p95 before any streaming work; then finish only the measured remaining stop→insert tail before building the streaming substrate.**