# Parakeet (CoreML/ANE) engine — VoiceTypr perf teardown

> Slice scope: the **macOS Parakeet path** — Rust manager (`src-tauri/src/parakeet/*`) ↔ Swift/FluidAudio
> sidecar (`sidecar/parakeet-swift/Sources/main.swift`). READ-ONLY analysis; no code changed.
> Branch `main`, HEAD `af63ab1`. Streaming *architecture* is settled in `06-oracle-decision.md` and is
> NOT re-derived here — this doc only quantifies its **perf** properties as levers.

## TL;DR

Parakeet's ANE decode is **not the bottleneck**: FluidAudio TDT batch runs ~120× RTF on Apple Silicon
(a 5 s clip decodes in tens of ms; `plans/028-transcription-latency-streaming.md` Evidence B). The wins
sit in three layers around the decode: **(1)** a path-based IPC round trip that makes Swift re-read &
re-decode a WAV Rust already wrote (small for batch, fatal for streaming); **(2)** a **cold ANE-program-
compile + first-decode** cost that VT never warms — the user's first real dictation after (re)spawn pays
it; **(3)** a whole-request `RwLock` write guard that serializes every sidecar command and holds no engine-
lease separation. The single highest-ROI fix is a **warm decode on preload** (amortize ANE compile off
the user's hotkey→text path); the single largest *streaming* prerequisite is replacing path-IPC with a
binary PCM side-channel.

---

## 1. Current-state map

### 1.1 The wire protocol (what crosses the boundary today)

The IPC is **newline-delimited JSON over the sidecar's stdin/stdout** (`sidecar.rs:159-165` writes;
`main.swift:222-308` reads). The only audio-bearing command is `Transcribe`, and it carries a **file
path**, never bytes:

```rust
// messages.rs:42-62  — audio payload is a STRING PATH
Transcribe {
    audio_path: String,
    language: Option<String>,
    translate_to_english: bool,
    ...
    chunk_duration: Option<f32>, overlap_duration: Option<f32>,
    attention: Option<String>, local_attention_context: Option<i32>,
    custom_vocabulary: Option<Vec<ParakeetVocabularyTerm>>,
},
```

Rust builds it from a `PathBuf` (`manager.rs:416-428`) and the Swift handler re-opens that path
(`main.swift:267-273` → `transcribeFile`):

```swift
// main.swift:463,466,490-492
let fileURL = URL(fileURLWithPath: audioPath)
guard FileManager.default.fileExists(atPath: audioPath) else { ... }
let result = try await withLibraryStdoutRedirected {
    try await manager.transcribe(fileURL, decoderState: &decoderState)
}
```

`AsrManager.transcribe(_ url:)` then re-reads + decodes the file internally
(`FluidAudio/.../TDT/AsrManager.swift:381-410`, "checks file size to decide streaming vs memory loading")
and `AudioConverter.resampleAudioFile` re-parses the WAV + resamples to 16 kHz Float32
(`FluidAudio/.../Shared/AudioConverter.swift`, cited in plan 028 Evidence C).

**Confirmed: no base64, no `audio_bytes`, no PCM-in-JSON anywhere** — `grep` for
`base64|audio_bytes|audio_data|pcm|raw_audio` across `src-tauri/src/parakeet/` returns zero matches. The
path is the entire handoff.

### 1.2 Model load / warmup — when it happens, what it does

VT triggers FluidAudio `LoadModel` from three sites, all of which only send the command — **none run a
warm decode**:

| Site | Trigger | Warms ANE compile? |
|---|---|---|
| `lib.rs:1886-1888` | **startup autoload** of the persisted parakeet model | Loads weights only |
| `settings.rs:765-770` | **model switch** (user picks a new model) | Loads weights only |
| `executor.rs:212-213` | **lazy, before every transcribe** (`load_model_with_cancel`) — idempotent short-circuit if already loaded (`main.swift:334-340`) | Loads weights only |

Critically, the **startup preload block at `lib.rs:1111-1165` is Whisper-only** — it queries
`whisper_state.get_model_path` and warms either the Vulkan sidecar
(`warm_whisper_gpu_sidecar_on_model_preload`, `audio.rs:1344`) or the Whisper `TranscriberCache`. There is
**no parakeet warm in that block**; the only startup parakeet load is the later `autoload_parakeet_model`
branch (`lib.rs:1886`), which is fire-and-forget `load_model` (no decode).

What FluidAudio *does* warm at load: `AsrManager` schedules a background `sharedMLArrayCache.prewarm`
(`AsrManager.swift:76-80`) that preallocates input `MLMultiArray` shapes — this is **memory prealloc, not
ANE-program compilation**. ANE compiles the model graph to its instruction set on the **first
`prediction()`** call. VT never issues a dummy prediction, so the user's first real dictation pays the
compile. (`MLModel.compileModel` for `.mlpackage`→`.mlmodelc` is a *separate*, already-amortized cost —
Parakeet ships compiled `.mlmodelc` via `AsrModels.loadFromCache`, `main.swift:369-371`.)

### 1.3 Sidecar lifecycle

Long-lived, lazily spawned, never idle-unloaded:

```rust
// sidecar.rs:276-299
pub struct ParakeetClient {
    binary_name: String,
    inner: RwLock<Option<ParakeetSidecar>>,
}
async fn ensure(&self, app) -> Result<RwLockWriteGuard<'_, Option<ParakeetSidecar>>, _> {
    let mut guard = self.inner.write().await;      // <-- WRITE lock
    if guard.is_none() { guard.replace(ParakeetSidecar::spawn(...).await?); }
    Ok(guard)                                       // <-- guard held by caller
}
```

`spawn` (`sidecar.rs:121-140`) calls `app.shell().sidecar(binary_name).spawn()` — forks the Swift
binary. The write guard returned by `ensure()` is then held **across the entire request** in `send`
(`sidecar.rs:312-316`) and `send_with_progress_and_cancel` (`sidecar.rs:358-375`), because `request`
(`sidecar.rs:142-265`) loops over `self.rx.recv()` line-by-line until the final response arrives.

Consequences:
- **Every sidecar command is serialized** through one write lock held for the request's full wall-time.
  A `Status`/`health_check` (`manager.rs:448-456`) blocks behind an in-flight `Transcribe`; two
  transcriptions cannot overlap. For single-user dictation this is usually fine, but it forbids the
  engine-lease/streaming topology 06-oracle marks P1 (handy `01-streaming-engine.md`: "lease the engine
  out of the mutex during decode").
- **No idle-unload watcher** (contrast handy's whisper idle-unload, `00-README.md:77`). Once spawned the
  ~500 MB CoreML resident set stays until `shutdown`/`kill`/timeout/cancel. Good for amortizing load; a
  memory cost otherwise.
- **Kill-on-recoverable-failure only** (`sidecar.rs:318-337, 389-397`): killed on `Timeout`, `Terminated`,
  or user-cancel, then `ensure` respawns fresh on the next command — paying spawn cost again.

### 1.4 The dead chunk config

Rust sends streaming-shaped knobs that Swift never reads:

```rust
// manager.rs:307-318  (load_model_with_cancel) and manager.rs:189-190 (download)
LoadModel { ..., precision: "bf16", attention: "local", local_attention_context: 256,
            chunk_duration: Some(120.0), overlap_duration: Some(15.0), eager_unload: Some(false) }
```

Swift's `LoadModel` handler (`main.swift:239-250`) reads only `model_version` and `force_download`; its
`loadModel(version:forceDownload:emitStatus:encoder:)` signature (`main.swift:311`) takes neither chunk
nor attention params. The `Transcribe` variant carries the same dead fields (`messages.rs:53-59`) but
`transcribe_with_custom_vocabulary` sets them all to `None` (`manager.rs:422-425`) and Swift ignores them
regardless (`main.swift:267-276`).

Net: ~80 bytes of JSON keys serialized on every load/transcribe and discarded. The values (120 s chunk /
15 s overlap / `local` attention / 256 local-context) loosely shadow FluidAudio's `SlidingWindowAsrManager`
defaults (15 s chunk / 10 s left-context, plan 028 Evidence C) — i.e. a **half-built streaming config that
was abandoned before Swift wiring**. It is both dead weight today and the natural seat for a future
session/streaming protocol.

### 1.5 The stdout→stderr redirect tax (per call)

Because FluidAudio/CoreML can emit diagnostics to `STDOUT_FILENO`, every native call is wrapped in a
descriptor swap (`main.swift:55-75`):

```swift
func withLibraryStdoutRedirected<T>(_ operation) async throws -> T {
    fflush(stdout); let savedStdout = dup(STDOUT_FILENO)   // dup
    dup2(STDERR_FILENO, STDOUT_FILENO)                     // dup2
    defer { fflush(stdout); dup2(savedStdout, STDOUT_FILENO); close(savedStdout) }  // dup2 + close
    return try await operation()
}
```

This wraps **both** `loadModels` (`main.swift:387-389`) **and** every `manager.transcribe(fileURL)`
(`main.swift:490-492`). Per transcribe that is ≈ `fflush`×2 + `dup`×1 + `dup2`×2 + `close`×1 = **6
syscalls of pure IPC hygiene**. Small individually, but it is on the critical path of *every* decode and
exists only because the protocol shares one FD for JSON and native noise.

---

## 2. Opportunities

Sorted by impact-per-effort. Impact = wall-time on the **user's first-real-dictation hotkey→text path**
unless noted.

| # | Opportunity | Mechanism (concrete change) | Est. impact | Effort | Risk | Invariant touched |
|---|---|---|---|---|---|---|
| **1** | **Warm decode on preload** (kill cold ANE compile) | Add a `Warmup` command to `messages.rs`; after `LoadModel` success, sidecar runs a 1 s silence `manager.transcribe(buffer)` to force ANE program compilation. Wire it into the startup autoload (`lib.rs:1886`) + model-switch (`settings.rs:765`) paths. | **~0.3–1.5 s** off first-dictation cold start (ANE first-prediction compile, **NEEDS MEASUREMENT** on M-series) | S | Low | none (idle-time work) |
| **2** | **Binary PCM side-channel** (replace path-IPC) | Add length-prefixed binary audio frames on stdin: `{"type":"audio_chunk","session":..,"bytes":N}\n` + N raw f32 bytes. Swift reads header line then `FileHandle.standardInput.read(ofLength:N)`. Reuse for both a zero-copy batch handoff and streaming `appendAudio(AVAudioPCMBuffer)`. | Batch: **~1–5 ms** (free, negligible vs plumbing tail). Streaming: **eliminates per-chunk file churn** — *prerequisite* for sub-second partials | M | Med | plan 008 (no hot-path alloc — applies to Rust capture tap, not IPC) |
| **3** | **Engine lease / drop the whole-request write lock** | Split `inner: RwLock<Option<ParakeetSidecar>>` into a spawn-guard + a per-request command channel; release the write lock once the command is written and the read loop owns its own receiver. Mirror handy's lease pattern (`01-streaming-engine.md`). | Enables `Status`/health/`Cancel` to not block on an in-flight decode; unblocks streaming batch+partial overlap | M | Med | 06-oracle P1 "engine lease / no decode mutex" |
| **4** | **Remove the dead chunk config** (or wire it) | Either drop `chunk_duration/overlap_duration/attention/local_attention_context/eager_unload` from `LoadModel`+`Transcribe` (`messages.rs:28-39,53-59`; `manager.rs:189-190,307-318`) or repurpose them as the real SlidingWindow/streaming config Swift actually consumes. | ~80 B/request hygiene + clarity; unblocks #2 streaming wiring | S | Low | none |
| **5** | **Eliminate the per-call stdout redirect** | Give the protocol a dedicated binary/JSON FD (a second pipe) so FluidAudio diagnostics go to stderr-only permanently; drop `withLibraryStdoutRedirected` from the transcribe path. | ~6 syscalls/transcribe (**sub-ms**, but removes a fragile dup2-on-hot-path) | M | Med | correctness (must not lose native diagnostics) |
| **6** | **Keep-alive + idle-unload** (memory vs latency trade) | Add a configurable idle timer that unloads the CoreML model (FluidAudio `asrManager?.cleanup()`, `main.swift:410`) but keeps the process alive; or a lighter "unload model, keep process" so re-load is faster than re-spawn. | Saves ~hundreds of MB resident when idle; re-load ≪ re-spawn | S | Low | none |
| **7** | **Spawn-on-startup instead of lazy-on-first-command** | Eagerly `ensure()` the sidecar during setup so first-dictation eats no fork+Swift-init. | **~0.1–0.3 s** off first command (Swift binary init, **NEEDS MEASUREMENT**) | S | Low | startup memory; plan 015 never-block-startup |

> **Decode is explicitly NOT in this table.** FluidAudio batch TDT is ~120× RTF (plan 028 Evidence B); a
> 5 s clip decodes in ~40 ms. Spending effort on the decoder itself is the wrong layer — confirmed by the
> parallel-chunk + ANE architecture in `FluidAudio/.../TDT/{ChunkProcessor,DualDecodeArbitration}.swift`.

---

## 3. Deep-dive — top opportunities

### 3.1 #1 Warm decode on preload (the single biggest first-dictation win)

**The gap.** `LoadModel` makes the model *resident* but the ANE has not yet compiled its program graph.
VT's load sites (`lib.rs:1886`, `settings.rs:765`, `executor.rs:212`) never issue a prediction, and
FluidAudio's own `sharedMLArrayCache.prewarm` (`AsrManager.swift:76-80`) only preallocates
`MLMultiArray` shapes — not an ANE program. So the **first `manager.transcribe(...)` after load pays:
CoreML `prediction()` → ANECompilerService program build**, which is the classic "first inference is slow"
phenomenon on Apple Silicon.

**The change.** Add to `messages.rs`:
```rust
Warmup { duration_secs: Option<f32> }   // default ~1.0s silence
```
Swift handler synthesizes a 1 s, 16 kHz mono silence `AVAudioPCMBuffer` and calls
`manager.transcribe(buffer, decoderState:)` (`AsrManager.swift:482-510` takes `[Float]` directly — no file
needed), discarding the result. Wire it:
- after successful autoload (`lib.rs:1888-1891`): `load_model(...).await?; manager.warmup(...).await;`
- after successful model switch (`settings.rs:766-768`).
The lazy `executor.rs:212-213` path need not warm (it's already covered by the above; the idempotent
short-circuit at `main.swift:334-340` keeps repeat loads cheap).

**Why it's safe.** Warmup is pure idle-time work on a silence buffer; it touches no user audio, no
clipboard, no focus. It cannot regress never-lose-speech (plan 015) or the audio-callback hot path
(plan 008). Worst case it adds ~1 s of background CPU once per (re)spawn — invisible to the user because
it runs before the first hotkey press.

**What it touches.** `messages.rs` (new variant + `operation_name`/`request_timeout_secs`), `main.swift`
(synthesize-silence + `transcribe`), `lib.rs` + `settings.rs` (await warmup after load).

**Verify.** Instrument first-real-decode wall-time with/without warmup across cold-spawn and warm
sidecar; expect the warmup *moves* the compile cost out of the hot path (it doesn't disappear — total
work is constant, but it leaves the user-visible latency budget). **NEEDS MEASUREMENT**: exact ANE
compile delta on M4 Pro (this workstation) vs reported older silicon.

### 3.2 #2 Binary PCM side-channel (the streaming prerequisite; a free batch tidy)

**Why path-IPC is wrong for streaming.** Today the round trip is: Rust writes the WAV to disk
(`recorder.rs` writer) → serializes `{"type":"transcribe","audio_path":"…"}` (`manager.rs:417`) → Swift
re-opens the path (`main.swift:463-466`) → FluidAudio re-reads + re-decodes + resamples
(`AsrManager.swift:381-410`, `AudioConverter.resampleAudioFile`). For a single batch short clip this is a
few ms of redundant work (the file is hot in the APFS page cache because Rust just wrote it, so it's a
memcpy, not disk I/O). For **streaming** it is fatal: FluidAudio's `StreamingAsrManager.appendAudio(_
AVAudioPCMBuffer)` (`Streaming/Streaming/StreamingAsrManager.swift:7-54`, cited in plan 028 Evidence C)
takes PCM buffers directly — a path-per-chunk protocol would mean N temp files, N JSON lines, N re-reads
for N chunks (e.g. ~31 chunks for 10 s @ 320 ms), plus unlink churn.

**The change (recommended: Option B — length-prefixed binary frames on the existing stdin).**
```
Rust → stdin:  {"type":"start_stream","session":7,"sample_rate":16000}\n
               <4-byte little-endian N><N raw f32 samples>
               {"type":"audio_chunk","session":7,"bytes":M}\n
               <M raw f32 bytes>
               ... 
               {"type":"end_stream","session":7}\n
Swift:  readLine() → on audio_chunk, FileHandle.standardInput.read(ofLength: M) → AVAudioPCMBuffer
```
- **Explicitly avoid base64-in-JSON**: +33% size, JSON parse cost, and the assignment + 06-oracle
  (`messages.rs` extension, plan 028 Phase 2: "prefer a binary side-channel over base64-in-JSON") both
  forbid it. FluidAudio and the whisper Vulkan sidecar both avoid it.
- Option C (mach shared-memory / mmap) is lower-copy but far more complex (handshake, cleanup,
  notarization surface) — overkill; Option D (a 2nd dedicated pipe) is equivalent to B with more FD
  plumbing. **B is the sweet spot**: single FD, one-line protocol extension, zero base64.

**Batch handoff estimate.** A 5 s 16 kHz mono s16 WAV is ~160 KB; re-read + WAV-decode + resample today
≈ **1–5 ms** (page-cache hot). Passing f32 PCM directly removes the WAV encode (Rust) + WAV decode +
resample (Swift) — call it **~1–5 ms saved per batch transcribe**, negligible vs the ~950 ms plumbing tail
(plan 028 Evidence A) but free. The real value is that **the same channel serves streaming with zero
extra IPC design**.

**What it touches.** `messages.rs` (session/chunk variants), `sidecar.rs` (write raw bytes after the
header line; the read loop already handles `CommandEvent::Stdout` bytes), `main.swift` (read exactly N
bytes after a chunk header), and a Swift-side streaming manager owning `appendAudio`. This is exactly the
`messages.rs` extension 06-oracle Step 5 describes (`06-oracle-decision.md:259-264`).

**Why it's safe.** Batch behavior is byte-identical if gated behind a feature flag / session id (06-oracle
Step 3: "no behavior change for batch path until a feature flag/session is active"). Never-lose-speech is
preserved because the FIFO command order (Feed/Finalize on one channel) is the handy invariant
(`01-streaming-engine.md`).

**Verify.** (a) Batch: byte-identical transcripts on a fixed corpus with the new handoff. (b) Streaming:
Feed/Finalize FIFO test — last audio chunk precedes finalize; cancel mid-stream ignored; stale session
dropped (06-oracle trap #3/#13).

### 3.3 #3 Engine lease — stop holding the write lock across the decode

**The problem.** `ensure()` returns a `RwLockWriteGuard` (`sidecar.rs:289-299`) that `send`
(`sidecar.rs:312-316`) and `send_with_progress_and_cancel` (`sidecar.rs:358-375`) hold for the full
request — including the multi-second decode. This is the anti-pattern 06-oracle flags P1 ("decode must
not hold a cache/model mutex across repeated decode calls"; handy leases the engine out of the manager
mutex, `00-README.md:35`, `01-streaming-engine.md`).

**Concretely.** Today a `health_check` (`manager.rs:448-456`) or a UI `status` poll queued during a
transcribe blocks until the decode returns — and on `Timeout`/cancel the guard path kills+respawns
(`sidecar.rs:318-337`), so a slow decode can mask a healthy sidecar. For streaming it's worse: a long-lived
session would hold the lock for the entire utterance.

**The change.** Separate "spawn/own the process" (a short lock) from "issue a command" (a per-request
_owned_ receiver + writer, no held guard). Write the command line, drop the spawn-guard, let the read
loop own its `rx`. This matches handy's single-mpsc-per-session worker (`01-streaming-engine.md`) and
unblocks overlapping batch + status and streaming partials.

**Effort/risk.** Medium / medium — touches the lifecycle hot path and the cancel/respawn logic
(`dispatch_cancellable`, `sidecar.rs:88-113`), which is carefully unit-tested around the
deadline-on-cancel invariant (`sidecar.rs:77-87`). Any rewrite must preserve "every attempt, including
cancel, is deadline-bounded."

---

## 4. Learn-from-others

- **FluidAudio itself already prewarms** (`AsrManager.swift:76-80`, `sharedMLArrayCache.prewarm`) and the
  Nemotron multilingual shared-models path explicitly comments *"Skip warmup — the shared models are
  already compiled & resident from preloadShared(). The first real chunk pays no cold-start penalty"*
  (`StreamingNemotronMultilingualAsrManager+Shared.swift:311-314`). **VT does not call the equivalent for
  TDT batch** — it stops at `loadModels`. This is direct evidence the library *expects* a warm step and
  VT omits it. → Opportunity #1.
- **FluidAudio streaming takes PCM buffers, not paths** (`StreamingAsrManager.appendAudio(_
  AVAudioPCMBuffer)`, plan 028 Evidence C). Any path-based streaming protocol would fight the library.
  → Opportunity #2.
- **Handy leases the engine out of the mutex during decode** so ~33 decodes/s run lock-free
  (`00-README.md:35`, `01-streaming-engine.md`). VT's whole-request `RwLock` is the inverse. →
  Opportunity #3. (Cite `07-competitive-sota.md` for cross-tool RTF numbers once it lands; today the
  ~120× RTF figure is from plan 028 Evidence B, itself source-verified against FluidAudio 0.15.2.)
- **whisper-rs Vulkan sidecar** (`whisper/gpu_sidecar.rs`) is VT's *other* newline-JSON sidecar and uses
  the same path-based `Transcribe{model_path,audio_path}` pattern (plan 028 Evidence B) — so a binary
  side-channel designed for Parakeet is reusable for the Windows Vulkan path too.
- **Apple guidance** (general knowledge, `[INFERENCE]`): CoreML ANE first-prediction latency is dominated
  by program compilation; the standard mitigation is a throwaway warmup prediction at load. VT not doing
  this is the gap, not a library limitation.

---

## 5. Measurement plan

Instrument with `tracing`/span logs (per 06-oracle Step 1) around these boundaries; report p50/p95 on a
fixed 2 s / 5 s / 15 s corpus.

| Metric | Where to span | Before / after |
|---|---|---|
| **Cold first-decode** (post-spawn, post-load) | spawn→first `Transcription` response, sidecar cold | establishes the ANE-compile floor (#1) |
| **Warm decode RTF** | `manager.transcribe` start→end inside Swift (`main.swift:486-494` already logs elapsed) | confirms decode is not the bottleneck (~120×) |
| **Path-IPC handoff** | Rust WAV-write-done → Swift `manager.transcribe` start; vs binary handoff | quantifies #2 batch delta (expect low ms) |
| **Per-chunk overhead** (streaming prototype) | N chunks × {temp-file write + JSON + read + decode + unlink} vs binary frames | quantifies #2 streaming delta (expect large) |
| **Lock-held duration** | write-guard acquire (`ensure`) → release, per command | shows #3 serialization headroom |
| **Spawn cost** | `sidecar().spawn()` → first `readLine()` ready | quantifies #7 |
| **Resident memory idle** | `asrManager` resident set over time with no dictation | informs #6 |

Sidecar stderr already emits high-signal timing (`main.swift:486-497` logs decode elapsed + audio
duration + RTF); add matching Rust-side spans at `manager.rs:416-432` (command send) and around
`ensure()`/`request` in `sidecar.rs`.

---

## 6. Prioritized recommendations

- **P1 — #1 Warm decode on preload.** Biggest first-dictation win, smallest risk, pure idle-time work.
  Ship alongside the existing startup autoload (`lib.rs:1886`) and model-switch (`settings.rs:765`). Do
  this **before** streaming — it makes the streaming first-partial gate (06-oracle: p50 ≤ 700 ms) far
  easier to hit.
- **P1 — #2 Binary PCM side-channel** as the *single* IPC redesign that serves both a tidier batch
  handoff and the streaming substrate. This is the `messages.rs` extension 06-oracle Step 5 mandates;
  building it path-aware (keep a decode fallback per the user's ffmpeg caveat — though ffmpeg is *not*
  in this IPC path; the WAV is already written by Rust) is straightforward. **No base64.**
- **P2 — #3 Engine lease.** Drop the whole-request write lock so status/cancel/streaming can overlap
  decode. Higher-touch (cancel/respawn invariants) but unlocks the 06-oracle P1 streaming topology.
- **P2 — #4 + #5 hygiene.** Remove (or finally wire) the dead chunk config; eliminate the per-call
  stdout redirect once a dedicated diagnostic FD exists. Low individual value, high clarity value, and
  #5 removes a fragile `dup2` on the decode hot path.
- **P3 — #6 / #7 lifecycle tuning.** Eager spawn-on-startup and/or model-unload-keep-process idle
  management, gated on measured spawn cost and resident-memory pressure.

---

## 7. Open questions / risks

1. **ANE-compile magnitude unmeasured on this hardware (M4 Pro).** The ~0.3–1.5 s estimate for #1 is
   `[INFERENCE]` from general CoreML behavior; the warm decode *moves* the cost, it does not eliminate it.
   Must measure before claiming a user-visible number. **NEEDS MEASUREMENT.**
2. **Warmup vs battery/thermals.** A 1 s ANE warmup on every (re)spawn is cheap, but on every
   model-switch during rapid A/B it could stack — consider warming once per resident model version, not
   per load command.
3. **Binary side-channel + the stdout-redirect (#5) interact.** A second pipe cleanly separates JSON
   from native diagnostics, which *also* solves #5; design them together rather than twice.
4. **Cancel/respawn invariant.** Opportunity #3 must preserve `dispatch_cancellable`'s deadline-on-cancel
   guarantee (`sidecar.rs:77-113`) and the kill-on-Timeout/cancel lifecycle (`sidecar.rs:389-397`); the
   unit tests in `sidecar.rs:409+` are the regression net.
5. **Dead config as a hook, not just debt.** Before deleting (#4), confirm with the streaming design
   (06-oracle Step 5) whether `chunk_duration`/`local_attention_context` should become *real*
   SlidingWindow/EOU knobs consumed by Swift — deletion now could force a re-add later.

---

### Provenance

Read-only against VoiceTypr `main` @ `af63ab1`. All citations are `path:line` relative to repo root.
FluidAudio citations are from the pinned checkout
`sidecar/parakeet-swift/.build/checkouts/FluidAudio/Sources/FluidAudio/`. The ~120× RTF figure is quoted
from `plans/028-transcription-latency-streaming.md` Evidence B (itself source-verified against FluidAudio
0.15.2 rev `7f963cd`); not independently re-benchmarked here. ANE-compile and Swift-binary-init costs
marked `[INFERENCE]` / **NEEDS MEASUREMENT** are not directly observed in this pass. No code was changed.
