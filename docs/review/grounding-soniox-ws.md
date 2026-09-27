# Grounding report — Soniox WebSocket streaming (plan 043)

All paths relative to `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-integration/`.

## 1. How Soniox REST works today (current code, not diff)

**Provider seam** — `src-tauri/src/cloud_stt/mod.rs`
- `CloudProvider::Soniox` id `"soniox"` (:51), model = `soniox::MODEL` = `"stt-async-v5"` (soniox.rs:7), key in secure store under `stt_api_key_soniox` (:100), base origin `https://api.soniox.com` (:111).
- Dispatch: `transcribe_typed` (:175-189) and `transcribe_typed_diarized` (:208-232; Soniox+Deepgram fill words).
- Warm-up at recording start: `warm_up()` → `common::warm_origin` HEAD to `https://api.soniox.com` (mod.rs:119-123, common.rs:124-130), invoked from `commands/audio.rs:4171-4186` (skipped when a remote connection is active or no key stored).

**Flow** — `src-tauri/src/cloud_stt/soniox.rs`
- Auth is `Authorization: Bearer <key>` throughout; key validation = GET `https://api.soniox.com/v1/models` (:11-20).
- 4-step async job: (1) multipart upload POST `/v1/files` → `id` (:76-112); (2) POST `/v1/transcriptions` with `{model, file_id, language_hints:[lang], context, enable_speaker_diarization}` built by `build_create_payload` (:22-53, :124-156); (3) poll GET `/v1/transcriptions/{id}` every 1000 ms, 180 s cap → `SttError::Timeout` (:158-199); (4) GET `/v1/transcriptions/{id}/transcript`, prefer `text`, else join `tokens[].text` **with inserted spaces** (:226-247 — note: RT tokens carry their own leading spaces, so this join strategy must NOT be copied into the WS path).
- Context: `crate::writing::compile_soniox_context` produces `SonioxContext { general: Vec<{key,value}>, terms: Vec<String>, text: Option<String> }` (writing.rs:1262-1274), serialized into the REST `context` field (soniox.rs:37-46). The WS config message accepts the identical `context` object shape → reuse directly.
- `parse_soniox_token` (:447-466) already parses `text/start_ms/end_ms/speaker` — same token shape as WS responses.

**Shared plumbing** — `src-tauri/src/cloud_stt/common.rs`
- `SttError { Auth, ModelUnavailable, RateLimited, Timeout, Network, Server, BadResponse }` (:14-23); `classify_status` maps HTTP codes (:44-55) — takes a `reqwest::StatusCode`, so WS in-band error codes need a parallel int-based mapping.
- `with_retry`: exactly one retry on transient after 400 ms (:157-170).
- Shared client: 30-min request timeout, 15 s connect timeout, `https_only(true)` outside tests (:88-121). None of this applies to a WS connection; **no WebSocket client exists in the tree** — `src-tauri/Cargo.toml` has `futures-util`, `tokio(full)`, `reqwest 0.13.4` (no `ws` capability), `tokio-util`; a new dep (`tokio-tungstenite` + rustls, or `reqwest-websocket`) is required.

**Executor (REST fallback path)** — `src-tauri/src/transcription/executor.rs:274-291`: `ActiveEngineSelection::Cloud` reads the key, calls `provider.transcribe_typed`, maps `SttError` via `from_stt_error`. This runs on the recorded WAV *after* stop — the WAV is written by the writer path independently of the stream tap, so it is always available as fallback.

## 2. Where a WS variant plugs in (streaming substrate, slices 037-042)

- **RT tap → worker**: `src-tauri/src/audio/stream_tap.rs`. The CPAL callback enqueues native-rate interleaved **i16** frames into a bounded FIFO (`enqueue_frame_rt` :152-182, drop-never-block); a std-thread worker pumps them into a `StreamTapSink` (:277-312). The trait to implement (:28-35):
  ```rust
  pub trait StreamTapSink: Send {
      fn send_frame(&mut self, samples: &[i16]);
      fn finalize(&mut self) -> Option<String>;
      fn cancel(&mut self);
  }
  pub type StreamTapSinkFactory = Arc<dyn Fn(u32, u16) -> Option<Box<dyn StreamTapSink>> + Send + Sync>;
  ```
  Factory receives `(sample_rate, channels)` — exactly what Soniox `pcm_s16le` config needs; **no resampling required** (unlike local engines which normalize to 16 kHz offline).
- **Reference sink**: `ParakeetPreviewStreamSink` in `src-tauri/src/commands/audio.rs:79-157` — bridges the sync worker thread to an async engine handle, emits `TranscriptionStreamEvent`s to the `pill` window through a `StreamSessionGate` (`emit_parakeet_stream_event` :141-157). Its `finalize` uses `tauri::async_runtime::block_on` (:106) — the pattern a WS sink would copy (with a timeout).
- **Gating**: settings read at `commands/audio.rs:4104-4126` (`transcription_mode == "live_preview"` OR dev keys `streaming_tap_enabled`/`streaming_engine_enabled`), then **hard-gated to Parakeet**: `streaming_tap_enabled && parakeet_streaming_enabled` (:4347-4349) and `config.current_engine != "parakeet" → None` inside `build_parakeet_stream_sink_factory` (:167-173). Slice 043 must generalize this gate + the `parakeet`-named factory/emit helpers.
- **Factory call timing risk**: the factory runs inside `recorder.start_recording` (audio/recorder.rs:310), on the start path with the recorder mutex held; the Parakeet factory `block_on`s model load/stream open (audio.rs:199-267). A WS sink must NOT block recording start on a TLS+WS handshake — open asynchronously and buffer frames until the config is accepted.
- **Session/ordering contract** — `src-tauri/src/transcription/stream.rs`: events `Started/Partial{committed,tentative}/Final{text}/Cancelled/Error` tagged `session_id` (recording generation) + monotonically increasing `revision` (:9-37); `StreamSessionGate::admit` drops stale sessions/revisions (:137-152); `assert_committed_monotonic` enforces committed is byte-prefix-append-only (:154-158).
- **Capability flip point**: `EngineStreamCapabilities::SONIOX = FINAL_ONLY` (stream.rs:83) → becomes `{supports_streaming, supports_committed_prefix, supports_tentative_tail, supports_endpointing: true, final_only: false}`. The coarse `provider_capabilities.rs:72-78` table (Soniox: `supports_structured_terms: true`) is separate and unchanged; note its invariant tests pin exact truth tables (:166-312).
- **Frontend consumers**: `src/pill.tsx`, `src/types/streaming.ts`, `src/components/RecordingPill.streaming.test.tsx` — already render committed/tentative; no schema change needed.
- **Worker discards `finalize()` text**: `run_noop_worker` does `let _ = sink.finalize()` (stream_tap.rs:299-303); today the batch result stays authoritative (plan 042 stance). For Soniox this is a cost decision, not just UX — see risks.

## 3. Current Soniox real-time WebSocket API (verified 2026-07 against soniox.com)

- **Endpoint**: `wss://stt-rt.soniox.com/transcribe-websocket` — note this is a **different origin** than the REST `api.soniox.com` that `warm_up()` currently warms. Source: https://soniox.com/docs/stt/api-reference/websocket-api
- **Auth handshake**: first frame is a JSON text config containing `"api_key": "<SONIOX_API_KEY|SONIOX_TEMPORARY_API_KEY>"` — key in message body, not a header. Temporary server-minted keys exist for browser clients (with `max_session_duration_seconds`); unnecessary here since the desktop app already holds the key in the encrypted store. `client_reference_id` (≤256 chars) is ignored under temp keys.
- **Config fields**: `model` (current: `"stt-rt-v5"`, pairs with our `stt-async-v5`; `stt-rt-v4` active, `stt-rt-v3` aliases to v4 — source https://soniox.com/docs/stt/models), `audio_format` (`"auto"` or raw: exact strings `pcm_s16le`, `pcm_f32le`, `mulaw`, `alaw`, …; raw requires `sample_rate` + `num_channels`), `language_hints[]`, `language_hints_strict`, `context{general,text,terms,translation_terms}` (superset of our `SonioxContext`), `enable_speaker_diarization`, `enable_language_identification`, `enable_endpoint_detection`, `max_endpoint_delay_ms` (500-3000, default 2000), `endpoint_sensitivity` (-1.0..1.0, v5 only), `endpoint_latency_adjustment_level` (0-3, v5 only), `translation`.
- **Audio**: binary frames after config. Docs examples pace 3840 bytes / 120 ms (= 16 kHz mono s16le); our per-callback frames (~10-40 ms) are finer-grained and acceptable, optionally coalesce. Max **300 minutes** per stream (413 error at cap).
- **Responses** (JSON text frames): `{tokens: [{text, start_ms, end_ms, confidence, is_final, speaker, language, translation_status, source_language}], final_audio_proc_ms, total_audio_proc_ms}`. Semantics (source https://soniox.com/docs/stt/rt/real-time-transcription): non-final tokens "may change, disappear, or be replaced" and are **reissued each response**; final tokens are "sent only once and never repeated". `enable_endpoint_detection` finalizes all pending tokens the moment the speaker stops.
- **Close semantics**: to end, send an **empty WebSocket frame** (text or binary); server flushes remaining responses, sends `{... "finished": true}`, then closes. Control messages: `{"type": "finalize"}` forces pending tokens final mid-stream; `{"type": "keepalive"}` required when idle — "if no keepalive or audio is received for >20 s, the connection may be closed"; send at least every 20 s (5-10 s common). Source: https://soniox.com/docs/stt/rt/connection-keepalive
- **Errors**: in-band error response `{tokens: [], error_code, error_type, error_message, more_info, request_id}` with HTTP-style codes (400/401/402/403/408/413/429/500/503) — arrives as a message, not an HTTP status.
- **Usage/billing**: "you are charged for the **full stream duration**, not just the audio processed" — keepalive idle time bills. Rate limits on concurrent WS sessions apply per plan.

## 4. Mapping onto `TranscriptionStreamEvent`

- `Started { engine: "soniox" }` — after WS open + config sent (or first response) — revision 0.
- `Partial` — per response: `committed` = running concatenation of all `is_final: true` token texts **concatenated verbatim** (RT tokens carry their own leading spaces; do not copy the space-inserting join at soniox.rs:230-241). Append-only finals satisfy `assert_committed_monotonic` (stream.rs:154-158) exactly. `tentative` = concatenation of the current response's `is_final: false` tokens, wholly replaced each response. `revision` = per-response counter via `AtomicU64` (pattern audio.rs:89-91).
- `Final` — on worker `Finalize`: send empty frame, drain responses (folding late finals into committed) until `finished: true` or timeout; `text` = committed total.
- `Cancelled` — on `Cancel`/stale generation: close the socket without finalize, emit event (pattern audio.rs:132-138).
- `Error` — on in-band error or transport failure: map `error_code` int → `SttError` (parallel to `classify_status`, common.rs:44-55), emit, and let the executor's REST-on-WAV path produce the authoritative result.
- `enable_endpoint_detection: true` is the natural fit for dictation (fast commits) and justifies flipping `supports_endpointing`.

## 5. Integration risks

1. **Double billing / result authority (biggest design decision)**: today the tap is preview-only and the executor still uploads the WAV to `stt-async-v5` after stop (executor.rs:287). With a WS sink both run → audio paid twice, and Soniox bills the *entire stream duration*. Plan 043 must decide: WS `Final` becomes the pasted result (skip REST) with REST-on-WAV only as fallback on WS error/gap — unlike the Parakeet stance where batch stays authoritative (stream_tap.rs:299-303 discards `finalize()` text; that discard must change for Soniox-authoritative mode).
2. **Blocking recorder start**: sink factory runs under the recorder mutex (recorder.rs:310); a synchronous TLS+WS connect there delays recording start by RTT. Need async connect + local frame buffering until config-ack; the existing warm-up (audio.rs:4171-4186) warms `api.soniox.com`, **not** `stt-rt.soniox.com` — add a warm for the RT origin.
3. **Reconnect mid-stream**: final tokens are never re-sent and a new session resets context/speakers. On drop, committed prefix is safe locally; audio after `final_audio_proc_ms` is lost server-side. Either buffer raw audio locally keyed by ms and re-send from the last final offset on a fresh session, or accept the preview gap and rely on REST-on-WAV for the final. The WAV writer path is untouched by tap failures (plan 038 invariant), so fallback is always possible.
4. **Auth/quota failure mid-stream** arrives as an in-band JSON error (401/402), not a transport error — must be parsed and mapped; `SttError::message` (common.rs:26-41) already covers the user-facing strings.
5. **Sync/async bridge**: `StreamTapSink` runs on a std thread; the WS client is tokio-based. Bridge with a bounded tokio mpsc + spawned task (mirror `ParakeetStreamHandle`); `send_frame` must never block; `finalize()`'s `block_on` needs a hard timeout (server flush + `finished` normally < 1-2 s) then REST fallback.
6. **Keepalive**: while recording, mic frames flow continuously (silence is still frames), so keepalive only matters if the WS is opened before frames start or frames are dropped — send `{"type":"keepalive"}` on a 10 s idle timer anyway; never hold the socket open between recordings (duration billing).
7. **Secret hygiene**: the api_key travels in the first WS message — the config frame must never be logged (current REST code only logs response bodies, common.rs:172-184; keep that discipline).
8. **Gate generalization**: `build_parakeet_stream_sink_factory`'s engine check (audio.rs:167-173), the `parakeet_streaming_enabled` clamp (:4347-4349), and the Parakeet-named emit helper all assume one streaming engine; also suppress WS streaming when a remote connection is active (same check as warm-up :4175-4180; Soniox `shareable_remote: false`, provider_capabilities.rs:73).
9. **New dependency surface**: no WS crate today; whichever is chosen must enforce `wss` (mirror the `https_only` rationale, common.rs:106-113) and native/rustls TLS consistent with reqwest 0.13.
10. **Pinned truth-table tests** will fail loudly on the capability flip: stream.rs:246-271 (`capability_shape_for_every_current_engine` asserts all-final-only) — intentional update site.

Sources: [Soniox WebSocket API](https://soniox.com/docs/stt/api-reference/websocket-api), [Real-time transcription](https://soniox.com/docs/stt/rt/real-time-transcription), [Connection keepalive](https://soniox.com/docs/stt/rt/connection-keepalive), [Models](https://soniox.com/docs/stt/models)