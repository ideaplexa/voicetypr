# Cloud STT network path + App shell — VoiceTypr perf teardown

> Scope: two perf surfaces in one doc. **(A) Cloud STT** (`src-tauri/src/cloud_stt/`) — provider HTTP path, the Soniox REST 1 s poll floor, client pooling, timeout/retry. **(B) App shell** — `[profile.release]`, startup, idle CPU/memory, frontend bundle. Streaming *architecture* is settled in `06-oracle-decision.md` and is NOT re-derived here; this doc quantifies the network/shell perf layer and cites `06` for the Soniox→WS streaming decision.

## TL;DR

- **Cloud:** the HTTP client is already correctly pooled (a single `LazyLock<reqwest::Client>` reused everywhere), and the Soniox 1 s poll floor is the **one dominant, quantifiable latency tax** on that provider — ~500 ms expected + up to 1 s worst-case *detection* delay added on top of the provider's own processing time, on every Soniox job (`soniox.rs:196`,`390`). WS streaming removes it entirely (cite `06` Q4/Step 6); while REST stays, an adaptive poll cadence recovers ~400 ms p50 on short clips for ~zero risk.
- **Shell:** the release profile ships **no LTO and no codegen-units override** despite a Sentry/Bugsink symbolication constraint that only forbids *stripping* — adding `lto="thin"` + `codegen-units=1` is free perf (binary −5…−15 %, hot path −2…−8 %) **with zero symbolication regression**, because line tables and the symbol table are governed by `debug`/`strip`, not by LTO. The biggest *idle* CPU surprise is **not** the trigger engine (it is event-driven, ~0 % CPU) but two always-on OS-poll threads — a 1.5 s CoreAudio device enumeration and a 250 ms state probe — plus three resident WebViews and a 1–3 GB resident model.

---

## A. Cloud STT network path

### A.1 Current-state map

**Shared, pooled client (good — verified, not assumed).** All five providers share one process-wide reqwest client built once:

```rust
// src-tauri/src/cloud_stt/common.rs:100-121
fn build_client(timeout: std::time::Duration) -> reqwest::Client {
    let builder = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(std::time::Duration::from_secs(15));
    #[cfg(not(test))]
    let builder = builder.https_only(true);
    builder.build().unwrap_or_else(|_| reqwest::Client::new())
}
static SHARED_CLIENT: std::sync::LazyLock<reqwest::Client> =
    std::sync::LazyLock::new(|| build_client(REQUEST_TIMEOUT));
pub(super) fn http_client() -> reqwest::Client {
    SHARED_CLIENT.clone()   // cheap Arc clone; same connection pool
}
```

Every transcription call takes it via `common::http_client()` — e.g. Soniox `let client = common::http_client();` (`soniox.rs:68`), and Deepgram/OpenAI/Groq route through `common::openai_compatible_transcribe`/`deepgram::transcribe_at` which use the same shared client. So **connection reuse + TLS session resumption + HTTP/2 multiplexing are already in place**: reqwest 0.12.22 (resolved in `src-tauri/Cargo.lock`, with `hyper-rustls` + `hyper-tls` + `rustls` + `quinn` deps) keeps `http2` on by default and negotiates it via ALPN; Soniox / OpenAI / Groq / Deepgram all serve h2. `[INFERENCE]` h3/QUIC (`quinn`) is linked but reqwest only uses it behind the `http3` feature, which is **not** enabled in `Cargo.toml:43` — so today it is h1/h2 only.

**One client NOT pooled.** API-key *validation* builds a throwaway client per call:

```rust
// src-tauri/src/cloud_stt/common.rs:202-209
pub(super) async fn get_validate(...) -> Result<(), SttError> {
    let client = build_client(validate_request_timeout());   // fresh client every validation
    ...
}
```

Validation is a once-per-settings-change tiny GET, so the cost is one extra TLS handshake — low frequency, but it is the one place pooling is bypassed.

**Soniox REST = upload → create → POLL → fetch transcript.** The poll is a fixed-cadence busy-wait:

```rust
// src-tauri/src/cloud_stt/soniox.rs:158-199  (identical block at soniox.rs:356-393 for diarized)
let status_url = format!("{}/transcriptions/{}", BASE, transcription_id);
let started = std::time::Instant::now();
let timeout = std::time::Duration::from_secs(180);
loop {
    let resp = common::with_retry(|| { /* GET status_url */ }).await?;
    ...
    match status {
        "completed" => break,
        "error"     => return Err(common::SttError::Server),
        _ => {
            if started.elapsed() > timeout { return Err(common::SttError::Timeout); }
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;  // <- the 1s floor
        }
    }
}
// then a 4th round-trip to GET .../transcript  (soniox.rs:201-220)
```

So even a job that finishes in 50 ms incurs: create RTT + (≥1 poll RTT) + up to **1000 ms sleep** before the completion is noticed + transcript RTT. The poll cadence is **flat** — 1 s whether the job is 0.1 s old or 179 s old — and there is no early-exit on a provider-supplied ETA.

**Timeouts.** `REQUEST_TIMEOUT = 30 min` is the per-request client deadline (`common.rs:88`), deliberately matched to the executor's 30 min watchdog so the *caller's* budget, not reqwest, bounds long uploads. `VALIDATE_TIMEOUT = 30 s` (`common.rs:98`). `connect_timeout = 15 s` (`common.rs:104`). The Soniox loop has its own 180 s ceiling (`soniox.rs:161`) independent of the 30 min client.

**Retry.** `with_retry` is one retry, flat 400 ms backoff, no jitter, no exponent, and it ignores any `Retry-After`:

```rust
// src-tauri/src/cloud_stt/common.rs:157-170
pub(super) async fn with_retry<T, F, Fut>(mut op: F) -> Result<T, SttError> ... {
    match op().await {
        Ok(v) => Ok(v),
        Err(e) if is_transient(&e) => {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            op().await
        }
        Err(e) => Err(e),
    }
}
```

`is_transient` = `RateLimited | Timeout | Network | Server` (`common.rs:65-70`). So a 429 from Groq/OpenAI retries once after a flat 400 ms even though providers publish `Retry-After`.

**Connection pre-warm exists but is narrow.** `warm_origin` does a fire-and-forget `HEAD origin` with an 8 s timeout (`common.rs:124-130`), wired through `CloudProvider::warm_up()` (`mod.rs:120-122`) and invoked only on a guarded path when a remote provider is active and has a key (`audio.rs:3923-3925`). It does **not** run at app launch for the configured provider.

**No streaming capability flag.** `ProviderCapabilities` (`provider_capabilities.rs:19-26`) carries `shareable_remote`, `supports_initial_prompt`, `supports_structured_terms`, `supports_vocabulary_terms`, `supports_translate_task` — **no** `supports_streaming`. So nothing in the capability model today distinguishes a REST-poll provider (Soniox) from a WS-capable one, which is exactly the seam `06` Step 2 says to add before the cloud WS vertical.

### A.2 Opportunities — Cloud

| # | Opportunity | Mechanism (concrete) | Est. impact | Effort | Risk | Invariant touched |
|---|---|---|---|---|---|---|
| C1 | **Soniox→WS streaming** (the real fix) | Add Soniox WS adapter emitting committed/tentative events; keep REST as fallback. Per `06` Q4/Step 6. | Removes the **entire** poll floor: first partial under the 1 s floor, no upload+poll+fetch RTT chain. Est. **−500 ms p50 / −1000 ms p95** detection on short clips + live partials. | M | Med (auth/reconnect/error taxonomy) | 015 never-lose-speech: REST stays as fallback |
| C2 | **Adaptive Soniox poll cadence** (REST stays) | Replace flat 1000 ms with a schedule: 250 ms for first ~6 polls, grow to 1000 ms after; honor a provider ETA/`retry_after` if present. | **−400…−600 ms p50** detection on short jobs (the common case); ~0 on long jobs. | S | Low | none — pure sleep tuning |
| C3 | **Honor `Retry-After` + jittered exponential backoff** | In `with_retry`, parse `Retry-After` on 429/503; backoff `400ms · 2^n` capped ~8 s with ±25 % jitter; bump max retries to 2–3 for `Network`/`Server`. | On rate-limit storms: turns 1 flat retry into graceful recovery; **avoids dropping valid jobs** under transient 429. Latency-neutral when healthy. | S | Low | none |
| C4 | **Pre-warm configured provider at launch** | Call `provider.warm_up()` for the *selected* engine in the existing startup spawn, not only on the `audio.rs:3923` conditional. | Saves one **TLS handshake (~100–300 ms)** off the first transcription of a session. | S | Low | none — fire-and-forget |
| C5 | **Pool the validation client** | Reuse `SHARED_CLIENT` but override timeout per-request (`client.get(url).timeout(VALIDATE_TIMEOUT)`); drop the per-call `build_client`. | −1 TLS handshake per validation; trivial absolute, removes a small footgun. | S | Low | none |

> **C1 is owned by the streaming decision (`06`).** This doc quantifies its floor cost (C2 is the REST-while-transitioning mitigation). C2–C5 are network-layer only and do not touch streaming.

### A.3 Deep-dive — top cloud opportunities

**C2 — adaptive poll cadence (do this today, regardless of WS).** The 1 s floor is the single largest *self-inflicted* cloud latency. Quantification: completion is detected on the *next* poll after the provider finishes, so expected detection delay = `interval/2` and worst case = `interval`. At 1000 ms that is **+500 ms mean / +1000 ms p95** added to every Soniox job before the transcript is even fetched. Soniox async jobs for short clips typically complete in a few hundred ms, so the floor frequently dominates the provider's own time. An adaptive schedule (start 250 ms, the cadence a caller actually wants for fresh jobs, then back off as the job ages) recovers most of that with zero behavioral risk: the 180 s loop ceiling (`soniox.rs:161`) and the `with_retry` wrapper are untouched. **Verify:** instrument `started.elapsed()` at `completed` break (`soniox.rs:187`) and histogram the *wasted* time = `elapsed − provider_finish_time` across 2 s / 10 s / 60 s clips before/after.

**C3 — backoff policy.** The flat 400 ms / single retry is harmless when healthy and inadequate under load. Two concrete fixes: (1) on `SttError::RateLimited` parse the response `Retry-After` (currently `log_http_body` discards the body, `common.rs:172-184` — the header is on the `Response` and reachable before the body is consumed); (2) jittered exponential so two concurrent Soniox polls that both hit a 503 don't retry in lockstep. **Verify:** wiremock test asserting retry count and inter-attempt delay distribution under 429-with-`Retry-After` and 503.

**C5 — pool validation.** `get_validate` is the only caller that bypasses `SHARED_CLIENT`. Reqwest lets you set a per-request `timeout()` that overrides the client default, so the 30 s validation deadline can stay without a bespoke client. One-line behavioral-equivalent change; the existing `VALIDATE_TIMEOUT_OVERRIDE` test seam (`common.rs:146-155`) keeps working because it feeds `validate_request_timeout()` which is passed to the per-request `.timeout()`.

---

## B. App shell (startup / memory / bundle / idle / release profile)

### B.1 Current-state map

**`[profile.release]` — `src-tauri/Cargo.toml:126-133`:**

```toml
[profile.release]
# Client-side crash symbolication for Bugsink (which does NOT symbolicate native
# server-side): line tables give function + file:line resolved in-process at
# capture (sentry `backtrace`); strip = "none" keeps the symbol table so macOS
# resolves names. Windows additionally emits voicetypr.pdb ...
debug = "line-tables-only"
strip = "none"
```

There is **no** `lto`, **no** `codegen-units`, **no** `opt-level`, **no** `panic` override. So today: `opt-level = 3` (cargo default), `lto = off`, `codegen-units = 16` (default), `panic = unwind`. The comment correctly identifies that *symbolication* depends on `debug` (line tables → `file:line` resolved in-process by sentry's `backtrace` feature) and `strip` (keeps the symbol table so the host resolves names), plus a Windows `.pdb` shipped beside the exe.

**Startup — `lib.rs` `setup()` runs synchronously before first paint.** Ordered, perf-relevant steps:
1. Panic hook chained in front of sentry's (`lib.rs:460-504`) — cheap.
2. macOS `ActivationPolicy::Accessory` (`lib.rs:548`).
3. Whisper/Parakeet managers constructed and `manage()`d (`lib.rs:581-597`) — cheap struct init, no model I/O.
4. `TranscriberCache`, `GpuSidecarClient`, `RemoteServerManager` managed (`lib.rs:605-622`).
5. **Trigger engine started synchronously**: `crate::trigger::engine_host::start_engine(...)` + `rebuild_engine_bindings(...)` (`lib.rs:1100-1101`). `start_engine` spawns the source + dispatcher threads and **blocks setup up to 5 s** on a readiness channel (`engine.rs:329` `ready_rx.recv_timeout(Duration::from_secs(5))`) — normally returns in low-ms once the CGEventTap is installed.
6. **Model preload is correctly async** — `tauri::async_runtime::spawn` (`lib.rs:1120`), so the 1–3 GB GGML load (hundreds of ms) does **not** block first paint. Falls back Vulkan sidecar warm → `TranscriberCache::get_or_create` (`lib.rs:1131-1156`).
7. **Pill + toast WebViews built synchronously in setup** — `WebviewWindowBuilder.build()?` for the pill (`lib.rs:1220`, macOS then `to_panel()` at `1224`) and toast (`lib.rs:1265`, `to_panel()` at `1271`). Each `build()` instantiates a WKWebView (macOS) / WebView2 (Windows) — the single most expensive per-window operation in setup.
8. A handful of deferred `tauri::async_runtime::spawn`s with fixed sleeps: log cleanup (`lib.rs:515`), 1500 ms wait-for-frontend (`lib.rs:755-757`), 2000 ms network-sharing autostart (`lib.rs:629-630`), autostart sync (`lib.rs:1279`).

The main window itself is created hidden (`tauri.conf.json:22 "visible": false`) — good; VT is a menubar app and the main UI is lazy-shown via `show_main_window` (`lib.rs:58-65`).

**Idle cost — what actually runs forever:**
- **Trigger engine is event-driven, not polled.** The macOS backend is an active `CGEventTap` on a CFRunLoop thread (`crates/keytrigger/src/backend/macos.rs:1`); the dispatcher blocks on `rx.recv()` (`engine.rs:289`) and only wakes on a real key event. Idle CPU ≈ 0. **The "always-on trigger engine" is not the idle cost.**
- **`DeviceWatcher` — a 1.5 s CoreAudio poll thread.** `thread::spawn` loop calling `AudioRecorder::get_devices()` (CPAL → CoreAudio enumeration) every `Duration::from_millis(1500)` (`device_watcher.rs:99-170`, sleep at `170`). Started for returning users once mic permission is granted (`lib.rs:832-835`). This is the **heaviest always-on poll** — device enumeration touches the CoreAudio HAL ~0.67 Hz, plus on every real change it emits + spawns a tray rebuild.
- **`RecorderWatchdog` — a 250 ms state probe.** `thread::spawn` loop (`recorder_watchdog.rs:43-84`, sleep at `82`) calling `get_recording_state(&app)` every 250 ms (4 Hz wake). When not recording it deliberately avoids the recorder mutex (`recorder_watchdog.rs:49-61`) — cheap per tick, but a constant timer wake.
- **Three resident WebViews.** main (hidden), pill (**`visible(true)`** at `lib.rs:1210`, always showing the 3-dot indicator), toast (hidden, `lib.rs:1257`). Each WKWebView process costs ~20–50 MB RSS even idle.
- **Resident model.** `TranscriberCache` `MAX_CACHE_SIZE = 1` (`cache.rs:10`), preload at startup → **1–3 GB resident** for the duration of the session (the cache comment is explicit: "1-3GB per model", `cache.rs:9`).

**Frontend bundle — `vite.config.ts` + `package.json` + measured `dist/assets/`:**
- Three rollup entries (`vite.config.ts:18-26`): `main`, `pill`, `toast`. **No `manualChunks`, no `build.target`, no minify options** — all defaults.
- Measured production bundle (built `dist/assets/*.js`, minified, not gzipped):
  - `main-DdXnVFps.js` **406 KB**
  - `globals-DLOvlvsR.js` **190 KB** (shared vendor chunk: react/radix/zustand)
  - `pill-xKAB42yI.js` **118 KB** ← outlier for a 3-dot indicator
  - `updateService-*.js` 43 KB, `ShareStatsModal-*.js` 4.8 KB, `toast-*.js` **1.1 KB**
  - `globals.css` 85 KB
  - **Total JS ≈ 763 KB**
- The pill is heavy because its dependency chain is not slim: `pill.tsx` → `SettingsProvider` (`src/contexts/SettingsContext.tsx`, pulls `invoke`/`listen`/`createLogger`) and `RecordingPill` → `PillShell` → `usePillController` (`useSetting` + `useRecording`). So the 80×40 px pill loads `globals(190 KB) + pill(118 KB) ≈ 308 KB` of JS. `toast.tsx` is 1.1 KB — proof the floor is near-zero when an overlay avoids the settings/recording context.
- Heavy deps: `radix-ui` **metapackage** `1.4.3` (`package.json:53`) alongside individual `@radix-ui/react-*` (`package.json:36-37`) — the metapackage is a known tree-shaking footgun; `lucide-react` (`package.json:51`); `@fontsource-variable/geist`.

### B.2 Opportunities — Shell

| # | Opportunity | Mechanism (concrete) | Est. impact | Effort | Risk | Invariant touched |
|---|---|---|---|---|---|---|
| S1 | **`lto="thin"` + `codegen-units=1`** | Add to `[profile.release]`. Keep `debug="line-tables-only"`, `strip="none"`, `panic` default. | Binary **−5…−15 %**; hot-path (decode/audio) **−2…−8 %** runtime via cross-crate inlining. | S | Low | Symbolication **unaffected** (see B.3) |
| S2 | **Slim the pill to a micro-bundle** | Cut the pill off `SettingsProvider`/`useRecording`; read only the events it renders (`audio-level`, recording state) via a tiny `listen()` — mirror the `toast.tsx` 1 KB floor. | **pill −90 %+** (118 KB → ~10–15 KB), faster pill WKWebView parse + lower idle RSS. | M | Low | 008/015 none — pill is display-only |
| S3 | **Coalesce/back off the device poll** | `DeviceWatcher`: lengthen interval to 3–5 s after a stable baseline; or drive off CoreAudio device-change notifications instead of polling. | Removes a **constant 0.67 Hz CoreAudio HAL enumeration** — the main idle CPU/thermal contributor. | M | Med (must not miss hotplug) | none |
| S4 | **Lazy-create the toast (and pill) WebView** | Build the toast WebView on first show, not in `setup()`; build the pill lazily too if `show_pill_indicator` is off. | Removes **1–2 WKWebView process launches** from startup (~50–150 ms each) + their idle RSS. | M | Med (pill must exist before first recording) | 008 audio path untouched |
| S5 | **`radix-ui` metapackage → per-primitive** | Drop `radix-ui@1.4.3` (`package.json:53`); keep only the `@radix-ui/react-*` actually used. | main/globals **−tens of KB**; better tree-shaking. | S | Low | none |
| S6 | **`manualChunks` + `build.target`** | Split vendor (react/radix) from app; set `build.target` to current Safari/WebView baseline. | Better long-term caching; smaller per-entry parse. | S | Low | none |
| S7 | **Watchdog 250 ms → event/condvar** | Replace `RecorderWatchdog`'s fixed 250 ms poll with a notify on state transition (or lengthen to 1 s when idle). | Removes a 4 Hz wake; minor CPU/battery. | M | Low | none — keep auto-stop semantics |
| S8 | **Optional resident model** | Make preload opt-in (or unload after N minutes idle), keeping the fast warm path. | Reclaims **1–3 GB RSS** when idle for users who open the app but rarely dictate. | M | Med (first-dictation cost returns) | 015 never-lose-speech: only affects speed, not correctness |

### B.3 Deep-dive — top shell opportunities

**S1 — release profile, reconciled with the symbolication constraint.** This is the headline shell win and it is safe. The constraint documented at `Cargo.toml:127-131` is precisely scoped: **Bugsink does not server-side symbolicate**, so native names + `file:line` must resolve *in-process* at capture time. That requires (a) the **symbol table** (so `strip = "none"` — already set) and (b) **line tables** (so `debug = "line-tables-only"` — already set). Neither is a function of LTO or codegen-units:

- `lto = "thin"` performs cross-crate MIR/LTO inlining and dead-code elimination at link time. It does **not** remove symbols or line tables. With `strip = "none"` + `debug = "line-tables-only"` unchanged, the sentry `backtrace` capture resolves exactly as today. The macOS host symbol resolution and the Windows `.pdb` path are untouched.
- `codegen-units = 1` lets the optimizer see the whole crate at once (more inlining, better vectorization in the whisper/cpal hot loops). It does not emit fewer symbols.
- **Explicitly do NOT add `panic = "abort"`.** With `panic = "abort"`, a panic kills the process before sentry's async transport can flush the event reliably, and it removes unwind tables the chained panic hook path (`lib.rs:460-504`) may rely on for clean teardown. Sentry's `panic` feature is already in the dep graph (`Cargo.toml:81`, `features=["backtrace","panic",...]`) and assumes unwind. Keep the default `unwind`.

Cost to flag: `lto="thin"` + `codegen-units=1` raise **link time ~30–80 %** (not incremental compile time — the per-CGU compile is unchanged; the hit is at final link). For a CI release build that is an acceptable trade; for `cargo run`-style local release iteration it is noticeable, so gate behind the existing release profile only. **Verify:** (1) build a release binary before/after, diff `wc -c` of the `.app/Contents/MacOS/Voicetypr` binary; (2) force a panic in a debug-rel-build and confirm Bugsink still resolves `file:line`; (3) run the existing whisper decode benchmark and compare tokens/s.

**S2 — the pill is 118 KB for three dots.** The pill is the always-visible overlay and it loads the full settings/recording context graph. `toast.tsx` (1.1 KB) is the existence proof that an overlay can be near-zero. The pill renders: animated dots, audio bars, and a status derived from `audio-level` + recording state — all of which arrive as Tauri events. Decoupling it from `SettingsProvider`/`useRecording` (push only the rendered state to it via `listen`) takes the pill bundle toward the toast floor. Win compounds with S4: a tiny pill is cheaper to spin up lazily. **Verify:** build, diff `dist/assets/pill-*.js`; visually QA the pill across recording states.

**S3 — the device poll is the real idle CPU.** Characterization: the trigger engine (often assumed to be the always-on cost) is event-driven and ~0 % idle; the actual constant work is `DeviceWatcher` enumerating CoreAudio every 1.5 s (`device_watcher.rs:170`). Two safe mitigations: (1) exponential-ish backoff — 1.5 s for the first ~10 polls after a change, then 5 s while stable, snap back to 1.5 s on any change; (2) longer term, subscribe to CoreAudio device-change notifications (`kAudioHardwarePropertyDevices` via `AudioObjectAddPropertyListener`) and drop the poll to a slow liveness check. The invariant is "don't miss a hotplug" — the backoff variant preserves detection latency right after a change, when it matters. **Verify:** log poll cadence + change-detection latency over a USB hotplug sequence before/after.

---

## Learn-from-others

(Companion doc `07-competitive-sota.md` carries the full competitive study; network/shell-specific techniques called out here.)

- **Connection pooling + HTTP/2 multiplexing.** Every low-latency STT client (Deepgram/OpenAI SDKs, Soniox's own SDK) reuses a single HTTP client and leans on h2 multiplexing so a warm connection serves many requests. VT already does this for transcription (`common.rs:116-121`) — competitive here. The gap is the **validation** client (C5) and **launch-time pre-warm** (C4); Deepgram's SDK pre-warms on client construction.
- **Poll vs stream.** Async/poll providers (Soniox Files API, AssemblyAI) all document a recommended initial poll interval of ~300 ms growing to 1 s — exactly C2. The Soniox **streaming** WS API exists precisely because poll-floor detection latency is unacceptable for interactive use; `06` Q4/Step 6 makes the same call for VT.
- **Retry policy.** Stripe/aws SDK convention: jittered exponential backoff + `Retry-After` honored on 429/503, capped attempts. VT's flat 400 ms/single-retry (`common.rs:157-170`) is the minimal version; the convention is C3.
- **Release profile.** `lto="thin"` + `codegen-units=1` is the documented default for latency-sensitive Rust apps (ripgrep, fd, hyper servers) that still need stack traces; symbol info is governed by `debug`/`strip`, not LTO — exactly the reconciliation in S1.
- **Overlay micro-frontends.** Tools that ship a tiny always-on overlay (Raycast, HandBrake's mini-windows) keep the overlay bundle to single-digit KB by isolating it from the app's state graph; VT's toast (1.1 KB) already follows this, the pill (118 KB) does not (S2).

## Measurement plan

| Metric | Where to instrument | Before/after |
|---|---|---|
| Soniox wasted detection time (C2/C1) | `soniox.rs:187` — log `started.elapsed()` and provider finish time at `completed` | histogram p50/p95 over 2 s/10 s/60 s clips |
| First-transcription TLS cost (C4/C5) | reqwest `Connection` reuse counter; log `warm_up` HEAD | ms to first byte, before/after launch warm |
| Retry behavior (C3) | wiremock 429/503 harness asserting attempt count + inter-attempt delay | retry distribution, `Retry-After` honored |
| Release binary size (S1) | `wc -c` on `Voicetypr.app/Contents/MacOS/Voicetypr` | bytes; confirm `file` still lists symbols |
| Symbolication regression (S1) | force a panic in a rel-with-extra-LTO build | Bugsink resolves `file:line` == baseline |
| Decode throughput (S1) | existing whisper benchmark (tokens/s, ms/clip) | tokens/s delta |
| Pill bundle + parse (S2) | `dist/assets/pill-*.js` size + WebView `did-finish-load` timing | KB + ms to first frame |
| Idle CPU (S3/S7) | `powermetrics`/Instruments on the idle app for 60 s | % CPU, wakeups/s |
| Startup first-paint (S4) | span from `setup START` log (`lib.rs:447`) to main window first paint | ms |
| Idle RSS (S8) | ` Resident` in Activity Monitor with vs without preload | MB |

## Prioritized recommendations

- **P1 — S1 (release profile).** Highest impact-per-effort, lowest risk, and it benefits *every* code path (decode, audio, network). Ship `lto="thin"` + `codegen-units=1`; do **not** add `panic="abort"`. One config change, gated to `[profile.release]`, with a symbolication smoke test.
- **P1 — C2 (adaptive Soniox poll).** The cheapest cloud win while REST remains; recovers ~400 ms p50 on the common short-clip case with a pure sleep-tuning change. Keep until C1 (WS) lands.
- **P2 — S2 (slim pill) + S5 (radix metapackage).** Bundle/parse/RSS wins on the always-visible surface; S2 also unblocks S4.
- **P2 — C3 (backoff) + C4 (launch warm).** Network robustness + first-request latency; small, isolated.
- **P2 — S3 (device poll backoff).** The genuine idle-CPU/thermal contributor; medium effort due to hotplug correctness.
- **P3 — S4 (lazy WebViews), S6 (manualChunks), S7 (watchdog), S8 (optional resident model), C5 (pool validate), C1 (WS, owned by `06`).** Real wins but either higher effort, higher risk, or owned by the streaming track.

## Open questions / risks

- **reqwest manifest vs lock.** `Cargo.toml:43` declares `reqwest = "0.13.4"` but `Cargo.lock` resolves **0.12.22** (with `hyper-rustls`+`hyper-tls`+`rustls`+`quinn`). `[INFERENCE]` 0.13 is not a published reqwest line as of the lock; the lock is authoritative for what builds. Confirm intended version — affects h3 availability and the `http3` feature story. NEEDS MEASUREMENT/confirmation against the maintainer's intent.
- **LTO link-time regression in local release iteration.** Acceptable in CI; decide whether a separate `[profile.release-fast-link]`/dev workflow is wanted for local runs.
- **Device-watch hotplug SLA (S3).** Any backoff must preserve "detect a newly plugged USB mic within ~1 poll of plugging it in while idle" — needs a hotplug test before shipping.
- **Lazy pill WebView (S4).** The pill must exist *before* the first recording's first audio-level event, or the first recording shows no indicator — needs pre-creation on first hotkey-armed state, not first recording.
- **Optional resident model (S8).** Trading 1–3 GB idle RSS for first-dictation latency is a UX call (015 only constrains correctness, not speed); gate behind a setting and measure the regression on the first utterance.
