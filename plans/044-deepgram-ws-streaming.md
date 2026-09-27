# Plan 044 — Deepgram WebSocket streaming (live preview + result-authority)

> Executable spec. Deepgram batch REST already ships (`cloud_stt/deepgram.rs`, nova-3,
> Token auth, keyterms, diarization). This adds Deepgram STREAMING, mirroring the
> proven Soniox pattern (plans 043 + 043b) end-to-end in ONE slice: WS live preview
> (pill committed/tentative) AND result-authority (WS-final is the pasted text, REST
> only as fallback) — so it never double-bills, same stance Codex forced for Soniox.
>
> ⚠️ Re-verify every file:line before editing; report mismatches instead of guessing.
> ⚠️ The live WS handshake is a MANUAL founder smoke (needs a Deepgram key). Unit
> tests cover mapping/config/authority only.

## Deepgram live WS protocol (verified 2026-07-10 against developers.deepgram.com)

- **Endpoint:** `wss://api.deepgram.com/v1/listen` (same origin as REST — warm-up
  already hits it).
- **Auth:** `Authorization: Token <api_key>` HTTP header on the upgrade request —
  SAME `AuthScheme::Token` as the REST path, but as a WS handshake header (NOT in a
  body frame like Soniox). Never logged.
- **Query params (streaming supports the REST set):** `model=nova-3` (reuse
  `deepgram::MODEL`), `language=<lang>` (trimmed, omit if empty),
  `encoding=linear16`, `sample_rate=<native>`, `channels=<n>`,
  `interim_results=true`, `smart_format=true`, plus repeatable `keyterm=<term>`
  (nova-3 only — reuse `compile_deepgram_keyterms` + the `is_nova3` rule, see
  deepgram.rs:28-58). Build the URL with `reqwest::Url::parse` +
  `query_pairs_mut()` so keyterms are percent-encoded.
- **Audio:** binary frames of raw little-endian s16 PCM after connect.
- **Results (JSON text):** `{"type":"Results","is_final":bool,"speech_final":bool,
  "channel":{"alternatives":[{"transcript":"..."}]}}`. Interim (`is_final:false`)
  transcripts cover the CURRENT window and are replaced wholesale each message;
  `is_final:true` finalizes that window's transcript EXACTLY ONCE (never re-issued).
  Transcripts do NOT carry leading spaces (unlike Soniox RT tokens) — the folder
  joins finals with a single space.
- **End of audio:** send text `{"type":"CloseStream"}` → server flushes remaining
  Results, sends a `{"type":"Metadata",...}` summary, then closes cleanly.
  There is NO `finished:true` flag: **a clean close AFTER CloseStream was sent IS
  the completion signal; a close BEFORE that is an incomplete stream** (this is the
  Soniox Codex-finding lesson — encode it from day one).
- **KeepAlive:** text `{"type":"KeepAlive"}` — Deepgram drops the socket after
  ~10-12s without audio; send every 5s while not finalizing.
- **Errors:** handshake rejection surfaces as a connect error (401 → Auth);
  in-stream failures surface as transport errors / close frames → `Network`.

## Changes

### 1. NEW `src-tauri/src/cloud_stt/deepgram_rt.rs` (stub exists — fill it)

Pure, socket-free response mapping, mirroring `soniox_rt.rs`:

- Serde types (container-level `#[serde(default)]` everywhere so `{}`/garbage never
  panics): `DeepgramRtResponse { r#type: String, is_final: bool, speech_final: bool,
  channel: DeepgramRtChannel }`, `DeepgramRtChannel { alternatives:
  Vec<DeepgramRtAlternative> }`, `DeepgramRtAlternative { transcript: String }`.
  Helper `fn transcript(&self) -> &str` returning `alternatives[0].transcript`
  (empty string when absent).
- `DeepgramRtPartial { committed: String, tentative: String, revision: u64 }` —
  same shape as `SonioxRtPartial`.
- `DeepgramRtFolder { committed, revision }` with `ingest(&mut self, resp) ->
  DeepgramRtPartial`:
  - Only `type == "Results"` mutates state; other types (`Metadata`, unknown) still
    bump the revision and return the current state unchanged (harmless no-ops).
  - `is_final:true` with a non-empty (after trim) transcript → append to
    `committed` with a single `' '` separator (no leading space on the first);
    EMPTY finals are skipped (silence windows must not inject bare spaces).
  - `is_final:false` → `tentative` = that transcript, replaced wholesale.
    A final Results message CLEARS tentative (its window just committed).
  - `committed` stays append-only ⇒ satisfies
    `StreamSessionGate::assert_committed_monotonic`.
  - `committed(&self) -> &str` accessor.

### 2. NEW `src-tauri/src/cloud_stt/deepgram_ws.rs` (stub exists — fill it)

Mirror `soniox_ws.rs` structure exactly (Control enum, sync handle over async task,
unbounded channel, oneshot final, `FINALIZE_TIMEOUT = 3s`, `KEEPALIVE_EVERY = 5s`):

- `DeepgramStreamConfig { api_key: String, sample_rate: u32, channels: u16,
  language: Option<String>, keyterms: Vec<String> }`.
- `fn build_listen_url(config) -> Result<reqwest::Url, SttError>`: params per the
  protocol section; keyterms appended only when `is_nova3(MODEL)` — import the
  existing helpers from `deepgram.rs` (make `is_nova3` + `MODEL` `pub(super)` as
  needed) rather than duplicating the rule.
- Connect: `let mut request = url.as_str().into_client_request()?;` then insert
  `Authorization: Token <key>` into `request.headers_mut()` (the
  `tokio_tungstenite::tungstenite::client::IntoClientRequest` trait; header value
  built with `http::HeaderValue::from_str` — never logged, and mark it
  `set_sensitive(true)`).
- Run loop (same select shape as soniox_ws.rs):
  - `Control::Chunk` → binary frame (move `samples_to_le_bytes` from
    `soniox_ws.rs` into `cloud_stt/common.rs` as `pub(super) fn` and reuse it in
    BOTH ws modules — keep its unit test, in common.rs).
  - `Control::Finalize` → set `finalizing = true`, send `{"type":"CloseStream"}`.
  - `Control::Cancel`/None → send Close, resolve `Err(SttError::Network)` (result
    must never read as a completed transcript — Soniox lesson).
  - Text message → `serde_json::from_str::<DeepgramRtResponse>(..).unwrap_or_default()`;
    fold via the folder; call `on_partial` only when a `Results` message changed
    committed or tentative (skip Metadata noise).
  - `Close`/EOF: **if `finalizing` → `Ok(folder.committed())`** (clean completion —
    Deepgram's equivalent of `finished:true`); **else → `Err(SttError::Network)`**
    with a warn (incomplete stream; REST fallback owns the result).
  - Transport error → `Err(SttError::Network)`.
  - Keepalive tick (`if !finalizing`) → `{"type":"KeepAlive"}`.
- Same `SonioxStreamHandle`-style API surface: `send_chunk` (never blocks),
  `async finalize() -> Result<String, SttError>` (3s bound), `cancel()`, `Drop`
  sends Cancel. Name them `DeepgramStreamHandle` / `open`.

### 3. `src-tauri/src/cloud_stt/mod.rs`

`mod deepgram_rt;` + `mod deepgram_ws;` (match the visibility soniox_rt/soniox_ws
use). No `CloudProvider` changes — Deepgram already exists.

### 4. `src-tauri/src/commands/audio.rs`

- **Generalize the authority side-channel** (it is engine-agnostic already — at most
  ONE cloud WS factory registers per recording): rename `SONIOX_WS_FINAL` →
  `CLOUD_WS_FINAL`, `SonioxWsFinalMap` → `CloudWsFinalMap`,
  `take_soniox_ws_final` → `take_cloud_ws_final`. Update the existing unit tests'
  names/references accordingly (semantics unchanged — do NOT weaken them).
- **`DeepgramPreviewStreamSink`**: mirror `SonioxPreviewStreamSink` byte-for-byte in
  shape — Partials from the WS callback; `finalize(dropped_frames)` emits Final,
  resolves the oneshot `Ok(text)` ONLY when `dropped_frames == 0` (any RT tap drop
  → `Err(SttError::Network)`, warn log — dropped audio invalidates authority);
  `cancel()` cancels the handle, drops the sender, emits Cancelled.
- **`build_deepgram_stream_sink_factory`**: mirror the Soniox factory — gates on
  tap/engine/live-preview/`current_engine == "deepgram"`, key from secure store
  (`CloudProvider::Deepgram.key_name()`), registers the oneshot in `CLOUD_WS_FINAL`
  (with the same prune-then-insert), compiles keyterms via
  `crate::writing::compile_deepgram_keyterms` (same load-settings pattern the
  Soniox factory uses for context), language from `config.speech_language`
  (trimmed→Option), emits Started with `engine: "deepgram"`.
- **Dispatch** (~line 4390, re-verify): add `"deepgram"` to the streaming-engine
  `matches!` and a `"deepgram" => build_deepgram_stream_sink_factory(...)` arm.
- **Authority arm** (~line 5540, re-verify): widen the Soniox-only match to
  `ActiveEngineSelection::Cloud { provider: CloudProvider::Soniox |
  CloudProvider::Deepgram, .. }` (still `task == Transcribe` guarded). Update its
  comment: engine-agnostic cloud WS authority.

### 5. `src-tauri/src/transcription/stream.rs`

- `EngineStreamCapabilities::DEEPGRAM` → `{ supports_streaming: true,
  supports_committed_prefix: true, supports_tentative_tail: true,
  supports_endpointing: true, final_only: false }` (speech_final = native
  endpointing), comment citing plan 044 + result-authority.
- Truth-table test: move Deepgram out of `final_only_engines`, assert its full
  shape like Soniox's.

### 6. Docs

Append an Outcome note to this plan after implementation (billing stance: single
stream bill on the happy path; REST fallback only on WS gap).

## Tests (unit, no network)

`deepgram_rt.rs` (mirror soniox_rt's suite):
1. Finals accumulate space-joined; committed is monotonic byte-prefix
   (assert via `StreamSessionGate::assert_committed_monotonic`); revisions strictly
   increase.
2. Empty/whitespace finals are skipped (no bare-space injection).
3. Tentative replaced wholesale each interim; cleared by a final.
4. Metadata/unknown `type` doesn't corrupt state.
5. Partial/garbage JSON deserializes via defaults without panic (incl. missing
   `alternatives`).

`deepgram_ws.rs`:
6. `build_listen_url`: required params present; language omitted when empty;
   keyterms appended (and percent-encoded) for nova-3; API key NOT in the URL.
7. `samples_to_le_bytes` test lives in common.rs after the move.

`stream.rs`: truth-table update (test 8).
`commands/audio.rs`: existing `take_*` tests renamed to `take_cloud_ws_final`,
unchanged semantics.

## Acceptance

- `cargo clippy --lib` + `cargo test --lib` green; `pnpm typecheck && pnpm test`
  green (frontend needs NO changes — capability-driven toggle appears for Deepgram).
- Non-Deepgram engines: zero behavior change; Soniox path byte-identical apart from
  the renames.
- Founder smoke: Deepgram + Live preview toggle → pill streams, pasted text ==
  streamed final; kill network mid-recording → REST fallback still pastes.

## Outcome (2026-07-10)

Implemented by GLM 5.2 (xhigh) from this spec; gated + hardened by Claude; Codex
round 1 UNSOUND (close-authority state machine: failed CloseStream send could
still promote a truncated prefix to Ok; any post-flag close treated as clean) →
fixed (send failure bails to Err immediately; completion requires successful
CloseStream AND the post-CloseStream Metadata summary — `close_is_complete`
truth-table pinned in tests) → round 2 SOUND.

Billing stance: single stream bill on the happy path (WS-final is pasted); REST
runs only on a WS gap (connect failure, drop, dropped RT frames, incomplete
shutdown, empty text, translate task). Live handshake remains a manual founder
smoke (Deepgram key + Live preview toggle).
