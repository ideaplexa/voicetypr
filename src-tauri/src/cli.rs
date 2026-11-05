use std::env;
use std::error::Error;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand};
use serde_json::json;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::Manager;

use crate::audio::recorder::AudioRecorder;
use crate::commands::ai::{ai_provider_key_names, cache_ai_api_key, CacheApiKeyArgs};
use crate::commands::audio::transcribe_audio_file_for_cli;
use crate::commands::keyring::keyring_get;
use crate::commands::license::check_license_status;
use crate::commands::model::get_model_status;
use crate::commands::remote::load_remote_settings;
use crate::commands::settings::get_settings;
use crate::parakeet::messages::{ParakeetStreamConfig, ParakeetStreamEngine};
use crate::parakeet::ParakeetManager;
use crate::remote::client::{
    self, RemoteServerConnection, TranscriptionRequest, TranscriptionSource,
};
use crate::remote::lifecycle::RemoteServerManager;
use crate::state::AppState;
use crate::transcription::{
    TranscriptionJob, TranscriptionResult, TranscriptionSource as ContractSource,
};
use crate::whisper::cache::TranscriberCache;
use crate::whisper::gpu_sidecar::GpuSidecarClient;
use crate::whisper::manager::WhisperManager;

#[derive(Parser, Debug)]
#[command(name = "voicetypr", version, about = "Voicetypr CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// Polish stdin or --text using the desktop Polish pipeline.
    Polish(crate::ai::polish_cli::PolishArgs),
    #[command(hide = true, name = "polish-eval")]
    PolishEval(crate::ai::polish_eval::EvalArgs),
    /// Show version, the selected model/engine, language, and which engines are available.
    Status(StatusArgs),
    /// List speech models and whether each one is downloaded.
    Models(OutputArgs),
    /// Transcribe an audio file locally, or via a remote Voicetypr server with --server.
    Transcribe(TranscribeArgs),
    /// Record from the microphone until silence, then transcribe the result.
    /// On transcription failure, prints a retry WAV path on stderr. Retry files
    /// expire after 24 hours (cleaned at CLI/desktop startup) or on app reset.
    Record(RecordArgs),
    /// Replay a WAV through the hidden Parakeet streaming prototype and print timing spans.
    #[command(hide = true, name = "stream-bench")]
    StreamBench(StreamBenchArgs),
}

impl CliCommand {
    /// Whether the invoked subcommand requested JSON output, so failures can be emitted
    /// as a parseable error object too.
    fn wants_json(&self) -> bool {
        match self {
            CliCommand::Polish(a) => a.json,
            CliCommand::PolishEval(_) => false,
            CliCommand::Status(a) => a.json,
            CliCommand::Models(a) => a.json,
            CliCommand::Transcribe(a) => a.json,
            CliCommand::Record(a) => a.json,
            CliCommand::StreamBench(_) => true,
        }
    }
}

#[derive(Args, Debug, Clone)]
struct OutputArgs {
    /// Emit JSON instead of human-readable text.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct StatusArgs {
    /// Emit JSON instead of human-readable text.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct TranscribeArgs {
    /// Path to the audio file to transcribe (wav, mp3, m4a, flac, ...).
    #[arg(long)]
    file: PathBuf,
    /// Model to use (e.g. "base", "large-v3-turbo"); defaults to the app's selected model. Run `voicetypr models` to list installed models.
    #[arg(long)]
    model: Option<String>,
    /// Engine override ("whisper" or "parakeet"); defaults to the selected model's engine.
    #[arg(long)]
    engine: Option<String>,
    /// Override the app's speech_language setting for this transcription run.
    #[arg(long)]
    language: Option<String>,
    /// Override Whisper audio context for this transcription run. Only applies when present.
    #[arg(long)]
    audio_ctx: Option<i32>,
    /// Override whisper speed mode (Metal flash-attention) for this run.
    /// Absent => fall back to the app setting; `--speed-mode true|false` forces it.
    #[arg(long)]
    speed_mode: Option<bool>,
    /// Transcribe via a remote Voicetypr server given as host:port (e.g. 192.168.1.10:47842) instead of locally.
    #[arg(long)]
    server: Option<String>,
    /// Remote server password. Prefer --password-stdin or VOICETYPR_REMOTE_PASSWORD for automation.
    #[arg(long)]
    password: Option<String>,
    /// Read the remote server password from stdin (safer than --password for automation).
    #[arg(long)]
    password_stdin: bool,
    /// Emit a JSON result (text, words, metadata); default prints just the transcript text.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct RecordArgs {
    /// Stop recording automatically after a stretch of silence (currently required).
    #[arg(long)]
    until_silence: bool,
    /// Model to use (e.g. "base", "large-v3-turbo"); defaults to the app's selected model. Run `voicetypr models` to list installed models.
    #[arg(long)]
    model: Option<String>,
    /// Engine override ("whisper" or "parakeet"); defaults to the selected model's engine.
    #[arg(long)]
    engine: Option<String>,
    /// Transcribe via a remote Voicetypr server given as host:port (e.g. 192.168.1.10:47842) instead of locally.
    #[arg(long)]
    server: Option<String>,
    /// Remote server password. Prefer --password-stdin or VOICETYPR_REMOTE_PASSWORD for automation.
    #[arg(long)]
    password: Option<String>,
    /// Read the remote server password from stdin (safer than --password for automation).
    #[arg(long)]
    password_stdin: bool,
    /// Emit a JSON result (text, words, metadata, stop_reason); default prints just the transcript text.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct StreamBenchArgs {
    /// Path to an i16 WAV file to replay at real-time pace.
    #[arg(long)]
    file: PathBuf,
    /// Parakeet model to use; defaults to the app's selected model.
    #[arg(long)]
    model: Option<String>,
    /// Streaming engine to benchmark; auto selects the production engine for the model.
    #[arg(long, default_value = "auto")]
    engine: String,
    /// EOU chunk size in milliseconds: 160, 320, or 1280.
    #[arg(long, default_value_t = 320)]
    chunk_ms: u16,
    /// Use .streaming with hypothesisChunkSeconds tuned to 0.5.
    #[arg(long)]
    tuned_hypothesis_500ms: bool,
    /// Use the live-preview window geometry (1s chunk / 0.5s right context).
    #[arg(long)]
    preview_geometry: bool,
}

pub fn maybe_run_from_env_with_context(
    context: tauri::Context<tauri::Wry>,
) -> Result<bool, Box<dyn Error>> {
    crate::initialize_tls();
    let first_arg = env::args().nth(1);
    let Some(first_arg) = first_arg.as_deref() else {
        return Ok(false);
    };
    let is_help_or_version = matches!(first_arg, "-h" | "--help" | "help" | "-V" | "--version");
    if !is_help_or_version
        && !matches!(
            first_arg,
            "status"
                | "models"
                | "transcribe"
                | "record"
                | "stream-bench"
                | "polish"
                | "polish-eval"
        )
    {
        return Ok(false);
    }

    // Windows release builds are GUI-subsystem (see main.rs `windows_subsystem = "windows"`),
    // so the process starts with no console and CLI output would vanish into a null handle.
    // We've confirmed this is a CLI route, so attach the parent terminal's console before
    // any println!/eprintln! or clap --help/--version output is produced.
    #[cfg(windows)]
    attach_parent_console();

    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            use clap::error::ErrorKind;
            // Help/version aren't failures: let clap print them to stdout and exit 0.
            if matches!(
                e.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                    | ErrorKind::DisplayVersion
            ) {
                e.exit();
            }
            // A real usage error: in --json mode emit a parseable object so agents can read
            // why parsing failed; otherwise use clap's human-readable usage output.
            if matches!(first_arg, "polish" | "polish-eval") {
                eprintln!(
                    "{}",
                    format_cli_error(
                        "Invalid Polish CLI arguments; use --help",
                        env::args().skip(1).any(|a| a == "--json")
                    )
                );
                std::process::exit(2);
            }
            if env::args().skip(1).any(|a| a == "--json") {
                eprintln!("{}", format_cli_error(&e.to_string(), true));
                std::process::exit(2);
            }
            e.exit();
        }
    };
    let Some(command) = cli.command else {
        return Ok(false);
    };

    // Fixtures run before Tauri setup, secure-store reads, license checks or HTTP.
    if let CliCommand::PolishEval(args) = &command {
        if args.fixture.is_some() {
            if let Err(error) = crate::ai::polish_eval::run_fixture(args) {
                emit_cli_error(&*error, false);
                std::process::exit(1);
            }
            return Ok(true);
        }
    }
    let measurement = matches!(command, CliCommand::Polish(_) | CliCommand::PolishEval(_));
    let wants_json = command.wants_json();
    if let CliCommand::Transcribe(args) = &command {
        // Before app setup, license checks and our own startup expiry sweep.
        crate::recording::file_jobs::refresh_retry(&args.file);
    }
    let result = tauri::async_runtime::block_on(async move {
        let app = build_cli_app(context).await?;
        let app_handle = app.handle().clone();
        crate::recording::file_jobs::startup(app_handle.path().app_data_dir()?).await;
        warm_ai_key_cache(&app_handle).await?;
        if !measurement {
            let _ = check_license_status(app_handle.clone()).await;
        }

        let outcome: Result<(), Box<dyn Error>> = async {
            match command {
                CliCommand::Polish(args) => crate::ai::polish_cli::run(&app_handle, args).await?,
                CliCommand::PolishEval(args) => {
                    crate::ai::polish_eval::run_live(&app_handle, args).await?
                }
                CliCommand::Status(args) => run_status(&app_handle, args).await?,
                CliCommand::Models(args) => run_models(&app_handle, args).await?,
                CliCommand::Transcribe(args) => run_transcribe(&app_handle, args).await?,
                CliCommand::Record(args) => run_record(&app_handle, args).await?,
                CliCommand::StreamBench(args) => run_stream_bench(&app_handle, args).await?,
            }
            Ok(())
        }
        .await;

        // #28: unload cached Whisper models (and their Metal buffers) before this process
        // exits — on success OR failure — so the ggml-metal device destructor does not
        // assert on a live residency set and SIGABRT after the output was already produced.
        if let Some(cache) = app_handle.try_state::<AsyncMutex<TranscriberCache>>() {
            cache.lock().await.clear();
        }
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        if let Some(manager) = app_handle.try_state::<ParakeetManager>() {
            manager.unload_onnx().await;
        }

        outcome
    });

    // Format failures for the human or the agent: --json gets a parseable {"error": ...}
    // object on stderr, everyone else gets a plain "Error: ..." line. Either way exit 1 so
    // scripts and agents can branch on the status code.
    if let Err(e) = result {
        emit_cli_error(&*e, wants_json);
        std::process::exit(1);
    }

    Ok(true)
}

/// Print a CLI error to stderr, honoring the subcommand's --json flag.
fn emit_cli_error(err: &dyn Error, json: bool) {
    eprintln!("{}", format_cli_error(&err.to_string(), json));
}

/// Format a CLI error message: a parseable `{"error": "..."}` object in --json mode (so
/// agents can read the failure), or a plain `Error: ...` line otherwise.
fn format_cli_error(message: &str, json: bool) -> String {
    if json {
        serde_json::to_string(&json!({ "error": message }))
            .unwrap_or_else(|_| format!("{{\"error\":\"{}\"}}", message.replace('"', "'")))
    } else {
        format!("Error: {message}")
    }
}

/// Attach this (GUI-subsystem) process to the parent terminal's console so CLI output is
/// visible on Windows. No-op when there is no parent console (e.g. a double-click launch).
///
/// Only streams the shell left unset are pointed at the console, so any redirection is
/// preserved: `voicetypr ... > out.json`, `... 2> err.txt`, and
/// `echo pw | voicetypr --password-stdin` keep their file/pipe handles, and stdin is never
/// overridden. MUST run before the first println!/eprintln!: Rust resolves the std handles
/// lazily on first use, so re-pointing them afterwards would be too late.
#[cfg(windows)]
fn attach_parent_console() {
    use std::fs::OpenOptions;
    use std::mem;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_HANDLE, STD_OUTPUT_HANDLE,
    };

    // SAFETY: kernel32 FFI; ATTACH_PARENT_PROCESS is the documented sentinel process id.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            return;
        }
    }

    // A stream needs the console screen buffer only when the shell did not already wire it to
    // a file/pipe; GetStdHandle returns NULL for an unset stream on a GUI-subsystem process.
    let needs_console = |which: STD_HANDLE| -> bool {
        // SAFETY: kernel32 FFI.
        match unsafe { GetStdHandle(which) } {
            Ok(h) => h.0.is_null() || h == INVALID_HANDLE_VALUE,
            Err(_) => true,
        }
    };

    let out_unset = needs_console(STD_OUTPUT_HANDLE);
    let err_unset = needs_console(STD_ERROR_HANDLE);
    if !out_unset && !err_unset {
        return;
    }

    // Point only the unset write streams at the console screen buffer; redirected streams keep
    // their inherited handle. We leak the File (mem::forget) because SetStdHandle does not
    // duplicate the handle and we need it for the process lifetime.
    if let Ok(file) = OpenOptions::new().write(true).open("CONOUT$") {
        let handle = HANDLE(file.as_raw_handle());
        // SAFETY: valid console handle held for the process lifetime via mem::forget.
        unsafe {
            if out_unset {
                let _ = SetStdHandle(STD_OUTPUT_HANDLE, handle);
            }
            if err_unset {
                let _ = SetStdHandle(STD_ERROR_HANDLE, handle);
            }
        }
        mem::forget(file);
    }
}

async fn build_cli_app(
    context: tauri::Context<tauri::Wry>,
) -> Result<tauri::App<tauri::Wry>, Box<dyn Error>> {
    #[cfg(not(target_os = "windows"))]
    crate::secure_store::initialize_encryption_key()?;

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .build(context)?;

    crate::ai::agent_cli::initialize_private_input(app.path().app_local_data_dir()?);

    #[cfg(target_os = "windows")]
    crate::secure_store::windows_identity::initialize(&app.path().app_data_dir()?)?;

    let models_dir = app.path().app_data_dir()?.join("models");
    std::fs::create_dir_all(&models_dir)?;
    let parakeet_dir = models_dir.join("parakeet");
    std::fs::create_dir_all(&parakeet_dir)?;

    app.manage(AsyncRwLock::new(WhisperManager::new(models_dir.clone())));
    app.manage(ParakeetManager::new(parakeet_dir));
    app.manage(AsyncMutex::new(TranscriberCache::new()));
    app.manage(GpuSidecarClient::new());
    app.manage(AppState::new());
    app.manage(AsyncMutex::new(RemoteServerManager::new()));
    app.manage(AsyncMutex::new(load_remote_settings(app.handle())));

    Ok(app)
}

async fn warm_ai_key_cache(app: &tauri::AppHandle) -> Result<(), Box<dyn Error>> {
    for key_name in ai_provider_key_names() {
        if let Some(api_key) = keyring_get(app.clone(), key_name.clone())? {
            // key_name is "ai_api_key_{provider}"; recover the provider id.
            let provider = key_name
                .strip_prefix("ai_api_key_")
                .map(str::to_string)
                .unwrap_or(key_name.clone());
            cache_ai_api_key(app.clone(), CacheApiKeyArgs { provider, api_key }).await?;
        }
    }
    Ok(())
}

async fn run_status(app: &tauri::AppHandle, args: StatusArgs) -> Result<(), Box<dyn Error>> {
    let settings = get_settings(app.clone()).await?;
    let availability = crate::recognition_availability_snapshot(app).await;

    if args.json {
        let payload = json!({
            "version": env!("CARGO_PKG_VERSION"),
            "settings": {
                "current_model": settings.current_model,
                "current_model_engine": settings.current_model_engine,
                "speech_language": settings.speech_language,
                "transcription_task": settings.transcription_task,
                "final_text_language": settings.final_text_language,
            },
            "availability": availability,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("version:  {}", env!("CARGO_PKG_VERSION"));
        println!(
            "model:    {} ({})",
            settings.current_model, settings.current_model_engine
        );
        println!("language: {}", settings.speech_language);
        println!("engines:  {}", format_availability(&availability));
    }
    Ok(())
}

async fn run_models(app: &tauri::AppHandle, args: OutputArgs) -> Result<(), Box<dyn Error>> {
    let response = get_model_status(app.clone()).await?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        for m in &response.models {
            let state = if m.downloaded {
                "ready"
            } else {
                "not downloaded"
            };
            println!("{:40} {:10} {}", m.display_name, m.engine, state);
        }
    }
    Ok(())
}

async fn run_transcribe(
    app: &tauri::AppHandle,
    args: TranscribeArgs,
) -> Result<(), Box<dyn Error>> {
    if !args.file.exists() {
        return Err(format!("Audio file not found: {}", args.file.display()).into());
    }
    if args.server.is_none() {
        check_license_status(app.clone()).await?;
    }
    let payload = if let Some(server) = args.server.as_deref() {
        let password = resolve_password(args.password.clone(), args.password_stdin)?;
        transcribe_via_remote(app, &args.file, server, password).await?
    } else {
        let settings = get_settings(app.clone()).await?;
        let (model, engine) =
            resolve_cli_model_selection(args.model.as_deref(), args.engine.as_deref(), &settings)?;
        let t = transcribe_audio_file_for_cli(
            app.clone(),
            args.file.to_string_lossy().to_string(),
            model.clone(),
            engine.clone(),
            args.language.clone(),
            args.audio_ctx,
            args.speed_mode,
        )
        .await?;
        json!({ "text": t.text, "words": t.words, "metadata": t.metadata, "model": model, "engine": engine })
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", payload["text"].as_str().unwrap_or_default());
    }
    Ok(())
}

fn resolve_cli_model_selection(
    model: Option<&str>,
    engine: Option<&str>,
    settings: &crate::commands::settings::Settings,
) -> Result<(String, Option<String>), String> {
    let model = model
        .or_else(|| (!settings.current_model.is_empty()).then_some(settings.current_model.as_str()))
        .ok_or_else(|| "No model specified and no model is selected in the app. Pass --model <name> (run `voicetypr models` to list installed models) or choose a default model in Voicetypr settings.".to_string())?;
    let engine = engine
        .or_else(|| {
            (!settings.current_model_engine.is_empty())
                .then_some(settings.current_model_engine.as_str())
        })
        .map(str::to_owned)
        // Pin the inferred provider before replacing its shorthand with an API
        // model id: the engine resolver cannot infer Soniox from stt-async-v6.
        .or_else(|| crate::cloud_stt::CloudProvider::from_id(model).map(|p| p.id().to_string()));
    let provider = engine
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
        .and_then(crate::cloud_stt::CloudProvider::from_id);
    let model = provider
        .map(|provider| provider.canonical_model_id(model))
        .unwrap_or(model)
        .to_string();
    Ok((model, engine))
}

async fn run_record(app: &tauri::AppHandle, args: RecordArgs) -> Result<(), Box<dyn Error>> {
    if !args.until_silence {
        return Err(
            "`record` currently supports only stop-on-silence. Re-run with --until-silence.".into(),
        );
    }

    check_license_status(app.clone()).await?;
    let settings = get_settings(app.clone()).await?;
    let recordings_dir = crate::recording::file_jobs::cli_directory(&app.path().app_data_dir()?)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S%3f");
    let uuid_part = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let output_path = recordings_dir.join(format!("cli-recording-{}_{}.wav", timestamp, uuid_part));
    let mut recording_guard = TempRecording {
        path: output_path.clone(),
        delete_on_drop: true,
    };

    let mut recorder = AudioRecorder::new();
    recorder.start_recording(
        output_path
            .to_str()
            .ok_or_else(|| "Invalid recording path".to_string())?,
        settings.selected_microphone.clone(),
        false,
        0,
        std::sync::Arc::new(|| false),
        None,
        std::time::Instant::now(),
    )?;
    eprintln!("Recording… stop by silence.");
    let stop_message = recorder.wait_for_recording_end()?;
    // Only finalized, usable captures are offered for retry.
    recording_guard.capture_finished()?;

    let payload = if let Some(server) = args.server.as_deref() {
        let password = resolve_password(args.password.clone(), args.password_stdin)?;
        let mut payload = transcribe_via_remote(app, &output_path, server, password).await?;
        payload["stop_reason"] = json!(stop_message);
        payload
    } else {
        let (model, engine) =
            resolve_cli_model_selection(args.model.as_deref(), args.engine.as_deref(), &settings)?;
        let t = transcribe_audio_file_for_cli(
            app.clone(),
            output_path.to_string_lossy().to_string(),
            model.clone(),
            engine.clone(),
            None,
            None,
            None,
        )
        .await?;
        json!({ "text": t.text, "words": t.words, "metadata": t.metadata, "model": model, "engine": engine, "stop_reason": stop_message })
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", payload["text"].as_str().unwrap_or_default());
    }
    recording_guard.delete_on_drop = true;
    Ok(())
}

#[derive(Default)]
struct StreamBenchState {
    first_partial_ms: Option<u128>,
    first_confirmed_ms: Option<u128>,
    partials: u64,
    confirmed_partials: u64,
}

fn resolve_stream_bench_engine(
    model: &str,
    requested_engine: &str,
) -> Result<ParakeetStreamEngine, String> {
    match requested_engine {
        "auto" => match model {
            "parakeet-unified-640ms" => Ok(ParakeetStreamEngine::UnifiedEnglish),
            "nemotron-multilingual-1120ms" => Ok(ParakeetStreamEngine::NemotronMultilingual),
            _ => Ok(ParakeetStreamEngine::DecodeAhead),
        },
        "sliding_window" => Ok(ParakeetStreamEngine::SlidingWindow),
        "eou" => Ok(ParakeetStreamEngine::Eou),
        "decode_ahead" => Ok(ParakeetStreamEngine::DecodeAhead),
        "unified_english" => Ok(ParakeetStreamEngine::UnifiedEnglish),
        "nemotron_multilingual" => Ok(ParakeetStreamEngine::NemotronMultilingual),
        other => Err(format!("Unsupported stream engine: {other}")),
    }
}

async fn run_stream_bench(
    app: &tauri::AppHandle,
    args: StreamBenchArgs,
) -> Result<(), Box<dyn Error>> {
    if !args.file.exists() {
        return Err(format!("Audio file not found: {}", args.file.display()).into());
    }

    let settings = get_settings(app.clone()).await?;
    let model = args
        .model
        .clone()
        .or_else(|| (!settings.current_model.is_empty()).then_some(settings.current_model.clone()))
        .ok_or_else(|| "No model specified and no model is selected in the app.".to_string())?;

    let mut reader = hound::WavReader::open(&args.file)?;
    let spec = reader.spec();
    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err("stream-bench currently accepts only 16-bit PCM WAV input".into());
    }
    if spec.sample_rate == 0 || spec.channels == 0 {
        return Err("Invalid WAV sample rate or channel count".into());
    }
    let samples = reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?;
    let total_frames = samples.len() / usize::from(spec.channels);
    let duration_ms = ((total_frames as f64 / f64::from(spec.sample_rate)) * 1000.0).round() as u64;

    let engine = resolve_stream_bench_engine(&model, &args.engine)?;
    if matches!(engine, ParakeetStreamEngine::Eou) && !matches!(args.chunk_ms, 160 | 320 | 1280) {
        return Err("--chunk-ms must be 160, 320, or 1280 for --engine eou".into());
    }

    let config = if args.preview_geometry {
        ParakeetStreamConfig::preview_geometry()
    } else if args.tuned_hypothesis_500ms {
        ParakeetStreamConfig::tuned_hypothesis_500ms()
    } else {
        ParakeetStreamConfig::streaming()
    };

    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return Err("Parakeet CLI streaming is unavailable on Windows.".into());
    }

    let parakeet_manager = app.state::<ParakeetManager>();
    parakeet_manager
        .load_model(app, &model)
        .await
        .map_err(|error| format!("Failed to load Parakeet model: {}", error))?;
    if matches!(engine, ParakeetStreamEngine::Eou) {
        let status = parakeet_manager
            .eou_model_status(app, args.chunk_ms)
            .await
            .map_err(|error| format!("Failed to check EOU model status: {}", error))?;
        if !status.downloaded {
            parakeet_manager
                .download_eou_model(app, args.chunk_ms, |_downloaded, _total, _phase| {})
                .await
                .map_err(|error| format!("Failed to download EOU model: {}", error))?;
        }
    }

    let started = Instant::now();
    let state = Arc::new(Mutex::new(StreamBenchState::default()));
    let callback_state = state.clone();
    let callback_started = started;
    let handle = parakeet_manager
        .open_stream(
            crate::parakeet::manager::ParakeetStreamRequest {
                app: app.clone(),
                model_name: &model,
                language: None,
                sample_rate: spec.sample_rate,
                channels: spec.channels,
                engine,
                chunk_ms: matches!(engine, ParakeetStreamEngine::Eou).then_some(args.chunk_ms),
                config: (!matches!(engine, ParakeetStreamEngine::Eou)).then_some(config.clone()),
            },
            move |partial| {
                if let Ok(mut state) = callback_state.lock() {
                    state.partials += 1;
                    let elapsed = callback_started.elapsed().as_millis();
                    if state.first_partial_ms.is_none() {
                        state.first_partial_ms = Some(elapsed);
                    }
                    if partial.is_confirmed {
                        state.confirmed_partials += 1;
                        if state.first_confirmed_ms.is_none() {
                            state.first_confirmed_ms = Some(elapsed);
                        }
                    }
                }
            },
        )
        .await
        .map_err(|error| format!("Failed to open Parakeet stream: {}", error))?;

    let chunk_frames = (spec.sample_rate / 10).max(1) as usize;
    let chunk_samples = chunk_frames * usize::from(spec.channels);
    let sent_samples = AtomicU64::new(0);
    for chunk in samples.chunks(chunk_samples) {
        handle
            .send_chunk(chunk)
            .map_err(|error| format!("Failed to send stream audio chunk: {}", error))?;
        let frames = chunk.len() / usize::from(spec.channels);
        sent_samples.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        tokio::time::sleep(Duration::from_secs_f64(
            frames as f64 / f64::from(spec.sample_rate),
        ))
        .await;
    }

    let final_text = handle
        .finalize()
        .await
        .map_err(|error| format!("Failed to finalize Parakeet stream: {}", error))?;
    let final_ms = started.elapsed().as_millis();
    let state = state.lock().map_err(|_| "stream bench state poisoned")?;
    if state.partials == 0 && final_text.trim().is_empty() {
        return Err("stream-bench produced no partials and an empty final transcript".into());
    }

    let payload = json!({
        "model": model,
        "file": args.file,
        "engine": args.engine,
        "chunk_ms": if matches!(engine, ParakeetStreamEngine::Eou) { Some(args.chunk_ms) } else { None },
        "config": if args.preview_geometry { "preview_geometry" } else if args.tuned_hypothesis_500ms { "tuned_hypothesis_500ms" } else { "streaming" },
        "sample_rate": spec.sample_rate,
        "channels": spec.channels,
        "duration_ms": duration_ms,
        "sent_samples": sent_samples.load(Ordering::Relaxed),
        "partials": state.partials,
        "confirmed_partials": state.confirmed_partials,
        "first_partial_ms": state.first_partial_ms,
        "first_confirmed_ms": state.first_confirmed_ms,
        "final_ms": final_ms,
        "final": final_text,
    });
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

struct TempRecording {
    path: PathBuf,
    /// Delete unfinished captures and successful transcriptions. A finalized
    /// recording survives transcription errors for an advertised, bounded retry.
    delete_on_drop: bool,
}

impl TempRecording {
    fn capture_finished(&mut self) -> Result<(), &'static str> {
        // Another process's explicit reset may unlink an open capture on Unix.
        if !self.path.is_file() {
            return Err("Recording was removed before transcription");
        }
        self.delete_on_drop = false;
        Ok(())
    }
}

impl Drop for TempRecording {
    fn drop(&mut self) {
        if self.delete_on_drop {
            if let Err(error) = std::fs::remove_file(&self.path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("Failed to remove CLI temporary recording");
                }
            }
        } else if std::fs::metadata(&self.path).is_ok() {
            // Error path: keep the capture so the user can retry transcription
            // instead of losing a long recording to a transient failure.
            eprintln!(
                "Recording preserved for retry for up to 24 hours (or until app reset): {}",
                self.path.display()
            );
        }
    }
}

async fn transcribe_via_remote(
    app: &tauri::AppHandle,
    file: &Path,
    server: &str,
    password: Option<String>,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let settings = get_settings(app.clone()).await?;
    let (host, port) = parse_server(server)?;
    let normalized_file = crate::recording::file_jobs::normalize(file.to_path_buf())
        .await
        .map_err(std::io::Error::other)?;
    let audio_data = std::fs::read(normalized_file.path())?;
    let request = TranscriptionRequest::new(audio_data, TranscriptionSource::Upload)
        .with_language_and_task(
            Some(settings.speech_language.clone()),
            Some(settings.transcription_task.clone()),
        );
    let timeout_ms = client::timeout_ms_for_wav_file(
        normalized_file.path().to_string_lossy().as_ref(),
        TranscriptionSource::Upload,
    );
    let connection = RemoteServerConnection::new(host, port, password);
    let response = client::transcribe_audio(&connection, request, timeout_ms).await?;

    let job = TranscriptionJob::from_legacy_settings(
        ContractSource::RemoteServer,
        "remote",
        response.model.clone(),
        Some(settings.speech_language.clone()),
        settings.transcription_task
            == crate::commands::settings::TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH,
    );
    let transcription = TranscriptionResult::new(&job, response.text)
        .with_transcript_language(response.transcript_language)
        .with_processing_duration_ms(Some(response.duration_ms));
    let writing = crate::writing::process_transcription(app.clone(), transcription)
        .await
        .map_err(|error| std::io::Error::other(error.user_message()))?;

    Ok(json!({
        "text": writing.final_text,
        "output_language": writing.output_language,
        "mode": writing.mode,
        "applied_operations": writing.applied_operations,
        "warnings": writing.warnings,
        "model": response.model,
        "duration_ms": response.duration_ms,
    }))
}

fn resolve_password(
    password: Option<String>,
    password_stdin: bool,
) -> Result<Option<String>, Box<dyn Error>> {
    if password_stdin {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        let trimmed = line.trim().to_owned();
        return Ok(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        });
    }
    if password.is_some() {
        return Ok(password);
    }
    Ok(env::var("VOICETYPR_REMOTE_PASSWORD")
        .ok()
        .filter(|s| !s.is_empty()))
}

fn parse_server(value: &str) -> Result<(String, u16), Box<dyn Error>> {
    let (host, port_str) = value.rsplit_once(':').ok_or_else(|| {
        format!("Invalid --server '{value}': expected host:port, e.g. 192.168.1.10:47842")
    })?;
    if host.is_empty() {
        return Err(format!("Invalid --server '{value}': missing host, expected host:port").into());
    }
    let port = port_str.parse::<u16>().map_err(|_| {
        format!("Invalid --server '{value}': '{port_str}' is not a valid port (1-65535)")
    })?;
    if port == 0 {
        return Err(
            format!("Invalid --server '{value}': '0' is not a valid port (1-65535)").into(),
        );
    }
    Ok((host.to_string(), port))
}

fn format_availability(snap: &crate::RecognitionAvailabilitySnapshot) -> String {
    let parts: Vec<&str> = [
        snap.whisper_available.then_some("whisper"),
        snap.parakeet_available.then_some("parakeet"),
        (snap.cloud_selected && snap.cloud_ready).then_some("cloud"),
        snap.remote_available.then_some("remote"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_reports_executed_soniox_model_not_the_legacy_argument() {
        let settings = crate::commands::settings::Settings {
            current_model_engine: String::new(),
            ..Default::default()
        };
        for engine in ["soniox", "SoNiOx"] {
            for requested in ["stt-async-v5", "stt-async-v6", "soniox", " stt-async-v5 "] {
                let (model, selected_engine) =
                    resolve_cli_model_selection(Some(requested), Some(engine), &settings).unwrap();
                assert_eq!(model, "stt-async-v6");
                assert_eq!(selected_engine.as_deref(), Some(engine));
            }
        }
        assert_eq!(
            resolve_cli_model_selection(Some("soniox"), None, &settings).unwrap(),
            ("stt-async-v6".to_string(), Some("soniox".to_string()))
        );
        for (model, engine) in [
            ("base.en", Some("whisper")),
            ("nova-2", Some("deepgram")),
            ("openai", None),
        ] {
            assert_eq!(
                resolve_cli_model_selection(Some(model), engine, &settings)
                    .unwrap()
                    .0,
                model
            );
        }
    }

    async fn assert_cli_model_resolution(command: &str) {
        use crate::transcription::engines::{resolve_engine_for_model, ActiveEngineSelection};

        // A memory-only runtime and absolute temporary store identifier keep
        // this on the real resolver without GUI, audio, network or user data.
        let dir = tempfile::tempdir().unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = dir.path().to_str().unwrap().to_string();
        let models_dir = dir.path().join("models");
        std::fs::create_dir(&models_dir).unwrap();
        let mut whisper = WhisperManager::new(models_dir.clone());
        let size = whisper.get_models_status()["base.en"].size;
        std::fs::File::create(models_dir.join("base.en.bin"))
            .unwrap()
            .set_len(size)
            .unwrap(); // Sparse catalog-size fixture; never loaded or decoded.
        whisper.refresh_downloaded_status();
        crate::initialize_tls();
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .manage(AsyncMutex::new(
                crate::remote::settings::RemoteSettings::default(),
            ))
            .manage(AsyncRwLock::new(whisper))
            .manage(ParakeetManager::new(dir.path().join("parakeet")))
            .build(context)
            .unwrap();
        assert_eq!(
            tauri_plugin_store::resolve_store_path(app.handle(), "secure.dat").unwrap(),
            dir.path().join("secure.dat")
        );
        crate::secure_store::initialize_encryption_key().unwrap();
        crate::secure_store::secure_set(
            app.handle(),
            crate::cloud_stt::CloudProvider::Soniox.key_name(),
            "unit-test-placeholder-not-a-credential",
        )
        .unwrap();

        let settings = crate::commands::settings::Settings {
            current_model: "stt-async-v5".to_string(),
            current_model_engine: String::new(),
            ..Default::default()
        };
        for (flags, expected_engine, expected_model) in [
            (vec!["--model", "soniox"], "soniox", "stt-async-v6"),
            (
                vec!["--model", "stt-async-v5", "--engine", "soniox"],
                "soniox",
                "stt-async-v6",
            ),
            (vec!["--engine", "soniox"], "soniox", "stt-async-v6"),
            (vec!["--model", "base.en"], "whisper", "base.en"),
        ] {
            let mut argv = vec!["voicetypr", command];
            if command == "record" {
                argv.push("--until-silence");
            } else {
                argv.extend(["--file", "sample.wav"]);
            }
            argv.extend(flags);
            let cli = Cli::try_parse_from(argv).unwrap();
            let (model, engine) = match cli.command.unwrap() {
                CliCommand::Transcribe(args) => resolve_cli_model_selection(
                    args.model.as_deref(),
                    args.engine.as_deref(),
                    &settings,
                )
                .unwrap(),
                CliCommand::Record(args) => resolve_cli_model_selection(
                    args.model.as_deref(),
                    args.engine.as_deref(),
                    &settings,
                )
                .unwrap(),
                _ => unreachable!(),
            };
            let active = resolve_engine_for_model(app.handle(), &model, engine.as_deref())
                .await
                .unwrap();
            assert_eq!(active.engine_name(), expected_engine);
            assert_eq!(active.model_name(), expected_model);
            assert_eq!(
                model, expected_model,
                "CLI JSON must report the executed model"
            );
            match active {
                ActiveEngineSelection::Cloud { provider, .. } => {
                    assert_eq!(provider, crate::cloud_stt::CloudProvider::Soniox);
                    assert_eq!(engine.as_deref(), Some("soniox"));
                }
                ActiveEngineSelection::Whisper { ref model_path, .. } => {
                    assert_eq!(model_path, &models_dir.join("base.en.bin"));
                    assert!(engine.is_none());
                }
                _ => panic!("Unexpected engine selection"),
            }
            let job = TranscriptionJob::from_legacy_settings(
                ContractSource::AudioFile,
                active.engine_name(),
                active.model_name(),
                None,
                false,
            );
            assert_eq!(serde_json::to_value(job).unwrap()["model"], expected_model);
        }
    }

    #[tokio::test]
    async fn transcribe_model_selection_reaches_real_engine_resolver() {
        assert_cli_model_resolution("transcribe").await;
    }

    #[tokio::test]
    async fn record_model_selection_reaches_real_engine_resolver() {
        assert_cli_model_resolution("record").await;
    }
    #[test]
    fn reset_removed_capture_fails_cleanly_without_advertising_a_missing_retry() {
        let data = tempfile::tempdir().unwrap();
        let path = data.path().join("removed.wav");
        std::fs::write(&path, b"capture").unwrap();
        let mut recording = TempRecording {
            path: path.clone(),
            delete_on_drop: true,
        };
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            recording.capture_finished(),
            Err("Recording was removed before transcription")
        );
        assert!(recording.delete_on_drop);
        drop(recording);
    }

    #[test]
    fn retry_refresh_precedes_cli_app_setup_and_startup_sweep() {
        let source = include_str!("cli.rs");
        let entry = source.split("let measurement =").nth(1).unwrap();
        assert!(
            entry.find("file_jobs::refresh_retry(&args.file)").unwrap()
                < entry.find("build_cli_app(context).await").unwrap()
        );
    }

    #[test]
    fn finalized_retry_recording_survives_error_but_success_and_capture_error_delete() {
        let data = tempfile::tempdir().unwrap();
        let dir = crate::recording::file_jobs::cli_directory(data.path()).unwrap();
        let path = dir.join("cli-recording-20261031123000123_abcdef12.wav");
        std::fs::write(&path, b"finalized capture").unwrap();
        drop(TempRecording {
            path: path.clone(),
            delete_on_drop: false,
        });
        assert!(
            path.exists(),
            "transcription errors must retain the advertised retry"
        );
        drop(TempRecording {
            path: path.clone(),
            delete_on_drop: true,
        });
        assert!(!path.exists());
        std::fs::write(&path, b"unfinished capture").unwrap();
        drop(TempRecording {
            path: path.clone(),
            delete_on_drop: true,
        });
        assert!(
            !path.exists(),
            "capture errors must not retain unusable files"
        );
    }

    #[test]
    fn measurement_commands_parse_without_exposing_text_in_debug() {
        let cli = Cli::try_parse_from([
            "voicetypr",
            "polish",
            "--text",
            "private dictation",
            "--keep-words",
            "--style",
            "message",
            "--app-category",
            "chat",
            "--language",
            "en",
            "--json",
        ])
        .unwrap();
        assert!(!format!("{cli:?}").contains("private dictation"));
        assert!(cli.command.as_ref().unwrap().wants_json());
        assert!(matches!(cli.command, Some(CliCommand::Polish(args)) if args.keep_words));
        let cli = Cli::try_parse_from([
            "voicetypr",
            "polish-eval",
            "--fixture",
            "sample.jsonl",
            "--concurrency",
            "2",
            "--out",
            ".tmp/eval",
        ])
        .unwrap();
        assert!(matches!(cli.command, Some(CliCommand::PolishEval(_))));
    }
    #[test]
    fn stream_bench_auto_selects_native_model_engines() {
        assert_eq!(
            resolve_stream_bench_engine("parakeet-unified-640ms", "auto").unwrap(),
            ParakeetStreamEngine::UnifiedEnglish
        );
        assert_eq!(
            resolve_stream_bench_engine("nemotron-multilingual-1120ms", "auto").unwrap(),
            ParakeetStreamEngine::NemotronMultilingual
        );
        assert_eq!(
            resolve_stream_bench_engine("parakeet-tdt-0.6b-v3", "auto").unwrap(),
            ParakeetStreamEngine::DecodeAhead
        );
    }

    #[test]
    fn parse_server_requires_port() {
        assert!(parse_server("localhost").is_err());
        let parsed = parse_server("localhost:47842").unwrap();
        assert_eq!(parsed.0, "localhost");
        assert_eq!(parsed.1, 47842);
    }

    #[test]
    fn transcribe_audio_context_defaults_to_none_and_accepts_override() {
        for (extra, expected) in [(vec![], None), (vec!["--audio-ctx", "512"], Some(512))] {
            let mut argv = vec!["voicetypr", "transcribe", "--file", "sample.wav"];
            argv.extend(extra);
            let cli = Cli::try_parse_from(argv).unwrap();
            assert!(
                matches!(cli.command, Some(CliCommand::Transcribe(args)) if args.audio_ctx == expected)
            );
        }
    }

    #[test]
    fn cli_parses_transcribe_command() {
        let cli = Cli::try_parse_from([
            "voicetypr",
            "transcribe",
            "--file",
            "sample.wav",
            "--server",
            "localhost:47842",
            "--password",
            "secret",
            "--json",
        ])
        .unwrap();

        match cli.command {
            Some(CliCommand::Transcribe(args)) => {
                assert_eq!(args.file, PathBuf::from("sample.wav"));
                assert_eq!(args.server.as_deref(), Some("localhost:47842"));
                assert_eq!(args.password.as_deref(), Some("secret"));
                assert!(args.json);
            }
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn cli_parses_record_command() {
        let cli =
            Cli::try_parse_from(["voicetypr", "record", "--until-silence", "--json"]).unwrap();

        match cli.command {
            Some(CliCommand::Record(args)) => {
                assert!(args.until_silence);
                assert!(args.json);
            }
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn cli_parses_status_json_flag() {
        let cli = Cli::try_parse_from(["voicetypr", "status", "--json"]).unwrap();
        match cli.command {
            Some(CliCommand::Status(args)) => assert!(args.json),
            other => panic!("unexpected command: {:?}", other),
        }

        let cli = Cli::try_parse_from(["voicetypr", "status"]).unwrap();
        match cli.command {
            Some(CliCommand::Status(args)) => assert!(!args.json),
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn cli_parses_models_json_flag() {
        let cli = Cli::try_parse_from(["voicetypr", "models", "--json"]).unwrap();
        match cli.command {
            Some(CliCommand::Models(args)) => assert!(args.json),
            other => panic!("unexpected command: {:?}", other),
        }

        let cli = Cli::try_parse_from(["voicetypr", "models"]).unwrap();
        match cli.command {
            Some(CliCommand::Models(args)) => assert!(!args.json),
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn format_availability_none_when_all_false() {
        let snap = crate::RecognitionAvailabilitySnapshot {
            whisper_available: false,
            parakeet_available: false,
            cloud_selected: false,
            cloud_ready: false,
            remote_selected: false,
            remote_status: crate::remote::settings::ConnectionStatus::default(),
            remote_last_checked: 0,
            remote_available: false,
        };
        assert_eq!(format_availability(&snap), "none");
    }

    #[test]
    fn format_availability_lists_available_engines() {
        let snap = crate::RecognitionAvailabilitySnapshot {
            whisper_available: true,
            parakeet_available: false,
            cloud_selected: true,
            cloud_ready: true,
            remote_selected: false,
            remote_status: crate::remote::settings::ConnectionStatus::default(),
            remote_last_checked: 0,
            remote_available: true,
        };
        assert_eq!(format_availability(&snap), "whisper, cloud, remote");
    }

    #[test]
    fn format_availability_cloud_requires_ready() {
        let snap = crate::RecognitionAvailabilitySnapshot {
            whisper_available: false,
            parakeet_available: false,
            cloud_selected: true,
            cloud_ready: false, // selected but not ready
            remote_selected: false,
            remote_status: crate::remote::settings::ConnectionStatus::default(),
            remote_last_checked: 0,
            remote_available: false,
        };
        assert_eq!(format_availability(&snap), "none");
    }

    #[test]
    fn format_cli_error_json_is_parseable() {
        let s = format_cli_error("boom \"quoted\"", true);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["error"], "boom \"quoted\"");
    }

    #[test]
    fn format_cli_error_plain_is_prefixed() {
        assert_eq!(format_cli_error("boom", false), "Error: boom");
    }

    #[test]
    fn parse_server_rejects_nonnumeric_port() {
        let err = parse_server("host:abc").unwrap_err().to_string();
        assert!(err.contains("not a valid port"), "got: {err}");
    }

    #[test]
    fn parse_server_rejects_missing_host() {
        let err = parse_server(":47842").unwrap_err().to_string();
        assert!(err.contains("missing host"), "got: {err}");
    }

    #[test]
    fn parse_server_rejects_port_zero() {
        let err = parse_server("host:0").unwrap_err().to_string();
        assert!(err.contains("not a valid port"), "got: {err}");
    }

    #[test]
    fn wants_json_reflects_flag() {
        let cli = Cli::try_parse_from(["voicetypr", "status", "--json"]).unwrap();
        assert!(cli.command.unwrap().wants_json());
        let cli = Cli::try_parse_from(["voicetypr", "status"]).unwrap();
        assert!(!cli.command.unwrap().wants_json());
    }

    #[test]
    fn warm_ai_key_cache_uses_catalog_provider_list() {
        // warm_ai_key_cache now iterates ai_provider_key_names() instead of a
        // hardcoded array. The dynamic list must still cover every provider the
        // hardcoded list did, and yield exact ai_api_key_<provider> keys.
        let names = ai_provider_key_names();
        for legacy in ["openai", "anthropic", "gemini", "custom"] {
            let key = format!("ai_api_key_{}", legacy);
            assert!(names.contains(&key), "dynamic key list missing {}", key);
        }
        // Every key is a proper ai_api_key_<provider>; stripping recovers a
        // valid provider id (what CacheApiKeyArgs re-prefixes).
        for key_name in &names {
            assert!(
                key_name.starts_with("ai_api_key_"),
                "unexpected key shape: {}",
                key_name
            );
        }
    }
}
