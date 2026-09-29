# Live Streaming Transcription Engine — Handy teardown

**TL;DR.** Handy makes text appear *while* the user is still speaking by running a dedicated worker thread that incrementally decodes 16 kHz audio frames as they arrive from the microphone, and emits a two-part `StreamTextEvent { committed, tentative }` to the overlay on every changed frame. The whole thing is built around three deliberately separate ideas: (1) a **single shared `mpsc` channel** that carries *both* audio frames and the finalize/cancel commands, so FIFO ordering is structurally guaranteed; (2) a **detached worker + RAII guard** that owns the engine *outside* the manager mutex so per-frame work never holds a global lock; and (3) a **committed/tentative split** inside the engine's `Stream` object so the on-screen prefix never flickers while the trailing words keep getting rewritten. A one-thread `TranscriptionCoordinator` serializes all lifecycle triggers (hotkey, signal, async paste) through a single mailbox, killing the start/stop/cancel races that plague dictation apps.

---

## 1. File map

| File | Lines | Purpose |
|---|---|---|
| `_handy-src/src-tauri/src/managers/transcription.rs` | 1955 | The streaming core: `StreamRouter`, `StreamCmd`, `start_stream`/`run_stream_worker`/`finalize_stream`/`cancel_stream`, the four atomics, the `StreamWorkerGuard` RAII, the `emit_stream_text`/`emit_stream_working` emitters, and the `StreamPerf` telemetry. |
| `_handy-src/src-tauri/src/transcription_coordinator.rs` | 184 | The lifecycle serializer: one thread + one `mpsc<Command>` mailbox, the `Stage = Idle \| Recording \| Processing` state machine, 30 ms press debounce. |
| `_handy-src/src-tauri/src/actions.rs` | 891 | Entry points: `TranscribeAction::start` (kicks `start_stream`) and `TranscribeAction::stop` (kicks `finalize_stream` with batch fallback). `FinishGuard` notifies the coordinator on pipeline completion. |
| `_handy-src/src-tauri/src/signal_handle.rs` | 38 | SIGUSR1/SIGUSR2 → coordinator mailbox (external triggers enter the same serialized path as hotkeys). |
| `_handy-src/src-tauri/src/managers/audio.rs` (excerpt) | — | Wires the recorder's per-frame callback to `router.feed(frame)`; the recorder holds `Arc<StreamRouter>` directly. |
| `_handy-src/src-tauri/src/audio_toolkit/audio/recorder.rs` (excerpt) | — | The frame-resampling + VAD pipeline that produces the 16 kHz, ~30 ms frames delivered to the callback. |
| `_handy-src/src-tauri/src/audio_toolkit/constants.rs` | — | `WHISPER_SAMPLE_RATE = 16000`. |

> The actual committed/tentative **split algorithm** lives in the published **`transcribe-cpp` crate (v0.1.0)** — the `Stream`, `StreamUpdate`, `StreamText` types. That crate is **not vendored** in `_handy-src` and its source is **not present** in this checkout's cargo registry (verified: `~/.cargo/registry/src/*` contains no `transcribe-*`). So §4 documents the *contract* Handy observes at its callsite precisely, and marks the internal mechanism `[INFERENCE]`.

---

## 2. Mechanism walkthrough

### 2.1 The event contract (what crosses the Rust→UI boundary)

Two tauri-specta events drive the streaming overlay (`managers/transcription.rs:47-83`):

```rust
/// `committed` is the append-only, flicker-free prefix; `tentative` is the
/// volatile suffix the model may still rewrite.
#[derive(..., tauri_specta::Event)]
pub struct StreamTextEvent {
    pub committed: String,   // transcription.rs:52
    pub tentative: String,   // transcription.rs:53
}

#[derive(..., tauri_specta::Event)]
pub struct StreamPhaseEvent {                 // transcription.rs:78
    pub phase: StreamPhase,                   // Listening | Working
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<StreamWorkKind>,         // transcribing | polishing
}
```

These are the **only** two events emitted by the streaming engine (`lib.rs:642-645` registers them in the specta bindings, not the legacy `events.ts`). The frontend never sees partial frames — only finished `{committed, tentative}` text snapshots and phase changes.

### 2.2 The frame path: microphone → worker (acceptance §1)

The recorder is given a per-frame callback at construction that holds the `Arc<StreamRouter>` **directly** — no Tauri state lookup, no manager mutex (`managers/audio.rs:148-168`):

```rust
.with_audio_callback({
    let router = stream_router;
    move |frame| { router.feed(frame); }   // audio.rs:165-167
})
```

The frames arriving here are already **16 kHz**, because the recorder's consumer thread resamples the device rate down to `WHISPER_SAMPLE_RATE` (`constants.rs:1` = 16000) into ~30 ms windows (`recorder.rs:477-481`, target `in_sample_rate/30.0` samples per window, `recorder.rs:495`). The VAD (`VadPolicy::Streaming`) gates which frames become `Speech` vs dropped as `Noise` (`recorder.rs:532-540`) — that VAD boundary is owned by slice 4, but the key point for this slice is: **only post-VAD speech frames reach `router.feed`**.

`StreamRouter::feed` is the hot path and is deliberately cheap (`transcription.rs:142-151`):

```rust
/// Forward a 16 kHz frame to the active streaming worker. Cheap no-op (a
/// single relaxed atomic load) when no stream is pending.
pub fn feed(&self, frame: &[f32]) {
    if !self.open.load(Ordering::Relaxed) {   // transcription.rs:145
        return;                               // ← THE no-stream cost
    }
    if let Some(tx) = self.tx.lock().unwrap().as_ref() {
        let _ = tx.send(StreamCmd::Feed(frame.to_vec()));   // :149
    }
}
```

**No-stream-pending cost = exactly one `Relaxed` atomic load** of the `open` flag (`transcription.rs:107, 145`). This matters because the audio callback fires ~33×/s (30 ms frames) *whether or not* a stream is running; a mutex lock or state lookup on every frame would be real overhead. When a stream *is* open, the frame is copied into a `Vec<f32>` and sent as `StreamCmd::Feed`.

### 2.3 Why Feed + Finalize share ONE channel (the FIFO guarantee)

`StreamCmd` is a single enum over the *one* worker channel (`transcription.rs:85-94`):

```rust
/// Commands sent to the streaming worker thread. Audio frames and the finalize
/// request travel the same channel so FIFO ordering guarantees every fed frame
/// is processed before finalize runs.
enum StreamCmd {
    Feed(Vec<f32>),
    /// Flush the stream and reply with the final text, or `None` ...           // :90-91
    Finalize(mpsc::Sender<Option<String>>),
    Cancel,
}
```

The router owns that channel (`transcription.rs:101-108`):

```rust
pub struct StreamRouter {
    tx: Mutex<Option<mpsc::Sender<StreamCmd>>>,  // :104 — present start_stream→finalize/cancel
    open: Arc<AtomicBool>,                        // :107 — checked first in feed()
}
```

Because `Feed` and `Finalize`/`Cancel` are variants of the **same** `mpsc::Sender`, they are totally ordered by channel insertion. `finalize_stream` calls `router.take()` (which atomically closes `open` to new feeds AND pulls the sender out) and *then* sends `Finalize` on that same sender (`transcription.rs:1039-1046`):

```rust
pub fn finalize_stream(&self) -> Result<Option<String>> {
    let Some(tx) = self.router.take() else { return Ok(None); };  // :1040 — closes open, grabs sender
    let (reply_tx, reply_rx) = mpsc::channel();
    if tx.send(StreamCmd::Finalize(reply_tx)).is_err() { ... }    // :1044 — queued BEHIND all prior Feeds
    ...
}
```

So the worker's `while let Ok(cmd) = rx.recv()` loop (`transcription.rs:927`) processes every `Feed` enqueued before the stop before it ever sees `Finalize`. There is no second "control" channel that could overtake frames, and no `try_recv` race. This is the structural guarantee that **no audio is lost between the last frame and finalize**, and that **cancel can't interleave between a frame and its decode**.

### 2.4 Worker lifecycle: monotonic id, RAII guard, token gating (acceptance §2)

`start_stream` is the spawn site (`transcription.rs:760-778`):

```rust
pub fn start_stream(&self) {
    if self.router.is_open() || self.active_stream_worker.load(Ordering::Acquire) != 0 {
        warn!("start_stream called while a stream worker is already active"); // :762
        return;
    }
    let worker_id = self.next_stream_worker_id.fetch_add(1, Ordering::Relaxed); // :765 — MONOTONIC, starts at 1
    if self.active_stream_worker
        .compare_exchange(0, worker_id, Ordering::AcqRel, Ordering::Acquire)   // :766-768
        .is_err() { warn!("...lost a race with another stream worker"); return; }
    let rx = self.router.open();            // :774
    self.stream_active.store(false, Ordering::Release);  // :775 — not "active" until the Stream actually begins
    let manager = self.clone();
    thread::spawn(move || manager.run_stream_worker(rx, worker_id));  // :778 — detached
}
```

Three things to notice:

1. **Monotonic `next_stream_worker_id`** (`transcription.rs:243, 269, 765`) starts at **1** and only ever `fetch_add`s — `0` is the sentinel meaning "no worker". This makes every worker globally unique for its process lifetime.
2. **`active_stream_worker` is taken with `compare_exchange(0 → worker_id)`** (`:766`): if two `start_stream` calls race, exactly one wins the CAS; the loser logs and returns. This covers the window *after* the router is closed but *before* the previous worker has fully exited (see the field doc at `:244-247`).
3. **`stream_active` is set false here**, not true. It only flips true once the engine's `Stream` actually begins decoding (`:919`), so the overlay/UI gate reflects a *real* in-flight decode, not a worker that's still waiting on a model load.

The worker's first act is to construct the RAII guard (`transcription.rs:781-787`):

```rust
fn run_stream_worker(&self, rx: mpsc::Receiver<StreamCmd>, worker_id: u64) {
    let _worker = StreamWorkerGuard {
        worker_id,
        active_stream_worker: Arc::clone(&self.active_stream_worker),
        active_engine_lease: Arc::clone(&self.active_engine_lease),
        stream_active: Arc::clone(&self.stream_active),
    };
```

Because the worker thread is **detached** (`thread::spawn`, not `JoinHandle`), it can exit via a normal return, an early return, **or a panic** that unwinds it. The guard's `Drop` therefore does the cleanup unconditionally and — critically — **token-checks** every clear so a stale/old worker cannot clobber a newer worker's flags (`transcription.rs:199-217`):

```rust
impl Drop for StreamWorkerGuard {
    fn drop(&mut self) {
        // Only clear stream_active if WE are still the active worker.
        if self.active_stream_worker.load(Ordering::Acquire) == self.worker_id {  // :201
            self.stream_active.store(false, Ordering::Release);                   // :202
        }
        // Return the engine lease — but only if our id still owns it.
        let _ = self.active_engine_lease.compare_exchange(                        // :204
            self.worker_id, 0, Ordering::AcqRel, Ordering::Acquire);
        // Relinquish "active worker" — same token check.
        let _ = self.active_stream_worker.compare_exchange(                        // :210
            self.worker_id, 0, Ordering::AcqRel, Ordering::Acquire);
    }
}
```

This is the classic **generation-token / compare_exchange pattern**: every mutation is conditional on the worker's own `worker_id` still matching. If a (hypothetical) start/finalize race ever let worker N+1 begin before worker N fully tore down, N's `Drop` would see `active_stream_worker == N+1 ≠ N` and **leave the flags alone** instead of zeroing a live worker's state. `Acquire`/`Release`/`AcqRel` orderings are used because these atomics publish engine-ownership and UI-visibility decisions across threads.

### 2.5 Engine leasing (acceptance §3): the engine leaves the mutex

The four atomics are documented together as deliberately independent flags (`transcription.rs:238-251`):

```rust
/// Streaming uses four independent flags: router open = frames should route,
/// worker active = no second worker may start, engine lease = engine is out
/// of the mutex, stream active = UI should show a live session.
```

Leasing happens in `run_stream_worker` after it waits out any in-progress model load on the `loading_condvar` (`:791-796`). First it claims the lease, then physically `take()`s the engine out of the mutex (`:804-832`):

```rust
if self.active_engine_lease
    .compare_exchange(0, worker_id, Ordering::AcqRel, Ordering::Acquire)        // :804-806
    .is_err() { ... drain_until_finalize(rx); return; }                          // someone else holds the engine
let mut engine = match self.lock_engine().take() {                              // :814 — engine LEAVES the mutex
    Some(e) => e,
    None => { /* model unloaded mid-race; release lease, drain, fall back */ }   // :816-831
};
```

**Why lease at all?** So that ~33 decode calls/second do **not** hold the global `engine` `Mutex` for the entire recording. The engine is *owned* by the worker thread for the stream's lifetime; other paths that need the engine (model unload, capability queries) see an empty mutex and consult the lease instead. `is_model_loaded()` is the canonical example (`transcription.rs:356-360`):

```rust
pub fn is_model_loaded(&self) -> bool {
    // The engine may be leased out to the streaming worker (taken out of
    // the mutex). It's still loaded, just in use, so report true.
    self.lock_engine().is_some() || self.active_engine_lease.load(Ordering::Acquire) != 0
}
```

Without the lease check, the idle watcher (`:279-343`) would see `engine == None` while you're mid-sentence and conclude the model is unloaded — and so would any "start" path that gates on `is_model_loaded()`. The lease keeps the world consistent: the model is loaded, it's just borrowed.

The lease structurally also **excludes concurrent batch transcription**: batch `transcribe()` does `engine_guard.take()` itself (`transcription.rs:1164-1176`) and would get `None` while a stream holds the engine — exactly the intended mutual exclusion, without a separate "streaming in progress" lock.

When the worker finishes, the engine is handed back via `return_engine`, which only restores it if the model id hasn't changed underneath (else the stale engine is dropped) (`transcription.rs:1017-1031`), and the guard's `Drop` returns the lease to `0`.

### 2.6 The committed/tentative emit (acceptance §4) — the anti-flicker mechanism

This is the core loop, inside the `'stream:` labeled block so the `&mut engine` borrow ends cleanly before `return_engine` (`transcription.rs:898-996`). The stream object comes from the engine session (`:911`):

```rust
let mut stream = match session.stream(&run_options, &StreamOptions::default()) {
    Ok(s) => s, ...
};
self.stream_active.store(true, Ordering::Release);   // :919 — NOW the UI may show a live session
...
while let Ok(cmd) = rx.recv() {                       // :927
    match cmd {
        StreamCmd::Feed(pcm) => {
            ...
            match stream.feed(&pcm) {                 // :933 — incremental decode
                Ok(update) => {
                    perf.record_update(update.revision, update.input_received_ms,
                                       update.audio_committed_ms, update.buffered_ms);
                    if update.committed_changed || update.tentative_changed {   // :942
                        let text = stream.text();      // :943
                        perf.record_emit();
                        self.emit_stream_text(&text.committed, &text.tentative); // :945
                    }
                    perf.maybe_log();
                }
                ...
            }
        }
```

**The emit (`transcription.rs:1086-1092`)** just forwards the two strings as the event:

```rust
fn emit_stream_text(&self, committed: &str, tentative: &str) {
    let _ = StreamTextEvent {
        committed: committed.to_string(),
        tentative: tentative.to_string(),
    }
    .emit(&self.app_handle);
}
```

**Where the split happens.** The split is **not** computed in `transcription.rs`. Handy treats the engine's `Stream` as the authority: `stream.feed(&pcm)` returns a `StreamUpdate` carrying `committed_changed: bool`, `tentative_changed: bool`, a monotonic `revision: i32`, and three timing fields (`input_received_ms`, `audio_committed_ms`, `buffered_ms`). Handy only *reads* the split via `stream.text()`, which yields an object with `.committed`, `.tentative`, and `.display()` (= committed + tentative). All of these types — `Stream`, `StreamUpdate`, the text object — are defined by the **`transcribe_cpp`** crate (returned by `Session::stream`; the `StreamOptions`/`Session` imports are at `transcription.rs:19-22`). I could **not** read the split algorithm because:

- It is a published crate (`transcribe-cpp = "0.1.0"`, `Cargo.toml:76,138`), **not vendored** under `_handy-src`.
- Its source is **absent** from this machine's cargo registry (`ls ~/.cargo/registry/src/* | grep transcribe` → none; a filesystem-wide `find` for `transcribe-cpp*` returned nothing).

So the **mechanism I can authoritatively state** (grounded in the contract Handy observes):

- The engine maintains an internal **monotonic `revision`** (`update.revision`, threaded into `StreamPerf::record_update` at `:1464-1469` and logged at `:1496`). Each `feed` either leaves the text unchanged or bumps the revision and sets `committed_changed`/`tentative_changed`.
- `committed` is **append-only**: it only ever grows (or stays), and once text is in `committed` it is **never rewritten** — that's the "flicker-free prefix" the doc-comment promises (`:48`).
- `tentative` is **volatile**: it is the trailing text the model is still uncertain about and may rewrite on the *next* `feed`.
- The overlay is only re-emitted when something actually changed (`if update.committed_changed || update.tentative_changed`, `:942`), so frames that decode to identical text are free.

**The anti-flicker mechanism at the code level.** Because `committed` is append-only, the frontend can render it as a stable prefix and *only* re-render the `tentative` tail on each event. The words the user has already "locked in" (in `committed`) never jump; only the still-forming trailing words flicker as the model revises them. This converts the classic streaming-STT problem ("every redraw rewrites the whole sentence, causing violent flicker") into a localized tail-flicker that reads as natural typing. `[INFERENCE]` The internal heuristic is almost certainly a **chunk/segment boundary**: the engine decodes audio in fixed chunks and promotes a chunk's text to `committed` only once enough subsequent audio confirms it (so the prefix is stable), while the most-recent chunk stays `tentative` until the next boundary. The `audio_committed_ms` vs `input_received_ms` fields exposed in `StreamUpdate` (`:939`, logged at `:1493-1494`) strongly imply the engine tracks *how many ms of audio have been promoted to committed* separately from *how much has been received* — i.e. there is a deliberate lag between "heard" and "committed" that produces the stability.

> **Honest gap.** The exact promotion rule (chunk size, confirmation lookahead, whether CJK tokenization changes the boundary) is inside `transcribe_cpp::Stream` and is **not verifiable** from this checkout. To confirm it you would need to `cargo fetch` the crate and read its `stream` module. The *contract* above is exact; the *algorithm* is inferred.

### 2.7 The finalize path (acceptance §5)

`Finalize` carries a reply `Sender` so the caller can block on the final string. Inside the worker (`transcription.rs:955-987`):

```rust
StreamCmd::Finalize(reply) => {
    let finalize_start = Instant::now();
    let result = match stream.finalize() {
        // After finalize the committed prefix holds the full text;
        // display() = committed + tentative is the safe read.
        Ok(update) => {
            perf.record_compute(finalize_start.elapsed());
            perf.record_update(update.revision, ...);
            Some(stream.text().display())              // :968 — committed + tentative, post-flush
        }
        Err(e) => { ... None }                          // finalize failed → batch fallback
    };
    ...
    finalize_reply = Some(reply);                       // :984
    finalize_result = Some(result);                     // :985
    break;                                              // :986 — exit the loop
}
```

Key reconciliation point: **after `finalize()`, the safe read is `stream.text().display()` (= committed ⊕ tentative)**, not `committed` alone. The comment at `:958-959` is explicit: "After finalize the committed prefix holds the full text; `display() = committed + tentative` is the safe read." So finalize flushes the volatile tail: whatever was still `tentative` at the last `Feed` gets promoted/frozen by `finalize()` and the union is taken as authoritative.

The worker then exits the `'stream:` block, `return_engine`s the engine (`:1009`), and sends the result on the reply channel (`:1010-1012`). The *caller* side (`finalize_stream`, `:1039-1067`) blocks on `reply_rx.recv_timeout(STREAM_FINALIZE_REPLY_TIMEOUT)` (30 s, `:37`), then runs the **shared post-processing** (custom-word fuzzy correction etc.) on the raw string and triggers `maybe_unload_immediately`:

```rust
let raw = match reply_rx.recv_timeout(STREAM_FINALIZE_REPLY_TIMEOUT) { ... };  // :1047
let filtered = post_process_transcription_text(raw, &settings, false);         // :1063 — custom_words via post-correction
self.maybe_unload_immediately("streaming transcription");                       // :1065
Ok(Some(filtered))
```

Note the comment at `:1061-1063`: streaming models never receive a decode prompt, so custom words are applied **only** through the shared fuzzy post-correction path (`custom_words_already_prompted = false`), unlike the batch whisper path which can seed an initial prompt.

`Ok(None)` semantics (`:1034-1038`): no usable stream was active (model not streaming-capable, never began, or finalize errored after the engine was returned) → the caller may fall back to batch transcription. A **timeout is `Err`, not `Ok(None)`** (`:1051-1057`) because the worker may still hold the engine — falling back to batch would contend with it, so the error is surfaced instead.

### 2.8 The fallback drains

When streaming can't run (not a transcribe-cpp model `:857-865`, unsupported `:867-872`, engine raced away `:814-832`, or begin failed `:913-916`), the worker must still complete the **finalize handshake** so `finalize_stream` unblocks. That's `drain_until_finalize` (`transcription.rs:1651-1662`):

```rust
fn drain_until_finalize(rx: mpsc::Receiver<StreamCmd>) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            StreamCmd::Feed(_) => {}                      // discard queued audio
            StreamCmd::Finalize(reply) => { let _ = reply.send(None); break; }  // caller → batch fallback
            StreamCmd::Cancel => break,
        }
    }
}
```

This guarantees the "finalize → `Ok(None)` → batch transcribe" contract holds even when the stream never started. `cancel_stream` (`:1070-1075`) is the analogous teardown for cancel: `router.take()` then send `StreamCmd::Cancel`, which makes the worker `stream.reset()` + `break` (`:988-991`).

### 2.9 The coordinator: one thread, one mailbox (acceptance §6)

`TranscriptionCoordinator` is the lifecycle serializer (`transcription_coordinator.rs:33-38`):

```rust
/// Serialises all transcription lifecycle events through a single thread
/// to eliminate race conditions between keyboard shortcuts, signals, and
/// the async transcribe-paste pipeline.
pub struct TranscriptionCoordinator { tx: Sender<Command> }
```

It owns a **single `mpsc` mailbox** and a **single consumer thread** (`:45-48`). The command and stage types (`:13-31`):

```rust
enum Command {
    Input { binding_id, hotkey_string, is_pressed, push_to_talk },   // :14-19
    Cancel { recording_was_active: bool },                            // :20-22
    ProcessingFinished,                                               // :23
}

enum Stage {                                                          // :27-31
    Idle,
    Recording(String), // binding_id
    Processing,
}
```

All three external triggers collapse to the same `Command::Input` message:
- **Hotkeys** → `send_input(...)` (`coordinator.rs:121-140`), called from the shortcut layer.
- **Signals** (SIGUSR1 = `transcribe_with_post_process`, SIGUSR2 = `transcribe`) → `send_transcription_input` (`signal_handle.rs:14-22, 29-33`), which calls the same `send_input` with `is_pressed: true, push_to_talk: false`.
- **Async paste pipeline completion** → `FinishGuard::drop` posts `ProcessingFinished` (`actions.rs:32-40`), and `notify_cancel` posts `Cancel` (`coordinator.rs:142-152`).

Because every trigger funnels through `tx.send(Command::…)`, the consumer thread applies them **one at a time, in arrival order**, against a single mutable `Stage`. The decision logic (`coordinator.rs:60-107`):

```rust
// Debounce rapid-fire press events (key repeat / double-tap).
// Releases always pass through for push-to-talk.
if is_pressed {
    let now = Instant::now();
    if last_press.is_some_and(|t| now.duration_since(t) < DEBOUNCE) {   // :65 — 30 ms
        debug!("Debounced press for '{binding_id}'");
        continue;
    }
    last_press = Some(now);
}
if push_to_talk {
    if is_pressed && matches!(stage, Stage::Idle) { start(...); }       // :73
    else if !is_pressed && matches!(&stage, Stage::Recording(id) if id == &binding_id) { stop(...); } // :75-78
} else if is_pressed {
    match &stage {
        Stage::Idle => start(...),                                      // :83 — toggle on
        Stage::Recording(id) if id == &binding_id => stop(...),         // :86 — toggle off
        _ => debug!("Ignoring press ...: pipeline busy"),               // :88 — drop, don't corrupt
    }
}
```

**The 30 ms debounce** (`DEBOUNCE = Duration::from_millis(30)`, `:10`) collapses OS key-repeat and accidental double-taps: a second press within 30 ms of the last is dropped (`:65-68`). Critically, **releases always pass through** (`:62-63`) so push-to-talk can never get stuck recording because a release was eaten by the debounce.

**`start`/`stop` set Stage defensively** (`coordinator.rs:161-184`): `start` only transitions to `Recording` if recording *actually* began (it re-checks `AudioRecordingManager::is_recording()`, `:167-170`), else stays `Idle`; `stop` transitions straight to `Processing`, and only `ProcessingFinished` (from the paste pipeline's `FinishGuard`) or a valid `Cancel` returns it to `Idle` (`:94-106`).

**The races this design eliminates.** With hotkey, signal, and async-paste all on separate threads, a naive design races on at least three axes:
1. **Press during Processing** — a hotkey arriving while the async paste is mid-flight could start a new recording that clobbers the in-progress transcription. The single mailbox serializes this: `Stage::Processing` is not `Idle`, so a press hits the `_ => pipeline busy` arm (`:88-91`) and is dropped. No lock needed — the state is only ever touched by the one consumer thread.
2. **Start vs start** — two rapid starts (key-repeat + signal) would double-open the recorder/stream. The 30 ms debounce plus the `Stage::Idle` guard prevent it; even if two legitimate presses slipped through, they'd be processed sequentially, so the second sees `Stage::Recording` and is ignored.
3. **Cancel during Processing** — `Command::Cancel` explicitly checks `!matches!(stage, Stage::Processing)` (`:98`) so a cancel can't tear down a transcription that's already past the stop point; it waits for `ProcessingFinished`.
4. **Stop vs stop / paste vs paste** — because the async paste runs in `tauri::async_runtime::spawn` (`actions.rs:605`) on the Tokio pool but its *completion* is reported through the synchronous `FinishGuard::drop → notify_processing_finished → tx.send`, there is no async/sync data race on `Stage`; the mailbox is the single writer.

The thread is wrapped in `catch_unwind` (`:49-113`) so a panic in `start`/`stop` logs and the loop continues rather than killing the serializer silently.

---

## 3. Why it feels lightweight / smooth / instant

- **Zero-overhead idle frames.** When no stream is pending, the per-frame callback does exactly one `Relaxed` atomic load and returns (`transcription.rs:145`). The recorder's always-on mic mode fires this ~33×/s whether or not you're dictating; making it a single load keeps the audio thread trivially cheap.
- **No global lock during decode.** The engine is *leased out* of the mutex (`:814`), so the 33 decodes/sec run on the worker thread with no contention. `is_model_loaded()` stays truthful via the lease (`:359`), so no other subsystem gets confused into reloading or unloading mid-sentence.
- **Structural FIFO, no locks for correctness.** Feed + Finalize on one channel means the worker *cannot* miss the last frames before stop; there is no "did the stop signal race the last buffer?" bug class.
- **Anti-flicker rendering.** The committed/tentative split means the user sees stable locked-in text with only a small live tail moving — it reads as fluent typing, not a jittering guess.
- **Lazy `stream_active`.** The UI "live session" flag is set only after the `Stream` actually begins (`:919`), not when the worker spawns, so the overlay never shows "live" during the model-load wait and then disappoints.
- **Telemetry that proves the real-time factor.** `StreamPerf` logs audio-seconds vs compute-seconds and the real-time factor `audio_secs / compute_secs` (`:1532-1538`) every 5 s (`:36`), with `input_received_ms` / `audio_committed_ms` / `buffered_ms` / `revision` to confirm the engine is keeping up with the mic in real time.
- **One-thread lifecycle.** The coordinator removes a whole category of "recording started but paste didn't fire" / "two recordings overlapped" bugs by making lifecycle transitions strictly sequential.

---

## 4. Voicetypr takeaways

| Priority | Mechanism to adopt | One-line rationale |
|---|---|---|
| **High** | **Single shared `mpsc` for frames + control.** Put `Feed(frame)`, `Finalize(reply)`, and `Cancel` on one channel (`transcription.rs:88-94`). | FIFO ordering structurally guarantees the last frame is decoded before finalize — eliminates the lost-tail and interleaving bug class with zero locks. |
| **High** | **RAII worker guard with generation-token `compare_exchange`.** Clear worker/lease/stream-active flags only if `active_stream_worker == my_id` (`transcription.rs:199-217`). | Cancel/panic/early-return safety for a *detached* worker thread, and staleness-proofing if a start/finalize race ever leaks through. |
| **High** | **Engine leasing (move the engine out of the mutex during decode).** `take()` the engine into the worker; gate `is_model_loaded()` on `active_engine_lease != 0` (`:356-360, 804-832`). | Lets ~33 decodes/sec run lock-free and structurally excludes concurrent batch transcription without a separate "busy" lock. Voicetypr's blocking decode currently holds a lock — this is the biggest smoothness win. |
| **High** | **`Arc<StreamRouter>` held directly by the audio callback**, with a `Relaxed`-load `open` fast-path (`:101-151`, `audio.rs:163-167`). | A single atomic load per frame when idle; no Tauri-state lookup or manager lock on the audio thread. Directly cuts frame-callback latency. |
| **High** | **One-thread `TranscriptionCoordinator` mailbox** for hotkey + signal + async-paste lifecycle (`coordinator.rs:33-117`). | Turns start/stop/cancel races into a sequential state machine; the `Stage::Processing` guard alone prevents "press during paste" corruption. |
| **Med** | **committed/tentative split in the streaming contract.** Emit `{committed: append-only, tentative: rewritable}` and have the UI render only the tail as volatile (`:51-54, 942-945`). | The single biggest *perceptual* smoothness lever: stable prefix + flickering tail reads as typing, not guessing. (Requires the engine to expose the split — see Open questions.) |
| **Med** | **`Ok(None)` vs `Err` finalize semantics with batch fallback** (`:1034-1067`, `actions.rs:650-660`). | Clean degradation: unsupported/empty stream → batch transcribe; only a true timeout (engine possibly still held) is surfaced as an error. Lets you ship streaming with a safe fallback. |
| **Med** | **`drain_until_finalize` for the can't-stream case** (`:1651-1662`). | Keeps the finalize handshake intact even when streaming never starts, so the fallback path never deadlocks the caller. Small but essential for robustness. |
| **Med** | **30 ms press debounce, releases always pass through** (`coordinator.rs:10, 62-70`). | Kills key-repeat double-starts without ever trapping push-to-talk in a stuck-recording state. |
| **Med** | **Deferred `stream_active` set (after Stream begins, not on spawn)** (`:775, 919`). | Prevents the overlay from advertising a live session that's actually still waiting on a model load. |
| **Med** | **`return_engine` with model-id guard** (`:1017-1031`). | If the model is switched/unloaded mid-stream, drop the stale engine instead of restoring a zombie — no half-broken engine ever re-enters the pool. |
| **Low** | **`StreamPerf` telemetry** (real-time factor, committed-vs-received audio ms, revision) (`:1426-1538`). | Cheap proof that streaming keeps up with the mic; invaluable for diagnosing "it lagged" reports. Adopt if you want observability parity. |
| **Low** | **`catch_unwind` around the coordinator loop** (`coordinator.rs:49-113`). | A panic in start/stop logs instead of silently killing the only lifecycle serializer. Defensive but cheap insurance. |

**What to AVOID:** the four-flag proliferation (`router open`, `active_stream_worker`, `active_engine_lease`, `stream_active`) is powerful but easy to misorder. If Voicetypr adopts it, lift the exact token-CAS invariants from `start_stream` and the guard `Drop` verbatim — half-applying this pattern is *worse* than a single mutex because the flags can drift out of sync.

---

## 5. Open questions / risks

- **`transcribe_cpp::Stream` split algorithm is unverified.** The committed/tentative promotion rule (chunk size, confirmation lookahead, CJK tokenization effects) is `[INFERENCE]`. The contract Handy observes is exact; the internals are not. **To confirm:** `cargo fetch` in `_handy-src/src-tauri` and read the `Stream`/`StreamUpdate`/`StreamText` types in the fetched `transcribe-cpp` source. If Voicetypr's engine (whisper.cpp / Parakeet) does not natively expose a committed/tentative split, the takeaways in §4 that depend on it (the Med row) require Voicetypr to *build* the split at its own adapter layer (e.g. commit a whisper segment only after N ms of following audio confirms it).
- **`frame.to_vec()` copy on every Feed** (`transcription.rs:149`). Each live frame is copied into a heap `Vec` to cross the channel. At ~33 frames/s this is negligible, but on a frame-starved or very high-frame-rate engine it's an allocation hot spot. A `crossbeam`/`flume` channel taking `&[f32]` slices (or a ring buffer) would remove it — but the current design's simplicity is a deliberate tradeoff.
- **Detached worker thread + `catch_unwind` reliance.** The worker is `thread::spawn` with no join; if it panics, the `StreamWorkerGuard` `Drop` runs during unwind (good) but the engine, if already moved into `engine` but not yet into the `Stream` borrow region, relies on `return_engine`/guard ordering to come back. The code paths look correct, but a fork adding new early-return points must thread the `return_engine` + `drain_until_finalize` discipline carefully or leak the engine out of the mutex.
- **Finalize 30 s timeout** (`STREAM_FINALIZE_REPLY_TIMEOUT`, `:37`). If the engine hangs, the caller gets `Err` and the worker may still hold the engine (no forced kill). There is no mechanism to *force-cancel* a wedged worker thread — it relies on the engine's `finalize()` eventually returning. A misbehaving native engine could hold the lease indefinitely, blocking all subsequent transcription until app restart.
- **`stream_active` vs `router.open` vs `active_stream_worker` consistency.** The three are set/cleared at different points and the system's correctness depends on their exact ordering (e.g. `cancel_stream` clears `stream_active` directly at `:1074` *and* the guard clears it again on drop at `:202`). This is belt-and-suspenders by design, but a refactor that changes one clear site could desync them. Tests in `_handy-src/src-tauri/src/managers/transcription.rs:1886-1929` exist but I did not deep-audit them for these invariants.
