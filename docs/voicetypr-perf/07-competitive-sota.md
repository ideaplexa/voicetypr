# Competitive + SOTA STT performance research — VoiceTypr perf teardown

> **Scope:** Source-verified external research — *how the fastest local-dictation tools and
> STT runtimes achieve speed + smoothness* — to arm every other perf slice with concrete,
> portable techniques. This is the one cross-cutting reference doc; each technique in §6 is
> tagged to the VT slice (hotpath/audio/whisper/parakeet/cloud/shell) that should fold it in.
>
> **Provenance:** VT repo `main` (HEAD `af63ab1`) for VT-side facts (`path:line`).
> whisper-rs 0.16.0 + vendored whisper.cpp read directly from the cargo registry at
> `~/.cargo/registry/.../whisper-rs-0.16.0/` and `whisper-rs-sys-0.15.0/whisper.cpp/`
> (whisper-rs 0.16.0 pins `whisper-rs-sys = "0.15"`, verified at
> `whisper-rs-0.16.0/Cargo.toml:88-89`). External techniques carry a URL + (where relevant)
> version/symbol. Benchmark numbers are quoted **as published by their source** and flagged
> with the hardware they were measured on; they are not VT measurements. Anything not directly
> observed is marked `[INFERENCE]`. "NEEDS MEASUREMENT" marks a number VT must produce itself.

## TL;DR — the five highest-leverage portable techniques

1. **VT ships with Metal **flash attention OFF**, even though whisper-rs 0.16 fully exposes it**
   (`WhisperContextParameters::flash_attn()`, `whisper-rs/src/whisper_ctx.rs:491`) and the vendored
   ggml Metal backend (whisper-rs-sys 0.15.0, which VT builds) dispatches `GGML_OP_FLASH_ATTN_EXT`
   (`ggml-metal-device.m:1069`, `ggml-metal-ops.cpp:425`) with 256 `flash_attn` kernels in
   `ggml-metal.metal`. VT's
   `transcriber.rs:57,78` builds `WhisperContextParameters::default()` and only calls `.use_gpu(true)`.
   **One-line toggle, zero C patch.** → slice **whisper**. [whisper.cpp PR #2152]
2. **VT never sets `audio_ctx`** (`whisper-rs/src/whisper_params.rs:252` is reachable; `transcriber.rs`
   never calls it), so every 2–8 s dictation pays the cost of a 30 s encoder window. `audio_ctx=768`
   (~halves the encoder, ~2× faster) for short clips. → slice **whisper**. [whisper.cpp discussion #297]
3. **Parakeet TDT on CoreML is ~110–155× real-time on M4 Pro** (`FluidInference/parakeet-tdt-0.6b-v3-coreml`;
   macparakeet README) — confirming plan 028's thesis that Parakeet short-dictation latency is *plumbing*,
   not decode. VT's app tail (`plans/028` Evidence A: ~450 ms + 500 ms restore) dwarfs the decode.
   → slices **parakeet + hotpath**.
4. **WhisperKit solved Whisper streaming** by making the **Audio Encoder natively streaming** and the
   **Decoder yield accurate text on partial audio** (ICML 2025 paper, arXiv:2507.10860) — this is the
   one thing whisper-rs/whisper.cpp *cannot* do (`WhisperState::full(&[f32])` takes a complete slice).
   The portable idea: **segmented decode-ahead + stable-prefix reduction** (see §2), not KV reuse.
   → slice **whisper**.
5. **VT's release profile leaves 20–30 % binary size and real runtime perf on the table** — no `lto`,
   no `codegen-units`, no `opt-level`, `strip="none"` (`src-tauri/Cargo.toml:126-133`). The conflict:
   VT keeps symbols deliberately for in-process Sentry symbolication. A *scoped* optimization profile
   (LTO + codegen-units=1, keep line-tables) is the safe middle. → slice **shell**. [Tauri "App Size"]

---

## 1. Competitor perf matrix — app → warmup/streaming/insertion/engine/default-model

| App | Engine | Default/best model | Streaming? | Warmup / cold-start | Insertion | Source |
|---|---|---|---|---|---|---|
| **Handy** (torn down) | whisper.cpp via `transcribe-cpp` crate | Whisper (Whisper-only product) | **Yes — real.** `StreamTextEvent{committed,tentative}`, append-only committed, two sibling spans. Idle mic = 1 `Relaxed` atomic load. Decode leases engine out of mutex. | idle-unload watcher frees GPU; `include_str!` baked catalog instant | **single paste at stop**, pill preview while speaking | VT Handy teardown `00-README.md`, `01`/`02`/`05` |
| **macparakeet** (`moona3k/macparakeet`) | Parakeet TDT 0.6B-v3 via **FluidAudio CoreML on ANE**; optional Nemotron streaming + WhisperKit | Parakeet v3 (multilingual) / v2 (EN-only) | **Yes** — Nemotron Speech Streaming EN 0.6B beta, configurable latency **80 ms–1120 ms** | first launch downloads ~465 MB CoreML + ~130 MB speaker assets; everything offline after | system-wide dictation, Swift/SwiftUI | [github.com/moona3k/macparakeet](https://github.com/moona3k/macparakeet); [macparakeet.com/blog/macparakeet-open-source](https://macparakeet.com/blog/macparakeet-open-source/) |
| **VoiceInk** (`beingpax/VoiceInk`) | **whisper.cpp bindings + FluidAudio Parakeet**; native Swift (AudioToolbox low-latency capture, Accessibility APIs for insert) | Whisper (tiny→large) **or** Parakeet | partial (whisper.cpp batch + Parakeet) | 100 % on-device by default; optional cloud AI-enhancement sends **text only** (user API keys) | system-wide, native Swift | [github.com/Beingpax/VoiceInk](https://github.com/Beingpax/VoiceInk); [tryvoiceink.com](https://tryvoiceink.com/) |
| **Superwhisper** | **whisper.cpp**, fully offline | exposes all Whisper sizes; **large-v3-turbo recommended** for Apple Silicon (≈8× faster than large-v3, fits 8 GB) | batch (paste final) | on-device; per-mode model selection | paste final result | [superwhisper.com/docs/models/voice](https://superwhisper.com/docs/models/voice); [todayonmac.com/superwhisper](https://www.todayonmac.com/superwhisper/) |
| **WhisperKit / Argmax** (engine, not a dictation app) | **CoreML on ANE** — recompiled Whisper encoder+decoder | large-v3-turbo optimized (1.6 GB → 0.6 GB) | **Yes — native.** Encoder "natively supports streaming inference"; decoder yields text on partial audio | ANE cold-compile paid on first inference; 1.3–1.8× over Metal on M3/M4 | n/a (framework) | [arXiv:2507.10860](https://arxiv.org/html/2507.10860v1); [github.com/argmaxinc/WhisperKit](https://github.com/argmaxinc/WhisperKit) |
| **Wispr Flow** | **cloud-only** (Baseten on AWS us-east-1; OpenAI/Anthropic/Cerebras subprocessors). No on-device mode at any price. | proprietary server models | streaming display | network-bound | paste; **<700 ms p99 claimed (Baseten)**, **~1–2 s practical** user-reported | [spokenly.app/blog/wispr-flow-review](https://spokenly.app/blog/wispr-flow-review); [wisprflow.ai/post/technical-challenges](https://wisprflow.ai/post/technical-challenges) |
| **Aqua Voice** | **cloud-only** (Avalon proprietary model, launched Aug 2025). No on-device. | Avalon | **Yes — streaming mode** ("words as the model transcribes") | **sub-50 ms startup** claimed | **450 ms–1 s text insertion**; streaming feels faster than chunk-paste | [aquavoice.com](https://aquavoice.com/); [getvoibe.com/resources/aqua-voice-review](https://www.getvoibe.com/resources/aqua-voice-review/) |

**What this tells VT:** the *local* leaders (Handy, VoiceInk, macparakeet) win on (a) streaming
contract + non-activating overlay, (b) **Parakeet/ANE for raw speed**, (c) native-Swift low-latency
capture. The *cloud* leaders (Wispr, Aqua) win on **streaming display of partials** but are
network-bound (450 ms–2 s). VT's hybrid (Whisper CPU/Metal + Parakeet ANE + cloud) can take the
**local speed ceiling** from Parakeet and the **streaming UX** from Handy — the two never combined in
a single competitor because every leader is engine-monoculture.

---

## 2. whisper.cpp / whisper-rs speed levers — technique → expected speedup → exposed in whisper-rs 0.16? → citation

> **Reading the "exposed?" column:** whisper-rs 0.16.0 splits params into **`WhisperContextParameters`**
> (set once at context creation — `use_gpu`, `flash_attn`, `gpu_device`, DTW) and **`FullParams`**
> (set per `state.full()` call). VT builds the context at `transcriber.rs:57` and the per-call params at
> `transcriber.rs:480-577`. The column states whether the lever is reachable **without patching C**.

| # | Technique | Expected effect | Exposed in whisper-rs 0.16? | Does VT use it? | Citation |
|---|---|---|---|---|---|
| 2.1 | **`set_audio_ctx(n)`** — trim encoder context from 1500 (=30 s) to a multiple of 64 | ~2× faster encoder at `audio_ctx=768` (~15 s); big win for 2–8 s clips (Whisper pads short audio to a 30 s window, so 3 s and 6 s decode in nearly the same time) | **YES** — `FullParams::set_audio_ctx`, `whisper_params.rs:252` (`self.fp.audio_ctx = audio_ctx`) | **NO** — VT never calls it (`transcriber.rs:480-577`) | [whisper.cpp discussion #297](https://github.com/ggml-org/whisper.cpp/discussions/297): "audio_ctx 768 → encoder ~2× faster; must be multiple of 64" |
| 2.2 | **Flash attention** (`flash_attn`) on Metal/CUDA | faster + lower-memory attention via IO-aware tiling; published as "faster processing on CUDA and Metal devices" | **YES** — `WhisperContextParameters::flash_attn(bool)`, `whisper_ctx.rs:491-494`; default `false` (`:477`). Metal kernels present (whisper-rs-sys 0.15.0, VT's build): `GGML_OP_FLASH_ATTN_EXT` dispatched in `ggml-metal-device.m:1069`/`ggml-metal-ops.cpp:425`, 256 `flash_attn` kernels in `ggml-metal.metal`; field on params at `whisper.cpp/include/whisper.h:118` | **NO** — `transcriber.rs:57,78` uses `.default()` + `.use_gpu(true)` only | [whisper.cpp PR #2152 / discussion #2155](https://github.com/ggml-org/whisper.cpp/discussions/2155) |
| 2.3 | **`large-v3-turbo`** (809 M, decoder pruned 32→4 layers) instead of `large-v3` (1.55 B) | **~4–7× faster**, +0.3–0.7 abs WER; ~8× real-time; turbo-int8 fits 1.5 GB | model swap (no API) — VT catalog already configurable | partial — VT supports multiple models; **default consideration** per plan 028 | [whispernotes.app/blog/introducing-whisper-large-v3-turbo](https://whispernotes.app/blog/introducing-whisper-large-v3-turbo); [distil-large-v3 (6.3×)](https://huggingface.co/distil-whisper/distil-large-v3) |
| 2.4 | **Greedy vs Beam search** | Greedy `{best_of:1}` ≈ 5× cheaper than `BeamSearch{beam_size:5}` | **YES** — `SamplingStrategy` enum (`whisper_params.rs:23-39`) | **CPU = Greedy, Metal = BeamSearch** (`transcriber.rs:480-488`). Note: `patience` is "Not implemented in whisper.cpp as of v1.7.6" (`whisper_params.rs` doc) so VT's `patience:-1.0` is a no-op | whisper.cpp default; whisper-rs `whisper_params.rs:34-37` |
| 2.5 | **`set_n_threads`** + ARM big.LITTLE tuning | leave 1 core for UI; cap perf-cores on aarch64 Windows | **YES** — `set_n_threads` (`whisper_params.rs:100`) | **YES** — `transcriber.rs:522-548` (hw−1, capped 4 on aarch64 Win) | whisper.cpp `-t`; VT already optimal here |
| 2.6 | **`set_single_segment`** + **`set_no_context`/`set_no_timestamps`** | avoids multi-segment + context overhead for short dictation | **YES** — `:149,135,142` | partial — VT sets `no_context`/`no_timestamps` only on CPU profile (`transcriber.rs:550-551`); **`single_segment` unused** | whisper-rs `whisper_params.rs:149` |
| 2.7 | **`set_segment_callback_safe`** for live partials | emit **completed segments during** `state.full()` → pill preview | **YES** — `SegmentCallbackData{segment,start,end,text}`, `whisper_params.rs:397-479` | **NO** (batch-only today) | whisper-rs `whisper_params.rs:422`; plan 028 Evidence C |
| 2.8 | **Built-in Silero VAD** (`set_vad_model_path`/`set_vad_params`) | VAD inside whisper.cpp; skip silence in decode | **YES** — `whisper_params.rs:834,847` (`WhisperVadParams`) | **NO** — VT does VAD in its own `audio/silence_detector.rs` instead | whisper-rs `whisper_params.rs:834` |
| 2.9 | **Quantization** (q5_0 / q5_K / q8_0 GGML) | q5 ≈ ½ size, small WER hit; q8 near-lossless; q5/Q8 flash kernels exist on Metal | model-file choice (whisper.cpp feature) | catalog-driven (model files) | Q4/Q5/Q8 `flash_attn_ext` Metal kernels in vendored `ggml-metal.metal` (whisper-rs-sys 0.15.0) |
| 2.10 | **`set_offset_ms`/`set_duration_ms`/`set_max_tokens`/`set_token_timestamps`** | bound decode window, cap runaway | **YES** — `:114,121,234,189` | mostly NO | whisper-rs `whisper_params.rs:114-243` |
| 2.11 | **KV-cache / encoder reuse (true streaming)** | incremental decode without re-encoding | **NO** — `WhisperState::full(&[f32])` takes a complete slice each call (`whisper_state/mod.rs:16-19`); no append-PCM API. Would require patching whisper.cpp C core | n/a — VT must use segmented decode-ahead (§2.12) | plan 028 Evidence C; VT `transcriber.rs:623` |
| 2.12 | **Segmented decode-ahead** (growing buffer → decode at min-window → emit all-but-last segment as final → advance head past emitted audio) | collapses post-stop latency by decoding *during* recording | implemented in VT's adapter layer on top of `full()` + `set_segment_callback_safe` | **NO** (future, plan 028 Phase 3) | reference: `itsmontoya/scribble` `src/backends/whisper/incremental.rs` (verified plan 028); [WhisperPipe arXiv:2604.25611](https://arxiv.org/pdf/2604.25611) |

**Cost of the `stream` example** (why VT must NOT copy it naively): whisper.cpp's
`examples/stream/stream.cpp` uses a **sliding window with overlap** — "the same audio samples are
included in multiple consecutive windows, repeatedly participating in encoding and decoding" and
"demands much higher GFLOPS"; "one cannot simply compose earlier processing results" because the
encoder–decoder cross-attention blocks partial-result reuse. [[WhisperFlow arXiv:2412.11272](https://arxiv.org/pdf/2412.11272);
[stream.cpp](https://github.com/ggml-org/whisper.cpp/blob/master/examples/stream/stream.cpp)].
**The correct portable pattern is `scribble`'s advance-head boundary (2.12), not the stream overlap.**

---

## 3. Parakeet / ANE levers

| Lever | Mechanism | Effect on VT | Source |
|---|---|---|---|
| **TDT architecture** (Token-and-Duration Transducer) | predicts tokens **and their durations in parallel** (not autoregressively one-at-a-time) | fundamentally lower latency than Whisper's autoregressive decoder; ~110–155× RTF on Apple Silicon makes decode near-free for short clips | [dicta.to/blog/nvidia-parakeet-mac-app](https://dicta.to/blog/nvidia-parakeet-mac-app/); [FluidInference/parakeet-tdt-0.6b-v3-coreml](https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v3-coreml) |
| **CoreML on ANE (not Metal, not MLX)** | routes encoder/decoder to the Neural Engine; GPU free for other work | `Parakeet TDT 0.6B v3 ~155× realtime, ~66 MB working memory per slot` (macparakeet). **MLX is 2.6× slower than CoreML** (Parakeet-MLX 0.4995 s vs Whisper-CoreML 0.1935 s/sample) | [macparakeet.com/agents](https://macparakeet.com/agents/); [arunbaby.com parakeet-vs-whisper](https://www.arunbaby.com/speech-tech/0073-whisper-vs-parakeet-asr-decision/) |
| **StreamingEOU 120M** | end-of-utterance signal via `<EOU>` token; cache-aware encoder+RNNT state; partial callbacks | **80 ms–160 ms** latency; the model plan 028 recommends for true low-latency Parakeet partials (new ~120 M CoreML download) | [nvidia/parakeet_realtime_eou_120m-v1](https://huggingface.co/nvidia/parakeet_realtime_eou_120m-v1); plan 028 Evidence C (~5×@160ms/~12×@320ms RTF) |
| **Nemotron streaming 0.6B** | configurable-latency streaming engine | 80 ms–1120 ms latency range; macparakeet ships this as beta (~600 MB EN-only) | [macparakeet releases](https://github.com/moona3k/macparakeet/releases) |
| **ANE warmup / cold-compile** | first-inference CoreML compile cost | VT pays this on first decode if preload hasn't finished (`plans/028` Evidence A); **macparakeet preloads ~465 MB on first launch** | plan 028 Evidence A; macparakeet README |
| **FluidAudio internal resample** | accepts any AVAudioFormat, resamples to 16 kHz mono Float32 internally | VT can ship native-rate PCM to the Swift sidecar and let FluidAudio convert — avoids VT's ffmpeg/rubato normalize step on the Parakeet path | plan 028 Evidence C; `FluidAudio .../Shared/AudioConverter.swift:7-34` |

**Key asymmetry for VT:** Parakeet is already ~instant for short clips on Apple Silicon — the
remaining latency is VT's **app plumbing** (~450 ms tail + 500 ms restore, plan 028 Evidence A), not
ANE inference. So for Parakeet users, **slice hotpath (Phase 0) beats any ANE micro-opt**. ANE lever
value is in *streaming* (live partials + long-form decode-ahead), not short-clip raw latency.

---

## 4. Cloud first-word / streaming latency table

| Provider | Transport (today VT) | Streams? | First-word / final latency (published) | Partial semantics | Source |
|---|---|---|---|---|---|
| **Deepgram** | REST raw-body `/v1/listen` (batch) | **Yes (WS)** | **~150 ms first-word**; ≤300 ms streaming; conversational <500 ms total | interim + `is_final`/`speech_final` | [deepgram.com/learn/streaming-speech-recognition-api](https://deepgram.com/learn/streaming-speech-recognition-api); [developers.deepgram.com/docs/measuring-streaming-latency](https://developers.deepgram.com/docs/measuring-streaming-latency) |
| **Soniox** | **REST async-poll, 1000 ms floor** (VT `soniox.rs`) | **Yes (WS)** | **249 ms median time-to-final-segment**, 281 ms P95, 310 ms P99 | token-by-token `is_final`, `max_non_final_tokens_duration_ms` | [soniox.com/benchmarks](https://soniox.com/benchmarks); [soniox.com/docs/.../real-time-latency](https://soniox.com/docs/speech-to-text/core-concepts/real-time-latency) |
| **OpenAI** | REST multipart (batch) | **Yes** — Realtime WS/WebRTC (`gpt-realtime-whisper`, tunable `delay`) + `stream=true` SSE on `gpt-4o-transcribe` | ~**500 ms time-to-first-byte** (US clients, Realtime API) | interim/final | [openai.com/index/introducing-gpt-realtime](https://openai.com/index/introducing-gpt-realtime/); [latent.space realtime-api](https://www.latent.space/p/realtime-api) |
| **Groq** | REST multipart (batch) | No | fast batch (~216× RTF), no interim | final only | plan 028 Evidence C; [groq](https://groq.com) |
| **Cohere** | REST multipart (batch) | No | hosted batch only | final only | plan 028 Evidence C |

**For VT:** the single clearest cloud win is **Soniox REST→WS** — removes the 1000 ms poll floor and
drops to a 249 ms median final. Deepgram WS adds ~150 ms first-word partials. These map cleanly to
VT's `committed`/`tentative` contract (provider finals → committed, interims → tentative).

---

## 5. Tauri app-shell perf levers

| Lever | Effect | VT today | Risk / constraint | Source |
|---|---|---|---|---|
| **`lto = true`** (fat LTO) | 20–30 % smaller binary; runtime perf gains from cross-crate inlining | **OFF** (VT `Cargo.toml:126-133` sets nothing) | **conflict:** VT keeps `debug="line-tables-only"` + `strip="none"` for in-process Sentry symbolication (Bugsink does *not* server-side symbolicate). LTO is symbol-safe; pair with kept line-tables | [Tauri "App Size"](https://v2.tauri.app/concept/size/); [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html) |
| **`codegen-units = 1`** | better LLVM optimization (serial crate compile) | **OFF** | slower release builds; symbol-safe | [v1.tauri.app/.../app-size](https://v1.tauri.app/v1/guides/building/app-size/) |
| **`opt-level`** ("s"/"z" size, 3 speed) | "z" smallest; "3" fastest | **unset** (=3 default for release) | opt-level=z trades speed for size — wrong for a hot decode path; prefer `opt-level=3` + `lto` | [Tauri App Size](https://v2.tauri.app/concept/size/) |
| **`panic = "abort"`** | ~200 KB smaller, cleaner crash logs, removes unwind tables | **OFF** | **conflict:** `panic=abort` breaks the `sentry` panic capture that VT relies on (`backtrace`+`panic` features). **Likely NOT safe for VT** without changing crash strategy | [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html); VT `Cargo.toml:81` sentry panic feature |
| **`strip`** | 20–30 % size cut | **"none"** (intentional — symbol table needed for macOS name resolution) | **do NOT strip** — conflicts with the documented symbolication strategy (`Cargo.toml:127-131`) | VT `Cargo.toml:132` |
| **Brotli asset compression** (frontend) | Tauri compresses embedded HTML/CSS/JS | on by default | tiny frontend → measurable only if bundle large | [oflight.co.jp Tauri v2 perf](https://www.oflight.co.jp/en/columns/tauri-v2-performance-bundle-size) |
| **`removeUnusedCommands` (Tauri 2.4+ ACL)** | dead-code-eliminate commands not in ACL | unknown | reduces surface + size | [Tauri 2.4 release notes](https://v2.tauri.app/concept/size/) |
| **IPC cost** | `invoke` is a serialized bridge; high-rate events are costly | VT emits mic-level via broadcast `emit`; pill level via channel | Handy lesson: gate high-rate channels by cached atomic + `emit_to`, skip when overlay off (teardown `02`) | VT Handy teardown `02-overlay-ux.md` |

**Recommendation for VT:** the **only safe shell win** is `lto = true` + `codegen-units = 1` (keep
`debug="line-tables-only"`, keep `strip="none"`, do **not** add `panic="abort"`). Expect 20–30 %
binary reduction + modest decode-loop speedup from inlining, zero symbolication regression. Needs a
build-time + startup smoke test. → slice **shell**. **NEEDS MEASUREMENT** on VT's actual binary.

---

## 6. Top 10 portable techniques ranked by expected impact for VT

> Each row tagged to the VT slice that should own it. Impact is *expected for VT's hot path*
> (not the source's headline), graded by the evidence above. "Reachable w/o C patch" =
> implementable on whisper-rs 0.16 without editing whisper.cpp.

| Rank | Technique | Expected impact for VT | Effort | Risk | Reachable w/o C patch? | VT slice(s) |
|---|---|---|---|---|---|---|
| **1** | **Enable Metal flash attention** (`ctx_params.flash_attn(true)` at `transcriber.rs:57`) | faster Metal decode + lower GPU mem on every Whisper call (exact % NEEDS MEASUREMENT; "faster" per PR #2152) | **S** (1 line) | Low — but **incompatible with DTW** (`whisper_ctx.rs:464`); VT doesn't use DTW so safe. Validate WER unchanged | **YES** | **whisper** |
| **2** | **`set_audio_ctx` for short clips** (e.g. 768 for <15 s) | ~2× faster encoder for the common 2–8 s dictation; removes 30 s-window padding cost | **S** | Med — must be multiple of 64; gate by audio length; validate WER on trimmed context | **YES** | **whisper** |
| **3** | **Phase 0: kill the fixed plumbing tail** (non-blocking clipboard restore, CGEvent paste, in-process normalize-first) | removes ~450 ms + 500 ms = ~950 ms of pure plumbing *every engine, every time* (plan 028 Evidence A) | **S/M** | Low (some already landed per oracle doc) | n/a | **hotpath** (+audio) |
| **4** | **Default to / promote `large-v3-turbo`** | ~4–7× faster Whisper than large-v3, +0.3–0.7 WER | **S** (catalog default) | Low (user can override) | n/a (model choice) | **whisper** |
| **5** | **Soniox REST → WebSocket** | removes 1000 ms poll floor → 249 ms median final; native partial/final | **M** | Med (auth/reconnect) | n/a | **cloud** |
| **6** | **`lto=true` + `codegen-units=1`** release profile (keep line-tables, no panic=abort) | 20–30 % smaller binary + decode-loop inlining gains | **S** | Low (symbol-safe); verify Sentry still resolves + startup time | n/a | **shell** |
| **7** | **Whisper segmented decode-ahead** (scribble-style advance-head + `set_segment_callback_safe`) | collapses post-stop latency by decoding *during* recording (long-form especially) | **L** | High (WER regressions, cancel/hard-timeout, lease) | **YES** (on `full()`) | **whisper** |
| **8** | **Parakeet StreamingEOU 120M** (live partials + EOU) | 80–160 ms streaming partials; true Handy-parity live preview on macOS | **L** | Med/High (new CoreML asset, activation, sidecar session) | n/a (FluidAudio) | **parakeet** |
| **9** | **Deepgram live WS** (interim + `is_final`) | ~150 ms first-word partials for cloud users | **M** | Med | n/a | **cloud** |
| **10** | **`single_segment` + tune beam (Greedy on Metal too / beam_size tuning)** | smaller decode cost for short single-utterance dictation; removes dead `patience` | **S** | Low (validate WER) | **YES** | **whisper** |

---

## Learn-from-others — the portable principles (not vibes)

- **WhisperKit's two breakthroughs** (arXiv:2507.10860): (1) the **Audio Encoder "natively supports
  streaming inference"** and the **Decoder yields accurate text on partial audio** — i.e. they
  *architecturally* solved what plain whisper.cpp cannot (no KV reuse); (2) a **compression**
  retaining WER within 1% while shrinking 1.6 GB → 0.6 GB. Neither is a whisper-rs API — but the
  *pattern* (streaming encoder + committed/partial split) is exactly VT's planned decode-ahead.
- **faster-whisper / CTranslate2** wins are **GPU/CUDA + int8** (~2× faster, 40% less VRAM; CT2 CUDA
  more optimized than whisper.cpp CUDA). [[SYSTRAN/faster-whisper](https://github.com/SYSTRAN/faster-whisper)]
  **This does NOT transfer to VT's macOS Metal / CPU paths** — int8 CT2 kernels are CUDA-specific.
  The transferable idea: aggressive **quantization** (q5/q8 GGML, already a Metal flash path) + the
  data point that decode is dominated by the *encoder context size* (hence #2 `audio_ctx`).
- **macparakeet's stack** (Swift 6 + FluidAudio CoreML, "no Electron, no web views, no Python
  subprocesses", `~66 MB/slot`, `~155× RTF`) is the **direct OSS competitor** and the proof that
  Parakeet-on-ANE is the local speed ceiling. [[github.com/moona3k/macparakeet](https://github.com/moona3k/macparakeet)]
- **Handy's perceptual wins** are *structural*, not tuning: append-only committed + two sibling spans
  (no relayout), `emit_to` + cached atomic for high-rate channels, engine leased out of mutex. (VT
  Handy teardown `00`/`01`/`02`.) These are owned by slices hotpath/shell, not this doc.
- **The `stream` example is a cautionary tale, not a template**: sliding-window overlap re-encodes
  overlap every step ("much higher GFLOPS", WhisperFlow arXiv:2412.11272). VT must use the
  **advance-head** boundary instead.

## Measurement plan (to prove each win)

| Technique | Instrument | Before/after metric |
|---|---|---|
| Flash attention (#1) | `transcriber.rs:604` `inference_start` span already logs `WHISPER_INFERENCE` ms | decode ms & peak GPU mem on fixed 5 s/15 s clips, flash on vs off; WER on acceptance corpus |
| `audio_ctx` (#2) | same span | decode ms at audio_ctx ∈ {0, 512, 768, 1024} for 2 s/5 s/15 s clips; WER delta |
| Phase 0 tail (#3) | stop→insert span log (oracle doc Step 1) | p50/p95 stop→paste on Parakeet + Whisper 2 s/5 s/15 s |
| turbo (#4) | decode ms + WER | large-v3 vs turbo decode ms + WER |
| Soniox WS (#5) | first-final-partial timestamp | REST-poll vs WS first-text ms; final WER |
| LTO (#6) | release binary size + cold start + decode ms | msi/dmg size; app launch ms; decode ms; Sentry resolves names? |
| Decode-ahead (#7) | decode-start-relative-to-stop timestamp | post-stop wait ms (long-form 30 s/120 s); WER vs batch |
| EOU (#8) | first-partial + first-stable-prefix ms | p50 ≤700 ms first partial gate (oracle doc Q2) |

## Open questions / risks

1. **Flash-attn WER on VT's Metal path** — published as "faster", but VT-specific WER on its model set
   NEEDS MEASUREMENT before default-on. Incompatible with any future DTW word-timestamps feature.
2. **`audio_ctx` trimming accuracy** — trimming context can drop trailing words on noisy/tail-speech
   clips; must be length-gated and paired with VT's never-lose-speech invariant (plan 015). [INFERENCE:
   safe lower bound likely ≥ clip-length-rounded-to-64; validate.]
3. **VT Parakeet numbers unmeasured** — all Parakeet RTF figures here are third-party (FluidInference,
   macparakeet). VT must baseline its own FluidAudio sidecar stop→insert p50/p95 before claiming
   "decode is free" (oracle doc Step 1).
4. **LTO + Sentry symbolication** — LTO is symbol-safe, but a full release build + crash must be
   verified to still resolve native names in-process (VT's whole symbolication model depends on it).
5. **faster-whisper int8 does not help VT** — do not chase it; CT2's CUDA kernels are irrelevant to
   macOS Metal/CPU. The transferable lever is quantization (#q5/q8 GGML) + audio_ctx.
6. **Aqua/Wispr latency is network-bound** — their streaming-*feel* comes from streaming display of
   partials, replicable by VT's pill preview without any cloud dependency.

---

*End of `07-competitive-sota.md`. Cross-references: VT Handy teardown `00-README.md` + `06-oracle-decision.md`;
plan `028-transcription-latency-streaming.md`. Every external technique carries a URL; whisper-rs 0.16
exposure is checked per lever in §2.*
