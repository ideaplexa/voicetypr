# Whisper Engine Decode — VoiceTypr perf teardown

> **Scope:** the decode-bound engine (`whisper-rs` 0.16). READ-ONLY analysis; no code
> changed. Every claim cites `path:line` relative to the VT repo root
> (`/Volumes/1tb-drive/developer/oss/voicetypr`, HEAD `af63ab1`). whisper-rs 0.16.0 API
> facts are source-verified against the vendored copy in
> `~/.cargo/registry/src/.../whisper-rs-0.16.0/src/`. Anything not directly observed is
> marked `[INFERENCE]` or `NEEDS MEASUREMENT`.

## TL;DR

The single biggest Whisper latency win is **`set_audio_ctx` — which is never set**, so every
2–8 s dictation clip is encoded over the **full 30 s / 1500-token window** (padded with
zeros by whisper.cpp). A length-adaptive `audio_ctx` is a **~3–8× encoder speedup on short
clips** with a well-understood WER cliff (truncate → hallucinate), and it is one line plus a
duration lookup. Behind it: the macOS Metal path wastes decoder time on **`beam_size=5`** that
buys ~0 WER on clean short dictation; the **model default is accidentally `large-v3-turbo`**
(6× faster) only via a fragile secondary sort key; **flash-attn is off** (a context-creation
flag VT never touches); and the Windows Vulkan **sidecar holds a process lock for the whole
request** but already has a warm-start. **Critically — the cache mutex is NOT held across
decode**, so the 06-oracle engine-lease pattern is unblocked at the locking layer; the real
constraint is the **single cached `WhisperContext`**.

---

## Current-state map

### Decode dispatch (the call that runs `state.full`)

`commands/audio.rs:1433-1453` — the CPU/Metal path. (Windows GPU is a separate sidecar path,
see §6.)

```rust
// audio.rs:1433
let transcriber = {
    let cache_state = app.state::<AsyncMutex<TranscriberCache>>();
    let mut cache = cache_state.lock().await;          // cache mutex ACQUIRED
    cache.get_or_create(model_path)?                   // returns Arc<Transcriber>
};                                                      // mutex DROPPED here (block ends)

let audio_path = audio_path.to_path_buf();
...
let result = tokio::task::spawn_blocking(move || {     // decode on blocking pool
    transcriber.transcribe_with_metadata_with_prompt(  // ...holding only the Arc clone
        &audio_path, language.as_deref(), translate,
        initial_prompt.as_deref(), should_cancel_for_decode,
    )
}).await...;
```

The `Arc<Transcriber>` is cloned out of the cache under the lock; the lock guard is dropped at
the closing brace; `state.full(...)` runs on a `spawn_blocking` thread holding **no cache
lock**. (Confirmed in §4 below.)

### Decode params (`whisper/transcriber.rs:480-581`)

```rust
// transcriber.rs:480 — sampling strategy is the ONLY branch on cpu_profile
let mut params = if self.cpu_profile {
    FullParams::new(SamplingStrategy::Greedy { best_of: 1 })   // CPU: greedy
} else {
    FullParams::new(SamplingStrategy::BeamSearch {              // Apple-Silicon Metal: beam
        beam_size: 5,
        patience: -1.0,   // NOTE: patience is a no-op in whisper.cpp v1.7.6 (plan 028)
    })
};
...
// transcriber.rs:543-548  thread count
let threads = std::cmp::max(1, hw.saturating_sub(1)) as i32;  // all cores - 1 (non-aarch64-win)
params.set_n_threads(threads);

// transcriber.rs:550-581  misc flags
params.set_no_context(self.cpu_profile);
params.set_no_timestamps(self.cpu_profile);
...
params.set_max_len(0);          // 0 = no limit
params.set_length_penalty(-1.0);
```

**`set_audio_ctx` is never called anywhere in the codebase** (grep for
`set_audio_ctx|audio_ctx|flash_attn|n_max_text_ctx` across `src-tauri/src` → no matches). So
`params.fp.audio_ctx == 0`, which whisper.cpp reads as "use the model's native
`n_audio_ctx` = **1500**" (30 s / 20 ms-per-token).

### State lifecycle — fresh per call (`whisper/transcriber.rs:584-623`)

```rust
// transcriber.rs:584
let mut state = self.context.create_state().map_err(...)?;   // NEW WhisperState every call
...
// transcriber.rs:621
params.set_abort_callback_safe(should_cancel_for_abort);     // plan-015 cancel hook
// transcriber.rs:623
match state.full(params, &resampled_audio) { Ok(_) => {...} }
```

### Context creation + GPU/backend selection (`whisper/transcriber.rs:57-103`)

```rust
// transcriber.rs:57 — context params are defaults; flash_attn never toggled
let mut ctx_params = WhisperContextParameters::default();
#[cfg(target_os = "macos")]
{
    let is_apple_silicon = std::env::consts::ARCH == "aarch64";
    if !is_apple_silicon { ctx_params.use_gpu(false); }   // Intel Mac → CPU
    else                 { ctx_params.use_gpu(true); }    // Apple Silicon → Metal
    ...
}
#[cfg(target_os = "windows")]
{ ctx_params.use_gpu(false); }   // Windows main binary = CPU; GPU via sidecar (§6)
```

`cpu_profile` is set `true` for Intel Mac + Windows + any CPU fallback
(`transcriber.rs:103,233`). It gates greedy-vs-beam **and** `no_context`/`no_timestamps`, so
the Metal (Apple Silicon) path keeps cross-segment context + timestamps ON.

### Model catalog + selection

`whisper/manager.rs:99-167` — four models:

| model | size | speed | accuracy | recommended |
|---|---|---|---|---|
| `base.en` | 148 MB (`:104`) | 8 (`:110`) | 5 (`:111`) | false (`:112`) |
| `large-v3` | 3.09 GB (`:121`) | 2 (`:127`) | 9 (`:128`) | **true** (`:129`) |
| `large-v3-turbo` | 1.62 GB (`:140`) | 7 — *"6x faster than large-v3"* (`:146`) | 9 — *"Comparable to large-v2"* (`:147`) | **true** (`:148`) |
| `small.en` | 488 MB (`:157`) | 7 (`:163`) | 6 (`:164`) | false (`:165`) |

Default model setting: `Settings::default().current_model == ""` ("Empty means auto-select",
`commands/settings.rs:82`). Auto-selection (`recognition/model_selection.rs:173-199`) sorts
*downloaded* models; on the non-CPU path (Apple Silicon / Windows-GPU) the sort is
`recommended DESC → accuracy DESC → size ASC` (`:192-197`). **Both large models are
`recommended=true` with `accuracy=9`**, so turbo wins **only on the size tiebreaker**
(1.62 GB < 3.09 GB). On the CPU path it wins on the speed tiebreaker (`:184-190`).

The cloud STT path already hard-codes turbo: `cloud_stt/groq.rs:7` →
`pub(super) const MODEL: &str = "whisper-large-v3-turbo";`.

### Cache — single model, mutex only at load (`whisper/cache.rs:8-48`)

```rust
// cache.rs:9-10
const MAX_CACHE_SIZE: usize = 1;   // exactly one model resident (1–3 GB each)
...
pub struct TranscriberCache {
    map: HashMap<String, Arc<Transcriber>>,
    lru_order: VecDeque<String>,
    max_size: usize,
}
```

`get_or_create` (`cache.rs:48-115`) clones the `Arc<Transcriber>` out under `&mut self`; the
`AsyncMutex` wrapping the whole cache lives at the Tauri state layer
(`AsyncMutex<TranscriberCache>`, `commands/audio.rs:1434`, `lib.rs:605`, `cli.rs:296`).

### Windows Vulkan sidecar (`whisper/gpu_sidecar.rs`)

A long-lived child process speaking newline-delimited JSON over stdin/stdout
(`:219-244`). The `send()` method acquires `self.process.lock().await` and holds it across the
entire request:

```rust
// gpu_sidecar.rs:504-530
async fn send(&self, app: &AppHandle, request: &SidecarRequest<'_>) -> ... {
    let mut guard = self.process.lock().await;            // process mutex ACQUIRED
    if guard.is_none() { guard.replace(GpuSidecarProcess::spawn(app).await?); }
    let result = match guard.as_mut() {
        Some(process) => {
            ...
            tokio::select! { biased;
                _ = &mut abort_notified => Err(...),
                res = process.request(request) => res,     // FULL REQUEST under the lock
            }
        }
        None => Err("Vulkan sidecar was not started".to_string()),
    };
    ... // guard dropped at fn end
}
```

Warm-start exists: `warm_on_preload` (`:336-366`) → `probe` (`:368-416`) spawns the process and
loads the model once; the process persists across requests (only `guard.take()`n on
error/abort/id-mismatch, `:538,547,559`).

---

## Opportunities table

Sorted by estimated impact-per-effort.

| # | Opportunity | Mechanism | Est. impact | Effort | Risk | Invariant touched |
|---|---|---|---|---|---|---|
| 1 | **Length-adaptive `set_audio_ctx`** | `params.set_audio_ctx(min(1500, max(FLOOR, ceil(dur_s×50)+pad)))` before `state.full` (`transcriber.rs:577`) | **~3–8× encoder time on <10 s clips; typical −150 to −500 ms** on dictation-length audio. `NEEDS MEASUREMENT` on M4 Pro | S | Med | WER cliff if `audio_ctx < actual clip tokens` → must floor above real length |
| 2 | **Greedy on Metal for short dictation** | Use `Greedy{best_of:1}` (or `beam_size=1`) on Apple Silicon when `dur < ~10 s`, not just on CPU (`transcriber.rs:480-488`) | **~1.3–2× decoder pass**; ~−40 to −150 ms on Metal. WER Δ ≈ 0 for clean short English | S | Low | Quality tradeoff named: beam helps noisy/long/multilingual |
| 3 | **Make `large-v3-turbo` the explicit dictation default** | Bump turbo `speed_score`/a `dictation_default` flag; onboarding nudges turbo first; add a "switched to large-v3? offer turbo" hint | **~6× decode vs large-v3** on identical hardware (manager comment `:146`); the biggest single knob if a user is on large-v3 | S | Low | Accuracy ≈ large-v2 (`:147`); loses tiny edge vs large-v3 |
| 4 | **Enable flash-attn at context creation** | `ctx_params.flash_attn(true)` in `Transcriber::new` for Apple Silicon (`transcriber.rs:78`) — it's a `WhisperContextParameters` field, not a per-call param (`whisper-rs` `whisper_ctx.rs:465,491`) | **~1.3–2× encoder** on long context `[INFERENCE] from whisper.cpp Metal benchmarks`; no effect on very short clips once #1 lands | S | Med | "Can't be used with DTW" (`whisper_ctx.rs:464`) — VT doesn't use DTW, safe |
| 5 | **Reuse `WhisperState` / avoid per-call alloc** | Pool states or `create_state` once and reset; `state.full` allocates KV-cache + scratch each call today (`transcriber.rs:585`) | **~5–30 ms** + less allocator churn/GC pressure on hot dictation | M | Med | whisper-rs `WhisperState` is `&mut self` on `full`; need 1 state per concurrent decode |
| 6 | **Tune `n_threads` per SoC / power** | Today `cores-1` (`transcriber.rs:543`); cap to perf cores on Apple Silicon P/E, and test fewer threads for short clips (thread-launch overhead) | **±5–15%**; `NEEDS MEASUREMENT` | S | Low | Don't starve UI thread (already reserves 1) |
| 7 | **Expose `set_single_segment` for decode-ahead** | When streaming/chunked (06-oracle Phase 3), `set_single_segment(true)` per chunk limits to 1 segment, smaller KV | Enables the segmented decode-ahead loop (scribble-style) | S | Low | Only for streaming path; not the batch path |
| 8 | **Release profile: LTO + codegen-units** | `[profile.release]` has no `lto`/`codegen-units`/`opt-level` (`Cargo.toml:126-133`); add `lto="thin"`, `codegen-units=1` | **~5–15%** decode on CPU/Windows; near-zero on Metal (kernel-bound) | S | Low-Med | Larger build time; verify Bugsink symbolication still works (`debug="line-tables-only"` kept) |
| 9 | **Windows sidecar: pipelined warm + keep-alive** | Keep the sidecar alive + model resident across recordings (already warm-starts on preload `:336`); avoid re-spawn | **−200 to −800 ms** cold-sidecar-spawn per recording on Windows `[INFERENCE]` | M | Med | Long-lived GPU process RAM; must handle driver-reset |

---

## Deep-dive — top opportunities

### #1 — Length-adaptive `set_audio_ctx` (the biggest short-clip knob)

**Why it's the #1.** whisper.cpp **pads audio shorter than 30 s up to 30 s**
(`n_samples_30s = 480000`), then the encoder's attention window is `n_audio_ctx × n_audio_ctx`
where `n_audio_ctx` defaults to the model's native **1500** (large) = a full 1500×1500
attention over 30 s of (mostly zero) mel frames. For a 3 s dictation clip that is ~2700 wasted
mel frames. `params.fp.audio_ctx` overrides `n_audio_ctx` (`whisper-rs` `whisper_params.rs:252`,
"Defaults to 0"). VT never sets it (grep-confirmed), so **every clip pays the full 1500-token
encoder regardless of length**.

**The mapping (source-derived).** One `audio_ctx` token = 20 ms of audio (30 s = 1500 tokens).
So a clip of `D` seconds needs `ceil(D × 50)` tokens. Setting `audio_ctx` below the real clip
length **truncates audio** → garbage/hallucination (the WER cliff). Setting it to the exact
length can clip the final phoneme; add ~10–15 % margin + a floor.

**Proposed change** (one block before `transcriber.rs:623`):

```rust
// length-adaptive encoder context — never truncate, only shrink the wasted 30s window
const AUDIO_CTX_FLOOR: i32 = 64;        // ~1.3 s floor for robustness on tiny clips
const AUDIO_CTX_CAP:   i32 = 1500;      // model native (large)
const MARGIN_PCT: f32 = 1.15;           // 15% headroom over measured length

let needed = (duration_seconds * 50.0 * MARGIN_PCT).ceil() as i32;
let audio_ctx = needed.clamp(AUDIO_CTX_FLOOR, AUDIO_CTX_CAP);
params.set_audio_ctx(audio_ctx);
log::info!("[PERFORMANCE] set_audio_ctx={} (dur={:.2}s)", audio_ctx, duration_seconds);
```

`duration_seconds` is already computed at `transcriber.rs:593` (`samples_count / 16000`), so
this is a pure insertion with no new I/O.

**Estimated impact (NEEDS MEASUREMENT, reasoning shown).** Encoder attention scales
~quadratically with `n_audio_ctx`; FFN scales linearly. For a 3 s clip: default 1500 vs
adaptive ~172 → attention ≈ (1500/172)² ≈ **76×** fewer attention FLOPs, FFN ≈ 8.7× fewer.
Realistic net (encoder is part encoder-attn, part FFN, plus decoder unaffected): **~3–8×
faster total `state.full`** for clips under ~6 s, tapering to ~1× near 30 s. On M4 Pro
(`large-v3` Metal) a 3 s clip today is [INFERENCE] ~150–350 ms of decode; expect **−100 to
−280 ms**. On Windows CPU this is the difference between usable and laggy for short clips.

**WER caveat (named).** This is the single safety item: if `audio_ctx` ever falls below the
clip's true token count, whisper.cpp silently drops the tail → hallucinated repeats /
truncation. The `MARGIN_PCT` + `AUDIO_CTX_FLOOR` + the existing 0.5 s minimum gate
(`transcriber.rs:596`) bound it. **Verification plan:** WER test sweep over clip lengths
{0.5,1,2,3,5,8,12,20,29}s at `audio_ctx = ceil(D×50×{1.0,1.1,1.15,1.25})`; assert no
degradation vs the full-context baseline; assert the last 200 ms of a known trailing word is
present. Never let margin < measured length.

**Constraints:** none of the plan-008 hot-path invariants apply (this is off the CPAL
callback, on `spawn_blocking`). Never-lose-speech (015): the floor + margin protects it.

### #2 — Drop beam to greedy on Metal for short dictation

**Today:** Apple Silicon → `BeamSearch{beam_size:5}` (`transcriber.rs:484-487`); only the CPU
profile goes greedy (`:482`). Beam runs `beam_size` decoder hypotheses token-by-token, so the
decoder pass is ~`beam_size`× deeper. `patience:-1.0` is a **no-op** in whisper.cpp v1.7.6
(plan 028 Evidence C), so it costs nothing but also tunes nothing.

**Change:** when `duration_seconds < BEAM_CUTOFF` (suggest ~10 s) **and** not
`translate`/multilingual-long-form, use `Greedy{best_of:1}` even on Metal. Keep beam for long
dictation / translation where the WER gain is real.

```rust
let short_clip = duration_seconds < 10.0;   // dur known at :593
let mut params = if self.cpu_profile || short_clip {
    FullParams::new(SamplingStrategy::Greedy { best_of: 1 })
} else {
    FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 })
};
```

**Impact (NEEDS MEASUREMENT).** whisper.cpp community benchmarks put beam=5 at ~1.5–2× the
decoder time of greedy for the same audio (not a clean 5× due to shared/prefix computation).
For short clips the decoder is a smaller fraction of total (encoder dominates), so absolute
savings are ~40–150 ms on Metal; the win compounds with #1 (once the encoder shrinks, the
decoder's relative cost grows, so greedy matters more). **WER tradeoff (named):** beam=5 vs
greedy on clean short English dictation is typically **<0.5 % WER** `[INFERENCE]`; the gap
widens for noisy audio, code-switching, and long-form. Greedy also reduces hallucinated
repetition in some short-clip cases.

**Constraints:** the Metal path currently keeps `no_context=false`/`no_timestamps=false`
(`transcriber.rs:550-551`) for cross-segment context; switching sampling strategy does not
touch those, so context carryover is preserved.

### #3 — Make `large-v3-turbo` the explicit default (don't rely on a sort tiebreak)

**Today's fragility.** Both large models are `recommended=true, accuracy_score=9`
(`manager.rs:129,148`). `pick_best_whisper_model` (`model_selection.rs:192-197`) selects turbo
**only** because of the `size ASC` tiebreaker — a refactor that reorders the sort, or a user
who manually picks `large-v3` in onboarding/tray, lands them on the **2.5× RAM, 6× slower**
model with **no accuracy gain over turbo** (turbo ≈ large-v2, large-v3 ≈ +1 over v2 on hard
audio). The cloud path already standardised on turbo (`groq.rs:7`).

**Recommended policy (concrete):**
1. Add a `dictation_default: bool` (or just rank turbo's `speed_score` above large-v3
   unambiguously — it is `7` vs `2` today, but the *non-CPU* sort ignores speed). Simplest:
   in `pick_best_whisper_model` non-CPU branch, sort `speed_score` **before** `accuracy` so
   turbo is preferred whenever accuracy ties (`model_selection.rs:192-197`).
2. Onboarding: if the user picks `large-v3`, show a one-line hint ("Turbo is ~6× faster with
   near-identical accuracy for dictation — recommended").
3. A "you're on large-v3; switch to turbo?" nudge in settings (like the existing GPU/CPU
   nudges).

**Impact:** for any user currently on `large-v3`, switching to turbo is **~6× decode**
(manager comment `:146`) at ~half the RAM (1.62 GB vs 3.09 GB, `:140` vs `:121`). This is the
largest absolute win available to users who picked the slow model. **Quality tradeoff
(named):** turbo trails `large-v3` on the hardest multilingual/noisy audio but matches
`large-v2` (`:147`) — for English/major-language dictation the difference is inaudible.

### #4 — Cache mutex during decode: CONFIRMED NOT HELD (with quote)

**This is explicitly confirmed, not inferred.** The `AsyncMutex<TranscriberCache>` guard is
scoped to a block that ends *before* decode:

```rust
// commands/audio.rs:1433-1444
let transcriber = {
    let cache_state = app.state::<AsyncMutex<TranscriberCache>>();
    let mut cache = cache_state.lock().await;        // ← lock held only here
    cache.get_or_create(model_path)?                 // ← returns Arc<Transcriber>
};                                                    // ← guard dropped (block end)
...
let result = tokio::task::spawn_blocking(move || {
    transcriber.transcribe_with_metadata_with_prompt(...)  // ← decode, NO cache lock
}).await...;
```

`get_or_create` returns `Arc<Transcriber>` by clone (`cache.rs:97,106,114`); the `Arc` (not the
guard) is moved into the blocking task. **`state.full(...)` holds no cache mutex.** The same
pattern is used at preload (`model.rs:957-960`, `lib.rs:1143-1145`).

**What this means for the 06-oracle engine-lease / decode-ahead pattern:** the locking layer
does **not** block concurrent decode — two recordings (or decode-ahead + finalize) can each
grab an `Arc<Transcriber>` clone and run `state.full` on independent `spawn_blocking` threads.
whisper-rs `WhisperContext::create_state()` (`whisper_state/mod.rs`) hands out independent
`WhisperState`s whose KV-cache/scratch are per-state, and the `WhisperContext` (model weights)
is read-only/shared — so **two states from one context can decode concurrently**.

**The real constraint is the single-context cache, not the mutex.** `MAX_CACHE_SIZE = 1`
(`cache.rs:10`) means one resident `WhisperContext` per model. That is *fine* for concurrency
(mmap'd weights are shared read-only), but it means:
- A model switch evicts + reloads (hundreds of ms, `cache.rs:14-16`).
- For decode-ahead you do **not** need a second context — you need a second **state** from the
  same cached context. So no proposal here to grow the cache; instead, when implementing the
  lease, create/reuse a pooled `WhisperState` per active decode (see #5).

**Proposed (lease/actor framing for 06-oracle, not a mutex change):** keep the lock scope
exactly as-is; introduce a small `DecodeStatePool` (one `WhisperState` per concurrent slot)
keyed by context, so decode-ahead + finalize never share a `&mut WhisperState`. This matches
whisper-rs's `full(&mut self, ...)` (`whisper_state/mod.rs:292`) requiring exclusive state.
Risk Low; invariant: never reuse a state mid-`full` (it holds the KV cache).

### #5 — State reuse, threads, flash-attn (whisper-rs 0.16 build flags)

**Fresh state per call.** `self.context.create_state()` at `transcriber.rs:585` allocates a new
`WhisperState` (KV cache + ggml scratch + compute buffer) every transcription. whisper.cpp's
state alloc is **not free**: for `large-v3` it is tens of MB and a non-trivial
malloc/zero-init, observed in VT's own logs as part of `WHISPER_INFERENCE` setup. Estimate
**~5–30 ms** + allocator fragmentation on a hot dictation loop (back-to-back recordings).
`NEEDS MEASUREMENT` with a span around `create_state` vs `state.full`.

**Reuse proposal:** pool `N` `WhisperState`s per cached context (where N = max concurrent
decodes, typically 1 today, 2 with decode-ahead). `full()` resets the KV cache internally, so
a state is reusable across calls as long as it isn't borrowed mid-decode. This dovetails with
#4's lease framing.

**`set_n_threads` tuning (`transcriber.rs:543-548`).** Today: `max(1, available_parallelism - 1)`
everywhere except `aarch64-windows` (capped to 4 for perf cores, `:530-539`). Two refinements:
- Apple Silicon has P+E cores; using `cores-1` can schedule encoder work on E-cores and *slow*
  the encoder (Amdahl through the slowest thread). `[INFERENCE]` a perf-core-only cap
  (profile-guided) can beat `cores-1` for short clips where thread-launch overhead also bites.
  `NEEDS MEASUREMENT`: sweep {2,4,6,8,cores-1} on M4 Pro.
- For very short clips (<1.5 s) the encoder is so small that N-thread fan-out overhead can
  exceed the gain; a lower thread count may win. Measure.

**flash-attn — exposed by whisper-rs 0.16, but it's a context-creation flag, not per-call.
Source-verified:** `WhisperContextParameters { flash_attn: bool, ... }` with
`pub fn flash_attn(&mut self, bool)` (`whisper-rs` `whisper_ctx.rs:465,477,491-494,584`).
VT constructs `WhisperContextParameters::default()` (`transcriber.rs:57`) and **never calls
`.flash_attn(true)`**, so `flash_attn == false` (default `:477`). Enabling it requires editing
the context-creation path (one line at `transcriber.rs:78` for Apple Silicon):
`ctx_params.flash_attn(true);`. Expected **~1.3–2× encoder** speedup `[INFERENCE] from
whisper.cpp Metal/flash-attn benchmarks`, biggest on long context; near-zero benefit on short
clips once #1 shrinks the window. **Caveat (named, source):** "Can't be used with DTW" — VT
uses no DTW, so safe. Metal flash-attn support/maturity varies by macOS SDK; gate behind a
feature flag and measure. **No other build flags are exposed:** VT pins
`whisper-rs = "0.16.0"` with features `metal` (macOS, `Cargo.toml:83`) / `openmp`
(Windows, `:97`) and plain on Windows-aarch64 (`:91,94`); there is no
`coreml`/`blas`/`cuda`/`vulkan` feature in use for the in-process path.

### #6 — Windows Vulkan sidecar: full-request process lock + warm-start

**The lock.** `send()` acquires `self.process.lock().await` (`gpu_sidecar.rs:509`) and holds it
across `process.request(...)` (`:525`) — i.e. the **entire transcription round-trip** (stdin
write → sidecar decode → stdout read, `:219-244`). Because there is a single sidecar process
with one stdin/stdout pair, this serializes requests by necessity — there is no concurrency to
recover here, only latency.

**IPC overhead.** Requests/responses are newline-delimited **JSON** (`:220-243`). For
`Transcribe`, the payload carries `audio_path` (a path, not audio bytes, `:57-78`), so IPC
payload size is tiny — the cost is the **context switch + serde + line read**, typically
sub-ms, dwarfed by decode. The bigger fixed cost is **sidecar process spawn + Vulkan device
init + model load on a cold sidecar** (`:512`), which `warm_on_preload` already amortises.

**Warm-start — already present.** `warm_on_preload` (`:336-366`) → `probe` (`:368`) spawns the
process and loads the model on preload, and the process is **kept alive** across requests
(only killed on error/abort/id-mismatch: `:536,547,559`). So consecutive recordings on the same
model skip the spawn+load. The timeout is `ceil(dur_s)×4 + 60` clamped to [180s, 1800s]
(`:768-774`); `CONTROL_REQUEST_TIMEOUT = 60s` (`:21`).

**Opportunities:**
- **Guarantee warm-alive across recordings** (not just on preload): today a prior error evicts
  the process and the next recording eats cold spawn. A keep-alive + health-probe on idle would
  recover the **~200–800 ms** cold-spawn cost `[INFERENCE]` for the recording-after-an-error.
- **Binary side-channel:** if decode-ahead lands on Windows (06-oracle Phase 3 mirroring), the
  JSON-path-per-chunk overhead compounds; a length-prefixed binary protocol (like the Parakeet
  sidecar discussion in 028) would cut per-chunk framing cost. Low priority — chunked decode
  on CPU Windows is the more likely Phase-3 path there.
- **Concurrency:** if two Windows transcriptions ever need to overlap (decode-ahead), one
  sidecar process cannot serve them concurrently (serial stdin/stdout). A process *pool* or a
  request-id multiplexed protocol would be needed — but this is future work gated on streaming.

---

## Learn from others

(Cross-reference `07-competitive-sota.md` for the full competitive matrix once available;
techniques below are source/fact-grounded.)

- **whisper.cpp `examples/stream` + `--mc`/audio_ctx.** The upstream CLI exposes
  `audio_ctx`/max-context precisely because short-clip decode over a full 30 s window is a
  known waste. Every fast Whisper wrapper (macOS `WhisperAX`, `whisper.cpp` server mode) sets
  it adaptively. VT not setting it is the outlier, not the norm.
- **`large-v3-turbo` is the de-facto low-latency default.** OpenAI positions turbo for
  real-time; Groq's fastest Whisper endpoint is `whisper-large-v3-turbo` (and VT's own cloud
  path already uses it, `groq.rs:7`). The local path should match.
- **`itsmontoya/scribble` (plan 028 Evidence C, source-verified).** Its whisper backend uses
  **segmented decode-ahead**: a growing buffer, a fresh `whisper_full` at a min-window, emit
  all-but-last segment as final, **advance the buffer head past emitted audio** so finalized
  audio is never re-decoded. This is the reference algorithm for VT's Phase 3 and **pairs
  naturally with `set_audio_ctx` + `set_single_segment`** (per-chunk small context).
- **flash-attn in whisper.cpp.** Stable since whisper.cpp ~v1.5; gives measurable encoder
  speedups especially on GPU backends; the DTW-incompatibility is the only documented
  constraint (irrelevant to VT).
- **Beam vs greedy.** Whisper's original paper and most production deployments (e.g.
  faster-whisper default) use **greedy / temperature-fallback** for real-time; beam is reserved
  for offline high-accuracy. VT's Metal beam=5 is more conservative than the SOTA realtime
  default.

---

## Measurement plan

All gated behind the existing `log_performance(...)` spans (`transcriber.rs:628-635`); no new
infra required for before/after.

| Win | Instrument | Before metric | After metric / assertion |
|---|---|---|---|
| #1 audio_ctx | Span `WHISPER_INFERENCE` + log `audio_ctx` + `duration_seconds` | ms for {2,5,10,29}s clips @ audio_ctx=1500 | ms at adaptive audio_ctx; **≥3× on ≤5 s**, ≈1× at 29 s; WER parity sweep |
| #2 beam→greedy | Same span + `sampling_strategy` log | ms beam=5 vs greedy per length | ≤2× decoder; WER Δ <0.5 % on clean short English |
| #3 turbo default | Span by model | `large-v3` ms vs `large-v3-turbo` ms same clip | ~6× (confirm manager claim `:146`) |
| #4 mutex (confirm) | Static (already shown) | n/a | Quote at `audio.rs:1433-1444` proves decode is lock-free |
| #5 state reuse | New span around `create_state` vs `full` | per-call `create_state` ms | pooled ms ≈ 0; verify concurrent `full` safety |
| flash-attn | Span encoder sub-phase (add) | baseline encoder ms | ~1.3–2× encoder on long context; DTW off (already) |
| #6 sidecar | Log sidecar spawn vs reuse (`:512` probe `load_time_ms` `:393`) | cold-spawn recording ms | warm recording skips spawn |

**Verify-criteria (correctness gates, per plan 015 never-lose-speech):**
- audio_ctx: trailing-word presence test across the length sweep; floor + margin must keep WER
  within an agreed bound of full-context baseline.
- beam/turbo: WER regression suite (existing test audio) within bound.
- flash-attn: identical transcription on a fixed clip vs baseline; abort/cancel still fires
  (`set_abort_callback_safe` at `transcriber.rs:621` unaffected).

---

## Prioritized recommendations

- **P1** (ship first, S effort, biggest short-clip ROI): **#1 length-adaptive `set_audio_ctx`** +
  **#3 make turbo the explicit default** (a sort-key fix + onboarding nudge). Together these
  attack the two largest knobs with negligible WER risk for dictation.
- **P1**: **#2 greedy-on-Metal for short clips** — same edit site as #1, compounds with it.
- **P2** (measure first): **#4/#5 state pool** (unblocks 06-oracle decode-ahead cleanly) and
  **#4 flash-attn** (one-line context flag, gated + measured). **#8 release-profile LTO** (CPU/
  Windows win, verify symbolication).
- **P2**: **#6 sidecar keep-alive across error recovery** (Windows cold-spawn latency).
- **P3** (depends on streaming): **#7 `set_single_segment`** for the Phase-3 chunked decode
  loop; **#6 binary side-channel / process pool** only if Windows decode-ahead is pursued.

## Open questions / risks

1. **flash-attn on Metal maturity** at the VT-pinned whisper.cpp revision — needs a gated
   measurement; if it regresses or destabilises, keep off (Low risk to defer).
2. **audio_ctx margin value** — the 1.15× + floor is an estimate; the correct margin should be
   derived from the WER sweep, not assumed. Truncation is a silent WER cliff.
3. **`n_threads` on Apple Silicon P/E** — `cores-1` may be pessimal; only a sweep settles it.
4. **Turbo vs large-v3 quality bound** — agree the acceptable WER delta before making turbo the
   hard default (plan 028 open-decision #5). For translation/long-form, large-v3 may stay the
   recommendation.
5. **Windows ARM64** (`Cargo.toml:93`) has plain whisper-rs (no openmp, no GPU sidecar) — CPU
  decode there benefits most from #1/#2/#8; confirm the thread cap (`transcriber.rs:530-539`)
  still applies.
