# Plan 043b — Soniox result-authority (Phase 2 of plan 043)

> Executable spec. Makes the Soniox realtime WS stream's final text the AUTHORITATIVE
> pasted result (single bill), with REST-on-WAV demoted to fallback-on-failure. Removes
> the `VOICETYPR_SONIOX_STREAMING_PREVIEW` dev gate so the Live-preview toggle ships for
> Soniox users. Parakeet/Whisper stances are UNCHANGED (batch stays authoritative there).
>
> ⚠️ Re-verify every file:line before editing; report mismatches instead of guessing.

## Why the design is a side-channel, not the worker summary

- `StreamTapSink::finalize()` (stream_tap.rs:30) returns `Option<String>`, but
  `finalize_sink` (stream_tap.rs:43-45) discards it, and the recorder joins the tap
  worker with `STREAM_TAP_JOIN_TIMEOUT = 100ms` (recorder.rs:63) then DETACHES — while
  the Soniox WS finalize can take up to 3s (`soniox_ws.rs FINALIZE_TIMEOUT`). Threading
  text through `StreamTapWorkerSummary` would either lose the text or block WAV stop.
  DO NOT change stream_tap.rs or recorder.rs.
- Instead: the sink resolves a `tokio::sync::oneshot` when its WS finalize completes
  (on the detached tap worker thread), and the ALREADY-ASYNC transcription task in
  commands/audio.rs awaits that oneshot (bounded) before deciding whether to call the
  REST executor at all.

## Changes — all in existing files

### 1. `src-tauri/src/commands/audio.rs` — WS-final slot + sink wiring

Add near the other module-level statics:

```rust
/// Single-slot handoff of the Soniox WS-final text from the (detached) stream-tap
/// worker to the transcription task, keyed by recording generation. Single-slot:
/// starting a new preview recording replaces any stale entry.
static SONIOX_WS_FINAL: Lazy<
    StdMutex<Option<(u64, tokio::sync::oneshot::Receiver<Result<String, SttError>>)>>,
> = Lazy::new(|| StdMutex::new(None));
```

(Use whatever `once_cell::sync::Lazy` / `std::sync::Mutex` aliases the file already
imports; add imports only if missing. `SttError` = `crate::cloud_stt::common::SttError`.)

- `build_soniox_stream_sink_factory`: create the oneshot, store
  `(recording_generation, receiver)` into the slot (replacing any old entry), pass the
  `Sender` into the sink as `final_tx: Option<oneshot::Sender<Result<String, SttError>>>`.
  DELETE the `soniox_streaming_preview_enabled()` check (line ~516) and its comment.
- `SonioxPreviewStreamSink::finalize()`: keep the event emission exactly as-is, and
  additionally resolve the oneshot: `Ok(text)` on success, `Err(error)` on failure
  (`if let Some(tx) = self.final_tx.take() { let _ = tx.send(...); }`). Clone what's
  needed — the emitted event already clones `text`.
- `cancel()`: drop the sender (take + drop, or send `Err(SttError::Network)`) so a
  waiting receiver resolves immediately instead of timing out.
- Update the sink + factory doc comments: no longer "PREVIEW ONLY"/"double-bills" —
  now "WS final is the authoritative result; REST-on-WAV runs only as fallback".

### 2. `src-tauri/src/commands/audio.rs` — transcription task takes WS authority

Add a helper:

```rust
/// Take the Soniox WS-final receiver for `generation` (if one was registered) and
/// await it, bounded. Returns the authoritative text only when the WS path produced
/// non-empty text; every other outcome (no slot, generation mismatch, WS error,
/// empty text, timeout) returns None so the caller falls back to REST-on-WAV.
async fn take_soniox_ws_final(generation: u64) -> Option<String>
```

- Take the slot's receiver only when the stored generation matches; a mismatched entry
  is stale — remove it either way.
- Await with `tokio::time::timeout(Duration::from_secs(4), rx)` — 4s covers the sink's
  3s WS drain plus scheduling slack; a finished WS resolves instantly.
- `Ok(Ok(Ok(text)))` with `!text.trim().is_empty()` → `Some(text)`; everything else →
  `None` with a `log::info!`/`warn!` stating the fallback reason (never log the text).

In the transcription task match (the `ActiveEngineSelection::Whisper|Parakeet|Cloud`
arm around line ~5409): BEFORE building the executor request, for
`ActiveEngineSelection::Cloud { provider: CloudProvider::Soniox, .. }` AND
`transcription_job_for_task.task == TranscriptionTask::Transcribe`, try
`take_soniox_ws_final(task_generation).await`:

- `Some(text)` → `Ok(TranscriptionResult::new(&transcription_job_for_task, text))`,
  skipping the executor entirely. Log: WS-final authoritative, REST skipped.
- `None` → existing `build_desktop_transcription_request` + `transcribe_with_app`
  path, unchanged (this is the REST fallback and the only path that double-bills —
  and only on WS failure).

Guards that make this safe by construction:
- Translate jobs (`TranslateToEnglish`) never take WS authority (WS config doesn't
  translate).
- If the factory never ran (no live preview, no key, remote active, non-soniox
  engine), the slot is empty/mismatched → REST path is byte-for-byte today's behavior.

### 3. `src-tauri/src/transcription/stream.rs` — remove the dev gate

- Delete `soniox_streaming_preview_enabled()` and its uses.
- `EngineStreamCapabilities::for_engine(ProviderEngine::Soniox)` returns
  `Self::SONIOX` unconditionally.
- Update the pinned truth-table test: Soniox now `supports_streaming: true,
  supports_committed_prefix: true, supports_tentative_tail: true,
  supports_endpointing: true, final_only: false`. Remove any env-var-flag tests for
  the gate. All other engines unchanged.

### 4. Docs/comments sweep

- `soniox_ws.rs` header comment: "the caller falls back to the authoritative
  REST-on-WAV path" is now "REST-on-WAV is the FALLBACK path; the WS final is
  authoritative when it succeeds with non-empty text". Same doc-truth sweep on
  `soniox_rt.rs` header if it references preview-only.
- `plans/043-soniox-ws-streaming.md`: append an Outcome note: Phase 2 shipped,
  result-authority = WS-final with REST fallback, dev gate removed, billing single
  except on WS failure.

## Tests (unit, no network)

In `commands/audio.rs` tests (or the nearest existing test mod):
1. `take_soniox_ws_final` returns the text when the slot holds a matching generation
   whose sender resolved `Ok("hello")`.
2. Generation mismatch → `None`, and the stale slot is cleared.
3. Sender resolved `Err(..)` → `None`.
4. Sender resolved `Ok("   ")` (whitespace-only) → `None`.
5. Sender dropped (cancel path) → `None` quickly (no 4s wait — oneshot closes).

In `stream.rs`: truth-table update as above; gate-env tests deleted.

## Acceptance

- `cargo clippy --lib` + `cargo test` green; `pnpm typecheck && pnpm test` green
  (frontend should need NO changes — ModelsSection is already capability-driven; the
  toggle simply appears for Soniox now).
- Non-soniox engines and soniox-without-live-preview: zero behavior change.
- Manual smoke (founder): Soniox + Live preview toggle (no env var needed), speak,
  confirm pill preview streams AND the pasted text equals the streamed final; pull
  network mid-recording to confirm REST fallback still pastes.
