# Voicetypr performance roadmap pressure-test review

> Review target: `docs/voicetypr-perf/00-MASTER-ROADMAP.md` plus supporting slices `01`-`05`, `07`, streaming context `../handy-teardown/06-oracle-decision.md`, and spot-checks in VT source. This is a pressure test, not a rubber stamp.

## 1. Verdict on the Tier-1-first thesis

**Verdict: mostly correct, but mis-framed as “free + safe.”** The roadmap is right that the next user-visible wins should precede the streaming build: the old ~950 ms paste tail is stale, Parakeet short dictation is now dominated by remaining app tail/first-use warmup rather than decode, and Whisper is decode-bound with reachable knobs (`audio_ctx`, turbo, flash-attn) (`01-hotpath-latency.md:9`, `04-parakeet-engine.md:10-17`, `03-whisper-engine.md:12-18`, `00-MASTER-ROADMAP.md:31-43`). However, the batch should be described as **measurement-gated low-effort wins**, not “safe/free.” Flash-attn is not transcript-identical and has a documented non-English/large-v3-turbo quality concern; `audio_ctx` can silently truncate tail speech if the floor/pad is wrong; Parakeet warm-on-preload only kills the first-prediction ANE compile **after** sidecar spawn and model load have already happened; and LTO is broad but release-process work, not an app-latency fix (`07-competitive-sota.md:13-15`, `07-competitive-sota.md:75-80`, `05-cloud-and-shell.md:183-189`). I would keep “instrument first, then Tier 1 before streaming,” but reorder the Tier-1 batch and promote a couple of “Tier 3 lightweight app” items into the first app-performance tranche.

---

## 2. Disagreements / re-prioritizations

### P1 — Flash-attn is not “free + safe”; it is “cheap + must be WER-gated.”

**Roadmap claim:** T1.1 says enable Metal flash-attention, risk low, incompatible only with DTW, verify WER unchanged (`00-MASTER-ROADMAP.md:35`). `07` confirms it is exposed in whisper-rs 0.16, default off, with Metal kernels present, and VT only calls `use_gpu(true)` today (`07-competitive-sota.md:19-25`, `07-competitive-sota.md:76`). Source spot-check agrees: `Transcriber::new` builds `WhisperContextParameters::default()` and toggles only GPU/CPU (`src-tauri/src/whisper/transcriber.rs:57,78,101,156-172`).

**Pressure-test:** DTW is **not** the only risk. There is a real transcript-output risk from numerical differences. A 2025 whisper.cpp issue reports `--flash-attn` on macOS Metal + `large-v3-turbo` producing deterministic but different Japanese output with less punctuation and more typos; maintainer response says results are expected not to be perfectly identical and notes large-v3/large-v3-turbo instability/VAD sensitivity (<https://github.com/ggml-org/whisper.cpp/issues/3020>). That is directly relevant because VT’s proposed default also promotes `large-v3-turbo` (`00-MASTER-ROADMAP.md:39`, `07-competitive-sota.md:77`).

**Decision:** ship the code path early, but **do not call it “safe default” until VT’s corpus passes**. Gate by model/language slice, not only overall WER. Include Japanese/non-English, punctuation-heavy, quiet/noisy, and short tail-word cases. Treat “English degradation much less noticeable” as insufficient for a system-wide dictation app.

**Recommended priority:** P1 as a guarded experiment/feature flag; P1 default-on only after WER corpus passes. If the corpus is English-only, default-on should be scoped to English or “fast mode” until broader coverage exists.

---

### P1 — `audio_ctx` is the biggest Whisper win, but the roadmap understates tail-speech risk and over-simplifies the floor.

**Evidence:** `set_audio_ctx` is never used; every 2–8 s clip pays the native 1500-token / 30 s audio context (`03-whisper-engine.md:12-15`, `03-whisper-engine.md:80-83`, `07-competitive-sota.md:26-28`, `07-competitive-sota.md:75`). Source spot-check confirms duration is computed only after params are already largely configured, and no `params.set_audio_ctx(...)` appears before `state.full(...)` (`src-tauri/src/whisper/transcriber.rs:480-623`).

**Roadmap risk wording:** T1.2 says length-gate, WER sweep, floor ctx (`00-MASTER-ROADMAP.md:36`). The slice proposes a floor of 64 tokens (~1.28 s) plus 15% margin (`03-whisper-engine.md:192` and deep-dive text around its floor recommendation).

**Pressure-test:** The risk is not only “if ctx < clip length.” The dangerous cases are exactly the ones users notice: short commands with the semantic payload in the last 200–500 ms, quiet speakers where gate/duration estimates undercount usable tail, and clips just above the 0.5 s app gate (`01-hotpath-latency.md:98-103`, `01-hotpath-latency.md:141-143`). A 64-token floor is too aggressive for product default. It leaves little room for pad, endpointing slop, and final phoneme/punctuation behavior. Also, `no_context`/`single_segment` interactions should be measured separately: VT currently sets `no_context` and `no_timestamps` only on CPU profile, while Metal keeps context/timestamps (`03-whisper-engine.md:73-75`, `03-whisper-engine.md:113-115`, `07-competitive-sota.md:80`). Changing `audio_ctx` plus `no_context`/`single_segment` in one rollout would confound WER.

**Decision:** make `audio_ctx` P1, but ship a conservative mapping first:

- round **up** to a multiple of 64;
- `needed = ceil((duration_seconds + tail_pad_seconds) * 50 * margin)`;
- use at least **0.5–1.0 s absolute tail pad** plus 15–25% relative margin;
- initial production floor should be closer to **256 tokens (~5.1 s)** than 64 for dictation-default safety [INFERENCE: safer product floor; exact floor must be corpus-gated];
- cap at 1500;
- run WER and “last word present” assertions over 0.5/1/2/3/5/8/12/20/29 s clips.

This sacrifices some of the theoretical 3 s speedup, but still cuts wasted 30 s-window work materially while honoring plan 015 never-lose-speech.

---

### P1 — Parakeet warm-on-preload is real, but it is not the whole “first-use” story.

**Roadmap claim:** T1.3 says warm Parakeet on preload kills ~0.3–1.5 s first-dictation ANE compile and is “the single biggest first-use win” (`00-MASTER-ROADMAP.md:37`). The Parakeet slice says VT load sites send `LoadModel` but none run a warm prediction, and FluidAudio prewarms arrays but not first `prediction()` / ANE program compile (`04-parakeet-engine.md:64-75`). Source spot-check agrees: startup autoload calls `parakeet_manager.load_model(...)`, not a warm decode (`src-tauri/src/lib.rs:1886` area); Swift `loadModel` runs `AsrModels.loadFromCache` and `manager.loadModels(models)` but no transcription until `transcribeFile` (`sidecar/parakeet-swift/Sources/main.swift:369-388`, `sidecar/parakeet-swift/Sources/main.swift:460-500`).

**Pressure-test:** “Warm-on-preload” is biggest **only if preload actually completed before the first hotkey and the sidecar process/model are already resident**. True cold first-use can include:

1. Swift sidecar spawn (`src-tauri/src/parakeet/sidecar.rs:121-140`),
2. model cache load from disk / CoreML compiled artifact resolution (`main.swift:369-388`),
3. `AsrManager` construction/load,
4. then first prediction ANE compile.

The Parakeet slice itself lists path-IPC, cold ANE compile, and whole-request `RwLock` serialization as the three surrounding costs (`04-parakeet-engine.md:10-17`). The roadmap should split the item into **P1a eager sidecar spawn + load selected model** and **P1b warm decode after load**. Warm decode alone does not help if the first command lazily spawns/loads.

**Decision:** keep warm decode as P1, but rename the item to **“Parakeet eager-resident preload + warm prediction.”** Instrument and report separate spans: `sidecar.spawn_ms`, `model.load_from_cache_ms`, `manager.loadModels_ms`, `first_prediction_ms`, `warmup_prediction_ms`, and `first_real_decode_ms`.

---

### P1/P2 — LTO is worthwhile, but it should not be in the same “instant app latency” batch as audio_ctx/warmup.

**Evidence:** `[profile.release]` keeps line tables and symbols for Bugsink/Sentry and does not set LTO/codegen-units (`src-tauri/Cargo.toml:126-133`, `05-cloud-and-shell.md:126-135`). The slice argues `lto="thin"` + `codegen-units=1` is symbol-safe and gives binary −5–15%, hot path −2–8%, but raises link time 30–80% and requires forced panic symbolication smoke (`05-cloud-and-shell.md:172`, `05-cloud-and-shell.md:183-189`, `05-cloud-and-shell.md:214-216`, `05-cloud-and-shell.md:224`, `05-cloud-and-shell.md:234`).

**Pressure-test:** This is a good release-profile hardening item, not the same class as one-line decode knobs. It requires a release build, binary-size comparison, startup/decode smoke, and a crash-path smoke because symbolication is a hard product constraint (`00-MASTER-ROADMAP.md:38`, `00-MASTER-ROADMAP.md:109`). It is broad but likely marginal for Metal/ANE-dominated paths; the Whisper slice estimates near-zero on Metal and ~5–15% on CPU/Windows (`03-whisper-engine.md:195-197` includes LTO table row context; `07-competitive-sota.md:136-140`).

**Decision:** P2 or “release gate after P1 code wins,” unless the team already has release-smoke automation. Do not block audio_ctx/Parakeet warm/stop-join/log fixes behind LTO. Use `lto="thin"`, not fat `lto=true`, unless measured; keep `panic=unwind`, `strip="none"`, `debug="line-tables-only"`.

---

### P1 — Turbo default is acceptable as a speed-first default, but do not call its WER equivalent to `large-v3`.

**Evidence:** VT catalog marks both `large-v3` and `large-v3-turbo` recommended with accuracy 9; turbo wins mainly by size/speed tiebreak (`03-whisper-engine.md:124-136`). External/slice data says turbo is ~4–7× faster with +0.3–0.7 absolute WER (`07-competitive-sota.md:77`, `00-MASTER-ROADMAP.md:39`).

**Pressure-test:** “Accuracy=9 for both” is a product bug if the UX implies no tradeoff. Turbo is the right dictation default for speed, and competitors recommend it on Apple Silicon (`07-competitive-sota.md:52`), but users doing noisy, multilingual, long-form, or accuracy-critical work should understand that `large-v3` may still win slices.

**Decision:** P1: make turbo the explicit **speed-first dictation default** with copy like “recommended for everyday dictation; large-v3 may be better for hardest audio.” Keep override. The WER corpus should compare turbo vs large-v3, but this should not block presenting turbo as default for latency-sensitive dictation if the fallback remains easy.

---

### P1/P2 — Some “Tier 3 lightweight” work should move up because it affects app feel more than the last 10–20 ms of paste-tail polishing.

**Roadmap placement:** bundle, lazy WebViews, idle watchers are Tier 3 (`00-MASTER-ROADMAP.md:65-68`).

**Evidence:** The shell slice shows the pill/toast WebViews are built synchronously in setup, each WKWebView/WebView2 creation is expensive, and three WebViews remain resident (`05-cloud-and-shell.md:144`, `05-cloud-and-shell.md:153`). The pill loads 118 KB plus a 190 KB globals chunk for a 3-dot overlay; toast is 1.1 KB, proving the floor is tiny (`05-cloud-and-shell.md:156-165`). DeviceWatcher polls CoreAudio every 1.5 s and RecorderWatchdog probes every 250 ms (`05-cloud-and-shell.md:151-152`).

**Pressure-test:** The last paste-tail trims (15 ms duplicate settle, 20 ms pill-hide sleep, 15 ms inter-event sleep) are risky because they need cross-app focus/paste QA (`01-hotpath-latency.md:145`, `00-MASTER-ROADMAP.md:52-54`, `06-oracle-decision.md:162`). By contrast, slimming the always-visible pill bundle and removing the Radix metapackage are low-risk display-only improvements (`05-cloud-and-shell.md:172-176`, `05-cloud-and-shell.md:191`, `05-cloud-and-shell.md:205`). Users will feel “lightweight” through startup, overlay responsiveness, idle CPU, and memory—not just stop→paste ms.

**Decision:** Promote **pill micro-bundle + Radix metapackage removal** to P1 app-feel tranche. Keep DeviceWatcher backoff P2 because of hotplug SLA risk, and lazy pill WebView P2 because first-recording indicator correctness must be verified (`05-cloud-and-shell.md:235-236`).

---

### P1/P2 — Soniox adaptive poll is missing from the roadmap’s first wave if cloud is part of “core service.”

**Evidence:** Soniox REST has a fixed 1000 ms poll floor; adaptive 250 ms early poll recovers ~400–600 ms p50 while REST remains (`05-cloud-and-shell.md:7`, `05-cloud-and-shell.md:71`, `05-cloud-and-shell.md:101-102`, `05-cloud-and-shell.md:111`). `07` says Soniox WS drops to 249 ms median final and maps naturally to committed/tentative (`07-competitive-sota.md:121`, `07-competitive-sota.md:126-128`).

**Pressure-test:** The roadmap puts Soniox WS in Tier 2 and omits adaptive REST as a near-free interim (`00-MASTER-ROADMAP.md:56`). If cloud usage is meaningful, adaptive poll is a more user-visible “free” win than, for example, LTO. WS still belongs to streaming/contract work; adaptive poll can ship sooner with low risk.

**Decision:** P1 if Soniox is recommended or used by a material user segment; otherwise P2.

---

## 3. Missing cross-cutting opportunities the six slices do not cover sharply enough

### P1 — Hot-path logging allocation and log I/O budget

**Finding:** `log_with_context` allocates a `Vec<String>`, formats each key/value, joins into a `String`, and then calls the log macro; it does not early-return on `log::log_enabled!(level)` (`src-tauri/src/utils/logger.rs:262-270`). Many call sites pass `log::Level::Debug` in hot-ish setup/record/decode paths, but release logging level is Info, so those allocations can happen even when the message is filtered (`src-tauri/src/lib.rs:323-326`). The plugin writes to stdout and a rotating log file in release (`src-tauri/src/lib.rs:315-321`).

**Expected impact:** [INFERENCE] Usually sub-ms per transcription, but it can pollute latency measurements, add allocator churn around decode/start/stop, and matter on repeated event paths. It is a better P1 “free” cleanup than risky paste-sleep removal.

**Owner:** shell/hotpath.

**Action:** Add `if !log::log_enabled!(level) { return; }` before allocating in `log_with_context`; downgrade non-essential release hotpath logs to Debug or sampled; keep errors and coarse performance spans. Verify before/after stop→paste and log file write volume.

---

### P1/P2 — Tokio runtime and blocking-pool control

**Finding:** `tokio = { version = "1.46.0", features = ["full"] }` is used (`src-tauri/Cargo.toml:42`), and VT uses Tauri’s async runtime plus many `tauri::async_runtime::spawn` and `tokio::task::spawn_blocking` calls across decode, normalize, clipboard, remote HTTP, CLI, startup timers (`src-tauri/src/commands/audio.rs:1443`, `src-tauri/src/commands/audio.rs:4929`, `src-tauri/src/commands/text.rs:123`, `src-tauri/src/lib.rs:515,629,749,755,803,833,1120`). There is no visible runtime worker-thread/max-blocking configuration in app code.

**Expected impact:** [INFERENCE] Not a single-call speedup if idle, but important under concurrent work: normalize + Whisper decode + clipboard + log cleanup + startup/model tasks can compete for the default blocking pool and async workers. Mis-sized blocking work can cause tail latency spikes even when average is fine.

**Owner:** backend architecture/shell.

**Action:** Add spans/counters around blocking-pool tasks and queue wait (where possible), and consider a dedicated STT blocking executor/semaphore for CPU decode/normalization so UI, clipboard, and command handling are not starved. Do not tune blindly; use p95 under concurrent startup/preload/record scenarios.

---

### P1/P2 — Whisper/Parakeet model mmap/load-time strategy

**Finding:** Whisper context creation uses `WhisperContext::new_with_params(model_path_str, ctx_params)` with defaults; no mmap/mlock choice is documented in the roadmap (`src-tauri/src/whisper/transcriber.rs:57,101,172`). Parakeet loads from cache via FluidAudio `AsrModels.loadFromCache` and then `manager.loadModels(models)` (`sidecar/parakeet-swift/Sources/main.swift:369-388`). The docs focus on decode warmup but not the model-residency/load mechanism.

**Expected impact:** [INFERENCE] Per-dictation impact is zero after cache, but first-use and memory pressure impact can be hundreds of ms and GB-scale. This directly affects whether Parakeet warm-on-preload is felt, and whether Whisper cache preload is worth its 1–3 GB RSS (`05-cloud-and-shell.md:143-154`).

**Owner:** whisper + parakeet + shell.

**Action:** Instrument model load separately from first inference. Investigate whisper.cpp/whisper-rs mmap/mlock/default behavior for 0.16 before changing anything; if exposed, measure load time, RSS, page faults, and first decode. For Parakeet, treat CoreML `.mlmodelc` load/cache as a black box until measured; do not assume mmap helps.

---

### P1/P2 — Metal warmup is separate from Parakeet ANE warmup

**Finding:** The roadmap warms Parakeet but not macOS Whisper Metal. VT preloads Whisper into cache or warms the Windows Vulkan sidecar, but the Metal path may still pay first real `state.full()` kernel compilation/cache effects [INFERENCE based on GPU runtime behavior]. `07` notes WhisperKit/ANE cold compile and Parakeet ANE warmup, but VT’s Metal warm path is not explicit (`07-competitive-sota.md:53`, `07-competitive-sota.md:106`).

**Expected impact:** [INFERENCE] Potentially tens to hundreds of ms on first Whisper dictation after model load; not recurring after warm. Worth measuring because it is a first-use product feel issue analogous to Parakeet.

**Owner:** whisper.

**Action:** After `TranscriberCache::get_or_create` preload, optionally run a tiny synthetic/silence decode on Metal behind an idle/preload guard. Measure first real `WHISPER_INFERENCE` before/after; WER unaffected because the warm result is discarded.

---

### P2 — `n_threads` tuning on Apple Silicon, not only ARM64 Windows

**Finding:** VT uses `available_parallelism() - 1` for non-Windows-aarch64 (`src-tauri/src/whisper/transcriber.rs:522-548`; summarized in `03-whisper-engine.md:69-70`, `03-whisper-engine.md:197`). That includes efficiency cores on Apple Silicon and may not be optimal for short clips or UI responsiveness.

**Expected impact:** [INFERENCE] ±5–15% and p95/thermal improvements; could be negative if over-tuned. More relevant on CPU fallback/Intel/Windows than Metal, but still worth controlled measurement.

**Owner:** whisper.

**Action:** Add a benchmark sweep by model/clip length/thread count. Do not hard-code M4 Pro; derive or expose a setting/auto-tune cache.

---

### P2 — Parakeet JSON IPC and dead fields are small for batch but toxic for streaming if left as-is

**Finding:** Parakeet IPC is newline-delimited JSON over stdin/stdout, and `Transcribe` carries a file path, never bytes (`04-parakeet-engine.md:26-33`, `04-parakeet-engine.md:60-62`; source `sidecar.rs:159-165`, `messages.rs:42-62`). Rust sends load/transcribe-shaped config fields that Swift ignores in `LoadModel` (`04-parakeet-engine.md:146-164`; source `src-tauri/src/parakeet/manager.rs:307-318`, `sidecar/parakeet-swift/Sources/main.swift:239-250`).

**Expected impact:** Batch JSON overhead is sub-ms; path re-open/re-decode is ~1–5 ms for hot short files [INFERENCE from slice], but the architecture becomes fatal for streaming because N chunks would mean N file churns (`04-parakeet-engine.md:196-219`).

**Owner:** parakeet/streaming.

**Action:** P2 for batch hygiene (remove/wire dead fields), P1 prerequisite for streaming (length-prefixed PCM/binary side-channel). Do not include binary side-channel in the first “stop→text” batch unless streaming work starts.

---

### P2 — Global allocator experiment, not default-by-faith

**Finding:** No `global_allocator`, mimalloc, jemalloc, or snmalloc was found in `src-tauri/src` or `src-tauri/Cargo.toml` (targeted grep returned no matches). VT allocates around `WhisperState::create_state`, log formatting, JSON, and audio normalization; the slice already notes fresh state allocs may cost 5–30 ms (`03-whisper-engine.md:196`).

**Expected impact:** [INFERENCE] Potentially smoother p95 and lower allocator overhead in decode/log/JSON-heavy paths; could be neutral or worse on macOS where system malloc is strong and CoreML/Apple frameworks may interact best with default allocator.

**Owner:** shell/whisper.

**Action:** Benchmark mimalloc and jemalloc behind a branch/profile only: decode p50/p95, app RSS, startup, idle, Parakeet sidecar unaffected. Do not ship without macOS notarization/package smoke and crash-symbolication smoke.

---

### P2 — Sentry overhead: probably not hot-path, but keep it out explicitly

**Finding:** VT initializes Sentry only when opted in and with no JS plugin/envelope IPC; panic hook is chained so Sentry captures panics (`src-tauri/src/lib.rs:370-382`, `src-tauri/src/lib.rs:466-503`). Sentry dependency uses `backtrace`, `panic`, `transport`, `rustls` (`src-tauri/Cargo.toml:81`).

**Expected impact:** [INFERENCE] No evidence of per-dictation Sentry overhead unless breadcrumbs/events are added later. The risk is future: adding breadcrumbs around every hotpath event would hurt latency and I/O.

**Owner:** shell/telemetry.

**Action:** Add a guardrail in the roadmap: no Sentry breadcrumbs/events on audio callback, per-frame level events, or per-token/partial streaming. Panic capture only; explicit perf review for any telemetry in stop→paste/decode spans.

---

### P2 — Live mic WAV round-trip and 16 kHz capture should not be buried as late footprint work

**Evidence:** The audio slice says the live mic path writes native WAV, reads it, writes normalized 16 kHz WAV, deletes raw, and engine reads again: 2 writes + 2 reads + unlink + f32→i16→f32 quantization, estimated 4–10 ms on SSD and more on slow/network FS (`02-audio-pipeline.md:18-23`, `02-audio-pipeline.md:108-118`). Capture-at-16 kHz mono can skip resample and cut capture file size ~6× when supported (`02-audio-pipeline.md:23-25`).

**Pressure-test:** The roadmap puts memory PCM handoff at T3.5 (`00-MASTER-ROADMAP.md:69`). That is fair if judged only by stop→paste ms, but it is more than footprint: it improves quality by removing quantization churn and creates the clean seam needed by streaming. It should be P2 substrate, not Tier 3 after cosmetic app footprint.

**Owner:** audio/streaming substrate.

---

### P3 — `opt-level=3` is already the default; document it to prevent a size-optimization regression

**Evidence:** Release profile does not set `opt-level`, so Cargo release defaults to 3 (`05-cloud-and-shell.md:135`, `07-competitive-sota.md:138`).

**Decision:** Do **not** add `opt-level="z"` for app size while chasing perf. If the roadmap includes release profile edits, explicitly set or document `opt-level=3` to prevent a future Tauri-size checklist from trading away decode speed.

---

## 4. Sharper guardrail risk calls

1. **WER bar must be per lever and per slice, not one aggregate pass.** Flash-attn, `audio_ctx`, turbo default, greedy/beam, `no_context`, and `single_segment` can each change output. Test them independently and then in combination. Include non-English, quiet speaker, noisy room, tail-word, punctuation, proper nouns, and 0.5–2 s clips. The streaming oracle already requires WER delta gates (`06-oracle-decision.md:54-58`, `06-oracle-decision.md:164`, `06-oracle-decision.md:359`).

2. **Flash-attn “safe” must be downgraded.** Use “DTW-incompatible and transcript-non-identical; default only after corpus pass.” The GitHub issue above is enough to reject “DTW only.”

3. **`audio_ctx` is a plan-015 risk.** A bad floor silently loses speech/tail words. The guard should include last-word assertions and raw/normalized duration mismatch checks, not just average WER (`00-MASTER-ROADMAP.md:108`, `03-whisper-engine.md:192`).

4. **LTO symbolication is likely safe but not assumed safe.** The roadmap correctly says smoke-verify; strengthen it to “release cannot ship until forced panic resolves native function + file:line in Bugsink/Sentry on macOS and Windows.” Keep `panic=unwind`; `panic=abort` is banned because Sentry panic capture relies on unwind/panic hook (`src-tauri/src/lib.rs:466-503`, `07-competitive-sota.md:139`, `05-cloud-and-shell.md:187`).

5. **Focus/paste sleeps are not “free tail.”** The 20 ms pill-hide sleep and paste sleeps protect cross-app focus/clipboard behavior (`01-hotpath-latency.md:145`, `01-hotpath-latency.md:DD-3/DD-4` via lines around `audio.rs:5561` and `text.rs:671`). Do not remove them before a focus-retain assertion and app matrix. The streaming oracle calls overlay focus steal a P1 trap (`06-oracle-decision.md:162`, `06-oracle-decision.md:252`).

6. **Plan 008: the level-meter allocation fix is correctness, not latency.** It belongs in the first batch because it closes the only documented RT callback allocation leak, but do not oversell user-visible latency (`02-audio-pipeline.md:13-18`, `00-MASTER-ROADMAP.md:41`).

7. **Never remove ffmpeg; only remove common-path spawn.** The roadmap already got this right. Keep ffmpeg for Ogg-Opus/WMA/HE-AAC/exotics (`00-MASTER-ROADMAP.md:73-85`, `02-audio-pipeline.md:24-28`).

8. **Streaming sequencing guardrail:** Do not let perf work undermine the decided streaming substrate: FIFO Feed/Finalize/Cancel, session/generation gating, committed/tentative, final paste once, no audio-callback emits (`06-oracle-decision.md:156-164`, `06-oracle-decision.md:211-224`, `06-oracle-decision.md:234-252`).

---

## 5. Final recommended Tier-1 batch

### Ship first as the Tier-1 performance batch

1. **Instrumentation spans first (P1, prerequisite).** Add stop→paste/decode/load/warm spans for 2/5/15 s Parakeet + Whisper and WER corpus harness; every other claim is currently flagged NEEDS MEASUREMENT (`00-MASTER-ROADMAP.md:95`, `06-oracle-decision.md:371-376`).

2. **Stop-thread join poll 100 ms → 5/10 ms (P1, keep).** Biggest zero-risk recurring Parakeet tail cut; pure wakeup latency, failure cap unchanged (`00-MASTER-ROADMAP.md:40`, `01-hotpath-latency.md:73-81`, `01-hotpath-latency.md:141`).

3. **Parakeet eager-resident preload + warm prediction (P1, modified).** Replace “warm on preload” with “spawn sidecar + load selected model + warm 1 s silence after load”; measure spawn/load/first-prediction separately (`00-MASTER-ROADMAP.md:37`, `04-parakeet-engine.md:64-75`, `src-tauri/src/lib.rs:1886` area, `main.swift:369-388`).

4. **Length-adaptive Whisper `audio_ctx`, conservative default (P1, modified).** Ship after WER/tail-word gate; multiple-of-64, cap 1500, floor initially ~256 not 64 [INFERENCE], 0.5–1.0 s tail pad + 15–25% margin (`03-whisper-engine.md:12-15`, `03-whisper-engine.md:80-83`, `03-whisper-engine.md:192`).

5. **Explicit `large-v3-turbo` speed-first dictation default (P1, keep with better UX copy).** Do not rely on size tiebreak; label tradeoff and keep large-v3 override (`03-whisper-engine.md:124-136`, `07-competitive-sota.md:77`).

6. **Flash-attn implementation behind WER-gated default (P1, modified).** Add the toggle path early, but default only after per-language/model WER pass; document non-identical outputs and the Japanese large-v3-turbo issue (`07-competitive-sota.md:19-25`, `07-competitive-sota.md:76`, GitHub issue #3020).

7. **Close level-meter allocation in CPAL callback (P1, keep).** Bounded `try_send`/atomic path; this is plan-008 compliance, not a latency headline (`02-audio-pipeline.md:13-18`, `00-MASTER-ROADMAP.md:41`).

8. **Guard `log_with_context` allocations + reduce release hotpath Info logging (P1, add).** This is a true near-free cross-cutting perf hygiene item; current helper allocates before log filtering and release writes stdout+file (`src-tauri/src/utils/logger.rs:262-270`, `src-tauri/src/lib.rs:315-326`).

9. **Pill micro-bundle + Radix metapackage cleanup (P1 app-feel add).** Move up from Tier 3: 308 KB JS for a tiny always-visible overlay is more “app feels heavy” than the final 10 ms paste sleep; display-only low risk (`05-cloud-and-shell.md:156-166`, `05-cloud-and-shell.md:172-176`, `05-cloud-and-shell.md:191`).

10. **Adaptive Soniox REST polling if cloud is material (P1 conditional / P2 otherwise, add).** Cheap 400–600 ms p50 cloud win while WS waits for streaming contract (`05-cloud-and-shell.md:101-102`, `05-cloud-and-shell.md:111`).

### Remove/defer from the first batch

1. **Move LTO/codegen-units out of the same P1 app-latency batch (P2/release gate).** Worth doing, but only after release crash-symbolication smoke; not a stop→paste or decode-knob unblocker (`05-cloud-and-shell.md:183-189`, `05-cloud-and-shell.md:214-216`, `00-MASTER-ROADMAP.md:109`).

2. **Defer risky paste/focus sleep cuts until after focus instrumentation (P2).** Duplicate settle/inter-event/pill-hide sleep cuts can save 15–45 ms but require cross-app QA and overlay focus proof (`01-hotpath-latency.md:145`, `00-MASTER-ROADMAP.md:52-54`, `06-oracle-decision.md:162`).

3. **Defer binary PCM Parakeet side-channel to streaming substrate (P2/P1-for-streaming, not Tier-1 stop→text).** Batch win is a few ms; streaming prerequisite is real (`04-parakeet-engine.md:12-18`, `04-parakeet-engine.md:196-219`).

4. **Defer DeviceWatcher/RecorderWatchdog backoff to P2.** Real idle win, but hotplug SLA/test matrix makes it less “near-free” (`05-cloud-and-shell.md:151-152`, `05-cloud-and-shell.md:174`, `05-cloud-and-shell.md:235`).

5. **Do not ship aggressive `audio_ctx` floor=64 or flash-attn default-on without corpus.** Both are exactly the kind of “safe” perf toggle that can create silent transcription regressions.

---

## 6. Critical files the implementer must read before changing anything

- Roadmap and source evidence:
  - `docs/voicetypr-perf/00-MASTER-ROADMAP.md` — current tiers and guardrails (`00-MASTER-ROADMAP.md:31-43`, `00-MASTER-ROADMAP.md:93-109`).
  - `docs/voicetypr-perf/01-hotpath-latency.md` — re-baselined stop→text tail and paste/focus risks (`01-hotpath-latency.md:9`, `01-hotpath-latency.md:141-145`).
  - `docs/voicetypr-perf/02-audio-pipeline.md` — plan-008 callback status and WAV/ffmpeg facts (`02-audio-pipeline.md:13-28`).
  - `docs/voicetypr-perf/03-whisper-engine.md` — decode knobs, WER risks, source lines (`03-whisper-engine.md:12-21`, `03-whisper-engine.md:192-197`).
  - `docs/voicetypr-perf/04-parakeet-engine.md` — sidecar/load/warm/IPC lifecycle (`04-parakeet-engine.md:10-18`, `04-parakeet-engine.md:26-62`, `04-parakeet-engine.md:64-75`).
  - `docs/voicetypr-perf/05-cloud-and-shell.md` — LTO, bundle, idle, Soniox poll (`05-cloud-and-shell.md:7-8`, `05-cloud-and-shell.md:151-179`, `05-cloud-and-shell.md:183-229`).
  - `docs/voicetypr-perf/07-competitive-sota.md` — external technique table and measurement caveats (`07-competitive-sota.md:13-15`, `07-competitive-sota.md:75-86`, `07-competitive-sota.md:136-145`).
  - `docs/handy-teardown/06-oracle-decision.md` — streaming invariants and sequencing (`06-oracle-decision.md:156-164`, `06-oracle-decision.md:211-224`, `06-oracle-decision.md:352-365`).

- Source spot-checks:
  - `src-tauri/src/whisper/transcriber.rs` — context params, `FullParams`, `n_threads`, duration, `state.full`.
  - `src-tauri/src/parakeet/manager.rs` — `LoadModel`, `Transcribe`, dead config fields, path payload.
  - `src-tauri/src/parakeet/sidecar.rs` — spawn, newline JSON, whole-request `RwLock` guard.
  - `sidecar/parakeet-swift/Sources/main.swift` — JSON event loop, load-from-cache/loadModels, transcribe file path.
  - `src-tauri/src/lib.rs` — Whisper preload, Parakeet autoload, logging, Sentry/panic hook, WebView setup.
  - `src-tauri/src/commands/audio.rs` — normalization-before-gate, pill-hide sleep, delivery generation gates.
  - `src-tauri/src/audio/recorder.rs` — stop-thread join poll.
  - `src-tauri/src/utils/logger.rs` — unconditional allocation in `log_with_context`.
  - `src-tauri/Cargo.toml` — release profile, tokio features, sentry/log deps.

---

## Single highest-ROI action

**Build the fixed latency + WER harness and use it immediately to ship the conservative Whisper `audio_ctx` rollout.** That one action converts the largest recurring Whisper decode win from “risky theory” into a measured product default: it can save hundreds of ms on common short dictations, and the same harness gates flash-attn, turbo default, Parakeet warmup, and paste-tail polish without violating the never-lose-speech/WER guardrails.