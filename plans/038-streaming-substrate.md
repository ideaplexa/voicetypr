# Plan 038 — Streaming slice 2: PCM tap + FIFO substrate (no-op worker)

> On feat/037-stream-contract, stacked on the (uncommitted) 037 contract. The
> riskiest slice of the streaming program: it touches the CPAL callback. Design
> rules are absolute: NO alloc/lock/block on the RT thread (plan 008, documented
> recorder.rs:33-53); batch WAV path byte-identical when the flag is off AND
> functionally unaffected when on; everything behind a default-off settings key.
> Grounding verified 2026-07-02 (Explore) — file:lines below are from main @ af63ab1.

## Verified architecture being extended

- RT callback: per-format closures (recorder.rs:551-620) convert into reused scratch
  buffers, then shared `process_audio(f32_samples, i16_samples)` closure
  (:463-549) does: level meter try_lock, silence try_send, then
  `recycle_rx.try_recv()` → `chunk.extend_from_slice(i16_samples)` →
  `writer_tx.try_send(WriterMsg::Chunk)`. Bounded channels: writer 1024
  (:29,:339), recycle pool 1028 preallocated before stream.play() (:348-350),
  chunk_capacity from max_callback_samples (:316).
- Stop: stop signal → drain barrier `stop_requested`/`callback_drained`
  (≤200ms, :633-648) → stream pause/drop → `finalize_flag.store(true)` +
  best-effort `writer_tx.try_send(Finalize)` + drop(writer_tx) (:690-695) →
  bounded writer join (:703). Chunks recycled by writer after write (:409-410).
- Format at tap: native-rate interleaved i16 (WAV spec :330-335). 16k-mono-f32
  conversion happens offline at transcription (transcriber.rs:367-441 via
  audio/resampler.rs resample_to_16khz).
- Gate mechanism: settings store bool read in start_recording (pattern
  commands/audio.rs:3858-3862); no cargo features in use.
- Session: `begin_recording_generation()` bumps at start_recording top
  (audio.rs:3838-3843); `recording_generation_is_stale` (:86-87). The 037
  StreamSessionGate already consumes this.

## Design

New module src-tauri/src/audio/stream_tap.rs (+ wiring in recorder.rs,
commands/audio.rs):

1. **Message type** (one FIFO for everything — oracle P1):
   `enum StreamTapMsg { Frame(Vec<i16>), Finalize, Cancel }`
2. **Channels**: bounded `sync_channel::<StreamTapMsg>(STREAM_QUEUE_CAPACITY=256)`
   + its own recycle pool `sync_channel::<Vec<i16>>(260)`, preallocated with
   `Vec::with_capacity(chunk_capacity)` BEFORE stream.play() (mirror :348-350).
   The tap NEVER shares the writer's pool.
3. **RT tap** in `process_audio`, after the writer send, only when the tap is
   active: `pool.try_recv()` → extend_from_slice(i16_samples) → `tap_tx.try_send`.
   Any failure (pool empty / queue full) increments a relaxed AtomicU64 drop
   counter and moves on — a tap drop must NEVER affect the writer path and NEVER
   log from the callback. (Streaming quality degrades; recording integrity is the
   writer's job alone.)
4. **Finalize enqueue point**: exactly where finalize_flag is stored
   (recorder.rs:690-695): `tap_finalize_flag.store(true)` first (disconnect-
   independent, mirroring the writer's pattern) + best-effort
   `tap_tx.try_send(Finalize)` + drop the recorder-held tap sender. Guarantees
   Finalize follows the last Frame because the drain barrier already flushed the
   callback's final chunk. Cancel path: cancel_recording reaches the same recorder
   stop (audio.rs:6992); the worker distinguishes via the session's cancellation
   flag — treat as drain-and-discard (consume remaining frames, emit nothing).
5. **No-op worker** (std thread, spawned at recording start when enabled): owns a
   `StreamSessionGate` (037) seeded with the captured generation; consumes the
   FIFO; counts frames/samples/drops; on Finalize logs ONE summary line via
   log_performance("STREAM_TAP", elapsed, "frames=…, samples=…, dropped=…,
   generation=…") and exits. NO events emitted, NO resampling yet (that's slice
   039/040) — but structure the worker loop so a converter/engine can slot in
   (worker receives a trait object or closure in later slices; keep it simple now).
6. **Gate**: settings key `streaming_tap_enabled` (bool, default false, NOT
   surfaced in UI) read in start_recording near audio.rs:3858 and passed into
   `recorder.start_recording` as a plain bool. When false: zero new work in the
   callback (a `None` tap — match on Option, no atomics touched beyond the
   Option check baked in at stream build time).

## Tests (the point)

Unit tests in stream_tap.rs + recorder.rs test module (mirror the plan-008 test
style at recorder.rs:1004-1080):
1. FIFO ordering: N Frames then Finalize → worker observes all N before Finalize.
2. Queue overflow: with a full queue, sends drop (counter increments), never block,
   writer path unaffected (simulate by not draining tap while writer drains fine).
3. Cancel: frames + Cancel → worker discards, emits no summary-with-content
   (or summary marked cancelled), exits cleanly.
4. Pool discipline: after worker consumes, buffers return to the pool (recycle
   round-trip), and the callback-side code path performs no allocation when the
   pool is exhausted (drop instead — assert counter).
5. Flag off: recorder starts/stops with tap disabled → no tap thread spawned,
   behavior identical (existing tests keep passing untouched).
6. Stale generation: worker seeded with generation N, events would be gated if a
   new generation begins (use recording_generation_is_stale) — worker exits/discards.

## Acceptance
- cargo test passes (1195 + new). pnpm typecheck/lint/test pass (no TS changes
  expected — confirm none needed).
- With flag off (default): `git diff`-level proof that the callback hot path adds
  only an Option/branch check; existing recorder tests unmodified and passing.
- No Tauri event emission anywhere in this slice.
- Do not commit.
