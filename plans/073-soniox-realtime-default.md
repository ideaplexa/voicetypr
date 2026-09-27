# Plan 073 — Soniox: realtime transport by default

Status: SPEC — Claude 2026-09-27. Target: 2.1.0-beta.2. Baseline `d04ffbd4`.

## Why

Users report Soniox is slow. Cause: outside live-preview mode, Soniox uses the
async REST flow (upload → create job → poll every 1 s → fetch → delete), so
stop → text costs several round trips plus up to a second of polling. The
realtime WebSocket path already exists for live preview and already owns the
pasted result (`take_cloud_ws_final`, `commands/audio.rs:6332`), with REST on the
recorded WAV as the fallback. Price: realtime $0.12/h vs async $0.10/h
(re-verified 2026-09), same context/vocabulary support.

Deepgram is out of scope: its prerecorded REST call is one synchronous request
(already fast), and its streaming price is higher. Revisit only with numbers.

## Change

When the selected engine is Soniox and a key is stored, stream every recording
over the realtime WS, whether or not live preview is on. Live preview only
decides whether partial text is shown in the pill.

1. `start_recording` (`commands/audio.rs` ~4871 and ~5110): add
   `let soniox_realtime = config.current_engine == "soniox";` and OR it into
   `streaming_tap_enabled` / `streaming_engine_enabled` for Soniox only. Other
   engines keep today's gating exactly (Parakeet's `parakeet_preview_sink_eligible`
   regression guard must stay intact).
2. `build_soniox_stream_sink_factory`: drop the `!live_preview_mode` early
   return; pass `show_preview = live_preview_mode` into the sink and the
   `on_partial` closure.
3. `SonioxPreviewStreamSink` (rename `SonioxStreamSink`): when `show_preview`
   is false, emit no `transcription-stream` events at all (no Started, Partial,
   Final, Error, Cancelled), but keep resolving `final_tx` exactly as today,
   including the dropped-frames invalidation. Keep the gate so preview-on
   behaviour is byte-identical.
4. Keep the existing log lines (`Cloud WS-final authoritative` / the fallback
   reason in `take_cloud_ws_final`). A typed `transport` (`ws` | `rest_fallback`)
   on `transcription.stage_finished` belongs to the observability slice (0.1b),
   not here. No content in either.

Unchanged: translation (`TranscriptionTask` other than Transcribe) still uses
REST (the WS config never translates) — but then do not open the WS at all for
translate jobs (no double billing). Cancel drops the WS and bills only the
streamed audio. The 4 s WS-final timeout and REST fallback stay.

## Review outcomes (gpt-6-astra medium + Claude, 2026-09-27)

- Fixed: an online remote server wins at stop, so no Soniox/Deepgram WS opens
  while one is online (deliberate for Deepgram too: its preview would bill a
  stream whose result the remote replaces).
- Fixed: translate jobs don't enable the tap for Soniox.
- Fixed: WS finals are tagged with their provider; a Soniox → Deepgram switch
  mid-recording can no longer paste Soniox text as Deepgram's result.
- Accepted: switching engine mid-recording, or a recording later discarded as
  no-speech, wastes the (short) stream already sent — cents per hour at most.
  Pinning the engine for the whole recording belongs to the clean core (0.4a).

## Tests (must exist before merge)

- Factory eligibility is a pure function (mirror `parakeet_preview_sink_eligible`):
  Soniox + key + regular mode → Some; Soniox + translate → None; Deepgram
  regular mode → None; Whisper/Parakeet regular mode → None (regression).
- Sink with `show_preview = false`: finalize resolves `final_tx` with the text,
  emits zero stream events; dropped frames → `Err` authority; cancel → no event.
- Sink with `show_preview = true`: unchanged events (existing tests stay green).
- Real check (Claude, not the implementer): with a Soniox key, 10 dictations in
  regular mode — log shows `Cloud WS-final authoritative`, no REST job created;
  measure stop → text before/after on the same clips via BlackHole.

## Acceptance

- Soniox regular mode: stop → text p50 drops clearly (target < 1 s on a good
  connection) with identical or better accuracy on the real-speech set.
- No REST call on the happy path; REST only on WS failure.
- `pnpm check` green on macOS; Windows fast check green.

## STOP conditions

- If the WS final is worse than REST on the real-speech set, stop and report.
- If keeping the tap on in regular mode changes recorder behaviour for other
  engines, stop.
