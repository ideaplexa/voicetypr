# Audio capture + resample + format-normalization pipeline — VoiceTypr perf teardown

> Scope: everything between the microphone and the engine's `&[f32]` input —
> CPAL capture, the WAV write/drain, normalization (resample + downmix +
> peak-gain + dither), and the ffmpeg-vs-in-process decode surface. READ-ONLY
> analysis of VoiceTypr `main` (HEAD `af63ab1`). Streaming *architecture* is
> already decided in `06-oracle-decision.md` and is **out of scope** here; this
> doc cites it for the live-tap/resampler prerequisite and stays on the perf
> layer. Engine decode itself is `03`/`05`/`01`.

## TL;DR

The **CPAL real-time capture path is plan-008 compliant** — a preallocated
chunk pool, bounded `sync_channel`s, and `try_*` everywhere mean the callback
allocates nothing and blocks on nothing, **except one minor leak**: the level
meter pushes updates through an *unbounded* `std::sync::mpsc::Sender::send`
(`level_meter.rs:47`) which heap-allocates a node ~10×/sec on the RT thread.
The two **big, independent wins** are (a) **collapse the WAV round-trip on the
live-mic path** — today the recording is written to disk as a native-rate WAV,
then *read back*, normalized, *written again* as a 16 kHz WAV, deleted, then
*read again* by the engine (2 writes + 2 reads + 1 unlink + a lossy
f32→i16→f32 quantization, ~4–10 ms on SSD, much more on a slow/network FS), and
(b) **capture at 16 kHz mono when the device supports it** to skip resample
*and* cut capture file size ~6×. The ffmpeg story is **already fixed on the
live-mic hot path** (in-process `normalizer` first, ffmpeg fallback); ffmpeg
still runs **only on import paths** (upload/clipboard/CLI/remote) — and a
symphonia in-process decoder (`audio/converter.rs`) already exists in the tree
but is **dead code** (`#[allow(dead_code)]`, referenced only by tests).

---

## 1. Current-state map

### 1.1 Capture — CPAL callback (the hot path, plan-008 surface)

The recorder records at the **device's default config** and defers all
format work downstream. There is **no** in-callback resample and **no**
in-callback mono mixdown:

```rust
// audio/recorder.rs:304
let config = device.default_input_config().map_err(|e| e.to_string())?;
...
// audio/recorder.rs:330-335  — native rate + native channels written verbatim
let spec = hound::WavSpec {
    channels: config.channels(),
    sample_rate: config.sample_rate().0,
    bits_per_sample: 16,
    sample_format: hound::SampleFormat::Int,
};
```

Capture data flows through a **recycled chunk pool** sized to the worst-case
callback payload so the RT thread never grows a buffer:

```rust
// audio/recorder.rs:316
let chunk_capacity = max_callback_samples(&device, &config);   // sizes pool
...
// audio/recorder.rs:343-350  — bounded recycle channel + preallocated pool
let (recycle_tx, recycle_rx) = mpsc::sync_channel::<Vec<i16>>(RECYCLE_CHANNEL_CAPACITY);
for _ in 0..(WRITER_QUEUE_CAPACITY + CHUNK_POOL_SLACK) {
    let _ = recycle_tx.send(Vec::with_capacity(chunk_capacity));
}
```

`max_callback_samples` (`recorder.rs:155-171`) derives the true upper bound
from the device's supported buffer-size *range* (floored at
`CHUNK_CAPACITY_MIN = 8192` and ceilinged at `MAX_REASONABLE_CALLBACK_FRAMES`,
`recorder.rs:134`), so `extend_from_slice` on the RT thread is guaranteed not
to reallocate.

The RT callback body (`process_audio`, `recorder.rs:473-548`) does, per frame:

```rust
// audio/recorder.rs:508-531  (hot path, abridged)
let sum: f32 = f32_samples.iter().map(|x| x * x).sum();          // stack
let rms = (sum / f32_samples.len() as f32).sqrt();
if let Ok(mut meter) = level_meter_clone.try_lock() {            // NON-blocking
    let _ = meter.process_samples(f32_samples);
}
if let Ok(mut detector) = silence_detector_clone.try_lock() {    // NON-blocking
    if let Some(event) = detector.update(rms) {
        let _ = silence_event_tx_clone.try_send(event);          // bounded, try
    }
}
let Ok(mut chunk) = recycle_rx.try_recv() else { ... return; };  // reused buffer
chunk.clear();
chunk.extend_from_slice(i16_samples);                            // no realloc (preallocated)
match writer_tx_clone.try_send(WriterMsg::Chunk(chunk)) { ... }  // bounded, try
```

Format-conversion scratch buffers (`i16_scratch`/`f32_scratch`) are allocated
**once** at stream build (`recorder.rs:554,573,592-593`) with
`Vec::with_capacity(chunk_capacity)` and only `clear()`+`extend()` inside the
callback. The writer worker uses hound's block `get_i16_writer` fast path
(`recorder.rs:389-397`) to drain its queue in microseconds.

### 1.2 Normalization (live-mic, in-process) — `audio/normalizer.rs`

```rust
// audio.rs:4929-4932  — runs in-process FIRST (spawn_blocking), ffmpeg only on failure
let in_proc = tokio::task::spawn_blocking(move || {
    crate::audio::normalizer::normalize_to_whisper_wav(&a, &d)
}).await;
```

`normalize_to_whisper_wav` (`normalizer.rs:20-119`) reads the native WAV back,
downmixes, resamples, peak-normalizes, TPDF-dithers, and writes a **new** 16 kHz
WAV:

```rust
// audio/normalizer.rs:46-57   read #1 (i16) → f32
let samples_i16: Vec<i16> = reader.samples::<i16>().collect::<Result<Vec<_>,_>>()?;
let samples_f32: Vec<f32> = samples_i16.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
// audio/normalizer.rs:60-64   mono downmix (equal-power, silent-channel aware)
// audio/normalizer.rs:67-71   rubato resample 48k → 16k  (skipped if already 16k)
// audio/normalizer.rs:76-86   peak-normalize (speech-gated gain cap)
// audio/normalizer.rs:89-96   TPDF dither + quantize → i16   (write #2)
// audio/normalizer.rs:107-116 hound write_sample × N  → normalized WAV
```

### 1.3 Resampler — `audio/resampler.rs`

Whole-buffer, **stateless** batch. Constructs a fresh `Fft` resampler per call
and processes the entire clip in one shot (`resampler.rs:20-44`). Fast-path
short-circuit at 16 kHz (`resampler.rs:10-14`). FFT is the right offline choice
(fastest + best quality for fixed-ratio), but it is **not** reusable as a
chunked/stateful resampler — see §5.

### 1.4 Engine re-read — `whisper/transcriber.rs`

Whisper reads the **normalized** WAV a second time and re-derives f32
(`transcriber.rs:385-445`): i16 read → `convert_integer_to_float_audio` →
stereo/multi-channel downmix → `resample_to_16khz` (short-circuits, it's already
16 k). Parakeet receives a file *path* (`parakeet/manager.rs`, FluidAudio
resamples internally). So the **16 kHz mono** work the normalizer just did is
re-checked and re-short-circuited by every engine.

### 1.5 ffmpeg surface — `audio.rs` import paths + `ffmpeg/mod.rs`

`to_wav_streaming` / `normalize_streaming` spawn ffmpeg as a child process with a
fixed `-ac 1 -ar 16000 -sample_fmt s16` chain (`ffmpeg/mod.rs:236-265`).
`resolve_binary` (`ffmpeg/mod.rs:25-75`) re-walks a multi-directory search list
**on every call** (resource dir, exe dir, every parent, PATH). It runs on:

- **Live mic: ❌ NOT ffmpeg.** Already in-process first, ffmpeg is only the
  fallback (`audio.rs:4933-4961`).
- **File upload:** ffmpeg for Whisper (`audio.rs:6419`), Parakeet (`:6448`),
  Cloud (`:6500`), Remote (`:6552`).
- **Clipboard audio:** ffmpeg for Remote (`audio.rs:6845`).
- **CLI → remote:** ffmpeg (`cli.rs:551`, `normalize_audio_for_remote`).

### 1.6 The in-process import decoder that already exists but is unused

`audio/converter.rs::convert_to_wav` (`converter.rs:14-191`) is a full
symphonia decode → downmix → rubato-resample → hound-write path. It is exported
(`audio/mod.rs:1`) but **dead in production** — `#[allow(dead_code)]`
(`converter.rs:13`) and referenced **only by `converter_tests.rs`**. Every
production import path calls `crate::ffmpeg::normalize_streaming` instead.

---

## 2. CPAL callback plan-008 compliance — explicit verdict

> Plan 008 invariant: **no allocation, no blocking lock, no blocking channel
> op, no drop under backpressure** in the CPAL callback.

| Risk class | In callback? | Evidence | Verdict |
|---|---|---|---|
| **Heap allocation** | **No** on the capture path; **one minor** on the level path | chunk is `recycle_rx.try_recv()` of a preallocated `Vec` (`recorder.rs:523`); `extend_from_slice` never grows it (sized by `max_callback_samples`, `:155-171`); scratch buffers preallocated once (`:554,573,592`). **But** `AudioLevelMeter::process_samples` calls an *unbounded* `mpsc::Sender::send` (`level_meter.rs:47`) ~10×/sec → 1 heap node/send | **Compliant w/ 1 caveat** |
| **Blocking lock** | **No** | every shared mutex taken via `try_lock()` with `if let Ok(...)` drop-on-contention (`recorder.rs:512,517`) | ✅ Compliant |
| **Blocking channel op** | **No** | `writer_tx.try_send`, `recycle_rx.try_recv`, `silence_event_tx.try_send` — all bounded `sync_channel` `try_*` (`recorder.rs:484,487,497,519,523,533,542`) | ✅ Compliant |
| **Drop under backpressure** | Safety valve only | pool-exhaust and queue-full both `fetch_add(dropped_chunks)` + drop (`recorder.rs:527,490,536`); by conservation `RECYCLE_CHANNEL_CAPACITY = WRITER_QUEUE_CAPACITY + CHUNK_POOL_SLACK` (`:53`) the pool is provably never exhausted in practice | ✅ Compliant (drop is the designed last resort) |
| **Tauri event emission from RT** | **No** | no `emit`/`emit_to` in `process_audio`; level/silence flow over std channels to the command layer (`recorder.rs:473-548`) | ✅ Compliant |

**The one caveat (real, minor):**

```rust
// audio/level_meter.rs:39-49
if self.sample_count >= self.update_interval {        // ~10×/sec
    self.sample_count = 0;
    let display_level = map_voice_level(self.smoothed_level);
    if let Err(e) = self.audio_level_tx.send(display_level) {   // ← unbounded mpsc SEND
        log::debug!("Failed to send audio level: ...");
    }
}
```

`audio_level_tx` is an **unbounded** `std::sync::mpsc::Sender`
(`recorder.rs:271`: `mpsc::channel::<f64>()`). An unbounded `mpsc::send`
allocates one heap node per message (messages are unbounded, so they must live
on the heap — the standard reason RT audio avoids `std::sync::mpsc::channel`).
At ~10 Hz this is ~10 small allocations/sec on the RT thread: **a genuine, if
minor, plan-008 allocation**. It cannot block (unbounded ⇒ always room), and it
is gated behind the `level_meter.try_lock()` success (`recorder.rs:512`), so it
is low-impact — but it is the *only* non-compliance in an otherwise exemplary
hot path. See opportunity **O1**.

---

## 3. ffmpeg vs in-process — per-path table (format-coverage-aware)

> Decoder-replacement research is **resolved** (source-verified against VT's
> `Cargo.lock` and the macOS sidecar; research run directly by the coordinator).
> This section records the recommendation; synthesis will formalize. **No
> blanket ffmpeg removal is proposed** — the common import formats move
> in-process; ffmpeg stays as the coverage fallback for Ogg-Opus / WMA /
> HE-AAC / exotic.

**Verified decoder coverage (Cargo.lock + macOS sidecar):**

- **symphonia 0.5.4** (`Cargo.lock`, `Cargo.toml:37` `features=["all"]`): codecs
  **flac, mp3, aac (LC only — no HE-AAC), adpcm, alac, pcm (int+float), vorbis**;
  formats **caf, isomp4 (m4a/mp4), mkv, ogg, riff (wav/avi)**. **No
  `symphonia-codec-opus`** (Opus landed in symphonia 0.6), **no WMA**.
- **macOS AVFoundation / CoreAudio** (via the Parakeet Swift sidecar,
  `sidecar/parakeet-swift/Sources/main.swift:597` → FluidAudio
  `AudioConverter.resampleAudioFile()` → `AVAudioFile(forReading:)`,
  *verified by the ffmpeg-replacement research, not re-read here*): decodes
  **mp3, m4a/AAC, ALAC, AIFF, CAF, WAV + Opus-in-CAF**. Does **NOT** decode
  **Ogg/WebM-Opus** (the `.ogg`/`.opus` container common in WhatsApp/Telegram
  voice notes) — Opus only *inside* CAF/m4a.
- **Shared gaps → ffmpeg fallback:** **Ogg-Opus, WMA, HE-AAC, AC3/DTS/AMR**.

**Recommended decode chain (per path):**

| # | Audio entry | Today | `path:line` | Recommended decode chain |
|---|---|---|---|---|
| 1 | **Live mic → local engine** | ✅ in-process first (`normalizer`+rubato), ffmpeg fallback | `audio.rs:4929-4961` | **Already landed** — no change |
| 2 | **Live mic → Cloud** | skip normalize entirely | `audio.rs:4904-4910` | n/a |
| 3 | **Live mic → Remote** | skip normalize | `audio.rs:4911-4917` | n/a |
| 4 | **Upload → Whisper** | ❌ ffmpeg only | `audio.rs:6419` | **symphonia → ffmpeg fallback** (covers Ogg-Opus / HE-AAC / WMA) |
| 5 | **Upload → Parakeet (macOS)** | ❌ ffmpeg only | `audio.rs:6448` | **route through sidecar `resampleAudioFile()` (AVFoundation) → ffmpeg fallback**; ffmpeg-free for the OS-native set, ffmpeg still covers Ogg-Opus/WMA |
| 6 | **Upload → Cloud** | ❌ ffmpeg only (→ canonical WAV) | `audio.rs:6500` | **symphonia → ffmpeg fallback** (or AVFoundation on macOS) |
| 7 | **Upload → Remote** | ❌ ffmpeg only | `audio.rs:6552` | **symphonia → ffmpeg fallback** |
| 8 | **Clipboard audio → Remote** | ❌ ffmpeg only | `audio.rs:6845` | **symphonia → ffmpeg fallback** |
| 9 | **CLI → remote** | ❌ ffmpeg only | `cli.rs:551` | **symphonia → ffmpeg fallback** |
| 10 | **ffmpeg `segment`** (chunking helper) | ffmpeg only | `ffmpeg/mod.rs:267-291` | keep (long-form tooling) |

**Net:** drop the ffmpeg **spawn** on the common import formats (mp3/m4a/AAC/
ALAC/AIFF/CAF/WAV/flac/vorbis); keep ffmpeg strictly as the **coverage fallback**
for Ogg-Opus (voice notes!), WMA, HE-AAC, and exotic codecs. The shape mirrors
the already-landed live-mic path (`audio.rs:4933-4961`). The already-written
`audio/converter.rs` (`converter.rs:14-191`) can be the symphonia first-try on
the Rust/Whisper path once wired in. **Precision caveat for the Parakeet/macOS
row (#5):** it is **not** "no ffmpeg ever" — it is ffmpeg-free *for the
OS-native set*, with ffmpeg still covering Ogg-Opus + WMA + HE-AAC.

### Spawn cost on the hot path (quantified, NEEDS MEASUREMENT)

**The live-mic critical path pays zero ffmpeg spawn cost today** — that is the
headline the Phase-0 landing got right. ffmpeg spawn cost is therefore an
**import-path / cold-clip** cost, not a stop→insert cost:

- `fork`/`posix_spawn` + dynamic-link of a static ffmpeg (macOS arm64): **~3–10 ms**
- ffmpeg init (libavcodec/libavformat register): **~20–60 ms cold**, ~10–30 ms warm
- `AVFormatContext` open + probe: **~5–20 ms**
- decode + WAV mux: proportional to length (~10–50 ms for a 5 s clip)
- **`resolve_binary` multi-dir walk** (`ffmpeg/mod.rs:25-75`, re-run every call): **~1–3 ms**

**Est. total ≈ 50–150 ms p50 per import** (higher cold), vs ~1–5 ms for an
in-process symphonia decode of the same short clip. `[NEEDS MEASUREMENT: timed
ffmpeg spawn vs symphonia on a fixed 2 s/5 s/15 s clip corpus across
mp3/m4a/ogg/flac.]` On Windows the sidecar binary resolution + `CREATE_NO_WINDOW`
(`ffmpeg/mod.rs:180-183`) adds a little more. The binary-resolve result is also
trivially cacheable (opportunity **O5**).

---

## 4. Opportunities

Sorted by **impact-per-effort**. Impact estimates flagged `[NEEDS MEASUREMENT]`
where not timed.

| # | Opportunity | Mechanism | Est. impact | Effort | Risk | Invariant touched |
|---|---|---|---|---|---|---|
| **O1** | Make level-meter path alloc-free | Convert `audio_level_tx` to a bounded `sync_channel` + `try_send`, or an atomic/ring buffer; matches the silence path which is already alloc-free | removes ~10 heap allocs/sec on RT thread (correctness/guardrail, not latency) | **S** | **Low** | plan-008 (tightens compliance) |
| **O2** | Collapse the WAV round-trip on the live-mic path | After drain, hand the in-memory i16/f32 PCM to a `normalize_to_pcm_memory` that returns f32 16 k mono; feed straight to the engine; keep the raw native WAV as the durable artifact (never-lose-speech), drop the intermediate normalized WAV write+read | ~4–10 ms/record on SSD, **much more** on slow/network FS; removes a lossy f32→i16→f32 quant round-trip | **M** | **Med** | plan-015 (keep raw WAV), normalization seam |
| **O3** | Capture at 16 kHz mono when the device supports it | Negotiate a 16 k mono `StreamConfig` (try `supported_input_configs` → build config) with robust fallback to `default_input_config`; skip resample + cut capture file ~6× | ~3–6× smaller capture I/O; removes resample alloc; simplifies seam; enables cheaper streaming later | **S/M** | **Med** | capture correctness; device-compat (must fallback) |
| **O4** | Wire in-process decode into import paths (defer decoder choice to research) | Re-use `converter.rs` as first-try on paths #4–#9 with ffmpeg fallback (same shape as `audio.rs:4933-4961`); **final decoder = ffmpeg-replacement research** | ~50–150 ms/import (spawn avoided); 0 ms on live mic | **M** | **Low–Med** | format coverage (Opus/exotic → fallback) |
| **O5** | Cache the resolved ffmpeg/ffprobe binary path | `resolve_binary` walks dirs + PATH every call (`ffmpeg/mod.rs:25-75`); resolve once, `OnceLock<PathBuf>` | ~1–3 ms/ffmpeg call | **S** | **Low** | none |
| **O6** | Fuse normalizer's peak-fold + gain-apply passes | `normalizer.rs:76` (fold) then `:79-86` (map) over the same buffer — one fused pass; also drop per-sample `thread_rng()` ×2 dither when gain≈1 | <1 ms/clip | **S** | **Low** | dither correctness (keep TPDF when writing) |
| **O7** | Stateful/chunked resampler for decode-ahead | Replace per-call `Fft::new` (`resampler.rs:20`) with a retained `SincFixedIn`/streaming resampler for the live tap | unblocks streaming; ~0 on today's batch path | **M/L** | **Med** | **DEFER → oracle doc / plan-028 Phase-1 prereq** |
| **O8** | Stream-decode in `converter.rs` instead of accumulate-all | `converter.rs:80-167` buffers the *entire* file as i16, then downmixes, then resamples — 3+ full passes + ≥4 Vecs; decode straight to f32 mono in one pass | memory ↓ for long imports; small CPU | **M** | **Low** | none (import only) |

---

## 5. Deep-dive — top opportunities

### O2 — Collapse the WAV round-trip (the biggest local-path IO win)

**Today's live-mic artifact flow (count the copies/round-trips):**

```
callback → recycled chunk → writer ──write#1──► native.wav  (48k/16bit, N ch)
                                              │
normalizer ──read#1──► i16→f32→downmix→rubato→dither→i16 ──write#2──► normalized.wav (16k mono)
audio.rs:4965 ──unlink──► native.wav
engine (whisper) ──read#2──► i16→f32 → (downmix skip) → (resample skip) ──► decode
```

= **2 file writes + 2 file reads + 1 unlink + 2 f32↔i16 quantization passes**
per recording. Sizing (5 s clip): native ≈ 938 KiB (48 k/stereo), normalized ≈
156 KiB (16 k/mono).

**Why it's lossy for no benefit on the local path:** the normalizer TPDF-dithers
and quantizes to i16 (`normalizer.rs:89-96`) purely so it can be *stored* as a
WAV — then the engine *immediately* converts it back to f32
(`transcriber.rs:399-400`). The dither noise is injected only to be rounded away
again. The intermediate 16 k WAV exists solely because the seam is
path-based (`TranscriptionRequest` carries a `Path`, plan-028 prereq #3).

**Concrete collapse (correctness-preserving):**

1. Keep the raw native WAV write exactly as-is — it is the durable
   never-lose-speech artifact (plan-015). Do **not** remove it.
2. Add `normalize_to_pcm_memory(input_wav) -> Result<Vec<f32>>` (a thin variant
   of `normalize_to_whisper_wav` that returns the resampled f32 buffer instead
   of writing it) — reuses the identical downmix/resample/gain logic, so WER is
   unchanged.
3. Add `transcribe_from_pcm(&[f32], 16_000)` alongside the WAV-reading
   `transcribe` entry in `whisper/transcriber.rs` (skip the i16 read + resample
   short-circuit; jump straight to `state.full`).
4. On the live-mic→local branch, call `normalize_to_pcm_memory` then
   `transcribe_from_pcm`. The normalized WAV write+read+unlink vanish.

**What you keep vs drop:** keep raw native WAV + one read (for normalize-in);
**drop write#2, read#2, the unlink, and both quant passes.** Net per recording:
**1 write + 1 read** instead of 2+2+unlink, plus no injected dither noise on the
local decode path.

**Estimated savings:** ~4–10 ms on Apple NVMe (syscall/open/finalize overhead
dominates; the bytes themselves are sub-ms at 3–7 GB/s). **Materially more on a
slow spinning disk or network FS** — which is exactly the pathological case the
3 s `WRITER_JOIN_TIMEOUT` / 8 s `STOP_JOIN_TIMEOUT` design
(`recorder.rs:61,71`) exists to defend against. `[NEEDS MEASUREMENT: timed
end-to-end with/without collapse on SSD, external HDD, and a network mount.]`

**Safety:** engine input is byte-identical *up to* the removed dither noise
(which is strictly *cleaner*). Parakeet still needs a file path today
(`parakeet/manager.rs`), so O2 lands for **Whisper-first**; Parakeet in-memory
handoff waits on the sidecar session protocol (plan-028 Phase 2).

### O3 — Capture at 16 kHz mono when supported

**Today:** `device.default_input_config()` (`recorder.rs:304`) — accepts whatever
the device advertises (typically 44.1/48 k, often stereo). The whole 3×–6×
over-capture is written, read, and resampled away downstream.

**Proposal:** before falling back to default, enumerate
`device.supported_input_configs()` and look for a mono 16 k config; build a
`StreamConfig { sample_rate: 16000, channels: 1, buffer_size }` and
`build_input_stream` on that. On match, the normalizer's resample
(`normalizer.rs:67-71`) and downmix (`:60-64`) both short-circuit, capture I/O
drops ~6× (48 k/stereo → 16 k/mono), and the resampler allocation disappears.

**Tradeoffs / why Med risk:**

- **macOS CoreAudio:** the HAL will insert its own `AudioConverter` for a
  non-native rate. Quality is fine; there can be a small added capture latency
  in the HAL, generally negligible for dictation. `[verify]`
- **Windows WASAPI shared mode:** 16 k mono is often *not* directly supported;
  requesting it can fail. **Robust fallback is mandatory** — on any config
  error, fall straight back to `default_input_config()`. Never let a 16 k
  attempt break capture (plan-015).
- **Correctness:** 16 k is already the engine contract, so capturing at 16 k is
  *more* faithful, not less. The only loss is the option to keep a
  higher-fidelity archive — acceptable for dictation.

**Est. impact:** smaller capture files (less disk, faster writer drain under
pressure), one fewer resample+alloc per recording, and a simpler seam for
future streaming (native-rate chunks would otherwise need a stateful resampler
per consumer — see O7). `[NEEDS MEASUREMENT: device support matrix across
built-in / USB / Bluetooth mics on macOS + Windows; capture xrun rate at 16 k
vs default.]`

### O1 — Close the one plan-008 allocation (level meter)

The unbounded `mpsc::Sender::send` at `level_meter.rs:47` is the **only**
allocation in the RT callback happy path. Two safe fixes:

1. **Mirror the silence path:** make `audio_level_tx` a bounded
   `sync_channel(N)` and use `try_send` (drop the level sample on `Full`, which
   is fine — the meter is smoothed, `:34`, and only ~10 Hz). This is
   alloc-free (fixed ring) and matches `silence_event_tx`
   (`recorder.rs:272`, `sync_channel(8)` + `try_send` at `:519`).
2. **Atomic/ring:** store the latest level in an `AtomicU64` (bit-pack the f32)
   and let the command layer poll — the Handy `emit_to`-gated-by-atomic pattern
   (`00-README.md` P2 #8).

Either is a ~5-line change and strictly tightens plan-008. Low impact on
*latency* (10 allocs/sec of a tiny node is invisible), high value as a
*guardrail correctness* fix.

### O4 — In-process decode on import paths (recommendation recorded)

The shape is already proven by the live-mic landing (`audio.rs:4933-4961`):
try in-process, on failure log + ffmpeg fallback. The decoder-replacement
research is resolved (§3), so the per-path chain is now concrete:

- **Rust/Whisper import (paths #4, #6–#9):** **symphonia → ffmpeg fallback.**
  `audio/converter.rs` is the symphonia first-try (it already decodes → downmix
  → rubato-resample → WAV, `converter.rs:14-191`); on failure (Ogg-Opus /
  HE-AAC / WMA) fall back to `crate::ffmpeg::normalize_streaming`.
- **Parakeet/macOS import (path #5):** **route through the sidecar's
  `resampleAudioFile()` (AVFoundation) → ffmpeg fallback.** FFmpeg-free for the
  OS-native set; ffmpeg still covers Ogg-Opus + WMA. This *avoids* the separate
  ffmpeg pre-normalize entirely on macOS by reusing the converter the engine
  already runs.
- **Where:** import/clipboard/CLI/remote (paths #4–#9), **never** the live mic
  (already in-process-first).
- **What it must keep:** ffmpeg fallback for the shared gaps (Ogg-Opus, WMA,
  HE-AAC, AC3/DTS) — per the user's ffmpeg nuance, never blanket removal.
- **What it saves:** ~50–150 ms/import spawn cost (§3), **0 ms** on the hot path.

### O7 — Stateful resampler (note + DEFER)

`resample_to_16khz` builds a fresh `Fft::<f32>` per call (`resampler.rs:20-28`)
and is whole-buffer — correct for today's batch path, unusable for a live tap.
A streaming consumer needs a *retained*, chunk-feeding resampler
(rubato `SincFixedIn` with input/output FIFO, or the OS-native converters
FluidAudio already uses — `plan-028` Evidence C §Parakeet). This is plan-028
**Phase-1 prerequisite #2** and the oracle doc's Step-3 substrate; **design
deferred there**. No action in this slice beyond the citation.

---

## 6. Learn from others

- **Handy** (`00-README.md` P2 #8, `01-streaming-engine.md`): high-rate mic
  level is delivered via `emit_to("<label>")` gated by a **cached atomic**, and
  **skipped entirely when the overlay is off** — exactly the O1 fix (atomic
  instead of unbounded mpsc) plus a zero-cost fast path. Handy's idle mic frame
  is a *single `Relaxed` atomic load* then return (`00-README.md` table row 1).
  VT's `SeqCst` drain-barrier loads (`recorder.rs:481,503`) are correct for the
  stop handshake but slightly heavier than needed on the non-stop path.
- **FluidAudio** (plan-028 Evidence C): accepts *any* AVAudio format and
  resamples to 16 k mono f32 internally (`AudioConverter`) — i.e. the engine
  side already owns resampling, which makes VT's separate normalizer step partly
  redundant for the Parakeet path and supports the O2 in-memory handoff.
- **rubato author guidance + `itsmontoya/scribble`** (plan-028 Evidence C):
  offline ⇒ `Fft` (VT's choice is right); streaming ⇒ `SincFixedIn` with a
  growing/advancing buffer and a retained resampler instance — validates O7's
  direction.
- **whisper.cpp `examples/stream`**: naive sliding-window re-decode; *not* a
  model for the resampler, but confirms the chunked-PCM-feed shape O7 must
  support.
- Competitive/SOTA decode + RTF numbers: see `07-competitive-sota.md` (once
  written) for the per-engine comparison this slice feeds into.

---

## 7. Measurement plan

Instrument (span/counters), per opportunity:

| Opp | Instrument | Before → After metric |
|---|---|---|
| O1 | counter: `allocations_in_callback` (use a `#[cfg(debug)]` `GlobalAlloc` wrapper or `dhat` in a test build) | ~10 allocs/sec → 0 on the RT thread |
| O2 | spans: `normalize_write_wav`, `engine_read_wav`, plus a wall-clock `stop→first-decode-sample` | writes 2→1, reads 2→1; ms saved per clip length × disk class |
| O3 | capture-file bytes; `xrun`/drop count; device-support matrix | capture bytes ~6×↓; xrun Δ≈0 (fallback must hold) |
| O4 | span: `import_normalize_total` (in-process branch vs ffmpeg branch) on a fixed mp3/m4a/ogg/flac corpus | ~50–150 ms/import spawn → ~1–5 ms in-process |
| O5 | span: `ffmpeg_resolve_binary` | ~1–3 ms → ~0 after first resolve |
| O6 | span: `normalize_compute` (fused vs split) | <1 ms/clip (micro) |

Re-baseline harness (consistent with oracle Step-1): fixed 2 s / 5 s / 15 s
clips across Whisper + Parakeet; capture p50/p95 stop→first-decode on SSD,
external HDD, network FS.

---

## 8. Prioritized recommendations

- **P1 — O2 (collapse WAV round-trip) + O1 (alloc-free level meter).** Both
  touch the local hot path, both correctness-positive, both independent of
  streaming. O2 is the largest measurable local-path win; O1 is the plan-008
  guardrail fix. Ship together behind the existing normalization seam.
- **P1 — O3 (capture at 16 k mono with fallback).** Compounds O2 (smaller
  artifact, no resample) and de-risks future streaming. Gated entirely on a
  robust fallback so capture never breaks.
- **P2 — O4 (in-process import decode).** Real per-import win (~50–150 ms) but
  off the stop→insert critical path; per-path decode chain now recorded in §3
  (symphonia/AVFoundation-first → ffmpeg fallback). Keep ffmpeg fallback for
  Ogg-Opus/WMA/HE-AAC.
- **P2 — O5 (cache binary resolve) + O6 (fuse normalizer passes).** Cheap,
  low-risk micro-wins; do alongside O4.
- **P3 — O7 (stateful resampler).** Not for today; required by plan-028 Phase-1
  / oracle Step-3. Cite, don't build here.
- **P3 — O8 (stream-decode in converter).** Only if long imports matter; fold
  into O4 if that lands.

## Open questions / risks

1. **Opus / HE-AAC coverage (resolved by research).** symphonia 0.5.4
   (`Cargo.lock`) has **no `symphonia-codec-opus`** and **AAC-LC only**; macOS
   AVFoundation decodes Opus only *inside* CAF/m4a, **not** Ogg/WebM-Opus.
   → the in-process paths **must** keep ffmpeg fallback for `.opus`/`.ogg-opus`
   (WhatsApp/Telegram voice notes), HE-AAC, and WMA. Action: wire the fallback
   (already the shape of `audio.rs:4933-4961`); no further verification needed.
2. **Device support for 16 k mono capture** across built-in / USB / Bluetooth
   mics on macOS + Windows — O3's fallback must be proven before default-on.
3. **Parakeet in-memory handoff** — O2's full payoff needs the sidecar to
   accept PCM (today it takes a path, `parakeet/manager.rs`); until then O2 is
   Whisper-only.
4. **Slow/network-FS behavior** — O2's savings scale with disk latency; the
   existing 3 s/8 s writer-join budgets (`recorder.rs:61,71`) hint these stalls
   are real and worth measuring, not assuming.
5. **Decoder choice (resolved by research).** Per-path chain recorded in §3:
   Rust/Whisper = symphonia → ffmpeg; macOS/Parakeet = AVFoundation (sidecar)
   → ffmpeg; ffmpeg stays as the coverage fallback. Target contract: 16 kHz
   mono s16 WAV / f32 engine input. **Open follow-up:** Windows-native import
   decode (Media Foundation) is *not* needed today — Windows uses the same
   symphonia → ffmpeg Rust path; revisit only if Windows Opus/WMA import
   volume justifies a native decoder.
