# Plan 030 — Perf Tier 1: genuinely-free wins (no WER risk)

> First executable slice of the perf roadmap (research branch `research/handy-teardown`,
> `docs/voicetypr-perf/00-MASTER-ROADMAP.md`). These three wins need no measurement gate:
> they change no transcript output and no user-visible behavior — they only remove
> latency slack and hot-path allocations. Verified against `main` @ `af63ab1`.

## Scope

| ID | Win | File | Change |
|----|-----|------|--------|
| T1.5 | Stop-thread join poll 100 ms → 5 ms | `src-tauri/src/audio/recorder.rs:782` | shorten poll sleep only; `STOP_JOIN_TIMEOUT` cap unchanged |
| T1.6 | Level-meter: no unbounded send on the RT audio thread | `src-tauri/src/audio/level_meter.rs:6,47` + `src-tauri/src/audio/recorder.rs:271` | unbounded `mpsc::channel` → `mpsc::sync_channel(8)` + `try_send`, drop on full |
| T1.7 | Guard `log_with_context` allocations behind level check | `src-tauri/src/utils/logger.rs:262-280` | early-return via `log::log_enabled!` before building context strings |

Out of scope here (own plans, gated by the T1.0 harness): `audio_ctx`, flash-attn/turbo
speed mode, Parakeet warm preload, pill micro-bundle (T1.8), streaming pill.

## T1.5 — join poll

`AudioRecorder::stop_recording` polls `thread_handle.is_finished()` every 100 ms inside
the `STOP_JOIN_TIMEOUT` window, adding ~0–100 ms (avg ~45 ms) of pure wakeup slack to
every stop→text path. Change `Duration::from_millis(100)` → `Duration::from_millis(5)`
at `recorder.rs:782`. Do not touch the timeout constant or error paths.

## T1.6 — level-meter channel

`recorder.rs:271` creates an unbounded `mpsc::channel::<f64>()`; `AudioLevelMeter::
process_samples` calls `send()` from the CPAL callback (~10×/s per its `update_interval`),
which heap-allocates on the RT thread — the one plan-008 violation found. Note line 272
already uses `sync_channel(8)` for silence events; make the level meter match:

- `recorder.rs:271`: `mpsc::channel::<f64>()` → `mpsc::sync_channel::<f64>(8)`.
- `level_meter.rs`: field/param type `Sender<f64>` → `SyncSender<f64>`; replace `send()`
  with `try_send()` and ignore the error (`let _ =`). A dropped level frame is invisible
  (next update lands ≤100 ms later); do NOT log from this path — logging is itself an
  RT-thread allocation.
- Consumer (`commands/audio.rs:4327` `recv()` loop) is unchanged — `Receiver` type is
  identical for bounded/unbounded.

## T1.7 — logger guard

`log_with_context` (`logger.rs:262`) builds a `Vec<String>` + `format!` joins before the
log macro's level check, so disabled Debug calls still pay the allocations. Add at the
top:

```rust
if !log::log_enabled!(level) {
    return;
}
```

Keep everything else identical (message format must not change — tests/log-greps may
rely on it).

## Acceptance

- `cargo test` in `src-tauri` passes (at minimum `audio::` and `utils::logger` tests).
- `cargo clippy` introduces no new warnings in the touched files.
- No public API/type changes outside `level_meter.rs`'s constructor signature.
- Behavior parity: recordings start/stop, level meter still animates in the pill
  (manual smoke on `pnpm tauri:dev`).
