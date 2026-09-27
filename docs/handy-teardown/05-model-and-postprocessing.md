# Model management & LLM post-processing — Handy teardown

**TL;DR.** Handy ships a *build-time-baked*, zero-network model catalog (`catalog.json`, compiled into the binary) that is normalized into a single `ModelDescriptor` shape and merged — at runtime — with on-disk discovery (custom `.bin`/`.gguf` files and the shared Hugging Face cache) into one `ModelInfo` registry the UI reads. Downloads are fully background, cancellable, resumable, SHA-256-verified, and progress is streamed to the UI through a throttled event bus (`model-download-progress`, max ~10/sec). The engine is loaded on-demand (eagerly on first record, or lazily per-transcription), held in a mutex, leased out to the streaming worker via an atomic CAS, and idle-unloaded by a background watcher thread after a user-configurable timeout. Separately, transcribed text can be optionally "polished" by any OpenAI-compatible LLM (OpenAI, Anthropic, Groq, Cerebras, OpenRouter, Z.AI, AWS Bedrock, or local Apple Intelligence) — off by default — which produces the `StreamWorkKind::Polishing` overlay phase. The whole thing feels lightweight because every long operation (download, hash, load, inference, polish) runs off the UI thread and reports through typed events a single Zustand store listens to.

---

## 1. File map

| File | LOC | Purpose |
|---|---|---|
| `src-tauri/src/managers/model.rs` | 2705 | The `ModelManager`: catalog seeding, on-disk discovery, download/verify/extract, delete, rescan, runtime-capability reconciliation, the `ModelInfo`/`ModelSource`/`EngineType` types. The biggest file in the backend. |
| `src-tauri/src/catalog/mod.rs` | 166 | Parses bundled `catalog.json` once into `Vec<ModelDescriptor>`; editorial `rank_of()`. |
| `src-tauri/src/catalog/catalog.json` | 3381 | Build-time-generated model catalog (HF `handy-computer` org). Data, not code. |
| `src-tauri/src/managers/model_capabilities.rs` | 269 | GGUF-header capability prober; `CapabilityProbe`, `Compatibility`, `KNOWN_ARCHES`. |
| `src-tauri/src/managers/transcription.rs` | 1956 | `TranscriptionManager`: engine load/unload, idle-unload watcher, the `active_engine_lease` streaming seam, `emit_stream_working`. |
| `src-tauri/src/llm_client.rs` | 279 | OpenAI-compatible chat-completion HTTP client (the "polish" egress). |
| `src-tauri/src/settings.rs` | 1205 | `PostProcessProvider`, `ModelUnloadTimeout`, all `post_process_*` settings, default providers/prompt. |
| `src-tauri/src/actions.rs` | 892 | `post_process_transcription`, `process_transcription_output`, the polish→overlay emission. |
| `src-tauri/src/commands/models.rs` | 203 | Tauri commands: `download_model`, `delete_model`, `set_active_model`/`switch_active_model`. |
| `src-tauri/src/commands/transcription.rs` | 41 | `get_model_load_status`, `set_model_unload_timeout`, `unload_model_manually`. |
| `src/stores/modelStore.ts` | 431 | Zustand store: model list, current model, download/verify/extract state, event listeners. |
| `src/stores/settingsStore.ts` | 602 | Settings store incl. all `post_process_*` fields + provider/api-key/model actions. |
| `src/hooks/useSettings.ts` | 78 | Thin selector over `settingsStore` exposing post-process helpers. |
| `src/components/model-selector/ModelSelector.tsx` | 275 | Dropdown + status machine; auto-selects on download-complete. |
| `src/components/model-selector/ModelStatusButton.tsx` | 77 | The colored status dot. |
| `src/components/model-selector/DownloadProgressDisplay.tsx` | 54 | Progress bar(s) + smoothed MB/s. |
| `src/components/settings/PostProcessingSettingsApi/usePostProcessProviderState.ts` | 236 | Wires provider/base-url/api-key/model selectors to settings. |
| `src/components/settings/PostProcessingToggle.tsx` | 29 | The single `post_process_enabled` on/off switch. |

---

## 2. Mechanism walkthrough

### 2.1 Catalog: how models are declared

The catalog is **baked into the binary at compile time** via `include_str!`, parsed exactly once into a `Lazy` static, and normalized into the same descriptor shape every other model producer uses:

```rust
// src-tauri/src/catalog/mod.rs:101-106
pub static CATALOG: Lazy<Vec<ModelDescriptor>> = Lazy::new(|| {
    let root: CatalogRoot = serde_json::from_str(include_str!("catalog.json"))
        .expect("bundled catalog.json is valid JSON matching the catalog schema");
    root.models.into_iter().map(ModelDescriptor::from).collect()
});
```

The on-disk catalog entry type (`catalog.json`) is mirrored by `CatalogModel`, which declares *only* the fields the descriptor needs (serde ignores the rest — `slug`, `family`, `license`, `parameters`, …):

```rust
// src-tauri/src/catalog/mod.rs:33-60
#[derive(Deserialize)]
struct CatalogModel {
    /// HF repo id, e.g. `handy-computer/whisper-small-gguf`.
    id: String,
    name: String,
    description: String,
    architecture: Option<String>,
    languages: Vec<String>,
    capabilities: CatalogCaps,
    speed_score: Option<f32>,
    accuracy_score: Option<f32>,
    files: Vec<QuantFile>,
    default_quant: Option<String>,
    recommended_rank: Option<u32>,
    /// Part of the small curated onboarding set (badged "Recommended"). Distinct
    /// from `recommended_rank`, which only orders the full list.
    #[serde(default)]
    recommended: bool,
}

#[derive(Deserialize)]
struct CatalogCaps {
    streaming: bool,
    translate: bool,
    lang_detect: bool,
}
```

Each `files[]` entry is a `QuantFile` (`{filename, quant, size_bytes}`) — one downloadable quantization. A real entry (`catalog.json:5-60`):

```json
{
  "id": "handy-computer/parakeet-unified-en-0.6b-gguf",
  "slug": "parakeet-unified-en-0.6b",
  "name": "Parakeet Unified EN 0.6B",
  "architecture": "parakeet",
  "family": "parakeet",
  "description": "Fast, accurate live English transcription",
  "languages": ["en"],
  "capabilities": { "streaming": true, "translate": false, "lang_detect": false, "timestamps": "token" },
  "speed_score": 79,
  "accuracy_score": 90,
  "files": [
    { "filename": "parakeet-unified-en-0.6b-Q4_K_M.gguf", "quant": "Q4_K_M", "size_bytes": 477274496 },
    { "filename": "parakeet-unified-en-0.6b-Q8_0.gguf", "quant": "Q8_0", "size_bytes": 731357568 },
    ...
  ],
  "default_quant": "Q8_0",
  "recommended": true,
  "recommended_rank": 1
}
```

**Metadata each entry carries**: `id` (HF repo), `name`, `description`, `architecture` (e.g. `"parakeet"`, `"whisper"`), `languages[]`, capability flags (`streaming`/`translate`/`lang_detect`/`timestamps`), `speed_score`/`accuracy_score` (0–100 in JSON → normalized to 0.0–1.0), `files[]` with per-quant byte size, `default_quant`, `recommended_rank` (sort order), `recommended` (the curated onboarding badge — distinct from rank). The catalog *does not* carry a SHA-256 hash: integrity verification is keyed to the `ModelSource::Url { sha256 }` path only (see §2.2); HF-sourced catalog models rely on hf-hub's ETag-based cache integrity instead.

The `From<CatalogModel> for ModelDescriptor` conversion (`catalog/mod.rs:62-99`) is where the catalog becomes a runtime descriptor. **Every catalog model is assigned `EngineType::TranscribeCpp`** regardless of architecture:

```rust
// src-tauri/src/catalog/mod.rs:71-97
ModelDescriptor {
    id: format!("{}/{}", m.id, default_filename),
    source: ModelSource::HuggingFace {
        repo_id: m.id,
        revision: "main".to_string(),
    },
    name: m.name,
    ...
    engine_type: EngineType::TranscribeCpp,
    caps: CapabilityProbe {
        verdict: Compatibility::Compatible, // curated org models we ship support for
        architecture: m.architecture,
        ...
        supports_streaming: Some(m.capabilities.streaming),
        supports_translation: Some(m.capabilities.translate),
        supports_language_detect: Some(m.capabilities.lang_detect),
    },
    files: m.files,
    default_quant: m.default_quant,
    // catalog scores are 0–100; ModelInfo / the UI bars use 0.0–1.0.
    speed_score: m.speed_score.unwrap_or(0.0) / 100.0,
    accuracy_score: m.accuracy_score.unwrap_or(0.0) / 100.0,
    recommended_rank: m.recommended_rank,
    recommended: m.recommended,
}
```

**Whisper-vs-Parakeet distinction (important nuance).** The `EngineType` enum (`model.rs:25-38`) has *both* a `TranscribeCpp` and a `Parakeet` variant — but they are **not** "Whisper" vs "Parakeet":

```rust
// src-tauri/src/managers/model.rs:25-38
pub enum EngineType {
    /// Any GGML/GGUF model loaded through transcribe-cpp (Whisper, Parakeet,
    /// Voxtral, Qwen3-ASR, Nemotron, …). The architecture is auto-detected from
    /// the file, so this one variant covers the whole transcribe-cpp family.
    TranscribeCpp,
    Parakeet,
    Moonshine,
    MoonshineStreaming,
    SenseVoice,
    GigaAM,
    Canary,
    Cohere,
}
```

- `EngineType::TranscribeCpp` is the **GGUF/Metal-GPU path** — used by *all* catalog models, Whisper *and* Parakeet-architecture GGUFs alike, because `transcribe-cpp` auto-detects the architecture from the GGUF `general.architecture` header key (`model_capabilities.rs:29-49` lists both `"whisper"` and `"parakeet"` in `KNOWN_ARCHES`). At load time the backend/Metal device is selectable.
- `EngineType::Parakeet` is the **legacy ONNX/CPU-ANE path** (`ParakeetModel::load` with `Quantization::Int8`, `transcription.rs:585-594`) — a separate, non-GGUF runtime.

So the *catalog* never distinguishes them by engine type — it distinguishes them by the `architecture` metadata field (`"parakeet"` vs `"whisper"`), which the prober surfaces and which `transcribe-cpp` reconciles at load. The `EngineType::Parakeet` (ONNX) variant is only populated by the legacy hardcoded table in `ModelManager::new()`.

The descriptor is rendered into the frontend-facing `ModelInfo` only by combining it with live `DiskStatus` (so status can be recomputed without rebuilding the descriptor) — `model.rs:186-214`. The registry is keyed by `id = "{repo_id}/{default_filename}"` so a catalog entry dedups with the same file later found in the HF cache (`catalog/mod.rs:64-69`).

### 2.2 Download pipeline

`ModelManager::download_model` (`model.rs:1826-2172`) branches on `ModelSource`:

```rust
// src-tauri/src/managers/model.rs:1835-1845
let (url, expected_sha256) = match &model_info.source {
    ModelSource::Url { url, sha256 } => (url.clone(), sha256.clone()),
    ModelSource::HuggingFace { repo_id, revision } => {
        return self
            .download_hf_model(&model_info, repo_id.clone(), revision.clone())
            .await;
    }
    ModelSource::Local => {
        return Err(anyhow::anyhow!("No download source for model"));
    }
};
```

**Two distinct transports:**

1. **`ModelSource::Url`** (legacy `blob.handy.computer` `.bin` files) — `download_model` `model.rs:1826-2172`. Resumable: writes to `{filename}.partial`, sends an HTTP `Range:` header to resume (`model.rs:1899-1901`), and if the server replies `200` instead of `206` it discards the partial and restarts to avoid corruption (`model.rs:1908-1921`). Streams `response.bytes_stream()` chunk-by-chunk, appending to the partial file.
2. **`ModelSource::HuggingFace`** (all catalog models) — `download_hf_model` `model.rs:1743-1824`. Uses `hf-hub`'s `ApiBuilder`/`download_with_progress_cancellable` into the **shared HF cache** (`~/.cache/huggingface/hub`), so downloads are reused by other tools. First checks `hf_cached_path` and short-circuits if already present (`model.rs:278-286`, `1752-1757`).

**Progress event contract** — both paths emit the *same* `model-download-progress` event with `DownloadProgress { model_id, downloaded, total, percentage }` (`model.rs:267-273`):

```rust
// src-tauri/src/managers/model.rs:353-368  (HF path; URL path emits inline at 1966/2002/2020)
fn emit(&self, downloaded: u64, total: u64) {
    let percentage = if total > 0 {
        (downloaded as f64 / total as f64) * 100.0
    } else { 0.0 };
    let _ = self.app_handle.emit(
        "model-download-progress",
        &DownloadProgress { model_id: self.model_id.clone(), downloaded, total, percentage },
    );
}
```

Both paths **throttle to ~10 updates/sec** so the UI never freezes:
- HF path: a shared `HfProgressState { last_emit }` and `now.duration_since(st.last_emit) >= Duration::from_millis(100)` (`model.rs:382-398`), always emitting the final byte on `finish`.
- URL path: `let throttle_duration = Duration::from_millis(100);` + `if last_emit.elapsed() >= throttle_duration` (`model.rs:1968-2004`), plus a guaranteed final 100% emit at `model.rs:2007-2020`.

**The full event lifecycle** (all global, payload is `model_id` string unless noted):
`model-download-progress` (throttled) → `model-verification-started` → `model-verification-completed` → [if `is_directory`] `model-extraction-started` → (`model-extraction-completed` | `model-extraction-failed{model_id,error}`) → `model-download-complete`. Cancellation: `model-download-cancelled`. Failure: `model-download-failed{model_id,error}`.

**Verification + on-disk layout.** After the stream completes, a byte-size sanity check (`model.rs:2026-2037`), then SHA-256 verification runs **in `spawn_blocking`** so the async executor isn't stalled hashing up-to-1.6 GB files (`model.rs:2039-2055`):

```rust
// src-tauri/src/managers/model.rs:2042-2055
let _ = self.app_handle.emit("model-verification-started", model_id);
let verify_result = tokio::task::spawn_blocking(move || {
    Self::verify_sha256(&verify_path, verify_expected.as_deref(), &verify_model_id)
}).await.map_err(|e| anyhow::anyhow!("SHA256 task panicked: {}", e))?;
verify_result?;
let _ = self.app_handle.emit("model-verification-completed", model_id);
```

`verify_sha256` (`model.rs:1693-1722`) skips when `expected_sha256` is `None` (custom user models), and on mismatch **deletes the partial** so the next attempt starts clean. For directory-based models (ONNX tarballs), the verified `.tar.gz` is unpacked into a `*.extracting` temp dir then atomically renamed to the final dir (`model.rs:2057-2148`); for file-based models, `partial` → final `rename` (`model.rs:2145-2148`). Final layout: `{app_data_dir}/models/{filename}` (single file) or `{filename}/` (directory).

**RAII cleanup.** A `DownloadCleanup` guard (`model.rs:424-444`) clears the `is_downloading` flag and removes the cancel token on *every* exit path; it is `disarm`ed only on the success path which does its own richer cleanup (`model.rs:2150-2161`).

**Blocking vs background?** Entirely **background / non-blocking**. `download_model` is `async` and `commands::download_model` is an `async` Tauri command (`commands/models.rs:40-48`) — the IPC call returns immediately after dispatching; all heavy I/O happens in the async runtime + `spawn_blocking`, and state flows back exclusively through events. A `CancellationToken` (stored in `cancel_flags` per model) is checked on every chunk (`model.rs:1975-1981`), and `cancel_download` (`model.rs:2321-2352`) triggers it and emits `model-download-cancelled`.

### 2.3 Load/unload lifecycle

Loading is owned by **`TranscriptionManager`** (not `ModelManager`) — `ModelManager` only knows disk state; `TranscriptionManager` owns the live engine. `ModelManager` is injected at construction: `TranscriptionManager::new(app_handle, model_manager)` (`transcription.rs:255`).

**The load path** (`load_model_with_device`, `transcription.rs:452-687`):

1. Emits `model-state-changed { event_type: "loading_started" }` (`transcription.rs:462-471`).
2. Resolves `model_info` + `model_path` from the `ModelManager` (`transcription.rs:473-492`), erroring with `loading_failed` if not downloaded.
3. **Drops the current engine before building the new one** so `transcribe-cpp` frees the previous native context first — avoids holding two large GGUFs in memory (`transcription.rs:494-505`).
4. Builds the engine by `EngineType` match (`transcription.rs:520-654`): `TranscribeCpp` → `Model::load_with(&path, &ModelOptions { backend, gpu_device })` (Metal GPU selectable via persisted accelerator or explicit device index); `Parakeet` → `ParakeetModel::load(&path, &Quantization::Int8)`; Moonshine/SenseVoice/GigaAM/Canary/Cohere analogously.
5. For `TranscribeCpp`, **reconciles runtime capabilities** against the GGUF ground truth — `set_runtime_capabilities` overwrites the pre-download catalog/probe view so streaming/translate/language badges reflect what the *loaded* model actually supports (`transcription.rs:560-571`).
6. Stores the engine in the `engine: Mutex<Option<LoadedEngine>>`, sets `current_model_id`, calls `touch_activity()` (resets the idle timer), and emits `loading_completed` (`transcription.rs:656-685`).

```rust
// src-tauri/src/managers/transcription.rs:520-521,544-548,583
let loaded_engine = match model_info.engine_type {
    EngineType::TranscribeCpp => {
        ...
        let model = Model::load_with(&model_path, &model_options).map_err(...)?;
        ...
        LoadedEngine::TranscribeCpp(session)
    }
    EngineType::Parakeet => {
        let engine = ParakeetModel::load(&model_path, &Quantization::Int8).map_err(...)?;
        LoadedEngine::Parakeet(engine)
    }
    ...
};
```

**Idle-unload (the "model-unload-timeout").** A dedicated **watcher thread** started in `new()` polls every 10 seconds (`transcription.rs:279-343`):

```rust
// src-tauri/src/managers/transcription.rs:281-338
thread::spawn(move || {
    while !shutdown_signal.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_secs(10)); // Check every 10 seconds
        ...
        let timeout = settings.model_unload_timeout;
        // Skip Immediately — that variant is handled by maybe_unload_immediately()
        // after each transcription. Treating it as 0s here would unload the
        // model mid-recording.
        if timeout == ModelUnloadTimeout::Immediately { continue; }

        // While recording, keep the idle timer fresh so the model is never
        // unloaded mid-session.
        if is_recording { manager_cloned.touch_activity(); continue; }

        if let Some(limit_seconds) = timeout.to_seconds() {
            let idle_ms = now_ms.saturating_sub(last);
            if idle_ms > limit_seconds * 1000 {
                if manager_cloned.is_model_loaded() {
                    ... manager_cloned.unload_model() ...
                }
            }
        }
    }
});
```

The timeout enum (`settings.rs:133-145`) with its conversion (`settings.rs:211-232`):

```rust
// src-tauri/src/settings.rs:133-145
#[serde(rename_all = "snake_case")]
pub enum ModelUnloadTimeout {
    Never,
    Immediately,
    Min2,
    #[default]
    Min5,
    Min10,
    Min15,
    Hour1,
    Sec15, // Debug mode only
}
```

Three distinct unload triggers coexist: (a) idle watcher (above), (b) `maybe_unload_immediately()` called after each transcription/cancellation when set to `Immediately` (`transcription.rs:431-441`, `utils.rs:36-37`), (c) explicit `unload_model` via tray/UI/quit (`commands/transcription.rs:32-40`, `lib.rs:784-786`). `unload_model` (`transcription.rs:385-416`) drops the engine (`*engine = None`), clears the model id, and emits `model-state-changed { event_type: "unloaded" }`.

**The "is loaded" report to the UI** flows two ways:
- *Pull*: `get_model_load_status` Tauri command returns `ModelLoadStatus { is_loaded, current_model }` (`commands/transcription.rs:7-30`). `is_model_loaded()` is lease-aware:

```rust
// src-tauri/src/managers/transcription.rs:356-360
pub fn is_model_loaded(&self) -> bool {
    // The engine may be leased out to the streaming worker (taken out of
    // the mutex). It's still loaded, just in use, so report true.
    self.lock_engine().is_some() || self.active_engine_lease.load(Ordering::Acquire) != 0
}
```
- *Push*: the `model-state-changed` events (`loading_started`/`loading_completed`/`loading_failed`/`unloaded`/`selection_changed`) that `ModelSelector` listens to.

**The seam where the loaded engine becomes available to the streaming worker** (cross-ref slice 1, but from the model side). Recording start kicks off a **background load** (`actions.rs:446` `tm.initiate_model_load()`), which spawns a thread that loads the persisted `selected_model` and, critically, clears `is_loading` + `notify_all()` on a condvar when done (`transcription.rs:690-717`):

```rust
// src-tauri/src/managers/transcription.rs:690-717
pub fn initiate_model_load(&self) {
    let mut is_loading = self.is_loading.lock().unwrap();
    if *is_loading { return; }
    if !reload_pending && self.is_model_loaded() { return; }
    *is_loading = true;
    let self_clone = self.clone();
    thread::spawn(move || {
        ...
        if let Err(e) = self_clone.load_model(&settings.selected_model) {
            error!("Failed to load model: {}", e);
        }
        let mut is_loading = self_clone.is_loading.lock().unwrap();
        *is_loading = false;
        self_clone.loading_condvar.notify_all();
    });
}
```

The streaming worker, in `run_stream_worker`, **waits on that condvar** so it never starts before the engine is ready, then **leases the engine out of the mutex** via an atomic compare-exchange on `active_engine_lease` (`transcription.rs:789-832`):

```rust
// src-tauri/src/managers/transcription.rs:804-832
// Take the engine out of the mutex so we own it during streaming,
// structurally excluding any concurrent batch transcription.
if self.active_engine_lease
    .compare_exchange(0, worker_id, Ordering::AcqRel, Ordering::Acquire)
    .is_err()
{
    warn!("Live preview: another worker already holds the transcription engine");
    ...
    return;
}
let mut engine = match self.lock_engine().take() {
    Some(e) => e,
    None => { /* model unloaded mid-stream → fall back to batch */ ... return; }
};
```

This is the precise hand-off: the engine is *physically taken* out of the `Mutex<Option<LoadedEngine>>` for the duration of streaming, so a concurrent batch transcription cannot touch it, while `is_model_loaded()` still returns `true` because `active_engine_lease != 0`. The `StreamWorkerGuard`'s `Drop` returns it (slice 1).

### 2.4 Post-processing / polish (the LLM egress path)

**Gating.** Polish is **off by default** (`settings.rs:540-542` `default_post_process_enabled() -> false`) and disabled per-shortcut unless `post_process_enabled` is true (`shortcut/mod.rs:395-398`). The master gate is the boolean `post_process` threaded through the transcription action. When set, after the final transcript lands, the Live overlay flips to a "polishing" phase *before* the network call:

```rust
// src-tauri/src/actions.rs:700-710
if post_process {
    if style == OverlayStyle::Live {
        tm.emit_stream_working(StreamWorkKind::Polishing);
    } else {
        show_processing_overlay(&ah);
    }
}
let processed = process_transcription_output(&ah, &transcription, post_process).await;
```

`emit_stream_working` (`transcription.rs:1078-1084`) emits the tauri-specta `StreamPhaseEvent { phase: Working, kind: Some(Polishing) }` — exactly the `StreamWorkKind` enum that the overlay (slice 2) renders as a spinner+label:

```rust
// src-tauri/src/managers/transcription.rs:70-74
#[serde(rename_all = "lowercase")]
pub enum StreamWorkKind {
    Transcribing,
    Polishing,
}
```

**The orchestration** — `post_process_transcription` (`actions.rs:77-313`) runs a series of cheap-skip guards, then builds the request:

```rust
// src-tauri/src/actions.rs:77-103
async fn post_process_transcription(settings: &AppSettings, transcription: &str) -> Option<String> {
    if is_blank_transcription(transcription) { return None; }           // skip empty
    let provider = match settings.active_post_process_provider().cloned() { ... }; // no provider → skip
    let model = settings.post_process_models.get(&provider.id).cloned().unwrap_or_default();
    if model.trim().is_empty() { return None; }                         // no model → skip
    let selected_prompt_id = match &settings.post_process_selected_prompt_id { ... }; // no prompt → skip
    let prompt = ... find prompt by id ...;
    if prompt.trim().is_empty() { return None; }
    let api_key = settings.post_process_api_keys.get(&provider.id).cloned().unwrap_or_default();
    ...
}
```

Each guard returns `None` (→ the original transcript is used) so a misconfigured polish path degrades silently to the raw transcription rather than erroring.

**Two request modes**, chosen by `provider.supports_structured_output` (`actions.rs:160-275` vs `277-312`):
- *Structured*: builds a JSON schema `{ "transcription": string }`, sends the transcript as the **user** message and the prompt (with `${output}` stripped) as the **system** message, then parses the JSON `transcription` field out of the response (`actions.rs:210-262`). Falls back to raw content if the model ignores the schema.
- *Legacy*: `${output}` in the prompt is text-replaced with the transcript and sent as a single user message (`actions.rs:277-289`).

**The HTTP client** — `llm_client.rs`. All providers speak the **OpenAI chat-completions wire format** (`POST {base_url}/chat/completions`); only auth headers differ. Anthropic uses `x-api-key` + `anthropic-version`; everyone else uses `Authorization: Bearer` (`llm_client.rs:79-94`):

```rust
// src-tauri/src/llm_client.rs:79-94
if !api_key.is_empty() {
    if provider.id == "anthropic" {
        headers.insert("x-api-key", HeaderValue::from_str(api_key)...);
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    } else {
        headers.insert(AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))...);
    }
}
```

The core call builds a `ChatCompletionRequest { model, messages, response_format, reasoning_effort, reasoning }` and POSTs it (`llm_client.rs:138-218`). Two reasoning knobs are supported and selectively disabled to keep polish cheap and JSON clean: `reasoning_effort` (OpenAI top-level) and a nested `reasoning { effort, exclude }` (OpenRouter). For polish, `custom` → `reasoning_effort: "none"`, `openrouter` → `reasoning { effort: "none", exclude: true }` so reasoning text can't pollute structured-output JSON parsing (`actions.rs:144-158`).

```rust
// src-tauri/src/llm_client.rs:182-195
let request_body = ChatCompletionRequest {
    model: model.to_string(),
    messages,
    response_format,
    reasoning_effort,
    reasoning,
};
let response = client.post(&url).json(&request_body).send().await
    .map_err(|e| format!("HTTP request failed: {}", e))?;
```

**Configured providers** (`settings.rs:558-648`): OpenAI, Z.AI, OpenRouter, Anthropic, Groq, Cerebras, AWS Bedrock (Mantle), Apple Intelligence (local, macOS-arm64 only, gated behind `cfg`), plus an editable "Custom" (default `localhost:11434/v1`, i.e. Ollama). Each provider carries `{ id, label, base_url, allow_base_url_edit, models_endpoint, supports_structured_output }` (`settings.rs:96-107`).

**Default prompt** (`settings.rs:675-681`) — note the `${output}` placeholder and "keep original language" instruction:

```
Clean this transcript:
1. Fix spelling, capitalization, and punctuation errors
2. Convert number words to digits (twenty-five → 25, ten percent → 10%, five dollars → $5)
3. Replace spoken punctuation with symbols (period → ., comma → ,, question mark → ?)
4. Remove filler words (um, uh, like as filler)
5. Keep the language in the original version (if it was french, keep it in french)

Preserve exact meaning and word order. Do not paraphrase or reorder content.
Return only the cleaned transcript.
Transcript:
${output}
```

**Privacy implication.** This is a **text-egress path**: enabling polish sends every transcribed utterance (the full transcript) over the network to the configured provider's chat-completions endpoint. It is off by default, but once enabled *and* a provider+key+model is configured, all polish-eligible transcriptions leave the machine. The only fully-local option is **Apple Intelligence** (`process_text_with_system_prompt`, `actions.rs:166-208`, macOS-arm64 only), which never touches the network. A fork concerned about privacy should surface this trade-off explicitly in the UI (Handy does not currently warn about egress beyond the toggle label).

### 2.5 Model UI / state

**Backend→frontend contract** is tauri-specta generated. The `ModelInfo` struct (`model.rs:59-81`) is the single shape the UI receives; it carries both static spec (name, size, scores, capabilities) and live disk state (`is_downloaded`, `is_downloading`, `partial_size`).

**`modelStore.ts`** (Zustand + Immer + `subscribeWithSelector`) is the single source of truth on the frontend. Its state (`modelStore.ts:23-56`): `models[]`, `currentModel`, plus `Record<string, true>` sets for `downloadingModels`/`verifyingModels`/`extractingModels`, and `downloadProgress`/`downloadStats` keyed by model id. `initialize()` (`modelStore.ts:268-429`) loads initial data then registers **one listener per backend event**, e.g.:

```ts
// src/stores/modelStore.ts:277-320  (abbreviated)
listen<DownloadProgress>("model-download-progress", (event) => {
  const progress = event.payload;
  set(produce((state) => { state.downloadProgress[progress.model_id] = progress; }));
  // ... smoothed MB/s into downloadStats (EWMA: speed*0.8 + instant*0.2)
});
listen<string>("model-download-complete", (event) => {
  set(produce((state) => {
    delete state.downloadingModels[modelId];
    delete state.downloadProgress[modelId];
    ...
  }));
  get().loadModels();
});
```

`loadModels()` (`modelStore.ts:78-116`) calls `commands.getAvailableModels()` and re-syncs `downloadingModels` from the backend's `is_downloading` flags — so the store self-heals if an event was missed. Actions map 1:1 to commands: `selectModel`→`setActiveModel`, `downloadModel`→`downloadModel`, `cancelDownload`→`cancelDownload`, `deleteModel`→`deleteModel`, `rescanLocalModels`→`rescanLocalModels`.

**`ModelSelector.tsx`** derives a `ModelStatus` union (`ready|loading|downloading|verifying|extracting|error|unloaded|none`) from three inputs combined: (a) `commands.getTranscriptionModelStatus()` on `currentModel` change (`ModelSelector.tsx:50-69`), (b) the `model-state-changed` listener (`:71-98`), (c) the store sets, prioritized in `getDisplayStatus()`:

```tsx
// src/components/model-selector/ModelSelector.tsx:238-243
const getDisplayStatus = (): ModelStatus => {
  if (Object.keys(verifyingModels).length > 0) return "verifying";
  if (Object.keys(extractingModels).length > 0) return "extracting";
  if (Object.keys(downloadProgress).length > 0) return "downloading";
  return modelStatus;
};
```

It also **auto-selects a freshly downloaded model** unless recording (`ModelSelector.tsx:100-122`), and shows an optimistic `pendingModelId` during a switch.

**`ModelStatusButton.tsx`** renders the colored dot — green=ready, pulsing-yellow=loading, pulsing-primary=downloading, pulsing-orange=verifying/extracting, red=error, gray=unloaded (`ModelStatusButton.tsx:28-49`).

**`DownloadProgressDisplay.tsx`** maps the `downloadProgress`/`downloadStats` records to a shared `ProgressBar` and shows live MB/s only when a single download is active (`DownloadProgressDisplay.tsx:24-51`).

**Settings/post-processing state.** `settingsStore` holds all `post_process_*` fields and exposes `setPostProcessProvider`, `updatePostProcessBaseUrl/ApiKey/Model`, `fetchPostProcessModels` (the last hits the provider's `/models` endpoint via `llm_client::fetch_models`). `useSettings` (`useSettings.ts`) is a thin selector. `usePostProcessProviderState` (`usePostProcessProviderState.ts:33-235`) wires the provider dropdown → on select, auto-fetches valid models so a stale model id can't silently 404 at runtime (`:94-108`); only the `custom` provider allows base-URL editing (`:119-130`). `PostProcessingToggle.tsx` is the single on/off bound to `post_process_enabled`.

---

## 3. Why it feels lightweight / smooth / instant

- **Zero-network catalog.** `catalog.json` is `include_str!`-ed into the binary (`catalog/mod.rs:103`), so the model list appears instantly on first launch with no network round-trip — the "what can I download?" surface is always present.
- **Background, non-blocking downloads.** `download_model` is async + `spawn_blocking` for hashing (`model.rs:2047`); the IPC returns immediately and all state flows back through throttled events (≤10/sec). The UI never blocks on a model file.
- **One unified event bus, one store.** Every lifecycle transition (download/verify/extract/complete/cancel/load/unload) is a distinct global event that one Zustand store listens to (`modelStore.ts:277-426`). Components subscribe to derived state, not raw events, so there's no listener sprawl and progress auto-clears.
- **Resumable + verified downloads.** Range-header resume (`model.rs:1899-1921`) + SHA-256 in a blocking thread means a interrupted 1.6 GB download picks up rather than restarting, and corruption is caught before a confusing load failure.
- **Lazy, lease-aware engine ownership.** The engine is loaded on-demand (`initiate_model_load` on record start, `actions.rs:446`), physically *taken* from the mutex for streaming (`transcription.rs:814`), and `is_model_loaded()` is lease-aware so the status dot stays correct mid-stream (`transcription.rs:356-360`). Idle-unload frees GBs of GPU memory automatically (`transcription.rs:315-336`).
- **Polish degrades gracefully.** Every polish guard returns `None` → raw transcript (`actions.rs:78-131`); a misconfigured provider never breaks transcription, and the "Polishing" overlay phase gives the user a visible reason for the brief extra latency (`actions.rs:703`).
- **Optimistic UI.** `pendingModelId` shows the target model immediately on switch (`ModelSelector.tsx:47,143-154`); the status dot animates through loading/downloading/verifying states so there's always feedback.

---

## 4. VoiceTypr takeaways

| Priority | Takeaway | Rationale / mechanism to port |
|---|---|---|
| **High** | **Catalog-driven model registry, build-time-baked.** Generate a `catalog.json` and `include_str!` it; normalize into one `ModelDescriptor` → `ModelInfo`. | Zero-network first-run model list; single declarative source for size/scores/caps/recommended; trivially extensible (`catalog/mod.rs:62-99`). Far better than hardcoded model tables. |
| **High** | **One typed event per lifecycle stage + one Zustand store listening to all of them.** | Eliminates listener sprawl, makes progress/verify/extract/load states self-healing, and keeps React re-renders minimal (`modelStore.ts:277-426`). |
| **High** | **Lease-aware engine ownership with `active_engine_lease` atomic + condvar hand-off.** | Lets streaming *own* the engine structurally (no concurrent batch corruption) while `is_model_loaded()` stays truthful (`transcription.rs:356-360, 804-832`). Critical if VoiceTypr adds live preview. |
| **High** | **Resumable + SHA-256-verified downloads, hashing in `spawn_blocking`.** | Large model files must resume and be integrity-checked without stalling the async executor (`model.rs:1899-1921, 2039-2055`). |
| **Med** | **Configurable idle-unload watcher (default 5 min).** | Frees GPU/RAM when idle; the recording-aware `touch_activity()` prevents mid-session unloads (`transcription.rs:299-337`). VoiceTypr's blocking-decode hard-timeout (existing skill) is complementary, not a substitute. |
| **Med** | **Throttled progress (≤10/sec) + smoothed EWMA MB/s.** | Prevents UI freeze on fast networks; the 0.8/0.2 speed smoothing avoids jittery numbers (`model.rs:388, modelStore.ts:302-316`). |
| **Med** | **Polish gating with all-skip guards + visible `Polishing` overlay phase.** | Off-by-default LLM polish that degrades to raw transcript on any misconfig; the dedicated `StreamWorkKind::Polishing` phase tells the user *why* there's a pause (`actions.rs:77-131, 700-707`). |
| **Med** | **Shared HF cache for downloads.** | `hf-hub`'s stock cache means models download once and are reused by Whisper.cpp/other tools (`model.rs:275-286, 1554-1561`). |
| **Low** | **RAII guards for cleanup (`DownloadCleanup`, `RescanGuard`, `LoadingGuard`).** | Guarantees `is_downloading`/`is_loading` flags clear on every error path without manual cleanup (`model.rs:421-444`). |
| **Low** | **GGUF-header pre-download capability probing.** | Surface honest streaming/translate/language badges before download, reconciled at load (`model_capabilities.rs:90-107`, `transcription.rs:560-571`). Only worth it if VoiceTypr supports drop-in custom models. |
| **Avoid** | **Don't conflate catalog `architecture` with `EngineType`.** | Handy's `EngineType::Parakeet` (ONNX) vs catalog `architecture:"parakeet"` (GGUF via TranscribeCpp) is a real footgun — a fork should pick one distinction axis. |
| **Avoid** | **Silent polish egress.** | Handy enables cloud text-egress with only a generic toggle label. A privacy-conscious fork should add an explicit "sends transcripts to {provider}" warning and default to Apple Intelligence / local LLM where available (`actions.rs:77`, `settings.rs:540`). |

---

## 5. Open questions / risks

- **Catalog models have no SHA-256.** Only `ModelSource::Url` entries carry a hash (`model.rs:43-49`); HF-sourced catalog models rely on hf-hub's ETag/LFS integrity (`model.rs:1743-1824`). `[INFERENCE]` A malicious-but-HF-hosted file with the right repo id would not be caught by an independent checksum. Worth confirming hf-hub's exact integrity guarantee if VoiceTypr copies this.
- **`Sec15` debug timeout vs `Immediately`.** Both map to short windows but are handled by *different* code paths (`Immediately` → `maybe_unload_immediately` post-transcription, `transcription.rs:292-297`; `Sec15` → the watcher thread). The comment at `transcription.rs:292-294` admits treating `Immediately` as 0s in the watcher "would unload the model mid-recording" — a subtle two-path invariant a fork should collapse into one.
- **Polish egress privacy surface.** No explicit user-facing warning that enabling polish sends transcripts off-device (only the toggle label). Not verified whether any telemetry/onboarding calls this out. `[INFERENCE]`
- **`set_runtime_capabilities` overrides are sticky.** A rescan is additive and preserves existing entries including runtime-probed caps (`model.rs:1142-1144`); if a user swaps the underlying file on disk, stale capabilities could persist until a restart. Not exercised in this read.
- **`download_hf_model` short-circuit.** It checks `hf_cached_path` and emits `model-download-complete` without a download (`model.rs:1752-1757`) — correct, but means a "download" can complete instantly with no progress events, which the UI handles only because `model-download-complete` triggers `loadModels()`.
