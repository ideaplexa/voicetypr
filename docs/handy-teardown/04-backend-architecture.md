# Backend Architecture & App Wiring — Handy teardown

**TL;DR.** Handy's Rust backend is a textbook example of *convention over cleverness*. A single `run()` function builds a `tauri::Builder`, hands tauri-specta a `collect_commands!`/`collect_events!` registry, and in `setup` constructs four domain **managers** as `Arc`s and stuffs them into Tauri `State`. Every cross-cutting concern — settings, audio, transcription, history — lives behind one manager, accessed by commands via `app.state::<Arc<T>>()`. Audio frames flow device → cpal callback → `mpsc` channel → consumer thread → `FrameResampler` (→16 kHz/30 ms) → optional Silero VAD → a one-line `router.feed(frame)` callback that is the **exact seam** between this slice and the streaming worker. Type safety between Rust and the React frontend is *generated*, not hand-written: tauri-specta emits `src/bindings.ts` on every debug build, so the frontend calls `commands.changeVadEnabledSetting(true)` and listens to `events.streamTextEvent` with full compile-time types — no stringly-typed `invoke`. The whole skeleton is unusually forkable because there is almost no hidden global state: the dependency graph is explicit and visible in one ~160-line function.

---

## 1. File map

| File | Lines | Purpose |
|---|---|---|
| `src-tauri/src/lib.rs` | 918 | App entry: `run()` builder, plugin registration, command/event registry via tauri-specta, `initialize_core_logic()` manager construction, headless CLI path, window/tray/autostart wiring. |
| `src-tauri/src/settings.rs` | 1204 | `AppSettings` schema (serde + `specta::Type`), per-field `#[serde(default)]`, `get_default_settings()`, store load/migrate (`get_settings`), `write_settings`, one-time migrations. |
| `src-tauri/src/managers/mod.rs` | 6 | Declares the manager submodules (`audio`, `history`, `model`, `transcription`, `gguf_meta`, `model_capabilities`). |
| `src-tauri/src/managers/audio.rs` | 591 | `AudioRecordingManager`: owns the `AudioRecorder`, mic life-cycle (always-on vs on-demand), mute, and the recorder→router callback wiring in `create_audio_recorder()`. |
| `src-tauri/src/managers/transcription.rs` | 1955 | `TranscriptionManager` + `StreamRouter` (the per-frame audio→worker bridge) + the `StreamTextEvent`/`StreamPhaseEvent` contract. *(Worker internals are slice 1; this doc quotes only the router seam and event structs.)* |
| `src-tauri/src/audio_toolkit/audio/recorder.rs` | 643 | `AudioRecorder`: cpal capture thread, mono mixdown, `FrameResampler` (→16 kHz), per-frame `handle_frame` with the VAD gate, the `AudioFrameCallback` type. |
| `src-tauri/src/audio_toolkit/audio/resampler.rs` | ~100 | `FrameResampler`: FFT resample + 30 ms frame chunking. |
| `src-tauri/src/audio_toolkit/vad/mod.rs` | 41 | `VoiceActivityDetector` trait, `VadFrame`, tuning constants. |
| `src-tauri/src/audio_toolkit/vad/silero.rs` | 58 | `SileroVad`: `vad_rs` ONNX engine wrapper, probability threshold. |
| `src-tauri/src/audio_toolkit/vad/smoothed.rs` | 110 | `SmoothedVad`: prefill + onset + hangover state machine wrapping any boolean VAD. |
| `src-tauri/src/commands/mod.rs` | 188 | Non-manager commands (`cancel_operation`, `get_app_settings`, `get_default_settings`, `set_log_level`, path/openers). |
| `src-tauri/src/shortcut/mod.rs` | 1236 | The `change_*_setting` command surface (each setting → one typed command). |
| `src/bindings.ts` | 1052 | **Generated** TS contract: typed `commands` object, `events` proxy, and all shared types. |
| `src/hooks/useSettings.ts` + `src/stores/settingsStore.ts` | 79 + 602 | Frontend settings path: Zustand store → generated commands, optimistic update + rollback, backend-refetch on `model-state-changed`. |

---

## 2. App bootstrap — `run()` and the builder pipeline

Everything starts in `src-tauri/src/lib.rs:524`. The function is the Tauri mobile/desktop entry point:

```rust
// src-tauri/src/lib.rs:524-528
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(cli_args: CliArgs) {
    // Detect portable mode before anything else
    portable::init();
    let console_filter = build_console_filter();
```

Portable-mode detection runs *first* (`portable::init()`) so that every later path resolution (`store_path`, `app_data_dir`, log dir, WebView2 cache) can redirect into a portable `Data/` directory instead of the OS app-data folder.

### 2.1 The tauri-specta builder — the single source of the command/event registry

Before the Tauri builder, Handy assembles a **specta builder** whose entire job is to know the command and event surface:

```rust
// src-tauri/src/lib.rs:530-640 (abbreviated)
let specta_builder = Builder::<tauri::Wry>::new()
    .commands(collect_commands![
        shortcut::change_binding,
        shortcut::reset_binding,
        shortcut::change_ptt_setting,
        // … ~120 commands total: every change_*_setting, model lifecycle,
        // audio device selection, history, transcription control, helpers …
        commands::models::get_available_models,
        commands::audio::set_selected_microphone,
        commands::transcription::unload_model_manually,
        helpers::clamshell::is_laptop,
    ])
    .events(collect_events![
        managers::history::HistoryUpdatePayload,
        managers::transcription::StreamTextEvent,
        managers::transcription::StreamPhaseEvent,
    ]);
```

`collect_commands!` / `collect_events!` (imported at `src-tauri/src/lib.rs:27` from `tauri_specta`) are the macros that register the Rust symbols as the IPC surface. **Three things matter here:**

1. **The list is the registry.** There is no second place to "register a command." If a function isn't in this macro, it is not callable from the frontend and (because it's how `invoke_handler` is built) not type-exported either.
2. **Events are first-class.** Only three event types exist in the entire app (`HistoryUpdatePayload`, `StreamTextEvent`, `StreamPhaseEvent`). This tightness is deliberate — see section 5.
3. **Every entry is a path to a `#[specta::specta]`-annotated function or a `tauri_specta::Event`-deriving struct**, so specta can read its signature and emit matching TypeScript.

### 2.2 The generation step (debug builds only)

Immediately after the registry is assembled, the bindings file is written:

```rust
// src-tauri/src/lib.rs:642-648
#[cfg(debug_assertions)] // <- Only export on non-release builds
specta_builder
    .export(
        Typescript::default().bigint(BigIntExportBehavior::Number),
        "../src/bindings.ts",
    )
    .expect("Failed to export typescript bindings");
```

So `src/bindings.ts` is regenerated on **every `cargo` debug build** and is never hand-edited (the file even says so on line 2: `// This file was generated by [tauri-specta]. Do not edit this file manually.`). A release build skips the export — the committed `bindings.ts` is the contract.

### 2.3 The Tauri builder — plugins, managed state, setup

```rust
// src-tauri/src/lib.rs:650-690 (abbreviated)
let invoke_handler = specta_builder.invoke_handler();

let headless_mode = cli_args.transcribe_file.is_some()
    || cli_args.list_devices || cli_args.list_models;

let mut builder = tauri::Builder::default()
    .device_event_filter(tauri::DeviceEventFilter::Always)
    .plugin(tauri_plugin_dialog::init())
    .plugin(LogBuilder::new() /* …three logging targets… */);
```

The plugin list is worth enumerating because it reveals Handy's external-dependency posture — all official Tauri plugins, no bespoke IPC:

`dialog`, `log`, `nspanel` (macOS only — the overlay's non-activating panel, `lib.rs:713`), `single_instance` (skipped in headless mode so `--transcribe-file` runs standalone, `lib.rs:701`), `fs`, `process`, `updater`, `os`, `clipboard_manager`, `macos_permissions`, `opener`, `store`, `global_shortcut`, `autostart`. Notably `.manage(cli_args.clone())` (`lib.rs:732`) puts the parsed CLI args into Tauri state, so any command can read `app.state::<CliArgs>()` — e.g. the close handler checks `no_tray` that way (`lib.rs:844`).

### 2.4 The `setup` closure — where state is born

```rust
// src-tauri/src/lib.rs:734-786 (key lines)
.setup(move |app| {
    specta_builder.mount_events(app);          // register the 3 events on this app
    // …headless short-circuit returns early here…
    let win_builder = tauri::WebviewWindowBuilder::new(app, "main", …)
        .title("Handy").inner_size(680.0, 570.0).visible(false);
    win_builder.build()?;

    let mut settings = get_settings(app.handle());
    // CLI --debug overrides (runtime-only)
    FILE_LOG_LEVEL.store(/* … */, Ordering::Relaxed);
    WEBVIEW_LOG_STREAMING.store(settings.debug_mode, Ordering::Relaxed);

    let app_handle = app.handle().clone();
    app.manage(TranscriptionCoordinator::new(app_handle.clone()));
    initialize_core_logic(&app_handle);
    overlay::update_overlay_enabled_cache(settings.overlay_style != OverlayStyle::None);
    std::thread::spawn(|| { let _ = get_available_accelerators(); }); // pre-warm GPU enum
    Ok(())
})
```

`specta_builder.mount_events(app)` is the runtime half of the event registry: it wires the three event types so `emit_to`/`emit` actually reach the webview. Then the window is built **hidden** (`visible(false)`), settings loaded, log-level atomics primed, the `TranscriptionCoordinator` managed (the orchestrator from slice 1), and `initialize_core_logic` does the heavy lifting.

### 2.5 `initialize_core_logic` — the manager construction graph

This is the most important function for forkability. It is a flat, ordered list of constructions that makes the entire dependency graph legible (`src-tauri/src/lib.rs:149-182`):

```rust
fn initialize_core_logic(app_handle: &AppHandle) {
    let model_manager =
        Arc::new(ModelManager::new(app_handle).expect("Failed to initialize model manager"));
    let transcription_manager = Arc::new(
        TranscriptionManager::new(app_handle, model_manager.clone())
            .expect("Failed to initialize transcription manager"),
    );
    let recording_manager = Arc::new(
        AudioRecordingManager::new(app_handle, transcription_manager.stream_router())
            .expect("Failed to initialize recording manager"),
    );
    let history_manager =
        Arc::new(HistoryManager::new(app_handle).expect("Failed to initialize history manager"));

    managers::transcription::init_transcribe_backend();
    managers::transcription::apply_accelerator_settings(app_handle);

    app_handle.manage(recording_manager.clone());
    app_handle.manage(model_manager.clone());
    app_handle.manage(transcription_manager.clone());
    app_handle.manage(history_manager.clone());
    // …signal handlers, tray, autostart, overlay window (see lib.rs:189-310)…
}
```

The construction **order encodes the dependency direction**: `ModelManager` (no deps) → `TranscriptionManager` (needs `model_manager`) → `AudioRecordingManager` (needs `transcription_manager.stream_router()`) → `HistoryManager` (no deps). Each is `Arc::new`-ed once and the *same clone* is both stored in Tauri state and (for the router) handed directly to the recorder. A fork that wants to add a "TranslationManager" adds one `Arc::new` + one `app_handle.manage` line.

Two intentional **deferrals** are called out in comments (`lib.rs:150-153`, `184-187`): Enigo (keyboard simulation) and global shortcuts are *not* initialized here — the frontend calls `initialize_enigo` / `initialize_shortcuts` after onboarding, so macOS permission dialogs never fire before the user is ready. This is an onboarding-smoothness decision enforced at the architecture layer (see slice 6 / OnboardingFlow).

---

## 3. Type-safe bindings — how `bindings.ts` is generated and consumed

### 3.1 The generation mechanism (Rust side)

Three ingredients produce a fully typed frontend contract with zero hand-maintenance:

- **`#[specta::specta]`** on every command fn (e.g. `src-tauri/src/shortcut/mod.rs:1146-1147`).
- **`#[derive(..., Type, tauri_specta::Event)]`** on every shared type/event (e.g. `src-tauri/src/managers/transcription.rs:50`).
- The **`collect_commands!` / `collect_events!`** registry + the **debug-build `specta_builder.export(..., "../src/bindings.ts")`** call (`lib.rs:530`, `642`).

`Type` (from `specta`) describes the shape; `tauri_specta::Event` adds the event-name mapping. The result is one TypeScript file with three regions.

### 3.2 Region 1 — typed commands

Each Rust command becomes an async function that wraps `TAURI_INVOKE` and returns a `Result<T, E>` discriminated union:

```ts
// src/bindings.ts:104-111 (one of ~120 generated commands)
async changeDebugModeSetting(enabled: boolean) : Promise<Result<null, string>> {
    try {
    return { status: "ok", data: await TAURI_INVOKE("change_debug_mode_setting", { enabled }) };
} catch (e) {
    if(e instanceof Error) throw e;
    else return { status: "error", error: e  as any };
}
},
```

The Rust original is `pub fn change_debug_mode_setting(app: AppHandle, enabled: bool) -> Result<(), String>` (annotated `#[specta::specta]`). specta drops the injected `app` argument, snake_cases the invoke name, and lifts the return type. The frontend never types an invoke string.

### 3.3 Region 2 — typed events

```ts
// src/bindings.ts:848-859
/** user-defined events **/

export const events = __makeEvents__<{
historyUpdatePayload: HistoryUpdatePayload,
streamPhaseEvent: StreamPhaseEvent,
streamTextEvent: StreamTextEvent
}>({
historyUpdatePayload: "history-update-payload",
streamPhaseEvent: "stream-phase-event",
streamTextEvent: "stream-text-event"
})
```

`__makeEvents__` (defined at `src/bindings.ts:1019-1052`) is a `Proxy` that maps each camelCase property to (a) `.listen/.once/.emit` bound to the snake_case event name, and (b) a call signature `events.streamTextEvent(webviewWindow)` for window-scoped listening. The Rust structs that back these (`src-tauri/src/managers/transcription.rs:50-83`):

```rust
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct StreamTextEvent {
    pub committed: String,
    pub tentative: String,
}
// …
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct StreamPhaseEvent {
    pub phase: StreamPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<StreamWorkKind>,
}
```

### 3.4 Region 3 — shared types

Every enum/struct reachable from a command or event signature is emitted as a TS type (the `user-defined types` block, `src/bindings.ts:865-992`). Enums become string-literal unions (e.g. `export type OverlayStyle = "none" | "minimal" | "live"` at `:949`), structs become object types, and `serde(rename_all)` is honored — so the `OverlayPosition` Rust enum's `#[serde(rename_all = "lowercase")]` lands as `"top" | "bottom"` in TS. **The frontend imports these types and is compile-checked against them** (`src/stores/settingsStore.ts:4-9` does `import type { AppSettings as Settings, ... } from "@/bindings"`).

### 3.5 Why this matters

There is no `invoke("change_debug_mode", ...)` anywhere in the React code. Renaming a Rust command or changing a field type is a compile error on the frontend, not a runtime `undefined`. The only "string" surface is the event-name map, and that map is itself generated from the Rust `tauri_specta::Event` derive.

---

## 4. The manager pattern — domain logic as Arc-held Tauri State

### 4.1 The shape

`managers/mod.rs` is six lines declaring submodules (`audio`, `gguf_meta`, `history`, `model`, `model_capabilities`, `transcription`). Each manager is a plain struct with three recurring properties:

1. **Constructed once**, in `initialize_core_logic`, wrapped in `Arc`.
2. **Holds an `app_handle: tauri::AppHandle`** so it can `emit` events back to the frontend without a caller passing it in.
3. **Owns its concurrency primitives internally** (`Arc<Mutex<…>>`, atomics, channels).

`AudioRecordingManager` is the archetype (`src-tauri/src/managers/audio.rs:175-188`):

```rust
#[derive(Clone)]
pub struct AudioRecordingManager {
    state: Arc<Mutex<RecordingState>>,
    mode: Arc<Mutex<MicrophoneMode>>,
    app_handle: tauri::AppHandle,
    recorder: Arc<Mutex<Option<AudioRecorder>>>,
    is_open: Arc<Mutex<bool>>,
    is_recording: Arc<Mutex<bool>>,
    did_mute: Arc<Mutex<bool>>,
    close_generation: Arc<AtomicU64>,
    cancel_generation: Arc<AtomicU64>,
    stream_router: Arc<StreamRouter>,
}
```

`#[derive(Clone)]` is cheap because every field is either an `Arc` or a `Clone`-cheap handle — so the manager can be cloned into spawned threads (e.g. `schedule_lazy_close` at `audio.rs:255` does `let rm = app.state::<Arc<AudioRecordingManager>>()` from a worker thread).

### 4.2 Dependency injection by constructor argument

There is no DI container. Dependencies are passed as constructor args, and the construction order in `initialize_core_logic` is the DAG:

```rust
// src-tauri/src/lib.rs:160-167
TranscriptionManager::new(app_handle, model_manager.clone())      // ← gets ModelManager
AudioRecordingManager::new(app_handle, transcription_manager.stream_router())  // ← gets router
```

`stream_router()` (`src-tauri/src/managers/transcription.rs:745-748`) returns `Arc::clone(&self.router)`, so the audio manager and the transcription manager literally share *one* `StreamRouter` allocation. The recorder closes over that same `Arc` (see section 7).

### 4.3 How commands reach a manager

Every command reads it out of Tauri state with a typed `app.state::<Arc<T>>()`:

```rust
// src-tauri/src/lib.rs:238 (tray handler), identical pattern in commands/*
let transcription_manager = app.state::<Arc<TranscriptionManager>>();
if !transcription_manager.is_model_loaded() { … }
```

This is the "service locator" flavor of DI: no global `static`, no trait-object registry, just `State<Arc<T>>`. It is forkable because (a) adding a manager is purely additive, (b) removing one produces a compile error at every `app.state::<Arc<RemovedManager>>()` call site, and (c) the managers don't know about each other except through explicitly injected `Arc`s.

### 4.4 The manager-vs-command split

Managers hold **state and long-running logic** (mic streams, model engines, the router). Commands (`commands/*`, `shortcut/mod.rs`) are **thin**: they read settings, mutate one field, write back, and/or call one manager method. The canonical setting-changer is three lines (`src-tauri/src/shortcut/mod.rs:1146-1153`):

```rust
#[tauri::command]
#[specta::specta]
pub fn change_vad_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.vad_enabled = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}
```

This uniform shape is why there are ~120 commands but they are all trivially auditable.

---

## 5. Settings — schema, defaults, persistence, frontend path

### 5.1 One struct, serde defaults everywhere

`AppSettings` (`src-tauri/src/settings.rs:323-441`) is a single flat struct deriving `Serialize, Deserialize, Type`. Its fork-superpower is that **every optional field carries its own `#[serde(default = "fn")]`**, so a store missing any key (or an older store lacking a new field) deserializes cleanly:

```rust
// src-tauri/src/settings.rs:323-330, 434-440 (excerpt)
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AppSettings {
    #[serde(default = "default_settings_schema_version")]
    pub settings_schema_version: u32,
    pub bindings: HashMap<String, ShortcutBinding>,
    // …
    #[serde(default = "default_vad_enabled")]
    pub vad_enabled: bool,
    #[serde(default = "default_overlay_style")]
    pub overlay_style: OverlayStyle,
}
```

### 5.2 Defaults are platform-aware

`get_default_settings()` (`settings.rs:749-858`) is `cfg`-gated for shortcuts and platform conventions:

```rust
// src-tauri/src/settings.rs:749-757
pub fn get_default_settings() -> AppSettings {
    #[cfg(target_os = "windows")]
    let default_shortcut = "ctrl+space";
    #[cfg(target_os = "macos")]
    let default_shortcut = "option+space";
    #[cfg(target_os = "linux")]
    let default_shortcut = "ctrl+space";
    // …
```

The same `cfg` pattern drives `PasteMethod::default()` (`settings.rs:201-209`, CtrlV on mac/win, Direct on Linux) and `KeyboardImplementation::default()` (`settings.rs:192-199`, HandyKeys everywhere except Linux). **Crucially the frontend fetches defaults from Rust** (`commands.getDefaultSettings()` → `get_default_settings()`), so the TS layer never duplicates platform logic — see `settingsStore.ts:67-68` comment.

### 5.3 Persistence — one store, one key, idempotent migrations

Handy persists *all* settings as a single JSON blob under one key (`"settings"`) in one tauri-plugin-store file:

```rust
// src-tauri/src/settings.rs:747, 936-967
pub const SETTINGS_STORE_PATH: &str = "settings_store.json";

pub fn get_settings(app: &AppHandle) -> AppSettings {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");
    let mut settings = if let Some(settings_value) = store.get("settings") {
        match serde_json::from_value::<AppSettings>(settings_value.clone()) {
            Ok(mut settings) => {
                if apply_settings_migrations(&mut settings, &settings_value) {
                    store.set("settings", serde_json::to_value(&settings).unwrap());
                }
                settings
            }
            Err(_) => { /* fall back to defaults, persist */ … }
        }
    } else { /* first run: persist defaults */ … };
    // …ensure_post_process_defaults…
    settings
}
```

Two design decisions stand out:

- **Reads also write.** `get_settings` runs `apply_settings_migrations` and, if it changed anything, writes back. Migrations are idempotent, so this converges after the first read of an older store (comment at `settings.rs:941-942`).
- **`write_settings` is the only write path** (`settings.rs:1032-1038`): `store.set("settings", serde_json::to_value(&settings))`. Every `change_*_setting` command goes read → mutate field → `write_settings`.

Migrations are version-gated by `settings_schema_version` (`CURRENT_SETTINGS_SCHEMA_VERSION = 1`, `settings.rs:447`) and handle: implicit onboarding completion for users who already picked a model (`:976-982`), the What's-New seen-version blanking on upgrade (`:984-992`), a one-time GPU-device-index reset (`:998-1009`), and the `overlay_position: "none"` → `OverlayStyle::None` rename (`:1011-1027`). The legacy `"none"` still deserializes via a serde alias (`settings.rs:117`) so old stores never fail to load.

### 5.4 Frontend path — Zustand + generated commands + optimistic rollback

`useSettings` (`src/hooks/useSettings.ts:46-78`) is a thin facade over a Zustand store. The store's `initialize()` (`settingsStore.ts:582-600`) fans out three loads and wires a backend-push re-fetch:

```ts
// src/stores/settingsStore.ts:582-599
initialize: async () => {
  const { refreshSettings, checkCustomSounds, loadDefaultSettings } = get();
  // Audio devices NOT refreshed here — App.tsx does it post-onboarding to
  // avoid macOS permission dialogs early (matches the Enigo/shortcut deferral).
  await Promise.all([loadDefaultSettings(), refreshSettings(), checkCustomSounds()]);
  listen("model-state-changed", () => { get().refreshSettings(); });
},
```

The clever part is the **`settingUpdaters` map** (`settingsStore.ts:76-164`), a `{ [K in keyof Settings]?: (value) => Promise<unknown> }` that maps *each settings key* to its generated command. `updateSetting` (`settingsStore.ts:279-308`) does an optimistic local set, calls the updater, and rolls back on error:

```ts
// src/stores/settingsStore.ts:289-307 (core)
set((state) => ({ settings: state.settings ? { ...state.settings, [key]: value } : null }));
const updater = settingUpdaters[key];
if (updater) { await updater(value); }
else if (key !== "bindings" && key !== "selected_model") {
  console.warn(`No handler for setting: ${String(key)}`);
}
// catch → roll back to originalValue; finally → setUpdating(key,false)
```

Because both the `AppSettings` type and each `commands.changeXxx` come from the generated `bindings.ts`, the `settingUpdaters` map is **type-checked end to end**: assigning the wrong command signature to a key is a TS compile error. The backend remains the source of truth (the `model-state-changed` listener re-fetches), so the optimistic update is purely a latency mask.

---

## 6. Why it feels lightweight / smooth

- **No stringly-typed IPC.** Every frontend call is `commands.X(args)` against generated types. There is no `invoke("maybe-typo")` to silently fail, and refactors are compile-checked both ways. This removes an entire class of "works on my machine" integration bugs.
- **State is legible and additive.** `initialize_core_logic` is ~30 lines that show the whole object graph. A fork never spelunks for a hidden singleton; it reads one function.
- **Settings never block startup.** The store read is synchronous but cheap (single JSON key), defaults are injected transparently, and the heaviest startup cost (GPU/accelerator enumeration) is explicitly pre-warmed on a background thread (`lib.rs:781-783`) so opening Advanced settings never freezes the UI.
- **Deferred side effects.** Enigo and global shortcuts are *deliberately* not initialized in `setup` — the frontend triggers them post-onboarding (`lib.rs:150-153`, `184-187`). The macOS Accessibility/mic prompts therefore never appear before the user has chosen to proceed, which is a big part of "smooth first run."
- **Logging is tiered.** Three log targets (`lib.rs:660-690`): console (respects `RUST_LOG`), file (respects the in-app log-level setting via a global atomic), and webview (only while debug mode is on — its sole consumer is the debug live-log viewer). Normal runs never broadcast potentially-sensitive log lines (file paths, transcribed text) onto the frontend event bus (`lib.rs:54-60`).
- **Single-instance with a CLI-arg forwarder** (`lib.rs:701-715`) means a second launch with `--toggle-transcription` forwards to the running app instead of double-starting — preserving the "one lightweight process" feel.

---

## 7. Audio → StreamRouter boundary (the slice 4 ↔ slice 1 seam)

This is the path a microphone sample takes to reach the streaming transcription worker, with the exact callback quoted.

### 7.1 Capture: cpal callback → mpsc channel

`AudioRecorder::open()` (`recorder.rs:128-268`) spawns a worker thread that builds a cpal input stream. `build_stream<T>` (`recorder.rs:296-352`) installs a closure that runs on the real-time audio thread:

```rust
// src-tauri/src/audio_toolkit/audio/recorder.rs:310-344 (core)
let stream_cb = move |data: &[T], _: &cpal::InputCallbackInfo| {
    if stop_flag.load(Ordering::Relaxed) {
        if !eos_sent { let _ = sample_tx.send(AudioChunk::EndOfStream); eos_sent = true; }
        return;
    }
    eos_sent = false;
    output_buffer.clear();
    if channels == 1 {
        output_buffer.extend(data.iter().map(|&s| s.to_sample::<f32>()));
    } else {
        // multi-channel → mono by averaging
        for frame in data.chunks_exact(channels) {
            let mono = frame.iter().map(|&s| s.to_sample::<f32>()).sum::<f32>() / channels as f32;
            output_buffer.push(mono);
        }
    }
    if sample_tx.send(AudioChunk::Samples(output_buffer.clone())).is_err() { /* … */ }
};
```

The real-time thread does **only** mono mixdown + a non-blocking `mpsc::send`. All heavy work (resampling, VAD, the router feed) happens on the consumer thread, so capture can never be stalled by VAD inference.

### 7.2 Resample to 16 kHz / 30 ms frames

The consumer thread (`run_consumer`, `recorder.rs:467`) creates one `FrameResampler` whose output rate is the Whisper constant and whose frame duration is 30 ms:

```rust
// src-tauri/src/audio_toolkit/audio/recorder.rs:477-481
let mut frame_resampler = FrameResampler::new(
    in_sample_rate as usize,
    constants::WHISPER_SAMPLE_RATE as usize,   // 16000 (constants.rs:1)
    Duration::from_millis(30),
);
```

`FrameResampler::new` (`resampler.rs:16-35`) only constructs an FFT resampler when `in_hz != out_hz` (`.then(...)` at `:23`); a 16 kHz device skips resampling entirely. `push` (`resampler.rs:37-64`) feeds input in fixed chunks through the FFT and then `emit_frames` (`:86-98`) accumulates output into exactly `frame_samples` (= 16000 × 0.03 = **480 samples**) frames, invoking the emit closure per complete frame. This 480-sample/30-ms frame is the unit the VAD and the router both consume — it matches `SILERO_FRAME_SAMPLES` (`silero.rs:10-11`).

The consumer loop wires each emitted frame into `handle_frame` (`recorder.rs:558-567`):

```rust
// src-tauri/src/audio_toolkit/audio/recorder.rs:558-567
frame_resampler.push(&raw, &mut |frame: &[f32]| {
    handle_frame(frame, recording, vad_policy, &vad, &audio_cb, &mut processed_samples)
});
```

### 7.3 The VAD gate

`handle_frame` (`recorder.rs:508-541`) is the keep/drop decision and the **exact place streaming is gated on speech**:

```rust
// src-tauri/src/audio_toolkit/audio/recorder.rs:508-541
fn handle_frame(samples: &[f32], recording: bool, vad_policy: VadPolicy,
                vad: &Option<VadConfig>, audio_cb: &Option<AudioFrameCallback>,
                out_buf: &mut Vec<f32>) {
    if !recording { return; }
    let mut emit = |buf: &[f32]| {
        out_buf.extend_from_slice(buf);
        if let Some(cb) = audio_cb { cb(buf); }   // ← the router feed lives here
    };
    if vad_policy == VadPolicy::Disabled { emit(samples); return; }
    if let Some(cfg) = vad {
        let mut det = cfg.detector.lock().unwrap();
        match det.push_frame(samples).unwrap_or(VadFrame::Speech(samples)) {
            VadFrame::Speech(buf) => emit(buf),   // speech → forward to recorder buf AND router
            VadFrame::Noise => {}                  // silence → dropped, router never sees it
        }
    } else { emit(samples); }
}
```

Two consequences: (1) the `audio_cb` (the router feed) is called **only for speech frames** when VAD is on — silence never reaches the streaming worker; (2) `out_buf` (the offline transcription buffer) and the streaming router see the *same* filtered frames, so the final batch transcription and the live stream are consistent.

### 7.4 Silero + smoothing — how "speech" is decided

The VAD is a two-layer composite. `SileroVad` (`silero.rs`) wraps the `vad_rs` ONNX Silero model and applies a probability threshold:

```rust
// src-tauri/src/audio_toolkit/vad/silero.rs:32-51
impl VoiceActivityDetector for SileroVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        if frame.len() != SILERO_FRAME_SAMPLES { /* bail */ }
        let result = self.engine.compute(frame).map_err(|e| anyhow::anyhow!("Silero VAD error: {e}"))?;
        if result.prob > self.threshold { Ok(VadFrame::Speech(frame)) }   // threshold = 0.3 (audio.rs:20)
        else { Ok(VadFrame::Noise) }
    }
    fn reset(&mut self) { self.engine.reset(); }   // clears LSTM hidden state per session
}
```

`SmoothedVad` (`smoothed.rs`) wraps any boolean VAD with a small state machine — **prefill** (replay buffered frames on onset so the first phoneme isn't clipped), **onset** (require N consecutive voice frames before declaring speech), and **hangover** (keep emitting N frames after voice ends so trailing consonants survive):

```rust
// src-tauri/src/audio_toolkit/vad/smoothed.rs:51-95 (state machine)
match (self.in_speech, is_voice) {
    (false, true) => {                       // potential onset
        self.onset_counter += 1;
        if self.onset_counter >= self.onset_frames {   // = 2 (vad/mod.rs:6)
            self.in_speech = true;
            self.hangover_counter = self.hangover_frames;
            self.onset_counter = 0;
            self.temp_out.clear();
            for buf in &self.frame_buffer { self.temp_out.extend(buf); }  // prefill replay
            Ok(VadFrame::Speech(&self.temp_out))
        } else { Ok(VadFrame::Noise) }
    }
    (true, true) => { self.hangover_counter = self.hangover_frames; Ok(VadFrame::Speech(frame)) }
    (true, false) => {                       // potential offset
        if self.hangover_counter > 0 { self.hangover_counter -= 1; Ok(VadFrame::Speech(frame)) }
        else { self.in_speech = false; Ok(VadFrame::Noise) }
    }
    (false, false) => { self.onset_counter = 0; Ok(VadFrame::Noise) }
}
```

Tuning constants (`vad/mod.rs:3-6`): `VAD_PREFILL_FRAMES = 15`, `VAD_OFFLINE_HANGOVER_FRAMES = 15`, `VAD_STREAMING_HANGOVER_FRAMES = 55`, `VAD_ONSET_FRAMES = 2`. The streaming profile uses a **much longer hangover tail (55 × 30 ms ≈ 1.65 s)** so the live stream doesn't flap closed between words. Because offline and streaming never run concurrently, Handy keeps *one* Silero engine and reconfigures its hangover per session (`recorder.rs:581-587`, `audio.rs:135-156`).

### 7.5 THE SEAM — recorder callback → `StreamRouter::feed`

The `AudioFrameCallback` type and its registration (`recorder.rs:66-126`):

```rust
// src-tauri/src/audio_toolkit/audio/recorder.rs:66-68
/// Callback invoked with each 16 kHz mono frame that passes the active capture
/// policy while recording. Used to feed a live streaming transcription as audio arrives.
pub type AudioFrameCallback = Arc<dyn Fn(&[f32]) + Send + Sync + 'static>;
```

It is wired in `create_audio_recorder`, which captures the shared `Arc<StreamRouter>` directly (not via Tauri state — see the router's doc comment):

```rust
// src-tauri/src/managers/audio.rs:163-168
.with_audio_callback({
    let router = stream_router;
    move |frame| {
        router.feed(frame);
    }
});
```

And `StreamRouter::feed` (`transcription.rs:144-151`) is engineered to be **nearly free when no stream is active** — the whole point of the always-on microphone that never wastes CPU when the user isn't dictating:

```rust
// src-tauri/src/managers/transcription.rs:96-151 (struct + feed)
/// … The recorder holds an `Arc<StreamRouter>` directly, so a frame with no
/// stream pending costs a single relaxed atomic load — no Tauri state lookup,
/// no mutex lock.
pub struct StreamRouter {
    tx: Mutex<Option<mpsc::Sender<StreamCmd>>>,
    open: Arc<AtomicBool>,
}
// …
pub fn feed(&self, frame: &[f32]) {
    if !self.open.load(Ordering::Relaxed) { return; }      // hot path: one atomic load
    if let Some(tx) = self.tx.lock().unwrap().as_ref() {
        let _ = tx.send(StreamCmd::Feed(frame.to_vec()));   // into the worker's mpsc (slice 1)
    }
}
```

So the full data path is: **cpal RT thread** → `AudioChunk::Samples` (mpsc) → **consumer thread** → `FrameResampler` (→480-sample/30-ms frames) → `SmoothedVad` gate → on `Speech`, the `emit` closure → `router.feed(frame)` → `StreamCmd::Feed` on the worker's channel. From there the streaming worker (slice 1) owns the story. The seam quoted above (`audio.rs:163-168`) is the single line a fork changes to redirect live audio.

---

## 8. VoiceTypr takeaways

### High

- **Generate the IPC contract with tauri-specta; stop hand-writing `invoke` strings.** Adopt `collect_commands!`/`collect_events!` + a debug-build `specta_builder.export("../src/bindings.ts")` (Handy: `lib.rs:530`, `642`). Every command/event/type becomes compile-checked across the Rust↔TS boundary. Renames and signature changes stop being runtime `undefined`s. *Rationale: eliminates an entire bug class and makes refactors safe.*
- **Adopt the explicit manager + `Arc` + `State` construction graph.** Put all domain logic behind managers constructed in one ordered `initialize_core_logic`-style function, each `Arc::new` + `app_handle.manage`, dependencies injected by constructor arg (`lib.rs:158-182`). *Rationale: the object graph is legible in one screen, additions are purely additive, removals are compile errors — maximally forkable.*
- **Adopt Silero VAD with onset/prefill/hangover smoothing to gate the stream.** The `SmoothedVad` state machine (`smoothed.rs:51-95`) with a long streaming hangover (55 frames ≈ 1.65 s) is what stops the live text from flapping. Feed the streaming worker *only* speech frames via the `emit` closure (`recorder.rs:520-540`). *Rationale: directly improves the "smooth streaming" feel and cuts worker load during silence.*
- **Make the per-frame audio→worker seam a cheap `Arc` callback with an atomic open-flag fast path.** Mirror `StreamRouter` (`transcription.rs:96-151`): the recorder holds the router by `Arc`, `feed()` does one relaxed atomic load and returns when no stream is open. *Rationale: lets an always-on microphone cost ~nothing when idle — core to Handy's "lightweight" feel.*

### Med

- **Keep the real-time audio thread minimal.** Do only mono mixdown + non-blocking `mpsc::send` in the cpal callback (`recorder.rs:310-344`); push resampling/VAD/router onto a consumer thread. *Rationale: prevents VAD inference or FFT work from glitching capture.*
- **Resample to 16 kHz in fixed 30-ms (480-sample) frames, matching the VAD model's expected frame.** Use `FrameResampler` with `in_hz != out_hz` short-circuit so native-16 kHz devices skip the FFT (`resampler.rs:23`). *Rationale: correct Whisper input, and zero overhead when no resample is needed.*
- **Persist all settings as one JSON blob under one store key, with per-field `#[serde(default)]` + idempotent read-time migrations.** (`settings.rs:323-441`, `936-967`, `970-1030`). *Rationale: schema evolution never breaks old stores; no per-field DB.*
- **Frontend settings via a Zustand store with a typed `settingUpdaters` map + optimistic update/rollback + backend re-fetch on events.** (`settingsStore.ts:76-164`, `279-308`). *Rationale: settings UI feels instant; backend stays source of truth; the map is end-to-end type-checked against generated types.*
- **Defer side-effectful initialization (Accessibility/shortcut/mic) to post-onboarding.** Follow Handy's "not initialized in `setup`; frontend calls `initialize_*` after onboarding" pattern (`lib.rs:150-153`, `184-187`). *Rationale: macOS permission prompts never ambush the user on first launch.*

### Low

- **Tier logging into console/file/webview targets with the webview target gated on debug mode.** (`lib.rs:660-690`, `54-60`). *Rationale: normal runs never broadcast sensitive log lines onto the frontend bus; debug live-log viewer still works.*
- **Pre-warm expensive one-shot enumeration (GPU/accelerator discovery) on a background thread at startup.** (`lib.rs:781-783`). *Rationale: opening an Advanced-settings panel never causes a UI freeze.*
- **Single-instance with a CLI-arg forwarder for `--toggle-*`** (`lib.rs:701-715`). *Rationale: second launch improves the running instance instead of double-starting — reinforces "one lightweight process."*
- **Keep one VAD engine, reconfigure hangover per session** (`audio.rs:135-156`, `recorder.rs:581-587`). *Rationale: halves VAD memory/CPU vs. keeping separate offline + streaming engines resident.*

---

## 9. Open questions / risks

- **`StreamRouter::feed` clones every frame** (`tx.send(StreamCmd::Feed(frame.to_vec()))`, `transcription.rs:149`). This is an allocation per 30 ms speech frame on the consumer thread. Acceptable at 16 kHz/30 ms (~33 allocs/s while speaking), but a fork chasing lower latency could use a `crossbeam` ring buffer or a `&[f32]`-borrowing channel. Not verified whether this shows up in profiles.
- **The VAD gate `unwrap_or(VadFrame::Speech(samples))` fallback** (`recorder.rs:534`) means a Silero inference *error* is treated as speech, not silence. This is a graceful-degradation choice (better to transcribe noise than drop a real utterance), but a fork wanting strict silence-trimming should change this to `VadFrame::Noise`.
- **The `vad_rs` crate is the one opaque dependency** in this slice — `SileroVad` delegates to `Vad::new`/`compute`/`reset` (`silero.rs:25,43,56`). Its ONNX model path is passed in from `create_audio_recorder`'s `vad_path` arg (`audio.rs:131`); where that path resolves (bundled resource vs. downloaded) is outside this slice and not verified here.
- **Settings is a single flat struct (~50 fields)** with one `change_*_setting` command each (~120 commands in the registry). This is extremely auditable but means the command surface grows linearly with settings; a fork adding many settings might prefer a generic `set_setting(key, value)` command (Handy explicitly chose not to — the typed surface is the point).
- **`get_settings` writes on every read that triggers a migration** (`settings.rs:946-948`). Safe because migrations are idempotent, but means a read-only inspector (e.g. headless `--list-models`) can mutate the store on first run after an upgrade. Not observed to cause issues, but worth knowing.
- **The `managers/model_capabilities` and `gguf_meta` modules** were declared but not torn down here (out of scope: model catalog is slice 5). A fork studying model selection should read those directly.
