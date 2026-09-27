# Plan 043 — Soniox WebSocket streaming (real live preview, multilingual)

> Authored from `docs/review/grounding-soniox-ws.md` (ultracode 2026-07-03; external
> API facts verified against soniox.com with source URLs). This is the slice that
> makes live preview ACTUALLY WORK for a shipping engine — Soniox streams interim
> tokens over WS, multilingual, no local-model dependency. It plugs into the exact
> streaming substrate (037-042) and the capability-driven "Transcription mode" toggle
> already built.
>
> ⚠️ VERIFICATION CEILING: the live WS handshake needs a Soniox API key in the user's
> secure store. Codex verifies compile + unit tests (token→committed/tentative mapping,
> capability flip, sync/async bridge, gate generalization); the live end-to-end
> handshake is a MANUAL smoke the user runs with their key. State this honestly.
>
> ⚠️ FRESHNESS: slice 045 touched audio.rs/stream_tap.rs/sidecar.rs streaming paths, so
> the grounding's line refs there may have shifted. Codex MUST re-verify every file:line
> before editing and stop-and-report on mismatch. (stream_tap.rs trait confirmed still at
> :28-31; the Parakeet sink/gate refs in audio.rs likely moved.)

## Where it plugs in (verified substrate)

- **RT tap → sink trait** (`stream_tap.rs:28-31`, confirmed current):
  `trait StreamTapSink { fn send_frame(&mut self, &[i16]); fn finalize(&mut self) ->
  Option<String>; fn cancel(&mut self); }` + `StreamTapSinkFactory = Arc<dyn Fn(u32,u16)
  -> Option<Box<dyn StreamTapSink>>>`. Factory gets `(sample_rate, channels)` — exactly
  what Soniox `pcm_s16le` raw config needs. **No resampling** (native-rate i16 frames).
- **Reference sink:** `ParakeetPreviewStreamSink` (grounding cited audio.rs:79-157 —
  re-locate) bridges the sync worker thread to an async handle, emits
  `TranscriptionStreamEvent`s to the pill via a `StreamSessionGate`. Copy the shape (with
  a hard finalize timeout).
- **Contract** (`transcription/stream.rs`): `Started/Partial{committed,tentative}/
  Final{text}/Cancelled/Error` with `session_id` (recording generation) + monotonic
  `revision`; `StreamSessionGate::admit` drops stale; `assert_committed_monotonic` enforces
  committed is byte-prefix-append-only.
- **Frontend:** `pill.tsx`, `types/streaming.ts`, `RecordingPill.streaming.test.tsx`
  already render committed/tentative — NO schema change.

## Soniox RT WebSocket API (verified 2026-07)

- **Endpoint:** `wss://stt-rt.soniox.com/transcribe-websocket` — DIFFERENT origin than REST
  `api.soniox.com` (warm-up currently warms the wrong origin for RT).
- **Auth:** first frame is JSON config with `"api_key"` in the BODY (not a header). Desktop
  holds the key in the encrypted store — no temp-key minting needed.
- **Config:** `model: "stt-rt-v5"` (pairs with our async `stt-async-v5`), `audio_format:
  "pcm_s16le"` + `sample_rate` + `num_channels` (raw), `language_hints[]`, `context{general,
  text,terms}` (our `SonioxContext` is a subset — reuse `compile_soniox_context`),
  `enable_endpoint_detection: true` (fast dictation commits), `enable_language_identification`.
- **Audio:** binary frames after config. Our ~10-40ms callback frames are fine (optionally
  coalesce toward the docs' 120ms/3840-byte cadence). Max 300 min/stream.
- **Responses (JSON):** `{tokens:[{text,start_ms,end_ms,confidence,is_final,speaker,language}],
  ...}`. Non-final tokens "may change/disappear/be replaced" and are **reissued each
  response**; final tokens "sent only once, never repeated". RT tokens carry their OWN
  leading spaces — do NOT copy the space-inserting join from REST `soniox.rs:230-241`.
- **Close:** send an **empty frame** → server flushes → `{...,"finished":true}` → closes.
  `{"type":"finalize"}` forces pending final mid-stream; `{"type":"keepalive"}` required if
  idle >20s (send every ~10s).
- **Errors:** in-band `{tokens:[],error_code,error_type,error_message,...}` with HTTP-style
  codes — a MESSAGE, not an HTTP status. Needs an int→`SttError` map parallel to
  `classify_status` (common.rs:44-55).
- **Billing:** charged for FULL stream duration incl. keepalive idle — never hold the socket
  open between recordings.

## Mapping onto `TranscriptionStreamEvent`

- `Started { engine: "soniox" }` after WS open + config sent, revision 0.
- `Partial`: `committed` = running concat of ALL `is_final:true` token texts, concatenated
  VERBATIM (tokens carry own spaces) — append-only, satisfies `assert_committed_monotonic`.
  `tentative` = concat of the current response's `is_final:false` tokens, wholly replaced
  each response. `revision` = per-response `AtomicU64`.
- `Final` on worker `Finalize`: send empty frame, drain (fold late finals into committed)
  until `finished:true` or timeout; `text` = committed total.
- `Cancelled` on `Cancel`/stale generation: close without finalize.
- `Error` on in-band error / transport failure: map code, emit, let executor REST-on-WAV
  produce the authoritative result.

## Scope

1. **New dep:** a WS client (`tokio-tungstenite` + rustls, or `reqwest-websocket`) — enforce
   `wss` (mirror `https_only` rationale, common.rs:106-113), TLS consistent with reqwest 0.13.
2. **`SonioxStreamSink`** implementing `StreamTapSink`: sync `send_frame` → bounded tokio mpsc
   → spawned WS task (mirror `ParakeetStreamHandle`); NEVER block the RT worker; async connect
   + local frame buffering until config-ack (must NOT block recorder start under the recorder
   mutex — grounding risk #2). `finalize()`'s `block_on` gets a HARD timeout (~1-2s) then REST
   fallback. Config frame NEVER logged (api_key hygiene).
3. **Result authority (the biggest decision — grounding risk #1):** with a WS sink both the
   stream AND the executor's REST-on-WAV run → double billing (Soniox bills full duration).
   Decision for this plan: **WS `Final` becomes the pasted result; REST-on-WAV runs only as
   fallback on WS error/gap.** This flips the Parakeet stance (batch authoritative); the
   worker's `finalize()` discard (stream_tap.rs:299-303) must change for Soniox-authoritative
   mode. Wire this behind the mode so Parakeet stays batch-authoritative.
4. **Capability flip:** `EngineStreamCapabilities::SONIOX` (stream.rs:83)
   FINAL_ONLY → `{supports_streaming, supports_committed_prefix, supports_tentative_tail,
   supports_endpointing: true, final_only: false}`. Updates the pinned truth-table test at
   stream.rs:246-271 (intentional site).
5. **Generalize the gate:** `build_parakeet_stream_sink_factory`'s engine check and the
   `parakeet_streaming_enabled` clamp assume one engine — make the factory dispatch on
   `EngineStreamCapabilities::for_engine(current_engine)` + engine id, so Soniox registers by
   its capability row + `SonioxStreamSink`. Suppress WS streaming when a remote connection is
   active (same check as warm-up). Rename the `parakeet`-specific emit helper to be engine-neutral.
6. **Warm-up:** add a warm for `stt-rt.soniox.com` (current warm hits `api.soniox.com`).
7. **Keepalive:** 10s idle timer sending `{"type":"keepalive"}`; never hold the socket between
   recordings.
8. **Reconnect (grounding risk #3):** on mid-stream drop, committed prefix is safe locally;
   accept the preview gap and rely on REST-on-WAV for the authoritative final (do NOT build
   audio-replay reconnection in this slice — note it as a follow-up).

## Tests
- Token→committed/tentative mapping: finals accumulate verbatim + monotonic; tentative
  replaced each response; reissued non-finals don't corrupt committed.
- int error_code → SttError map.
- capability truth-table update.
- sync→async bridge: `send_frame` never blocks; finalize timeout → falls back.
- gate generalization: factory returns the Soniox sink for soniox engine + live_preview,
  None otherwise; suppressed under active remote connection.

## Acceptance
- `cargo test` + clippy + `pnpm typecheck/lint/test` green.
- Default (regular mode / non-soniox engine): zero behavior change.
- Live WS handshake = MANUAL smoke by the user with a Soniox key (documented commands).
- Result-authority + billing decision documented in the plan Outcome after implementation.
- Do not commit.
```

---

## Execution spec (RE-GROUNDED 2026-07-06 — supersedes the stale line refs above)

The plan above was grounded 2026-07-03; **046/047/048/049 have all landed since**, so the
old file:line anchors drifted. These are the verified current-tree anchors + the resolved
engineering decisions. Branch: **`feat/043-soniox-streaming` off `integrate/perf-ux-2026-07`**
(clean base; independent of feat/049 — streaming consumes raw i16 tap frames, not the decode
path). Worktree: voicetypr-integration (same one; new branch, NOT a new worktree).

### Verified current anchors
- `StreamTapSink` trait: `audio/stream_tap.rs:28-31` — `send_frame(&mut self, &[i16])`,
  `finalize(&mut self) -> Option<String>`, `cancel(&mut self)`. UNCHANGED.
- Reference sink (COPY ITS SHAPE): `ParakeetPreviewStreamSink` `commands/audio.rs:77`
  (struct), `impl StreamTapSink` `:96`, `emit_parakeet_stream_event` `:139`,
  `build_parakeet_stream_sink_factory` `:157`.
- Reference sync→async bridge (MIRROR IT): `ParakeetStreamHandle` `parakeet/sidecar.rs:40`
  — `tokio::sync::mpsc::UnboundedSender<..Control>` + `tauri::async_runtime::spawn` (`:636`).
- Event contract (NO schema change): `TranscriptionStreamEvent` `transcription/stream.rs:11`
  — `Started{session_id,engine,revision}` / `Partial{..,committed,tentative}` /
  `Final{..,text}` / `Cancelled` / `Error{..,error}`. `StreamSessionGate` + monotonic
  `assert_committed_monotonic` in same file.
- Capability row to flip: `EngineStreamCapabilities::SONIOX` `stream.rs:83` (fields:
  `supports_streaming, supports_committed_prefix, supports_tentative_tail,
  supports_endpointing, final_only`). `for_engine` map at `stream.rs:91-95`.
- #14a gate sites (string-compare → capability dispatch): factory guard
  `commands/audio.rs:167` (`config.current_engine != "parakeet"`); streaming enable
  `commands/audio.rs:4002-4004` (`parakeet_streaming_enabled = config.current_engine ==
  "parakeet"`).
- Cloud dispatch + warm: `CloudProvider::from_id` `audio.rs:3827`; provider warm
  `audio.rs:3838` (`provider.warm_up()`), AI warm `:3850`. Soniox `warm_up()` → warms
  `api.soniox.com`; RT needs `stt-rt.soniox.com` added.
- REST token join to CONTRAST (do NOT copy — RT tokens carry own spaces):
  `cloud_stt/soniox.rs:230-241` inserts spaces between tokens. RT concatenation is verbatim.
- Error classifier to PARALLEL: `SttError` `common.rs:15`, `classify_status` `:44-55`
  (401→Auth, 403/404→ModelUnavailable, 408→Timeout, 429→RateLimited, 5xx→Server, else→BadResponse).

### Dependency decision (resolved)
`tokio-tungstenite` with the **rustls** connector. Rationale: `rustls`/`tokio-rustls`/
`hyper-rustls` are ALREADY in the tree (reqwest pulls them) → reuses the existing TLS stack,
**no second TLS backend, negligible binary growth** (we just shipped −60%; don't regress it).
tokio-tungstenite is version-decoupled from reqwest (the tree already has reqwest 0.12 AND
0.13). Enforce `wss://` only (mirror `common.rs` https_only rationale). Verify the added dep
does NOT introduce a duplicate rustls major or native-tls.

### ⚠️ CRITICAL trap for the #14a routing (C-ROUTE)
`PARAKEET` capability is `FINAL_ONLY` (dormant — `stream.rs:82` comment: EOU broken upstream),
YET the string-compare STILL wires Parakeet's streaming sink. A naive "string == parakeet" →
`for_engine(...).supports_streaming` swap would **stop routing Parakeet to its sink and break
the shipped Parakeet live preview**. C-ROUTE MUST reconcile this so Parakeet behavior is
byte-identical (either keep a Parakeet arm, or route on "has a registered sink factory for
this engine" rather than the raw capability bool). Behavior-preservation for Parakeet is a
hard acceptance gate with an explicit test.

### Split (Hybrid — default; adjust per owner's pick)
**GLM slices (pure, unit-testable, no live socket, wired into nothing until C-ROUTE):**
- `S-MAP` — a pure module (e.g. `cloud_stt/soniox_rt.rs`): `fn fold_tokens(resp) ->
  {committed_append: String, tentative: String}`. committed = concat of `is_final:true`
  token `text` VERBATIM (append-only → passes `assert_committed_monotonic`); tentative =
  concat of this response's `is_final:false` token `text`, replaced each response. + a
  revision counter helper. Unit tests: finals accumulate + monotonic; tentative replaced;
  reissued non-finals don't corrupt committed; verbatim spacing (NOT the REST join).
- `S-ERR` — `fn rt_error_to_stt(code: i64) -> SttError` paralleling `classify_status`. + tests.
- `S-CAP` — flip `EngineStreamCapabilities::SONIOX` to
  `{supports_streaming:true, supports_committed_prefix:true, supports_tentative_tail:true,
  supports_endpointing:true, final_only:false}`; update the truth-table test at its
  intentional site (`stream.rs` tests).
- `S-WARM` — add the `stt-rt.soniox.com` warm origin alongside the REST one.

**Claude slices (concurrency core — where no-compiler models fail):**
- `C-WS` — `SonioxStreamSink` + the tokio-tungstenite task: connect `wss://stt-rt.soniox.com/
  transcribe-websocket`; first frame = JSON config with `api_key` in BODY (NEVER logged),
  `model:"stt-rt-v5"`, `audio_format:"pcm_s16le"` + `sample_rate` + `num_channels`,
  `language_hints[]`, `context{}` (reuse `compile_soniox_context`),
  `enable_endpoint_detection:true`, `enable_language_identification`; buffer frames until
  config-ack; binary audio frames; 10s keepalive (`{"type":"keepalive"}`); finalize = empty
  frame → drain late finals until `finished:true` or HARD ~1-2s timeout.
- `C-BRIDGE` — sync `StreamTapSink` impl → `mpsc::UnboundedSender` → the WS task (mirror
  `ParakeetStreamHandle`). `send_frame` NEVER blocks (must not stall the recorder mutex —
  grounding risk #2); `finalize()` `block_on` gets a hard timeout then REST fallback.
- `C-ROUTE` — generalize `build_parakeet_stream_sink_factory` + the `parakeet_streaming_enabled`
  gate to capability/engine dispatch (see the CRITICAL trap above); register `SonioxStreamSink`
  for Soniox + `live_preview`; suppress under active remote connection; rename
  `emit_parakeet_stream_event` engine-neutral.
- `C-AUTH` — result-authority: WS `Final` is the pasted result; REST-on-WAV runs only on WS
  error/gap. Wired behind the mode so Parakeet stays batch-authoritative.

### Verification ceiling (state honestly in the Outcome)
Automated: build + clippy + `cargo test` (S-MAP/S-ERR/S-CAP + bridge non-block + finalize
timeout→fallback + gate generalization + Parakeet-unchanged) + `pnpm typecheck/lint/test`.
**The live WS handshake needs the owner's Soniox key in secure storage → MANUAL smoke only.**
Billing: RT bills full stream duration incl. keepalive idle; WS-authoritative + REST-fallback
avoids double-bill on the happy path but pays both on WS error. Opt-in (live_preview + Soniox).

---

## Outcome — Phase 2 (result-authority, plan 043b)

**Shipped.** The Soniox realtime WS stream's final text is now the AUTHORITATIVE pasted
result (single bill on the happy path), with REST-on-WAV demoted to fallback-on-failure.

Mechanism: a `tokio::sync::oneshot` side-channel (`SONIOX_WS_FINAL` in `commands/audio.rs`)
bridges the detached stream-tap worker (which resolves the oneshot when its WS finalize
completes, up to 3s) to the already-async transcription task (`take_soniox_ws_final`,
bounded 4s). `StreamTapSink::finalize()`'s discarded return value and the recorder's
100ms tap-join-then-detach are untouched — the side-channel threads around them.

- **Result-authority** = WS-final when non-empty; REST-on-WAV runs only when the WS path
  produced no usable text (no slot, generation mismatch, error, empty, timeout, cancel).
- **Dev gate removed**: `soniox_streaming_preview_enabled()` /
  `VOICETYPR_SONIOX_STREAMING_PREVIEW` deleted; `for_engine(Soniox)` returns `SONIOX`
  unconditionally. The Live-preview toggle now ships for Soniox users with no env var.
- **Billing**: single stream on the happy path; double-bills (WS + REST) ONLY on WS
  failure.
- **Parakeet/Whisper stances unchanged**: batch stays authoritative; only Soniox +
  `TranscriptionTask::Transcribe` takes WS authority (translate tasks never do — WS config
  doesn't translate).
- Non-soniox engines and soniox-without-live-preview: zero behavior change.
- **Verification**: unit tests for `take_soniox_ws_final` (matching gen, mismatch+clear,
  error, whitespace, dropped-sender-quick); capability truth-table updated. Live WS
  handshake remains a MANUAL founder smoke.
